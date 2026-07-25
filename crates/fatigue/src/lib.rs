// SPDX-License-Identifier: Apache-2.0

//! Fatigue-level recognition for 404-snf.
//!
//! Consumes the windowed feature vector [`FatigueFeatures`] — built by
//! `snf_bridge::features` from radar indicators — and produces a
//! [`FatigueLevel`] through a small ONNX graph.
//!
//! # Why the model emits bins rather than a number
//!
//! The graph's output is [`BIN_COUNT`] logits over ordinal fatigue bins, not a
//! single regressed level. That is not a modelling flourish; the confidence is
//! load-bearing. `snf_bridge::confidence` decides from it whether the
//! pneumatics may move at all, so a point estimate with a separately-predicted
//! "confidence" head — a number the model is free to make up — would be exactly
//! the wrong shape. Taking the softmax expectation for the level and the
//! normalized entropy for the confidence means the two come out of the same
//! distribution and cannot disagree: a model that spreads its mass reports both
//! a middling level *and* low confidence, which is the honest answer.
//!
//! See `network/` for the training project that produces `fatigue.onnx`.
//!
//! # Feature gate
//!
//! Real inference is behind the `ort` feature, which pulls in ONNX Runtime.
//! Without it [`FatigueModel::infer`] returns a deterministic zero-confidence
//! stub, so this crate stays checkable on a macOS dev host with no ONNX Runtime
//! and no aarch64-linux binary to download. `crates/app` enables `ort`.
//! Note that a zero-confidence stub verdict is below the action threshold, so a
//! stub build publishes telemetry and never actuates — which is the right
//! behaviour for a build that cannot actually see anything.

use serde::{Deserialize, Serialize};

/// Number of features the ONNX graph consumes, in the order
/// [`FatigueFeatures::to_input`] writes them.
///
/// This must match `network/snf_net/contract.py`. Both sides are asserted
/// against the loaded graph at start-up ([`FatigueModel::load`]), so a mismatch
/// is a start-up error rather than silently-wrong verdicts.
pub const FEATURE_COUNT: usize = 14;

/// Number of ordinal bins the graph emits.
pub const BIN_COUNT: usize = 5;

/// Fatigue level each bin sits at. The decoded level is the expectation of this
/// under the output distribution.
pub const BIN_CENTERS: [f32; BIN_COUNT] = [0.0, 25.0, 50.0, 75.0, 100.0];

/// Graph input tensor name, fixed by `network/export.py`.
#[cfg(feature = "ort")]
const INPUT_NAME: &str = "features";
/// Graph output tensor name, fixed by `network/export.py`.
#[cfg(feature = "ort")]
const OUTPUT_NAME: &str = "logits";

/// Windowed features fed to the fatigue model.
///
/// Physical units throughout — normalization is a layer *inside* the ONNX
/// graph, so retraining with different statistics needs no change here. Rates
/// are accompanied by explicit validity flags rather than being sentinel-coded:
/// a missing heart rate is `hr_bpm = 0.0` **and** `hr_valid = false`, and the
/// model is trained to read the flag. Without it a dropout would look like
/// profound bradycardia, which reads as maximal fatigue — the failure mode that
/// inflates a mat under someone the radar has simply lost track of.
///
/// Built by `snf_bridge::features::FeatureExtractor`; see that module for how
/// each field is derived and over what window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FatigueFeatures {
    /// Heart rate at the end of the window, bpm. `0.0` when unavailable.
    pub hr_bpm: f32,
    /// Heart rate minus this subject's running baseline, bpm.
    pub hr_baseline_delta: f32,
    /// Least-squares heart-rate slope across the window, bpm per minute.
    pub hr_slope: f32,
    /// Standard deviation of heart rate across the window, bpm.
    pub hr_std: f32,

    /// Respiration rate at the end of the window, breaths/min. `0.0` when
    /// unavailable.
    pub rr_bpm: f32,
    /// Respiration rate minus baseline, breaths/min.
    pub rr_baseline_delta: f32,
    /// Respiration slope, breaths/min per minute.
    pub rr_slope: f32,
    /// Standard deviation of respiration rate across the window. The single
    /// most informative feature here: breathing becomes markedly *more regular*
    /// approaching sleep, well before the rate itself moves.
    pub rr_std: f32,

    /// `ln(1 + 1000 · motion_energy_mps2)` at the end of the window.
    pub motion_log_energy: f32,
    /// Slope of the above, per minute.
    pub motion_slope: f32,
    /// Fraction of the window spent above the movement threshold.
    pub motion_active_fraction: f32,

    /// Hours since the session started. Weak, but it is the one channel that
    /// cannot drop out.
    pub time_on_task_h: f32,

    /// Whether a heart-rate reading backs this window.
    pub hr_valid: bool,
    /// Whether a respiration reading backs this window.
    pub rr_valid: bool,
}

