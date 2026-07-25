# SPDX-License-Identifier: Apache-2.0

"""The ONNX interface contract, in one place.

Both sides of the boundary have to agree on this exactly: the Rust extractor in
``crates/bridge/src/features.rs`` fills the input vector in this order, and
``crates/fatigue/src/lib.rs`` reads the output logits with this bin layout. If
you change anything here, change it there in the same commit.
"""

from __future__ import annotations

# ── Input ────────────────────────────────────────────────────────────────────

#: Feature names, in the order the model consumes them. Physical units
#: throughout — normalization is a layer *inside* the graph (see
#: ``model.Normalize``), so the Rust side never carries a copy of the
#: statistics and can never drift out of sync with the weights.
FEATURES: tuple[str, ...] = (
    "hr_bpm",  # 0  heart rate, bpm; 0.0 when unavailable
    "hr_baseline_delta",  # 1  hr minus this subject's running baseline, bpm
    "hr_slope",  # 2  bpm per minute across the window
    "hr_std",  # 3  standard deviation of hr across the window, bpm
    "rr_bpm",  # 4  respiration rate, breaths/min; 0.0 when unavailable
    "rr_baseline_delta",  # 5  rr minus baseline, breaths/min
    "rr_slope",  # 6  breaths/min per minute
    "rr_std",  # 7  standard deviation of rr across the window
    "motion_log_energy",  # 8  ln(1 + 1000 * motion_energy_mps2)
    "motion_slope",  # 9  d(motion_log_energy) per minute
    "motion_active_fraction",  # 10 fraction of window above the moving threshold
    "time_on_task_h",  # 11 hours since the session started
    "hr_valid",  # 12 1.0 when a heart-rate reading backs the window
    "rr_valid",  # 13 1.0 when a respiration reading backs the window
)

N_FEATURES = len(FEATURES)

#: Index by name, so the rule and the simulator never hard-code positions.
IDX = {name: i for i, name in enumerate(FEATURES)}

# ── Output ───────────────────────────────────────────────────────────────────

#: Ordinal bins, KSS-flavoured. The head emits one logit per bin; the runtime
#: takes the softmax expectation for the level and the normalized entropy for
#: the confidence. Regressing the level directly would give a point estimate
#: with no honest uncertainty attached — and confidence is load-bearing here,
#: since it gates whether the mat is allowed to move at all.
BIN_CENTERS: tuple[float, ...] = (0.0, 25.0, 50.0, 75.0, 100.0)

N_BINS = len(BIN_CENTERS)

#: Names for logs and plots only.
BIN_NAMES: tuple[str, ...] = (
    "alert",
    "slightly-tired",
    "tired",
    "very-tired",
    "fighting-sleep",
)

# ── Windowing ────────────────────────────────────────────────────────────────

#: Verdicts arrive at the vitals rate, which defaults to 2 Hz
#: (``StreamState::default`` in ``crates/bridge/src/control.rs``).
SAMPLE_HZ = 2.0

#: Short window the slope/std features are computed over, in seconds. Long
#: enough for a respiration-rate estimate to show its variability, short enough
#: that the verdict still tracks a person settling.
WINDOW_S = 90.0

#: Time constant of the per-subject baseline EWMA, in seconds. The baseline is
#: what "deviation" is measured against, so it has to move far slower than the
#: window: minutes-long drift is the signal, not the noise.
BASELINE_TAU_S = 900.0

#: Motion energy below this (m/s²) counts as quiescent for
#: ``motion_active_fraction``. Matches the spirit of
#: ``IndicatorConfig::moving_velocity_threshold_mps`` on the radar side.
MOTION_ACTIVE_THRESHOLD = 0.02
