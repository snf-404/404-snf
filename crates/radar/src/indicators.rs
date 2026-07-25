// SPDX-License-Identifier: Apache-2.0

//! Stateful, transparent indicators derived from parsed radar frames.

use std::time::{Duration, Instant};

#[cfg(feature = "vital-signs")]
use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::{RadarFrame, RadarPoint};
#[cfg(feature = "vital-signs")]
use crate::{RadarProtocol, VitalSignsReading};

/// Cartesian region in which points contribute to the activity indicator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadarRoi {
    pub min_x_m: f32,
    pub max_x_m: f32,
    pub min_y_m: f32,
    pub max_y_m: f32,
    pub min_z_m: f32,
    pub max_z_m: f32,
}

impl Default for RadarRoi {
    fn default() -> Self {
        Self {
            min_x_m: -2.0,
            max_x_m: 2.0,
            min_y_m: 0.2,
            max_y_m: 4.0,
            min_z_m: -2.0,
            max_z_m: 2.0,
        }
    }
}

impl RadarRoi {
    fn contains(self, point: &RadarPoint) -> bool {
        point.x >= self.min_x_m
            && point.x <= self.max_x_m
            && point.y >= self.min_y_m
            && point.y <= self.max_y_m
            && point.z >= self.min_z_m
            && point.z <= self.max_z_m
    }
}

/// Tuning for the activity and optional vital-rate indicators.
#[derive(Clone, Debug)]
pub struct IndicatorConfig {
    pub roi: RadarRoi,
    /// Ignore points below this SNR when SNR is available.
    pub min_snr_db: Option<f32>,
    pub moving_velocity_threshold_mps: f32,
    pub short_activity_window: Duration,
    pub long_activity_window: Duration,
    /// Relative short-vs-long EWMA difference required to report a trend.
    pub activity_trend_threshold: f32,
    pub activity_confident_point_count: usize,
    #[cfg(feature = "vital-signs")]
    pub target_subject_id: Option<u16>,
    #[cfg(feature = "vital-signs")]
    pub vital_median_window: Duration,
    #[cfg(feature = "vital-signs")]
    pub vital_warmup: Duration,
    #[cfg(feature = "vital-signs")]
    pub maximum_vital_gap: Duration,
    #[cfg(feature = "vital-signs")]
    pub heart_rate_range_bpm: (f32, f32),
    #[cfg(feature = "vital-signs")]
    pub respiration_range_bpm: (f32, f32),
    #[cfg(feature = "vital-signs")]
    pub motion_contamination_rms_mps: f32,
}

impl Default for IndicatorConfig {
    fn default() -> Self {
        Self {
            roi: RadarRoi::default(),
            min_snr_db: Some(6.0),
            moving_velocity_threshold_mps: 0.05,
            short_activity_window: Duration::from_secs(10),
            long_activity_window: Duration::from_secs(60),
            activity_trend_threshold: 0.15,
            activity_confident_point_count: 10,
            #[cfg(feature = "vital-signs")]
            target_subject_id: None,
            #[cfg(feature = "vital-signs")]
            vital_median_window: Duration::from_secs(5),
            #[cfg(feature = "vital-signs")]
            vital_warmup: Duration::from_secs(20),
            #[cfg(feature = "vital-signs")]
            maximum_vital_gap: Duration::from_secs(2),
            #[cfg(feature = "vital-signs")]
            heart_rate_range_bpm: (30.0, 220.0),
            #[cfg(feature = "vital-signs")]
            respiration_range_bpm: (4.0, 60.0),
            #[cfg(feature = "vital-signs")]
            motion_contamination_rms_mps: 0.10,
        }
    }
}

/// Direction of the short-term activity level relative to its long baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityTrend {
    Rising,
    Falling,
    #[default]
    Steady,
}