impl FatigueFeatures {
    /// Flatten into the graph's input order. Must match `contract.FEATURES`.
    pub fn to_input(&self) -> [f32; FEATURE_COUNT] {
        [
            self.hr_bpm,
            self.hr_baseline_delta,
            self.hr_slope,
            self.hr_std,
            self.rr_bpm,
            self.rr_baseline_delta,
            self.rr_slope,
            self.rr_std,
            self.motion_log_energy,
            self.motion_slope,
            self.motion_active_fraction,
            self.time_on_task_h,
            f32::from(u8::from(self.hr_valid)),
            f32::from(u8::from(self.rr_valid)),
        ]
    }
}

/// The pipeline's verdict.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FatigueLevel {
    /// Fatigue level, 0 (alert) .. 100 (severely fatigued).
    pub level: u8,
    /// Confidence, 0.0 .. 1.0. Derived from the output distribution's entropy,
    /// so it is a genuine measure of how concentrated the model's belief is
    /// rather than a separately-predicted number.
    pub confidence: f32,
}

/// Decode raw graph output into a verdict.
///
/// `level` is the softmax expectation over [`BIN_CENTERS`]; `confidence` is
/// `1 - H(p)/ln(BIN_COUNT)`, so a one-hot distribution gives 1.0 and a uniform
/// one gives 0.0.
///
/// Public and separately tested because it is the half of inference that has no
/// ONNX Runtime in it — the arithmetic that turns numbers into a decision about
/// moving an actuator is worth checking on any host, not only where the model
/// runs.
pub fn decode(logits: &[f32; BIN_COUNT]) -> FatigueLevel {
    // Shift by the maximum before exponentiating: the graph is unbounded, and a
    // large logit would otherwise overflow to inf and poison the whole vector.
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !max.is_finite() {
        return FatigueLevel::default();
    }

    let mut probabilities = [0.0f32; BIN_COUNT];
    let mut total = 0.0f32;
    for (p, &logit) in probabilities.iter_mut().zip(logits.iter()) {
        *p = (logit - max).exp();
        total += *p;
    }
    // Finiteness first, so the comparison that follows is on an ordered value —
    // a NaN total would otherwise slip through `total <= 0.0`.
    if !total.is_finite() || total <= 0.0 {
        return FatigueLevel::default();
    }

    let mut level = 0.0f32;
    let mut entropy = 0.0f32;
    for (p, center) in probabilities.iter().zip(BIN_CENTERS.iter()) {
        let p = p / total;
        level += p * center;
        if p > 0.0 {
            entropy -= p * p.ln();
        }
    }

    let confidence = 1.0 - entropy / (BIN_COUNT as f32).ln();
    FatigueLevel {
        level: level.clamp(0.0, 100.0).round() as u8,
        confidence: confidence.clamp(0.0, 1.0),
    }
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
    /// Load an ONNX model from `path` and check its interface.
    ///
    /// The shape check is not ceremony. The features and the weights are
    /// separate artifacts that travel separately — a `Repose.toml` pointing at
    /// last week's model is an ordinary mistake — and a graph with the wrong
    /// input width would otherwise fail deep inside ORT with an error that says
    /// nothing about which side is stale.
    pub fn load(path: &str) -> Result<Self, FatigueError> {
        use ort::value::{Outlet, ValueType};

        // Split out so the builder chain can use `?` throughout. ORT's builder
        // methods return `BuilderResult` — `Result<SessionBuilder,
        // Error<SessionBuilder>>`, an error type that carries the builder back
        // for recovery — which does not chain with `and_then` against the plain
        // `Error` that `commit_from_file` returns. `?` coerces between them;
        // combinators do not.
        fn open(path: &str) -> ort::Result<ort::session::Session> {
            use ort::session::{Session, builder::GraphOptimizationLevel};

            let mut builder = Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                // One thread: the graph is ~1.7k parameters at 2 Hz, so a
                // thread pool would cost more in scheduling than the arithmetic
                // it splits, and this shares four cores with the radar reader
                // and BlueZ.
                .with_intra_threads(1)?;
            builder.commit_from_file(path)
        }

        let session = open(path).map_err(|e| FatigueError::Load(format!("{path}: {e}")))?;

        let inputs = session.inputs();
        if inputs.len() != 1 || inputs[0].name() != INPUT_NAME {
            return Err(FatigueError::Load(format!(
                "{path}: expected a single input named `{INPUT_NAME}`, found {:?}",
                inputs.iter().map(Outlet::name).collect::<Vec<_>>()
            )));
        }
        // The feature vector and the weights are separate artifacts that travel
        // separately — a `Repose.toml` pointing at last week's model is an
        // ordinary mistake. Catch a width mismatch here, where the message can
        // name both numbers, rather than letting ORT reject the tensor at the
        // first inference with an error that says nothing about which side is
        // stale. A dynamic axis is reported as -1 and is left alone.
        if let ValueType::Tensor { shape, .. } = inputs[0].dtype() {
            match shape.last() {
                Some(&width) if width >= 0 && width != FEATURE_COUNT as i64 => {
                    return Err(FatigueError::Load(format!(
                        "{path}: model takes {width} features, this build produces \
                         {FEATURE_COUNT}; the model and `crates/bridge/src/features.rs` \
                         are out of step"
                    )));
                }
                _ => {}
            }
        }

        let outputs = session.outputs();
        if !outputs.iter().any(|o| o.name() == OUTPUT_NAME) {
            return Err(FatigueError::Load(format!(
                "{path}: no output named `{OUTPUT_NAME}`, found {:?}",
                outputs.iter().map(Outlet::name).collect::<Vec<_>>()
            )));
        }

        Ok(Self { session })
    }

    /// Run inference on one feature vector.
    ///
    /// Takes `&mut self` because ORT's `Session::run` does; the session carries
    /// per-run state and is not shared across threads here.
    pub fn infer(&mut self, features: &FatigueFeatures) -> Result<FatigueLevel, FatigueError> {
        use ort::value::Tensor;

        let input = features.to_input();
        let tensor = Tensor::from_array((vec![1_i64, FEATURE_COUNT as i64], input.to_vec()))
            .map_err(|e| FatigueError::Inference(format!("input tensor: {e}")))?;

        let outputs = self
            .session
            .run(ort::inputs![INPUT_NAME => tensor])
            .map_err(|e| FatigueError::Inference(e.to_string()))?;

        let (shape, data) = outputs[OUTPUT_NAME]
            .try_extract_tensor::<f32>()
            .map_err(|e| FatigueError::Inference(format!("output tensor: {e}")))?;

        let logits: [f32; BIN_COUNT] = data.try_into().map_err(|_| {
            FatigueError::Inference(format!(
                "expected {BIN_COUNT} logits, got {} (shape {shape:?})",
                data.len()
            ))
        })?;
        Ok(decode(&logits))
    }
}

