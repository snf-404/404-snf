// SPDX-License-Identifier: Apache-2.0

//! Ultra-light fatigue inference for 404-snf.
//!
//! The ONNX graph is trained and exported by `ml/`. It consumes six bounded,
//! dimensionless features, applies embedded standardization and a seven-parameter
//! logistic-linear model, and returns one fatigue score in `0..100`.
//!
//! Confidence is deliberately not learned: it is derived here from channel
//! availability and baseline warm-up. A plausible score cannot make the device
//! act when the radar has lost its physiological signals.

use serde::{Deserialize, Serialize};

pub const FEATURE_COUNT: usize = 6;

#[cfg(feature = "ort")]
const INPUT_NAME: &str = "features";
#[cfg(feature = "ort")]
const OUTPUT_NAME: &str = "fatigue_score";

/// Features in exactly the order used by `ml/src/fatigue_lite/features.py`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FatigueFeatures {
    /// Positive when heart rate is below the subject's personal baseline.
    pub heart_slowdown: f32,
    /// Positive when respiration is below the subject's personal baseline.
    pub respiration_slowdown: f32,
    /// `1` at point-cloud stillness, `0` at or above 0.10 m/s RMS speed.
    pub motion_quietness: f32,
    /// One minus the fraction of moving radar points.
    pub moving_point_quietness: f32,
    /// Positive when short-term motion energy is below long-term energy.
    pub recent_motion_drop: f32,
    /// Product of heart and respiration slowdown evidence.
    pub cardiorespiratory_agreement: f32,
    /// Whether a trustworthy heart-rate window backs the features.
    pub hr_valid: bool,
    /// Whether a trustworthy respiration window backs the features.
    pub rr_valid: bool,
    /// Personal-baseline warm-up, `0..1` (fully ready after ten minutes).
    pub baseline_confidence: f32,
}

impl FatigueFeatures {
    pub fn to_input(&self) -> [f32; FEATURE_COUNT] {
        [
            self.heart_slowdown,
            self.respiration_slowdown,
            self.motion_quietness,
            self.moving_point_quietness,
            self.recent_motion_drop,
            self.cardiorespiratory_agreement,
        ]
    }

    /// Sensor confidence used by the downstream actuation gate.
    pub fn confidence(&self) -> f32 {
        let channel_confidence = match (self.hr_valid, self.rr_valid) {
            (true, true) => 1.0,
            (true, false) | (false, true) => 0.45,
            (false, false) => 0.0,
        };
        channel_confidence * self.baseline_confidence.clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FatigueLevel {
    /// Fatigue proxy, 0 (alert) .. 100 (severely fatigued).
    pub level: u8,
    /// Measurement confidence, derived from signal quality rather than learned.
    pub confidence: f32,
}

#[derive(Debug)]
pub enum FatigueError {
    Load(String),
    Inference(String),
}

impl core::fmt::Display for FatigueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Load(message) => write!(f, "fatigue model load error: {message}"),
            Self::Inference(message) => write!(f, "fatigue inference error: {message}"),
        }
    }
}

impl std::error::Error for FatigueError {}

pub struct FatigueModel {
    #[cfg(feature = "ort")]
    session: ort::session::Session,
}

impl core::fmt::Debug for FatigueModel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("FatigueModel")
    }
}