/// Gross body-activity measurements in physical, inspectable units.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GrossActivity {
    pub contributing_points: usize,
    /// Mean of squared radial velocities, in `(m/s)^2`.
    pub motion_energy_mps2: f32,
    pub rms_radial_speed_mps: f32,
    pub moving_point_fraction: f32,
    pub short_term_energy_mps2: f32,
    pub long_term_energy_mps2: f32,
    pub trend: ActivityTrend,
    /// Point-support confidence only; this is not medical confidence.
    pub confidence: f32,
}

/// Availability/quality state for a vendor-provided vital-rate estimate.
#[cfg(feature = "vital-signs")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VitalStatus {
    NoSubject,
    InvalidVendorValue,
    WarmingUp,
    MotionContaminated,
    Valid,
}

/// Raw and stabilized form of a heart or respiration rate.
///
/// `range_bin` and `breathing_deviation` are subject-level values carried
/// straight from the frame's vendor vital record (both the heart and the
/// respiration estimate of a given subject report the same pair). They are
/// `None` only when no vendor reading backs the estimate (`NoSubject`); an
/// out-of-range rate still yields the record, so they survive
/// `InvalidVendorValue`.
#[cfg(feature = "vital-signs")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VitalRateEstimate {
    pub subject_id: Option<u16>,
    /// Unmodified value from TI's result TLV.
    pub raw_bpm: Option<f32>,
    /// Rolling median after warm-up and plausibility checks.
    pub stabilized_bpm: Option<f32>,
    pub confidence: f32,
    pub status: VitalStatus,
    /// TI vital result range bin for the tracked subject.
    pub range_bin: Option<u16>,
    /// Vendor breathing-deviation value from the vital record.
    pub breathing_deviation: Option<f32>,
}

#[cfg(feature = "vital-signs")]
impl VitalRateEstimate {
    fn unavailable(status: VitalStatus, subject_id: Option<u16>) -> Self {
        Self {
            subject_id,
            raw_bpm: None,
            stabilized_bpm: None,
            confidence: 0.0,
            status,
            range_bin: None,
            breathing_deviation: None,
        }
    }
}

/// All implemented indicators for one input frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndicatorSnapshot {
    pub frame_number: u32,
    pub activity: GrossActivity,
    #[cfg(feature = "vital-signs")]
    pub heart_rate: Option<VitalRateEstimate>,
    #[cfg(feature = "vital-signs")]
    pub respiration_rate: Option<VitalRateEstimate>,
}

/// Stateful extractor for activity and, when enabled, vital rates.
pub struct IndicatorEngine {
    config: IndicatorConfig,
    short_energy: Option<f32>,
    long_energy: Option<f32>,
    last_activity_at: Option<Instant>,
    #[cfg(feature = "vital-signs")]
    vitals: VitalState,
}

impl IndicatorEngine {
    pub fn new(config: IndicatorConfig) -> Self {
        Self {
            config,
            short_energy: None,
            long_energy: None,
            last_activity_at: None,
            #[cfg(feature = "vital-signs")]
            vitals: VitalState::default(),
        }
    }

    /// Consume one frame at its host receipt time.
    pub fn update(&mut self, received_at: Instant, frame: &RadarFrame) -> IndicatorSnapshot {
        let activity = self.update_activity(received_at, frame);
        #[cfg(feature = "vital-signs")]
        let (heart_rate, respiration_rate) = if frame.protocol == RadarProtocol::VitalSigns {
            let (heart, respiration) =
                self.update_vitals(received_at, frame, activity.rms_radial_speed_mps);
            (Some(heart), Some(respiration))
        } else {
            self.vitals.reset();
            (None, None)
        };

        IndicatorSnapshot {
            frame_number: frame.frame_number(),
            activity,
            #[cfg(feature = "vital-signs")]
            heart_rate,
            #[cfg(feature = "vital-signs")]
            respiration_rate,
        }
    }

