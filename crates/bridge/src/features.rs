// SPDX-License-Identifier: Apache-2.0

//! Windowed feature extraction: indicator snapshots in, model input out.
//!
//! The fatigue model does not see an instant, it sees a **window**. That is the
//! whole reason this module is stateful and [`crate::map`] is not. Drowsiness is
//! not visible in a heart rate — it is visible in a heart rate that has been
//! sagging for twenty minutes, in breathing that has become metronomic, in
//! motion that has decayed toward the noise floor. An instantaneous reading of
//! 62 bpm is meaningless; 62 bpm against a personal baseline of 71, trending
//! down, is the signal.
//!
//! So [`FeatureExtractor`] keeps two horizons:
//!
//! * a **window** ([`WINDOW`], 90 s) that slope, standard deviation and the
//!   motion duty fraction are computed over. Long enough for a respiration
//!   estimate to show its variability, short enough that the verdict still
//!   tracks someone settling.
//! * a **baseline** ([`BASELINE_TAU`], 15 min EWMA) that deviations are measured
//!   against. It has to move far slower than the window, because minutes-long
//!   drift is the signal rather than the noise — a baseline that chased the
//!   window would subtract out exactly what we are trying to see.
//!
//! # These constants are not deployment configuration
//!
//! Unlike the wiring in `Repose.toml`, nothing here is a knob. The window and
//! the baseline time constant are baked into the ONNX weights: `network/`
//! trained against features computed exactly this way, so changing one without
//! retraining silently feeds the model a distribution it has never seen. They
//! are mirrored in `network/snf_net/contract.py`; the two must move together.
//!
//! # Missing readings
//!
//! A dropped channel is reported as *invalid*, never as zero. `hr_bpm = 0.0`
//! with `hr_valid = true` would be a heart that has stopped; the model is
//! trained to read the flag and fall back on the channels that remain, and to
//! widen its output distribution when it does — which the confidence gate then
//! turns into a mat that stays still. A channel that is present but sparse is
//! also reported invalid: a least-squares slope fitted through eight samples
//! spanning four seconds is noise with a large magnitude, and the model would
//! read it as a violent trend.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use snf_fatigue::FatigueFeatures;
use snf_radar::{IndicatorSnapshot, VitalRateEstimate};

/// Span the window statistics cover.
pub const WINDOW: Duration = Duration::from_secs(90);

/// Time constant of the per-subject baseline EWMA.
pub const BASELINE_TAU: Duration = Duration::from_secs(900);

/// Motion energy above which a sample counts as movement, in `(m/s)²`.
pub const MOTION_ACTIVE_THRESHOLD: f32 = 0.02;

/// A channel needs valid readings spanning at least this fraction of the window
/// before its slope and spread are trusted.
const MIN_VALID_SPAN: f64 = 0.5;

/// …and at least this many of them, so a burst of samples in a long-but-sparse
/// window cannot qualify on span alone.
const MIN_VALID_SAMPLES: usize = 8;

/// One retained snapshot: only what the features are computed from.
#[derive(Clone, Copy, Debug)]
struct Sample {
    at: Instant,
    hr_bpm: Option<f32>,
    rr_bpm: Option<f32>,
    /// `ln(1 + 1000 · motion_energy_mps2)`, precomputed so a long window is not
    /// re-logged on every tick.
    motion_log: f32,
    motion_active: bool,
}

/// Rolling window and baselines behind [`FatigueFeatures`].
///
/// One per session. Reset it (or make a new one) when the subject changes —
/// the baselines are personal, and inheriting someone else's is worse than
/// having none.
#[derive(Debug, Default)]
pub struct FeatureExtractor {
    started_at: Option<Instant>,
    samples: VecDeque<Sample>,
    hr_baseline: Option<f32>,
    rr_baseline: Option<f32>,
    baseline_at: Option<Instant>,
}