#[cfg(feature = "ort")]
impl FatigueModel {
    pub fn load(path: &str) -> Result<Self, FatigueError> {
        use ort::value::{Outlet, ValueType};

        fn open(path: &str) -> ort::Result<ort::session::Session> {
            use ort::session::{Session, builder::GraphOptimizationLevel};

            let mut builder = Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_intra_threads(1)?;
            builder.commit_from_file(path)
        }

        let session = open(path).map_err(|error| FatigueError::Load(format!("{path}: {error}")))?;
        let inputs = session.inputs();
        if inputs.len() != 1 || inputs[0].name() != INPUT_NAME {
            return Err(FatigueError::Load(format!(
                "{path}: expected one input named `{INPUT_NAME}`, found {:?}",
                inputs.iter().map(Outlet::name).collect::<Vec<_>>()
            )));
        }
        if let ValueType::Tensor { shape, .. } = inputs[0].dtype()
            && shape
                .last()
                .is_some_and(|&width| width >= 0 && width != FEATURE_COUNT as i64)
        {
            return Err(FatigueError::Load(format!(
                "{path}: model input width does not match {FEATURE_COUNT} features"
            )));
        }

        let outputs = session.outputs();
        if outputs.len() != 1 || outputs[0].name() != OUTPUT_NAME {
            return Err(FatigueError::Load(format!(
                "{path}: expected one output named `{OUTPUT_NAME}`, found {:?}",
                outputs.iter().map(Outlet::name).collect::<Vec<_>>()
            )));
        }
        if let ValueType::Tensor { shape, .. } = outputs[0].dtype()
            && shape.last().is_some_and(|&width| width >= 0 && width != 1)
        {
            return Err(FatigueError::Load(format!(
                "{path}: `{OUTPUT_NAME}` must contain one score"
            )));
        }
        Ok(Self { session })
    }

    pub fn infer(&mut self, features: &FatigueFeatures) -> Result<FatigueLevel, FatigueError> {
        use ort::value::Tensor;

        let input = features.to_input();
        if input.iter().any(|value| !value.is_finite()) {
            return Err(FatigueError::Inference(
                "non-finite input feature".to_string(),
            ));
        }
        let tensor = Tensor::from_array((vec![1_i64, FEATURE_COUNT as i64], input.to_vec()))
            .map_err(|error| FatigueError::Inference(format!("input tensor: {error}")))?;
        let outputs = self
            .session
            .run(ort::inputs![INPUT_NAME => tensor])
            .map_err(|error| FatigueError::Inference(error.to_string()))?;
        let (shape, data) = outputs[OUTPUT_NAME]
            .try_extract_tensor::<f32>()
            .map_err(|error| FatigueError::Inference(format!("output tensor: {error}")))?;
        if data.len() != 1 {
            return Err(FatigueError::Inference(format!(
                "expected one score, got {} (shape {shape:?})",
                data.len()
            )));
        }
        let score = data[0];
        if !score.is_finite() {
            return Err(FatigueError::Inference(
                "model emitted a non-finite score".to_string(),
            ));
        }
        Ok(FatigueLevel {
            level: score.clamp(0.0, 100.0).round() as u8,
            confidence: features.confidence(),
        })
    }
}

#[cfg(not(feature = "ort"))]
impl FatigueModel {
    pub fn load(path: &str) -> Result<Self, FatigueError> {
        std::fs::metadata(path).map_err(|error| FatigueError::Load(format!("{path}: {error}")))?;
        Ok(Self {})
    }

    pub fn infer(&mut self, _features: &FatigueFeatures) -> Result<FatigueLevel, FatigueError> {
        Ok(FatigueLevel::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_order_matches_python_contract() {
        let features = FatigueFeatures {
            heart_slowdown: 1.0,
            respiration_slowdown: 2.0,
            motion_quietness: 3.0,
            moving_point_quietness: 4.0,
            recent_motion_drop: 5.0,
            cardiorespiratory_agreement: 6.0,
            ..FatigueFeatures::default()
        };
        assert_eq!(features.to_input(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn confidence_fails_safe_on_missing_channels_and_warmup() {
        let ready = FatigueFeatures {
            hr_valid: true,
            rr_valid: true,
            baseline_confidence: 1.0,
            ..FatigueFeatures::default()
        };
        assert_eq!(ready.confidence(), 1.0);
        assert_eq!(
            FatigueFeatures {
                rr_valid: false,
                ..ready
            }
            .confidence(),
            0.45
        );
        assert_eq!(
            FatigueFeatures {
                hr_valid: false,
                rr_valid: false,
                ..ready
            }
            .confidence(),
            0.0
        );
        assert_eq!(
            FatigueFeatures {
                baseline_confidence: 0.2,
                ..ready
            }
            .confidence(),
            0.2
        );
    }
}
