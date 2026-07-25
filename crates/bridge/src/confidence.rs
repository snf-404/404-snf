// SPDX-License-Identifier: Apache-2.0

//! How much of a verdict is allowed to reach the actuators.
//!
//! The fatigue model reports a level *and* how sure it is, and those are not
//! independent things to be used separately: the confidence is the normalized
//! entropy of the same distribution the level is the mean of (see
//! [`snf_fatigue::decode`]). A wide distribution means "somewhere between rested
//! and exhausted", and its mean — a comfortable middle number — is the least
//! trustworthy output the model can produce. Acting on it at full strength is
//! precisely wrong.
//!
//! So confidence is a **weight on the level**, not a separate alarm:
//!
//! ```text
//!   weight
//!     1.0 ┤                              ╭──────────────
//!         │                          ╭───╯
//!         │                      ╭───╯
//!     0.5 ┤                  ╭───╯
//!         │              ╭───╯
//!         │          ╭───╯
//!     0.0 ┼──────────╯
//!         └────┬─────────┬─────────────┬──────────────
//!            0.0       0.30          0.80        1.0
//!                            confidence
//! ```
//!
//! * **below [`ACTION_FLOOR`]** — the verdict is withheld entirely. The
//!   controller is not told anything, so its own `verdict_timeout` pins the
//!   sections to neutral and, if the silence continues, ends the episode. The
//!   BLE Fatigue record still goes out, carrying
//!   [`LOW_CONFIDENCE`](snf_ble::protocol::fatigue_flags::LOW_CONFIDENCE) so a
//!   client can show "not sure" rather than a level the device is quietly
//!   ignoring.
//! * **between [`ACTION_FLOOR`] and [`FULL_TRUST`]** — the level is scaled by a
//!   smooth logistic. Lower confidence is *more conservative*: the scaled level
//!   sits lower, which puts the inflation controller in a gentler mode (or none
//!   at all) and reduces the speed it approaches it at. Uncertainty therefore
//!   makes the mat quieter, never more active.
//! * **above [`FULL_TRUST`]** — the verdict passes through untouched.
//!
//! # Why a logistic rather than a straight line
//!
//! The weight multiplies the level, and the level drives
//! [`InflationController`](crate::inflation::InflationController), whose mode
//! thresholds are *step* functions of it. A linear ramp would drag the effective
//! level across those thresholds at a constant rate as confidence wandered,
//! producing mode changes driven by the model's certainty rather than by the
//! person. The logistic spends most of its travel in the middle of the band and
//! flattens at both ends, so confidence hovering near either boundary — which is
//! where it spends most of its time — does not keep nudging the level across a
//! threshold. The hysteresis in the controller handles the rest.

use snf_fatigue::FatigueLevel;

/// Below this confidence, nothing is actuated at all.
pub const ACTION_FLOOR: f32 = 0.30;

/// At or above this confidence, the verdict is used as-is.
pub const FULL_TRUST: f32 = 0.80;

/// Steepness of the logistic across the transition band. Chosen so the curve is
/// visibly S-shaped rather than near-linear (`k → 0`) or near-step (`k → ∞`);
/// at 8 the endpoints sit at about 2% and 98% of the raw logistic before
/// renormalization, which is a gentle enough correction not to distort it.
const SHARPNESS: f32 = 8.0;

/// What the control loop should do with a verdict.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trust {
    /// Too uncertain to act on. Do not feed the controller; publish the record
    /// with `LOW_CONFIDENCE` set.
    Withheld {
        /// The level the model reported, for telemetry and logs. Deliberately
        /// not usable as a control input — it is carried, not applied.
        reported_level: u8,
    },
    /// Act, on `level` — the reported level scaled by `weight`.
    Applied {
        /// What to hand [`InflationController::observe`](crate::inflation::InflationController::observe).
        level: u8,
        /// The scaling that produced it, in `0.0..=1.0`. For logging; the level
        /// already has it applied.
        weight: f32,
    },
}

impl Trust {
    /// The level to act on, or `None` when the verdict was withheld.
    pub fn level(self) -> Option<u8> {
        match self {
            Trust::Withheld { .. } => None,
            Trust::Applied { level, .. } => Some(level),
        }
    }
}

/// Whether a confidence is high enough to act on at all.
///
/// Shared with [`crate::map::fatigue_telemetry`], so the `LOW_CONFIDENCE` flag a
/// client sees and the decision the actuators make can never disagree.
pub fn actionable(confidence: f32) -> bool {
    confidence.is_finite() && confidence >= ACTION_FLOOR
}