    fn update_activity(&mut self, received_at: Instant, frame: &RadarFrame) -> GrossActivity {
        let points = frame.points.iter().filter(|point| {
            point.x.is_finite()
                && point.y.is_finite()
                && point.z.is_finite()
                && point.velocity.is_finite()
                && self.config.roi.contains(point)
                && self
                    .config
                    .min_snr_db
                    .is_none_or(|minimum| point.snr_db.is_none_or(|snr| snr >= minimum))
        });

        let mut count = 0_usize;
        let mut squared_velocity_sum = 0.0_f32;
        let mut moving_count = 0_usize;
        for point in points {
            count += 1;
            squared_velocity_sum += point.velocity * point.velocity;
            if point.velocity.abs() >= self.config.moving_velocity_threshold_mps {
                moving_count += 1;
            }
        }

        let energy = if count == 0 {
            0.0
        } else {
            squared_velocity_sum / count as f32
        };
        let moving_point_fraction = if count == 0 {
            0.0
        } else {
            moving_count as f32 / count as f32
        };

        let elapsed = self
            .last_activity_at
            .and_then(|previous| received_at.checked_duration_since(previous))
            .unwrap_or_default();
        self.short_energy = Some(update_ewma(
            self.short_energy,
            energy,
            elapsed,
            self.config.short_activity_window,
        ));
        self.long_energy = Some(update_ewma(
            self.long_energy,
            energy,
            elapsed,
            self.config.long_activity_window,
        ));
        self.last_activity_at = Some(received_at);

        let short = self.short_energy.unwrap_or(energy);
        let long = self.long_energy.unwrap_or(energy);
        let relative_difference = (short - long) / long.max(1e-6);
        let trend = if relative_difference > self.config.activity_trend_threshold {
            ActivityTrend::Rising
        } else if relative_difference < -self.config.activity_trend_threshold {
            ActivityTrend::Falling
        } else {
            ActivityTrend::Steady
        };
        let confidence = if self.config.activity_confident_point_count == 0 {
            1.0
        } else {
            (count as f32 / self.config.activity_confident_point_count as f32).min(1.0)
        };

        GrossActivity {
            contributing_points: count,
            motion_energy_mps2: energy,
            rms_radial_speed_mps: energy.sqrt(),
            moving_point_fraction,
            short_term_energy_mps2: short,
            long_term_energy_mps2: long,
            trend,
            confidence,
        }
    }

    #[cfg(feature = "vital-signs")]
    fn update_vitals(
        &mut self,
        received_at: Instant,
        frame: &RadarFrame,
        rms_speed_mps: f32,
    ) -> (VitalRateEstimate, VitalRateEstimate) {
        if self.vitals.last_seen.is_some_and(|last| {
            received_at.saturating_duration_since(last) > self.config.maximum_vital_gap
        }) {
            self.vitals.reset();
        }

        let configured_id = self.config.target_subject_id;
        let desired_id = configured_id.or(self.vitals.subject_id);
        let reading = desired_id
            .and_then(|id| {
                frame
                    .vital_signs
                    .iter()
                    .find(|reading| reading.subject_id == id)
            })
            .or_else(|| {
                if configured_id.is_none() {
                    frame.vital_signs.first()
                } else {
                    None
                }
            });

        let Some(reading) = reading else {
            return (
                VitalRateEstimate::unavailable(VitalStatus::NoSubject, desired_id),
                VitalRateEstimate::unavailable(VitalStatus::NoSubject, desired_id),
            );
        };

        if self.vitals.subject_id != Some(reading.subject_id) {
            self.vitals.reset();
            self.vitals.subject_id = Some(reading.subject_id);
        }
        self.vitals.last_seen = Some(received_at);
        prune_samples(
            &mut self.vitals.heart_samples,
            received_at,
            self.config.vital_median_window,
        );
        prune_samples(
            &mut self.vitals.respiration_samples,
            received_at,
            self.config.vital_median_window,
        );
        let heart_valid = in_range(reading.heart_rate_bpm, self.config.heart_rate_range_bpm);
        let respiration_valid = in_range(
            reading.breathing_rate_bpm,
            self.config.respiration_range_bpm,
        );
        if heart_valid {
            self.vitals.heart_first_valid_at.get_or_insert(received_at);
            self.vitals
                .heart_samples
                .push_back((received_at, reading.heart_rate_bpm));
        }
        if respiration_valid {
            self.vitals
                .respiration_first_valid_at
                .get_or_insert(received_at);
            self.vitals
                .respiration_samples
                .push_back((received_at, reading.breathing_rate_bpm));
        }

        let heart_warmed_up = self.vitals.heart_first_valid_at.is_some_and(|first| {
            received_at.saturating_duration_since(first) >= self.config.vital_warmup
        });
        let respiration_warmed_up = self.vitals.respiration_first_valid_at.is_some_and(|first| {
            received_at.saturating_duration_since(first) >= self.config.vital_warmup
        });
        let contaminated = rms_speed_mps >= self.config.motion_contamination_rms_mps;
        let heart = rate_estimate(
            reading,
            reading.heart_rate_bpm,
            heart_valid,
            &self.vitals.heart_samples,
            heart_warmed_up,
            contaminated,
        );
        let respiration = rate_estimate(
            reading,
            reading.breathing_rate_bpm,
            respiration_valid,
            &self.vitals.respiration_samples,
            respiration_warmed_up,
            contaminated,
        );
        (heart, respiration)
    }
}

