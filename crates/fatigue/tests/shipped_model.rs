// SPDX-License-Identifier: Apache-2.0

//! End-to-end checks against the ONNX artifact that ships.

#![cfg(feature = "ort")]

use snf_fatigue::{FatigueFeatures, FatigueModel};

const MODEL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ml/out/fatigue.onnx");

fn model() -> FatigueModel {
    FatigueModel::load(MODEL).expect("ml/out/fatigue.onnx is tracked; run `just ml-train`")
}

fn alert() -> FatigueFeatures {
    FatigueFeatures {
        motion_quietness: 0.2,
        moving_point_quietness: 0.4,
        hr_valid: true,
        rr_valid: true,
        baseline_confidence: 1.0,
        ..FatigueFeatures::default()
    }
}

fn drowsy() -> FatigueFeatures {
    FatigueFeatures {
        heart_slowdown: 1.0,
        respiration_slowdown: 1.0,
        motion_quietness: 0.9,
        moving_point_quietness: 0.9,
        recent_motion_drop: 0.8,
        cardiorespiratory_agreement: 1.0,
        hr_valid: true,
        rr_valid: true,
        baseline_confidence: 1.0,
    }
}

#[test]
fn shipped_model_loads() {
    let _ = model();
}

#[test]
fn alert_is_lower_than_drowsy() {
    let mut model = model();
    let alert = model.infer(&alert()).expect("alert inference");
    let drowsy = model.infer(&drowsy()).expect("drowsy inference");
    assert!(alert.level < 25, "{alert:?}");
    assert!(drowsy.level > 60, "{drowsy:?}");
    assert!(drowsy.level > alert.level + 40);
    assert_eq!(alert.confidence, 1.0);
    assert_eq!(drowsy.confidence, 1.0);
}

#[test]
fn missing_channels_cannot_actuate() {
    let features = FatigueFeatures {
        motion_quietness: 1.0,
        moving_point_quietness: 1.0,
        baseline_confidence: 1.0,
        hr_valid: false,
        rr_valid: false,
        ..FatigueFeatures::default()
    };
    let verdict = model().infer(&features).expect("blind inference");
    assert_eq!(verdict.confidence, 0.0);
}

#[test]
fn each_evidence_moves_score_up() {
    let mut model = model();
    let baseline = model.infer(&alert()).expect("baseline").level;
    for feature in [
        FatigueFeatures {
            heart_slowdown: 1.0,
            ..alert()
        },
        FatigueFeatures {
            respiration_slowdown: 1.0,
            ..alert()
        },
        FatigueFeatures {
            motion_quietness: 1.0,
            ..alert()
        },
        FatigueFeatures {
            moving_point_quietness: 1.0,
            ..alert()
        },
        FatigueFeatures {
            recent_motion_drop: 1.0,
            ..alert()
        },
    ] {
        assert!(model.infer(&feature).expect("inference").level > baseline);
    }
}

#[test]
fn missing_model_is_an_error() {
    assert!(FatigueModel::load("/nonexistent/snf/fatigue.onnx").is_err());
}
