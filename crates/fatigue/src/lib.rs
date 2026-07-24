// SPDX-License-Identifier: Apache-2.0

//! Fatigue-level recognition pipeline for 404-snf.
//!
//! Consumes features derived from `snf-radar` (vitals + motion) and produces a
//! [`FatigueLevel`] via an ONNX model. Real inference runs through the
//! framework's `consortium-ort` crate under the `ort` feature — the intended
//! landing-evaluation integration point. Without `ort`, [`FatigueModel::infer`]
//! returns a deterministic stub so the crate is host-checkable with no ONNX
//! Runtime present.
//!
//! Scaffold only: no model is loaded and no real inference is performed.

use serde::{Deserialize, Serialize};

/// Feature vector fed to the fatigue model, extracted (in Rust) from radar
/// frames. Placeholder shape.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FatigueFeatures {
    /// Breathing rate, breaths per minute.
    pub breathing_rate_bpm: f32,
    /// Heart rate, beats per minute.
    pub heart_rate_bpm: f32,
    /// Aggregate body-motion energy over the window.
    pub motion_energy: f32,
}

/// The pipeline's verdict.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FatigueLevel {
    /// Fatigue level, 0 (alert) .. 100 (severely fatigued).
    pub level: u8,
    /// Confidence, 0.0 .. 1.0.
    pub confidence: f32,
}

/// Errors from loading or running the model.
#[derive(Debug)]
pub enum FatigueError {
    /// The model file could not be loaded.
    Load(String),
    /// Inference failed.
    Inference(String),
}

impl core::fmt::Display for FatigueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FatigueError::Load(m) => write!(f, "fatigue model load error: {m}"),
            FatigueError::Inference(m) => write!(f, "fatigue inference error: {m}"),
        }
    }
}

impl std::error::Error for FatigueError {}

/// A loaded fatigue model.
///
/// Under the `ort` feature this owns a `consortium-ort` session; otherwise it is
/// an empty stub.
#[derive(Debug)]
pub struct FatigueModel {
    #[cfg(feature = "ort")]
    _session: (), // placeholder for the consortium-ort/ort session handle
}

impl FatigueModel {
    /// Load an ONNX model from `path`.
    ///
    /// Validates that the file exists and is readable so a missing model fails
    /// at start-up rather than silently producing verdicts. The `ort` feature
    /// path additionally builds a `consortium-ort` session from the bytes; the
    /// default build stops after the existence check (no ONNX Runtime present).
    pub fn load(path: &str) -> Result<Self, FatigueError> {
        std::fs::metadata(path).map_err(|e| FatigueError::Load(format!("{path}: {e}")))?;
        Ok(Self {
            #[cfg(feature = "ort")]
            _session: (),
        })
    }

    /// Run inference on one feature vector.
    ///
    /// Stub returns a fixed low-fatigue verdict; the real path runs the ONNX
    /// graph through `consortium-ort`.
    pub fn infer(&self, _features: &FatigueFeatures) -> Result<FatigueLevel, FatigueError> {
        Ok(FatigueLevel {
            level: 0,
            confidence: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_rejects_a_missing_model_file() {
        let err = FatigueModel::load("/nonexistent/snf/fatigue.onnx").unwrap_err();
        assert!(matches!(err, FatigueError::Load(_)));
    }

    #[test]
    fn load_accepts_an_existing_file() {
        // Any readable path stands in for a model in the stub build; this crate's
        // own manifest is guaranteed to exist at test time.
        let existing = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let model = FatigueModel::load(existing).expect("manifest exists");
        // Stub inference is defined and total.
        let verdict = model.infer(&FatigueFeatures::default()).unwrap();
        assert_eq!(verdict.level, 0);
    }
}
