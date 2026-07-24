// SPDX-License-Identifier: Apache-2.0

//! Pure conversions from domain types to BLE telemetry payloads.
//!
//! Every scaling factor and sentinel here is dictated by `PROTOCOL.md` §7–§8:
//! rates are `bpm × 100` (`0xffff` unavailable), motion energy is `(m/s)² ×
//! 1_000_000`, RMS speed is `mm/s`, the moving fraction is Q15, confidences are
//! `0..=100`. The functions never panic: out-of-range or non-finite inputs are
//! clamped or mapped to the unavailable sentinel.

use snf_fatigue::{FatigueFeatures, FatigueLevel};
use snf_radar::{IndicatorSnapshot, VitalRateEstimate, VitalStatus};

use snf_ble::protocol::{
    Fatigue, RATE_UNAVAILABLE, SUBJECT_UNKNOWN, Vitals, fatigue_flags, header_flags, vitals_flags,
};

/// A BLE Vitals payload together with the telemetry-header flags that describe
/// its quality (`DEGRADED`, `STALE` — `PROTOCOL.md` §6, §7). The caller passes
/// `header_flags` to [`publish`](snf_ble::BleTransport::publish).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappedVitals {
    pub payload: Vitals,
    pub header_flags: u8,
}

/// Map an indicator snapshot to a BLE Vitals payload.
///
/// The stabilized (median-filtered) rate is what gets published; a raw-but-
/// unstable estimate is reported as unavailable with the relevant status flag.
/// When a rate is motion-contaminated the last good value is sent with the
/// header `STALE` + `DEGRADED` flags set, per the §7 rule that status precedes
/// value.
///
/// `range_bin` (§7 `range_bin`) and `breathing_deviation` (§7
/// `breathing_deviation_q8_8`, vendor unit × 256) come straight from the frame's
/// vendor vital record via [`VitalRateEstimate`]. They are subject-level, so
/// both the heart and respiration estimate carry the same pair; the heart
/// estimate is read first, falling back to respiration, and to `0` when no
/// vendor reading backs the snapshot.
pub fn vitals(snapshot: &IndicatorSnapshot) -> MappedVitals {
    let activity = &snapshot.activity;
    let heart = snapshot.heart_rate.as_ref();
    let respiration = snapshot.respiration_rate.as_ref();

    let mut status_flags = 0u16;
    let mut header = 0u8;

    let tracked =
        activity.contributing_points > 0 || has_subject(heart) || has_subject(respiration);
    if tracked {
        status_flags |= vitals_flags::SUBJECT_TRACKED;
    }

    if is_valid(heart) {
        status_flags |= vitals_flags::HEART_VALID;
    }
    if is_valid(respiration) {
        status_flags |= vitals_flags::RESPIRATION_VALID;
    }
    if any_status(heart, respiration, VitalStatus::WarmingUp) {
        status_flags |= vitals_flags::WARMING_UP;
        header |= header_flags::DEGRADED;
    }
    if any_status(heart, respiration, VitalStatus::MotionContaminated) {
        status_flags |= vitals_flags::MOTION_CONTAMINATED;
        // Contaminated values are last-good, not fresh (§7).
        header |= header_flags::DEGRADED | header_flags::STALE;
    }
    if any_status(heart, respiration, VitalStatus::InvalidVendorValue) {
        status_flags |= vitals_flags::VENDOR_VALUE_INVALID;
    }

    let payload = Vitals {
        subject_id: subject_id(heart)
            .or_else(|| subject_id(respiration))
            .unwrap_or(SUBJECT_UNKNOWN),
        status_flags,
        heart_rate_x100: rate_x100(stabilized(heart)),
        respiration_rate_x100: rate_x100(stabilized(respiration)),
        heart_confidence: confidence_u8(heart.map(|h| h.confidence).unwrap_or(0.0)),
        respiration_confidence: confidence_u8(respiration.map(|r| r.confidence).unwrap_or(0.0)),
        activity_confidence: confidence_u8(activity.confidence),
        motion_energy_um2_s2: scale_u32(activity.motion_energy_mps2, 1_000_000.0),
        rms_speed_mm_s: scale_u16(activity.rms_radial_speed_mps, 1000.0),
        moving_fraction_q15: scale_u16(activity.moving_point_fraction.clamp(0.0, 1.0), 32767.0),
        range_bin: range_bin(heart)
            .or_else(|| range_bin(respiration))
            .unwrap_or(0),
        breathing_deviation_q8_8: breathing_deviation_q8_8(heart)
            .or_else(|| breathing_deviation_q8_8(respiration))
            .unwrap_or(0),
    };

    MappedVitals {
        payload,
        header_flags: header,
    }
}

