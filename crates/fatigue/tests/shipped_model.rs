// SPDX-License-Identifier: Apache-2.0

//! End-to-end checks against the **actual model file that ships**.
//!
//! The unit tests in `src/lib.rs` cover the decode arithmetic on synthetic
//! logits. This covers the thing they cannot: that `network/out/fatigue.onnx`,
//! loaded through `ort`, fed a feature vector laid out by
//! [`FatigueFeatures::to_input`], produces the verdict the training run said it
//! would. Every one of those steps is a place the Python and Rust sides can
//! silently disagree — a permuted field, a stale export, a renamed tensor.
//!
//! Expected values come from running the same vectors through `onnxruntime` in
//! Python (see `network/README.md`). Tolerances are loose enough for
//! floating-point and execution-provider differences and tight enough that a
//! reordered feature would fail.
//!
//! Runs only with the `ort` feature:
//!
//! ```bash
//! cargo test -p snf-fatigue --features ort
//! ```

#![cfg(feature = "ort")]

use snf_fatigue::{FatigueFeatures, FatigueModel};

/// The tracked artifact, relative to this crate.
const MODEL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../network/out/fatigue.onnx"
);

fn model() -> FatigueModel {
    FatigueModel::load(MODEL).expect("network/out/fatigue.onnx is tracked; run `just net-train`")
}

/// Someone awake: rates at their own baseline, ordinary breathing irregularity,
/// moving around.
fn alert() -> FatigueFeatures {
    FatigueFeatures {
        hr_bpm: 72.0,
        hr_baseline_delta: 0.5,
        hr_slope: 0.1,
        hr_std: 1.8,
        rr_bpm: 16.0,
        rr_baseline_delta: 0.3,
        rr_slope: 0.05,
        rr_std: 2.6,
        motion_log_energy: 4.0,
        motion_slope: 0.1,
        motion_active_fraction: 0.6,
        time_on_task_h: 0.5,
        hr_valid: true,
        rr_valid: true,
    }
}

/// Sleep onset: breathing gone metronomic, heart decelerating below baseline,
/// motion decayed to nothing, hours in.
fn drowsy() -> FatigueFeatures {
    FatigueFeatures {
        hr_bpm: 58.0,
        hr_baseline_delta: -9.0,
        hr_slope: -1.6,
        hr_std: 1.0,
        rr_bpm: 11.0,
        rr_baseline_delta: -3.5,
        rr_slope: -0.5,
        rr_std: 0.35,
        motion_log_energy: 0.6,
        motion_slope: -0.8,
        motion_active_fraction: 0.01,
        time_on_task_h: 3.0,
        hr_valid: true,
        rr_valid: true,
    }
}

/// The radar has lost both rate channels. Motion is all that is left.
fn blind() -> FatigueFeatures {
    FatigueFeatures {
        motion_log_energy: 2.5,
        motion_active_fraction: 0.3,
        time_on_task_h: 1.0,
        hr_valid: false,
        rr_valid: false,
        ..FatigueFeatures::default()
    }
}

#[test]
fn the_shipped_model_loads_and_passes_its_interface_check() {
    let _ = model();
}

#[test]
fn an_alert_subject_reads_low_with_high_confidence() {
    let verdict = model().infer(&alert()).expect("inference");
    assert!(verdict.level <= 3, "expected ≈0, got {verdict:?}");
    assert!(verdict.confidence > 0.95, "{verdict:?}");
}

#[test]
fn a_drowsy_subject_reads_high_with_high_confidence() {
    let verdict = model().infer(&drowsy()).expect("inference");
    assert!(verdict.level >= 95, "expected ≈100, got {verdict:?}");
    assert!(verdict.confidence > 0.90, "{verdict:?}");
}

/// The safety property this whole design exists for.
///
/// With both rate channels gone the model still emits a *level* — around 43,
/// which on its own would put the inflation controller into `Nudge` and start
/// moving air under someone the radar cannot see. What stops that is the
/// confidence: near zero, far below the actuation floor, so the verdict is
/// withheld entirely. A model that reported a plausible level and a confident
/// tone here would be actively dangerous.
#[test]
fn a_blind_reading_is_middling_but_not_believed() {
    let verdict = model().infer(&blind()).expect("inference");
    assert!(
        (35..=50).contains(&verdict.level),
        "expected a middling level, got {verdict:?}"
    );
    assert!(
        verdict.confidence < 0.30,
        "a verdict with no rate channels must fall below the actuation floor, got {verdict:?}"
    );
}

/// Reordering a field in `FatigueFeatures` would still compile and still infer;
/// only a check against a known response catches it. Perturbing one feature at a
/// time and requiring the level to move the *expected direction* pins the layout
/// far more tightly than any single-vector assertion.
#[test]
fn each_feature_moves_the_verdict_the_way_the_rule_says() {
    let mut model = model();
    let baseline = model.infer(&alert()).expect("inference").level;

    // Respiratory regularity: the strongest single indicator. Collapsing the
    // spread alone should lift the level substantially.
    let mut regular = alert();
    regular.rr_std = 0.3;
    assert!(
        model.infer(&regular).expect("inference").level > baseline + 10,
        "collapsing respiratory variability should raise fatigue"
    );

    // Stillness.
    let mut still = alert();
    still.motion_log_energy = 0.4;
    still.motion_active_fraction = 0.0;
    assert!(
        model.infer(&still).expect("inference").level > baseline + 5,
        "going still should raise fatigue"
    );

    // Cardiac deceleration below the personal baseline.
    let mut slow = alert();
    slow.hr_baseline_delta = -10.0;
    assert!(
        model.infer(&slow).expect("inference").level > baseline,
        "a heart rate below baseline should raise fatigue"
    );

    // Time on task is a weak monotone prior, so it must move the level up but
    // never dominate a subject who is plainly awake and moving.
    let mut late = alert();
    late.time_on_task_h = 6.0;
    let late_level = model.infer(&late).expect("inference").level;
    assert!(
        late_level >= baseline,
        "time on task should not reduce fatigue"
    );
    assert!(
        late_level < 60,
        "time alone must not override an awake, moving subject: got {late_level}"
    );
}

/// Dropping a channel has to cost confidence — that is what the validity bits
/// are in the feature vector for.
#[test]
fn losing_a_channel_costs_confidence() {
    let mut model = model();
    let both = model.infer(&alert()).expect("inference").confidence;

    let mut no_hr = alert();
    no_hr.hr_valid = false;
    no_hr.hr_bpm = 0.0;
    no_hr.hr_baseline_delta = 0.0;
    no_hr.hr_slope = 0.0;
    no_hr.hr_std = 0.0;
    let without_hr = model.infer(&no_hr).expect("inference").confidence;

    assert!(
        without_hr < both,
        "losing heart rate should reduce confidence: {without_hr} vs {both}"
    );
    assert!(
        without_hr > 0.30,
        "one good channel should still clear the actuation floor, got {without_hr}"
    );
}

/// A missing model must be an ordinary error, not a panic — `crates/app`
/// degrades to telemetry-only on it.
#[test]
fn a_missing_model_is_an_error() {
    assert!(FatigueModel::load("/nonexistent/snf/fatigue.onnx").is_err());
}