impl Default for IndicatorEngine {
    fn default() -> Self {
        Self::new(IndicatorConfig::default())
    }
}

fn update_ewma(previous: Option<f32>, value: f32, elapsed: Duration, window: Duration) -> f32 {
    let Some(previous) = previous else {
        return value;
    };
    if window.is_zero() {
        return value;
    }
    let alpha = 1.0 - (-elapsed.as_secs_f32() / window.as_secs_f32()).exp();
    previous + alpha.clamp(0.0, 1.0) * (value - previous)
}

#[cfg(feature = "vital-signs")]
#[derive(Default)]
struct VitalState {
    subject_id: Option<u16>,
    heart_first_valid_at: Option<Instant>,
    respiration_first_valid_at: Option<Instant>,
    last_seen: Option<Instant>,
    heart_samples: VecDeque<(Instant, f32)>,
    respiration_samples: VecDeque<(Instant, f32)>,
}

#[cfg(feature = "vital-signs")]
impl VitalState {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(feature = "vital-signs")]
fn prune_samples(samples: &mut VecDeque<(Instant, f32)>, now: Instant, window: Duration) {
    while samples
        .front()
        .is_some_and(|(at, _)| now.saturating_duration_since(*at) > window)
    {
        samples.pop_front();
    }
}

#[cfg(feature = "vital-signs")]
fn in_range(value: f32, range: (f32, f32)) -> bool {
    value.is_finite() && value >= range.0 && value <= range.1
}

#[cfg(feature = "vital-signs")]
fn rate_estimate(
    reading: &VitalSignsReading,
    raw_bpm: f32,
    valid: bool,
    samples: &VecDeque<(Instant, f32)>,
    warmed_up: bool,
    contaminated: bool,
) -> VitalRateEstimate {
    if !valid {
        return VitalRateEstimate {
            subject_id: Some(reading.subject_id),
            raw_bpm: Some(raw_bpm),
            stabilized_bpm: None,
            confidence: 0.0,
            status: VitalStatus::InvalidVendorValue,
            range_bin: Some(reading.range_bin),
            breathing_deviation: Some(reading.breathing_deviation),
        };
    }

    let median = median(samples.iter().map(|(_, value)| *value));
    let confidence = median.map_or(0.0, |center| {
        let mean_absolute_deviation = samples
            .iter()
            .map(|(_, value)| (value - center).abs())
            .sum::<f32>()
            / samples.len().max(1) as f32;
        let stability = 1.0 - (mean_absolute_deviation / center.max(1.0) * 5.0).min(1.0);
        stability * (samples.len() as f32 / 5.0).min(1.0)
    });
    let (status, stabilized_bpm, confidence) = if !warmed_up {
        (VitalStatus::WarmingUp, None, 0.0)
    } else if contaminated {
        (VitalStatus::MotionContaminated, median, confidence * 0.5)
    } else {
        (VitalStatus::Valid, median, confidence)
    };

    VitalRateEstimate {
        subject_id: Some(reading.subject_id),
        raw_bpm: Some(raw_bpm),
        stabilized_bpm,
        confidence,
        status,
        range_bin: Some(reading.range_bin),
        breathing_deviation: Some(reading.breathing_deviation),
    }
}