/// The fraction of a verdict to act on, at a given confidence.
///
/// Zero below [`ACTION_FLOOR`], one above [`FULL_TRUST`], and a renormalized
/// logistic between — renormalized so it reaches exactly 0 and 1 at the
/// boundaries rather than the 2%/98% a raw logistic would leave, which would put
/// a small step at each end of the band.
pub fn weight(confidence: f32) -> f32 {
    if !confidence.is_finite() || confidence < ACTION_FLOOR {
        return 0.0;
    }
    if confidence >= FULL_TRUST {
        return 1.0;
    }

    let t = (confidence - ACTION_FLOOR) / (FULL_TRUST - ACTION_FLOOR);
    let logistic = |x: f32| 1.0 / (1.0 + (-x).exp());
    let low = logistic(-SHARPNESS / 2.0);
    let high = logistic(SHARPNESS / 2.0);
    ((logistic(SHARPNESS * (t - 0.5)) - low) / (high - low)).clamp(0.0, 1.0)
}

/// Apply the confidence policy to a verdict.
pub fn gate(verdict: FatigueLevel) -> Trust {
    if !actionable(verdict.confidence) {
        return Trust::Withheld {
            reported_level: verdict.level,
        };
    }
    let weight = weight(verdict.confidence);
    Trust::Applied {
        // Rounding down rather than to nearest: at the boundary between two
        // inflation modes the quieter one is the right answer.
        level: (f32::from(verdict.level) * weight).floor() as u8,
        weight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(level: u8, confidence: f32) -> FatigueLevel {
        FatigueLevel { level, confidence }
    }

    #[test]
    fn below_the_floor_nothing_is_actuated() {
        for confidence in [0.0, 0.1, 0.299] {
            assert_eq!(weight(confidence), 0.0);
            assert_eq!(
                gate(verdict(90, confidence)),
                Trust::Withheld { reported_level: 90 }
            );
            assert!(!actionable(confidence));
        }
    }

    #[test]
    fn above_full_trust_the_verdict_is_untouched() {
        for confidence in [0.8, 0.9, 1.0] {
            assert_eq!(weight(confidence), 1.0);
            assert_eq!(
                gate(verdict(73, confidence)),
                Trust::Applied {
                    level: 73,
                    weight: 1.0
                }
            );
        }
    }

    /// The transition has to actually reach its endpoints, or there is a step
    /// at each boundary — exactly what the smooth band exists to avoid.
    #[test]
    fn the_transition_is_continuous_at_both_boundaries() {
        assert!(weight(ACTION_FLOOR) < 1e-6);
        assert!(weight(ACTION_FLOOR + 1e-4) < 0.01);
        assert!(weight(FULL_TRUST - 1e-4) > 0.99);
        assert_eq!(weight(FULL_TRUST), 1.0);
    }

    #[test]
    fn the_transition_is_monotone() {
        let mut previous = -1.0;
        for step in 0..=1000 {
            let confidence = step as f32 / 1000.0;
            let w = weight(confidence);
            assert!(w >= previous, "weight dipped at {confidence}");
            assert!((0.0..=1.0).contains(&w));
            previous = w;
        }
    }

    /// The midpoint of the band should be the midpoint of the curve — that is
    /// what makes it a symmetric S rather than a biased one.
    #[test]
    fn the_curve_is_symmetric_about_the_band_midpoint() {
        let middle = (ACTION_FLOOR + FULL_TRUST) / 2.0;
        assert!((weight(middle) - 0.5).abs() < 1e-5);

        for offset in [0.05f32, 0.10, 0.20] {
            let below = weight(middle - offset);
            let above = weight(middle + offset);
            assert!(
                ((below + above) - 1.0).abs() < 1e-4,
                "asymmetric at ±{offset}: {below} + {above}"
            );
        }
    }

    /// The whole point: less certainty must mean a gentler command, never a
    /// stronger one.
    #[test]
    fn lower_confidence_is_never_more_aggressive() {
        let mut previous = 0u8;
        for step in 30..=100 {
            let confidence = step as f32 / 100.0;
            let level = gate(verdict(100, confidence)).level().unwrap();
            assert!(
                level >= previous,
                "level fell as confidence rose, at {confidence}"
            );
            previous = level;
        }
        assert_eq!(previous, 100);
    }

    /// Scaling is applied to the level, so a tired-but-uncertain reading lands
    /// in a gentler inflation mode rather than the same one at lower speed.
    #[test]
    fn mid_band_confidence_scales_the_level_down() {
        let scaled = gate(verdict(90, 0.55)).level().unwrap();
        assert!(
            (40..=50).contains(&scaled),
            "half trust in a level-90 verdict should read around 45, got {scaled}"
        );
    }

    /// A non-finite confidence is a model or transport fault. Both NaN *and*
    /// infinity fail closed — an infinite confidence is not maximal certainty,
    /// it is a broken number, and reading it as "trust completely" would invert
    /// the policy at exactly the wrong moment.
    #[test]
    fn non_finite_confidence_fails_closed() {
        for confidence in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(weight(confidence), 0.0, "weight({confidence})");
            assert!(!actionable(confidence));
            assert!(matches!(
                gate(verdict(80, confidence)),
                Trust::Withheld { .. }
            ));
        }
    }
}