impl FeatureExtractor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seconds of history currently retained. Mostly for tests and logging.
    pub fn window_span(&self) -> Duration {
        match (self.samples.front(), self.samples.back()) {
            (Some(first), Some(last)) => last.at.duration_since(first.at),
            _ => Duration::ZERO,
        }
    }

    /// Fold one indicator snapshot in and produce the current feature vector.
    ///
    /// Safe to call at any rate: every time-dependent quantity is computed from
    /// real timestamps rather than a sample count, so a client changing
    /// `vitals_hz` mid-session (`PROTOCOL.md` §12) shifts how much history the
    /// window holds but not what the features mean.
    pub fn update(&mut self, now: Instant, snapshot: &IndicatorSnapshot) -> FatigueFeatures {
        let started_at = *self.started_at.get_or_insert(now);

        let hr_bpm = stabilized(snapshot.heart_rate.as_ref());
        let rr_bpm = stabilized(snapshot.respiration_rate.as_ref());
        let motion_energy = sanitize(snapshot.activity.motion_energy_mps2).unwrap_or(0.0);

        self.update_baselines(now, hr_bpm, rr_bpm);

        self.samples.push_back(Sample {
            at: now,
            hr_bpm,
            rr_bpm,
            motion_log: (1.0 + 1000.0 * motion_energy.max(0.0)).ln(),
            motion_active: motion_energy > MOTION_ACTIVE_THRESHOLD,
        });
        // Drop anything that has fallen out of the window. `now` can in
        // principle be older than a retained sample if the caller passes
        // instants out of order, so this uses a saturating comparison rather
        // than `duration_since`.
        while self
            .samples
            .front()
            .is_some_and(|s| now.saturating_duration_since(s.at) > WINDOW)
        {
            self.samples.pop_front();
        }

        let hr = self.channel(|s| s.hr_bpm, self.hr_baseline);
        let rr = self.channel(|s| s.rr_bpm, self.rr_baseline);
        let motion = self.channel(|s| Some(s.motion_log), None);

        // Motion energy is a *point* observation — unlike the rate channels it
        // needs no window to be meaningful, so it is read from the newest
        // sample rather than from `motion.last`. That matters on a cold start:
        // `ChannelStats::default()` would report 0.0, and 0.0 is not "unknown"
        // to the model, it is "completely still", which is the strongest
        // quiescence evidence there is. The rate channels have validity flags to
        // say "no reading"; motion has none, so it must never report a value it
        // does not mean. Only the slope genuinely needs history, and 0.0 there
        // reads as "no trend", which is both true and in-distribution.
        let motion_log_energy = self.samples.back().map_or(0.0, |s| s.motion_log);

        let active = self.samples.iter().filter(|s| s.motion_active).count();
        let motion_active_fraction = if self.samples.is_empty() {
            0.0
        } else {
            active as f32 / self.samples.len() as f32
        };

        FatigueFeatures {
            hr_bpm: hr.last,
            hr_baseline_delta: hr.baseline_delta,
            hr_slope: hr.slope,
            hr_std: hr.std,

            rr_bpm: rr.last,
            rr_baseline_delta: rr.baseline_delta,
            rr_slope: rr.slope,
            rr_std: rr.std,

            motion_log_energy,
            motion_slope: motion.slope,
            motion_active_fraction,

            time_on_task_h: now.duration_since(started_at).as_secs_f32() / 3600.0,

            hr_valid: hr.valid,
            rr_valid: rr.valid,
        }
    }

    /// Advance both baselines toward the newest readings.
    ///
    /// `alpha = 1 - exp(-dt/tau)` uses the real elapsed time, so the baseline's
    /// 15-minute horizon holds regardless of the notification rate. An absent
    /// reading leaves that baseline untouched rather than decaying it toward
    /// nothing — a channel that dropped out for a minute should come back to
    /// the baseline it left.
    fn update_baselines(&mut self, now: Instant, hr_bpm: Option<f32>, rr_bpm: Option<f32>) {
        let dt = self
            .baseline_at
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or(Duration::ZERO);
        self.baseline_at = Some(now);

        let alpha = 1.0 - (-dt.as_secs_f32() / BASELINE_TAU.as_secs_f32()).exp();
        blend(&mut self.hr_baseline, hr_bpm, alpha);
        blend(&mut self.rr_baseline, rr_bpm, alpha);
    }

    /// Window statistics for one channel.
    fn channel(
        &self,
        pick: impl Fn(&Sample) -> Option<f32>,
        baseline: Option<f32>,
    ) -> ChannelStats {
        let points: Vec<(f64, f32)> = self
            .samples
            .iter()
            .filter_map(|s| pick(s).map(|v| (s.at, v)))
            .scan(None, |first: &mut Option<Instant>, (at, v)| {
                let origin = *first.get_or_insert(at);
                // Minutes, because the model's slope features are per minute.
                Some((at.saturating_duration_since(origin).as_secs_f64() / 60.0, v))
            })
            .collect();

        let span_ok = points
            .last()
            .zip(points.first())
            .is_some_and(|(last, first)| {
                last.0 - first.0 >= WINDOW.as_secs_f64() * MIN_VALID_SPAN / 60.0
            });
        if points.len() < MIN_VALID_SAMPLES || !span_ok {
            return ChannelStats::default();
        }

        let last = points[points.len() - 1].1;
        let mean = points.iter().map(|(_, v)| *v as f64).sum::<f64>() / points.len() as f64;
        let variance = points
            .iter()
            .map(|(_, v)| (*v as f64 - mean).powi(2))
            .sum::<f64>()
            / points.len() as f64;

        ChannelStats {
            last,
            baseline_delta: baseline.map_or(0.0, |b| last - b),
            slope: slope(&points),
            std: variance.sqrt() as f32,
            valid: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ChannelStats {
    last: f32,
    baseline_delta: f32,
    slope: f32,
    std: f32,
    valid: bool,
}

/// Least-squares slope of value against minutes. Zero when undefined.
fn slope(points: &[(f64, f32)]) -> f32 {
    let n = points.len() as f64;
    let x_mean = points.iter().map(|(x, _)| *x).sum::<f64>() / n;
    let y_mean = points.iter().map(|(_, y)| *y as f64).sum::<f64>() / n;

    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (x, y) in points {
        numerator += (x - x_mean) * (*y as f64 - y_mean);
        denominator += (x - x_mean).powi(2);
    }
    if denominator <= 0.0 {
        return 0.0;
    }
    (numerator / denominator) as f32
}

/// EWMA step that seeds on the first reading instead of ramping from zero.
fn blend(baseline: &mut Option<f32>, reading: Option<f32>, alpha: f32) {
    let Some(value) = reading else { return };
    *baseline = Some(match *baseline {
        None => value,
        Some(current) => current + alpha * (value - current),
    });
}

/// The stabilized rate, if the estimate carries one and it is finite.
fn stabilized(estimate: Option<&VitalRateEstimate>) -> Option<f32> {
    estimate.and_then(|e| e.stabilized_bpm).and_then(sanitize)
}

/// Reject NaN and infinity at the boundary. The radar's vendor TLVs are not
/// guaranteed finite, and a NaN reaching the EWMA would poison the baseline for
/// the rest of the session — every subsequent comparison against it silently
/// false, with no error anywhere.
fn sanitize(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use snf_radar::{ActivityTrend, GrossActivity, VitalStatus};

    use super::*;

    fn estimate(bpm: f32) -> VitalRateEstimate {
        VitalRateEstimate {
            subject_id: Some(1),
            raw_bpm: Some(bpm),
            stabilized_bpm: Some(bpm),
            confidence: 0.9,
            status: VitalStatus::Valid,
            range_bin: Some(4),
            breathing_deviation: Some(0.1),
        }
    }

    fn snapshot(hr: Option<f32>, rr: Option<f32>, motion: f32) -> IndicatorSnapshot {
        IndicatorSnapshot {
            frame_number: 0,
            activity: GrossActivity {
                contributing_points: 12,
                motion_energy_mps2: motion,
                rms_radial_speed_mps: 0.1,
                moving_point_fraction: 0.2,
                short_term_energy_mps2: motion,
                long_term_energy_mps2: motion,
                trend: ActivityTrend::Steady,
                confidence: 0.9,
            },
            heart_rate: hr.map(estimate),
            respiration_rate: rr.map(estimate),
        }
    }

    /// Feed `count` samples at 2 Hz, with values from `value(i)`.
    fn run(
        extractor: &mut FeatureExtractor,
        start: Instant,
        count: usize,
        mut value: impl FnMut(usize) -> (Option<f32>, Option<f32>, f32),
    ) -> FatigueFeatures {
        let mut features = FatigueFeatures::default();
        for i in 0..count {
            let (hr, rr, motion) = value(i);
            let now = start + Duration::from_millis(500 * i as u64);
            features = extractor.update(now, &snapshot(hr, rr, motion));
        }
        features
    }

    /// A window that has just started has no slope worth trusting, so both rate
    /// channels report invalid rather than emitting a noisy one.
    #[test]
    fn a_cold_window_is_invalid() {
        let mut extractor = FeatureExtractor::new();
        let features = run(&mut extractor, Instant::now(), 4, |_| {
            (Some(70.0), Some(14.0), 0.01)
        });
        assert!(!features.hr_valid);
        assert!(!features.rr_valid);
    }

    /// Motion has no validity flag, so it must never report a value it does not
    /// mean. On a cold window it reports what it just measured — reporting 0.0
    /// would tell the model the subject is perfectly still, which is the single
    /// strongest piece of fatigue evidence the rule has.
    #[test]
    fn a_cold_window_still_reports_real_motion() {
        let mut extractor = FeatureExtractor::new();
        let features = run(&mut extractor, Instant::now(), 2, |_| {
            (Some(70.0), Some(14.0), 0.35)
        });
        let expected = (1.0f32 + 1000.0 * 0.35).ln();
        assert!(
            (features.motion_log_energy - expected).abs() < 1e-4,
            "cold window reported {} instead of the measured {expected}",
            features.motion_log_energy
        );
        // The slope, which genuinely needs history, is the one that reads zero.
        assert_eq!(features.motion_slope, 0.0);
    }

    #[test]
    fn a_full_window_reports_both_channels() {
        let mut extractor = FeatureExtractor::new();
        let features = run(&mut extractor, Instant::now(), 180, |_| {
            (Some(70.0), Some(14.0), 0.01)
        });
        assert!(features.hr_valid && features.rr_valid);
        assert_eq!(features.hr_bpm, 70.0);
        assert!(features.hr_std.abs() < 1e-5, "constant input has no spread");
        assert!(
            features.hr_slope.abs() < 1e-3,
            "constant input has no slope"
        );
    }

    /// The unit that matters: a rate falling 6 bpm over 90 s is -4 bpm/min.
    #[test]
    fn slope_is_per_minute() {
        let mut extractor = FeatureExtractor::new();
        let features = run(&mut extractor, Instant::now(), 181, |i| {
            (Some(70.0 - 6.0 * i as f32 / 180.0), Some(14.0), 0.01)
        });
        assert!(
            (features.hr_slope - -4.0).abs() < 0.1,
            "expected ≈-4 bpm/min, got {}",
            features.hr_slope
        );
    }

    /// A dropped channel must read invalid, never as a zero rate — a heart rate
    /// of 0 bpm is what would drive the mat hardest.
    #[test]
    fn a_dropped_channel_is_invalid_not_zero() {
        let mut extractor = FeatureExtractor::new();
        let features = run(&mut extractor, Instant::now(), 180, |i| {
            // Heart rate present only for the first quarter, then lost.
            ((i < 45).then_some(70.0), Some(14.0), 0.01)
        });
        assert!(!features.hr_valid, "sparse channel should not qualify");
        assert_eq!(features.hr_bpm, 0.0);
        assert!(features.rr_valid, "respiration was present throughout");
    }

    /// The baseline is slow on purpose: after 90 s of a 15-minute EWMA a 10 bpm
    /// step should still be most of the way visible as a deviation.
    #[test]
    fn baseline_lags_far_behind_the_window() {
        let mut extractor = FeatureExtractor::new();
        let start = Instant::now();
        run(&mut extractor, start, 180, |_| {
            (Some(70.0), Some(14.0), 0.01)
        });
        let features = run(&mut extractor, start + Duration::from_secs(90), 180, |_| {
            (Some(60.0), Some(14.0), 0.01)
        });
        assert!(
            features.hr_baseline_delta < -8.0,
            "a 10 bpm drop should still read as a large deviation, got {}",
            features.hr_baseline_delta
        );
    }

    #[test]
    fn motion_duty_tracks_the_threshold() {
        let mut extractor = FeatureExtractor::new();
        let features = run(&mut extractor, Instant::now(), 180, |i| {
            // Every fourth sample is above the movement threshold.
            (Some(70.0), Some(14.0), if i % 4 == 0 { 0.5 } else { 0.001 })
        });
        assert!((features.motion_active_fraction - 0.25).abs() < 0.02);
    }

    /// Non-finite vendor values must not reach the baselines.
    #[test]
    fn non_finite_readings_are_rejected() {
        let mut extractor = FeatureExtractor::new();
        let features = run(&mut extractor, Instant::now(), 180, |i| {
            let hr = if i == 90 { f32::NAN } else { 70.0 };
            (
                Some(hr),
                Some(14.0),
                if i == 30 { f32::INFINITY } else { 0.01 },
            )
        });
        assert!(features.hr_bpm.is_finite());
        assert!(features.hr_baseline_delta.is_finite());
        assert!(features.motion_log_energy.is_finite());
        assert!(features.hr_std.is_finite());
    }

    /// Window contents are bounded by time, not by sample count, so a long
    /// session does not grow the buffer without limit.
    #[test]
    fn the_window_is_bounded_by_time() {
        let mut extractor = FeatureExtractor::new();
        run(&mut extractor, Instant::now(), 2000, |_| {
            (Some(70.0), Some(14.0), 0.01)
        });
        assert!(extractor.window_span() <= WINDOW);
        assert!(
            extractor.samples.len() <= 185,
            "{}",
            extractor.samples.len()
        );
    }

    #[test]
    fn time_on_task_accumulates() {
        let mut extractor = FeatureExtractor::new();
        let start = Instant::now();
        run(&mut extractor, start, 10, |_| {
            (Some(70.0), Some(14.0), 0.01)
        });
        let features = run(
            &mut extractor,
            start + Duration::from_secs(3600),
            10,
            |_| (Some(70.0), Some(14.0), 0.01),
        );
        assert!((features.time_on_task_h - 1.0).abs() < 0.01);
    }
}