#[cfg(feature = "vital-signs")]
fn median(values: impl Iterator<Item = f32>) -> Option<f32> {
    let mut values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[middle - 1] + values[middle]) * 0.5)
    } else {
        Some(values[middle])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "vital-signs")]
    use crate::VitalSignsReading;
    use crate::{FrameHeader, RadarProtocol};

    fn frame(number: u32, velocities: &[f32]) -> RadarFrame {
        RadarFrame {
            protocol: RadarProtocol::OutOfBox,
            header: FrameHeader {
                frame_number: number,
                num_detected_objects: velocities.len() as u32,
                ..FrameHeader::default()
            },
            points: velocities
                .iter()
                .map(|velocity| RadarPoint {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                    velocity: *velocity,
                    snr_db: Some(20.0),
                    noise_db: None,
                })
                .collect(),
            range_profile: None,
            processing_stats: None,
            temperature_stats: None,
            #[cfg(feature = "vital-signs")]
            targets: Vec::new(),
            #[cfg(feature = "vital-signs")]
            previous_point_associations: Vec::new(),
            #[cfg(feature = "vital-signs")]
            vital_signs: Vec::new(),
            unknown_tlv_types: Vec::new(),
        }
    }

    #[test]
    fn calculates_activity_in_physical_units() {
        let mut engine = IndicatorEngine::default();
        let snapshot = engine.update(Instant::now(), &frame(1, &[0.0, 0.3, 0.4]));

        assert!((snapshot.activity.motion_energy_mps2 - 0.25 / 3.0).abs() < 1e-6);
        assert!((snapshot.activity.rms_radial_speed_mps - (0.25_f32 / 3.0).sqrt()).abs() < 1e-6);
        assert_eq!(snapshot.activity.moving_point_fraction, 2.0 / 3.0);
        assert_eq!(snapshot.activity.contributing_points, 3);
    }

    #[test]
    fn ignores_bad_snr_and_out_of_roi_points() {
        let mut input = frame(1, &[0.2, 0.4]);
        input.points[0].snr_db = Some(1.0);
        input.points[1].y = 10.0;
        let snapshot = IndicatorEngine::default().update(Instant::now(), &input);
        assert_eq!(snapshot.activity.contributing_points, 0);
        assert_eq!(snapshot.activity.motion_energy_mps2, 0.0);
    }

    #[test]
    fn reports_rising_and_falling_activity() {
        let started = Instant::now();
        let config = IndicatorConfig {
            short_activity_window: Duration::from_secs(1),
            long_activity_window: Duration::from_secs(20),
            ..IndicatorConfig::default()
        };
        let mut engine = IndicatorEngine::new(config);
        engine.update(started, &frame(1, &[0.01]));
        let rising = engine.update(started + Duration::from_secs(2), &frame(2, &[1.0]));
        assert_eq!(rising.activity.trend, ActivityTrend::Rising);

        let falling = engine.update(started + Duration::from_secs(10), &frame(3, &[0.0]));
        assert_eq!(falling.activity.trend, ActivityTrend::Falling);
    }

    #[cfg(feature = "vital-signs")]
    #[test]
    fn warms_up_and_stabilizes_vendor_vitals() {
        let started = Instant::now();
        let config = IndicatorConfig {
            vital_warmup: Duration::from_secs(2),
            vital_median_window: Duration::from_secs(5),
            ..IndicatorConfig::default()
        };
        let mut engine = IndicatorEngine::new(config);
        let mut input = vital_frame(7, 72.0, 15.0, 0.0);

        let initial = engine.update(started, &input);
        assert_eq!(initial.heart_rate.unwrap().status, VitalStatus::WarmingUp);
        input.vital_signs[0].heart_rate_bpm = 74.0;
        engine.update(started + Duration::from_secs(1), &input);
        input.vital_signs[0].heart_rate_bpm = 73.0;
        let ready = engine.update(started + Duration::from_secs(2), &input);
        let heart = ready.heart_rate.unwrap();
        assert_eq!(heart.status, VitalStatus::Valid);
        assert_eq!(heart.stabilized_bpm, Some(73.0));
        // Subject-level vendor fields ride straight through from the record.
        assert_eq!(heart.range_bin, Some(10));
        assert_eq!(heart.breathing_deviation, Some(0.1));
    }

    #[cfg(feature = "vital-signs")]
    #[test]
    fn retains_raw_value_when_motion_contaminates_vitals() {
        let started = Instant::now();
        let config = IndicatorConfig {
            vital_warmup: Duration::ZERO,
            ..IndicatorConfig::default()
        };
        let mut engine = IndicatorEngine::new(config);
        let input = vital_frame(7, 72.0, 15.0, 0.5);
        let result = engine.update(started, &input).heart_rate.unwrap();
        assert_eq!(result.status, VitalStatus::MotionContaminated);
        assert_eq!(result.raw_bpm, Some(72.0));
        assert_eq!(result.stabilized_bpm, Some(72.0));
    }

    #[cfg(feature = "vital-signs")]
    #[test]
    fn rejects_implausible_rate_and_resets_on_subject_change() {
        let started = Instant::now();
        let config = IndicatorConfig {
            vital_warmup: Duration::ZERO,
            ..IndicatorConfig::default()
        };
        let mut engine = IndicatorEngine::new(config);
        let invalid = vital_frame(7, f32::NAN, 15.0, 0.0);
        assert_eq!(
            engine.update(started, &invalid).heart_rate.unwrap().status,
            VitalStatus::InvalidVendorValue
        );

        let changed = vital_frame(8, 80.0, 18.0, 0.0);
        let result = engine
            .update(started + Duration::from_secs(1), &changed)
            .heart_rate
            .unwrap();
        assert_eq!(result.subject_id, Some(8));
        assert_eq!(result.stabilized_bpm, Some(80.0));
    }

    #[cfg(feature = "vital-signs")]
    #[test]
    fn reports_subject_loss_and_restarts_after_a_gap() {
        let started = Instant::now();
        let config = IndicatorConfig {
            vital_warmup: Duration::from_secs(1),
            maximum_vital_gap: Duration::from_secs(1),
            ..IndicatorConfig::default()
        };
        let mut engine = IndicatorEngine::new(config);
        let input = vital_frame(7, 72.0, 15.0, 0.0);
        engine.update(started, &input);
        assert_eq!(
            engine
                .update(started + Duration::from_secs(1), &input)
                .heart_rate
                .unwrap()
                .status,
            VitalStatus::Valid
        );

        let mut missing = input.clone();
        missing.vital_signs.clear();
        assert_eq!(
            engine
                .update(started + Duration::from_millis(1500), &missing)
                .heart_rate
                .unwrap()
                .status,
            VitalStatus::NoSubject
        );
        assert_eq!(
            engine
                .update(started + Duration::from_secs(3), &input)
                .heart_rate
                .unwrap()
                .status,
            VitalStatus::WarmingUp
        );
    }

    #[cfg(feature = "vital-signs")]
    fn vital_frame(
        subject_id: u16,
        heart_rate_bpm: f32,
        breathing_rate_bpm: f32,
        velocity: f32,
    ) -> RadarFrame {
        let mut result = frame(1, &[velocity]);
        result.protocol = RadarProtocol::VitalSigns;
        result.vital_signs.push(VitalSignsReading {
            subject_id,
            range_bin: 10,
            breathing_deviation: 0.1,
            heart_rate_bpm,
            breathing_rate_bpm,
            heart_waveform: [0.0; 15],
            breath_waveform: [0.0; 15],
        });
        result
    }
}