#[cfg(not(feature = "ort"))]
impl FatigueModel {
    /// Validate that a model file exists, so a missing one fails at start-up
    /// rather than silently producing verdicts.
    pub fn load(path: &str) -> Result<Self, FatigueError> {
        std::fs::metadata(path).map_err(|e| FatigueError::Load(format!("{path}: {e}")))?;
        Ok(Self {})
    }

    /// Stub inference: a zero-confidence verdict, which
    /// `snf_bridge::confidence` will refuse to act on. Enable the `ort` feature
    /// for the real graph.
    pub fn infer(&mut self, _features: &FatigueFeatures) -> Result<FatigueLevel, FatigueError> {
        Ok(FatigueLevel::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The feature vector's order is a contract shared with `network/`; pin it
    /// so a field reordering during a refactor cannot silently permute what the
    /// model sees.
    #[test]
    fn input_order_matches_the_training_contract() {
        let features = FatigueFeatures {
            hr_bpm: 1.0,
            hr_baseline_delta: 2.0,
            hr_slope: 3.0,
            hr_std: 4.0,
            rr_bpm: 5.0,
            rr_baseline_delta: 6.0,
            rr_slope: 7.0,
            rr_std: 8.0,
            motion_log_energy: 9.0,
            motion_slope: 10.0,
            motion_active_fraction: 11.0,
            time_on_task_h: 12.0,
            hr_valid: true,
            rr_valid: false,
        };
        assert_eq!(
            features.to_input(),
            [
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 1.0, 0.0
            ]
        );
    }

    #[test]
    fn a_uniform_distribution_has_no_confidence() {
        let verdict = decode(&[0.0; BIN_COUNT]);
        assert_eq!(verdict.level, 50);
        assert!(verdict.confidence < 1e-6, "{verdict:?}");
    }

    #[test]
    fn a_peaked_distribution_is_confident() {
        let verdict = decode(&[0.0, 0.0, 0.0, 30.0, 0.0]);
        assert_eq!(verdict.level, 75);
        assert!(verdict.confidence > 0.99, "{verdict:?}");
    }

    /// The level is an expectation, so mass split between two bins lands
    /// between them rather than snapping to one.
    #[test]
    fn split_mass_interpolates_between_bins() {
        let verdict = decode(&[0.0, 0.0, 10.0, 10.0, 0.0]);
        assert_eq!(verdict.level, 62); // midway between 50 and 75, rounded
        assert!(
            (0.3..0.8).contains(&verdict.confidence),
            "two-of-five bins should land mid-band, got {verdict:?}"
        );
    }

    /// The graph is unbounded and runs on inputs derived from a radar that can
    /// hand over anything; `exp` of a large logit must not become `inf`.
    #[test]
    fn extreme_logits_do_not_overflow() {
        let verdict = decode(&[0.0, 0.0, 1.0e30, 0.0, 0.0]);
        assert_eq!(verdict.level, 50);
        assert!(verdict.confidence.is_finite());

        assert_eq!(decode(&[f32::NAN; BIN_COUNT]), FatigueLevel::default());
        assert_eq!(
            decode(&[f32::NEG_INFINITY; BIN_COUNT]),
            FatigueLevel::default()
        );
    }

    #[test]
    fn confidence_is_bounded() {
        for logits in [
            [0.0, 1.0, 2.0, 1.0, 0.0],
            [-50.0, 0.0, 50.0, 0.0, -50.0],
            [1.0, 1.0, 1.0, 1.0, 1.0],
        ] {
            let verdict = decode(&logits);
            assert!((0.0..=1.0).contains(&verdict.confidence), "{verdict:?}");
            assert!(verdict.level <= 100);
        }
    }
}