/// Extract the feature vector the fatigue model consumes (`snf-fatigue`).
/// Missing rates default to `0.0`; the model is responsible for treating that as
/// "no reading" rather than a real bradycardia.
pub fn fatigue_features(snapshot: &IndicatorSnapshot) -> FatigueFeatures {
    FatigueFeatures {
        heart_rate_bpm: stabilized(snapshot.heart_rate.as_ref()).unwrap_or(0.0),
        breathing_rate_bpm: stabilized(snapshot.respiration_rate.as_ref()).unwrap_or(0.0),
        motion_energy: snapshot.activity.motion_energy_mps2,
    }
}

/// Map a fatigue verdict to a BLE Fatigue payload (`PROTOCOL.md` §8).
/// `model_revision` identifies the model that produced the verdict.
pub fn fatigue_telemetry(verdict: FatigueLevel, model_revision: u32) -> Fatigue {
    let mut status_flags = 0u16;
    if verdict.confidence > 0.0 {
        status_flags |= fatigue_flags::VALID;
    }
    Fatigue {
        level: verdict.level.min(100),
        confidence: confidence_u8(verdict.confidence),
        status_flags,
        model_revision,
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn subject_id(estimate: Option<&VitalRateEstimate>) -> Option<u16> {
    estimate.and_then(|e| e.subject_id)
}

fn has_subject(estimate: Option<&VitalRateEstimate>) -> bool {
    estimate.is_some_and(|e| e.status != VitalStatus::NoSubject)
}

fn stabilized(estimate: Option<&VitalRateEstimate>) -> Option<f32> {
    estimate.and_then(|e| e.stabilized_bpm)
}

fn range_bin(estimate: Option<&VitalRateEstimate>) -> Option<u16> {
    estimate.and_then(|e| e.range_bin)
}

/// Vendor breathing deviation as the §7 Q8.8 field (`value × 256`), saturated to
/// `i16`. A non-finite vendor value is treated as absent.
fn breathing_deviation_q8_8(estimate: Option<&VitalRateEstimate>) -> Option<i16> {
    estimate
        .and_then(|e| e.breathing_deviation)
        .filter(|value| value.is_finite())
        .map(|value| {
            (value * 256.0)
                .round()
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16
        })
}

fn is_valid(estimate: Option<&VitalRateEstimate>) -> bool {
    estimate.is_some_and(|e| e.status == VitalStatus::Valid && e.stabilized_bpm.is_some())
}

fn any_status(
    heart: Option<&VitalRateEstimate>,
    respiration: Option<&VitalRateEstimate>,
    status: VitalStatus,
) -> bool {
    heart.is_some_and(|e| e.status == status) || respiration.is_some_and(|e| e.status == status)
}

/// `bpm × 100` as `u16`, or [`RATE_UNAVAILABLE`] for a missing/implausible rate.
/// Clamped to `65534` so a real reading can never collide with the sentinel.
fn rate_x100(bpm: Option<f32>) -> u16 {
    match bpm {
        Some(value) if value.is_finite() && value > 0.0 => (value * 100.0)
            .round()
            .clamp(0.0, (RATE_UNAVAILABLE - 1) as f32)
            as u16,
        _ => RATE_UNAVAILABLE,
    }
}

/// A `0.0..=1.0` confidence as a `0..=100` percentage.
fn confidence_u8(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn scale_u32(value: f32, factor: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * factor).round().clamp(0.0, u32::MAX as f32) as u32
}

fn scale_u16(value: f32, factor: f32) -> u16 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * factor).round().clamp(0.0, u16::MAX as f32) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use snf_radar::{ActivityTrend, GrossActivity};

    fn estimate(bpm: Option<f32>, status: VitalStatus, confidence: f32) -> VitalRateEstimate {
        VitalRateEstimate {
            subject_id: Some(7),
            raw_bpm: bpm,
            stabilized_bpm: bpm,
            confidence,
            status,
            range_bin: Some(44),
            breathing_deviation: Some(0.5),
        }
    }

    fn snapshot(
        heart: Option<VitalRateEstimate>,
        respiration: Option<VitalRateEstimate>,
        activity: GrossActivity,
    ) -> IndicatorSnapshot {
        IndicatorSnapshot {
            frame_number: 1,
            activity,
            heart_rate: heart,
            respiration_rate: respiration,
        }
    }

    fn activity(points: usize, energy: f32) -> GrossActivity {
        GrossActivity {
            contributing_points: points,
            motion_energy_mps2: energy,
            rms_radial_speed_mps: energy.sqrt(),
            moving_point_fraction: 0.5,
            short_term_energy_mps2: energy,
            long_term_energy_mps2: energy,
            trend: ActivityTrend::Steady,
            confidence: 1.0,
        }
    }

    #[test]
    fn maps_valid_vitals_with_protocol_scaling() {
        let snap = snapshot(
            Some(estimate(Some(72.5), VitalStatus::Valid, 0.9)),
            Some(estimate(Some(15.0), VitalStatus::Valid, 0.8)),
            activity(10, 0.25),
        );
        let mapped = vitals(&snap);
        assert_eq!(mapped.payload.subject_id, 7);
        assert_eq!(mapped.payload.heart_rate_x100, 7250);
        assert_eq!(mapped.payload.respiration_rate_x100, 1500);
        assert_eq!(mapped.payload.heart_confidence, 90);
        assert_eq!(mapped.payload.respiration_confidence, 80);
        // 0.25 (m/s)^2 * 1e6, rms sqrt(0.25)=0.5 m/s -> 500 mm/s, fraction 0.5 -> Q15.
        assert_eq!(mapped.payload.motion_energy_um2_s2, 250_000);
        assert_eq!(mapped.payload.rms_speed_mm_s, 500);
        assert_eq!(mapped.payload.moving_fraction_q15, 16_384);
        // Subject-level vendor fields carried through from the estimate.
        assert_eq!(mapped.payload.range_bin, 44);
        assert_eq!(mapped.payload.breathing_deviation_q8_8, 128); // 0.5 × 256
        assert_eq!(
            mapped.payload.status_flags,
            vitals_flags::SUBJECT_TRACKED
                | vitals_flags::HEART_VALID
                | vitals_flags::RESPIRATION_VALID
        );
        assert_eq!(mapped.header_flags, 0);
    }

    #[test]
    fn motion_contamination_sets_stale_and_degraded() {
        let snap = snapshot(
            Some(estimate(Some(70.0), VitalStatus::MotionContaminated, 0.4)),
            None,
            activity(5, 1.0),
        );
        let mapped = vitals(&snap);
        // Last-good value still carried, but flagged.
        assert_eq!(mapped.payload.heart_rate_x100, 7000);
        assert!(mapped.payload.status_flags & vitals_flags::MOTION_CONTAMINATED != 0);
        assert_eq!(
            mapped.header_flags,
            header_flags::DEGRADED | header_flags::STALE
        );
    }

    #[test]
    fn unavailable_rate_uses_sentinel_not_zero() {
        let snap = snapshot(
            Some(estimate(None, VitalStatus::WarmingUp, 0.0)),
            None,
            activity(0, 0.0),
        );
        let mapped = vitals(&snap);
        assert_eq!(mapped.payload.heart_rate_x100, RATE_UNAVAILABLE);
        assert_eq!(mapped.payload.respiration_rate_x100, RATE_UNAVAILABLE);
        assert_eq!(mapped.payload.subject_id, 7); // still a tracked subject
        assert!(mapped.payload.status_flags & vitals_flags::WARMING_UP != 0);
        assert_eq!(mapped.header_flags, header_flags::DEGRADED);
    }

    #[test]
    fn no_subject_reports_unknown() {
        let snap = snapshot(None, None, activity(0, 0.0));
        let mapped = vitals(&snap);
        assert_eq!(mapped.payload.subject_id, SUBJECT_UNKNOWN);
        assert_eq!(mapped.payload.status_flags, 0);
        // No vendor reading backs the snapshot: both fields fall back to zero.
        assert_eq!(mapped.payload.range_bin, 0);
        assert_eq!(mapped.payload.breathing_deviation_q8_8, 0);
    }

    #[test]
    fn fatigue_verdict_maps_to_payload() {
        let f = fatigue_telemetry(
            FatigueLevel {
                level: 42,
                confidence: 0.75,
            },
            0xABCD,
        );
        assert_eq!(f.level, 42);
        assert_eq!(f.confidence, 75);
        assert_eq!(f.status_flags, fatigue_flags::VALID);
        assert_eq!(f.model_revision, 0xABCD);

        // Zero-confidence verdict is not marked valid.
        let none = fatigue_telemetry(
            FatigueLevel {
                level: 0,
                confidence: 0.0,
            },
            1,
        );
        assert_eq!(none.status_flags, 0);
    }

    #[test]
    fn features_default_missing_rates_to_zero() {
        let snap = snapshot(
            Some(estimate(Some(60.0), VitalStatus::Valid, 0.9)),
            None,
            activity(3, 0.1),
        );
        let features = fatigue_features(&snap);
        assert_eq!(features.heart_rate_bpm, 60.0);
        assert_eq!(features.breathing_rate_bpm, 0.0);
        assert_eq!(features.motion_energy, 0.1);
    }
}
