# SPDX-License-Identifier: Apache-2.0

"""The teacher: an analytic fatigue score, written down rather than learned.

Stage A has no labels, so the model cannot learn what fatigue *is*. What it can
do is learn a smooth, differentiable, ONNX-exportable approximation of a rule we
write down from the physiology — which buys three things a hand-coded `if` ladder
in Rust would not:

1. the ONNX I/O contract becomes real and exercised end to end, so swapping in a
   model trained on actual recordings is a file copy and a ``revision`` bump in
   ``Repose.toml`` rather than a Rust change;
2. the output is smooth. A threshold ladder produces cliffs, and a cliff in the
   fatigue level is a cliff in commanded duty — the control model downstream
   assumes ``F`` moves continuously enough for its derivative term to mean
   something;
3. **uncertainty comes out of the same function as the score.** The rule emits a
   distribution, not a number, and the student learns to reproduce its spread.
   That is what makes the runtime's confidence honest enough to gate actuation.

## What the rule believes

Sleep onset and accumulating fatigue show up in cardiorespiratory and motion
signals in a handful of well-documented ways. Each becomes one piece of
*evidence* in ``[-1, 1]``, positive meaning "more fatigued":

| Evidence          | Why                                                        |
| ----------------- | ---------------------------------------------------------- |
| respiratory       | The strongest single indicator available here. Breathing    |
| regularity        | becomes markedly *more regular* approaching sleep — the     |
|                   | variance collapses well before the rate itself moves much.  |
| respiratory rate  | Rate drifts down below the subject's own baseline.          |
| cardiac decel.    | Parasympathetic tone rises; heart rate falls below baseline.|
| heart-rate trend  | The direction matters on its own: a sagging rate is         |
|                   | evidence even before it has crossed the baseline.           |
| motion quiescence | Gross activity decays toward the noise floor.               |
| motion decay      | Again the direction, which leads the level.                 |
| duty fraction     | Time spent above the movement threshold collapses.          |
| time on task      | A saturating monotone prior. Weak, but never unavailable —  |
|                   | it is the one channel that cannot drop out.                 |

Deliberately **not** here: heart-rate variability. Real HRV needs beat-to-beat
intervals, and the TI vital-signs demo hands us a smoothed *rate*, at 2 Hz. The
``hr_std`` feature is mostly estimator noise and is weighted accordingly — it
informs the spread, not the score.

## Where the spread comes from

``sigma`` widens for exactly the reasons a person would hedge:

* a channel is missing entirely (``rr_valid``/``hr_valid`` clear) — and
  respiration costs more than heart rate, because it carries the most weight;
* the evidences **disagree** — near-zero motion but a heart rate running above
  baseline is not a tired person, it is an unexplained one;
* the session is young, so the baselines the deviations are measured against
  have not converged yet.

The runtime turns that spread into confidence, and confidence into how much of
the verdict is allowed to reach the actuators. So a rule that is honest about
not knowing produces a mat that stays still — which is the correct failure.
"""

from __future__ import annotations

import numpy as np

from .contract import BIN_CENTERS, IDX

# ── Evidence weights ─────────────────────────────────────────────────────────
# Relative, normalized to sum to 1 below. Respiration dominates because it is
# both the earliest and the least motion-contaminated of the available channels.
WEIGHTS: dict[str, float] = {
    "rr_regularity": 0.26,
    "rr_depression": 0.14,
    "hr_deceleration": 0.14,
    "hr_trend": 0.08,
    "motion_quiescence": 0.16,
    "motion_decay": 0.07,
    "motion_duty": 0.08,
    "time_on_task": 0.07,
}

# ── Spread model ─────────────────────────────────────────────────────────────
# Calibrated against what the runtime does with the result. With bin centres 25
# apart, a discretized Gaussian gives confidence ≈ 0.99 at sigma 7, ≈ 0.8 at 13,
# ≈ 0.5 at 23 and ≈ 0.3 at 34. The gates downstream sit at 0.3 and 0.8, so these
# constants are chosen to put each failure in the band it belongs in:
#
#   both channels, agreeing, warmed →  7  → act on the verdict in full
#   heart rate missing              → 18  → act, scaled down
#   respiration missing             → 25  → act, scaled well down
#   both missing                    → 36  → do not act, and say so over BLE
SIGMA_BASE = 7.0
SIGMA_NO_RR = 18.0  # respiration missing: the primary evidence is gone
SIGMA_NO_HR = 11.0  # heart rate missing
SIGMA_DISAGREEMENT = 16.0  # scaled by the spread across the three channels
DISAGREEMENT_FLOOR = 0.30  # below this the channels are merely noisy, not opposed
DISAGREEMENT_FULL = 0.95  # flat contradiction
SIGMA_WARMUP = 10.0  # scaled by how far the baselines still have to go
WARMUP_H = 1.0 / 6.0  # baselines are trusted after ~10 minutes
SIGMA_MAX = 60.0

#: Which evidences belong to which physical channel. Disagreement is measured
#: *between* these groups, never within one: two respiratory evidences pointing
#: different ways is one channel being equivocal, which the score already
#: averages out. Respiration saying "asleep" while motion says "moving" is a
#: genuine contradiction and is what should widen the distribution.
CHANNELS: dict[str, tuple[str, ...]] = {
    "respiratory": ("rr_regularity", "rr_depression"),
    "cardiac": ("hr_deceleration", "hr_trend"),
    "motion": ("motion_quiescence", "motion_decay", "motion_duty"),
}


def _smoothstep(x: np.ndarray, lo: float, hi: float) -> np.ndarray:
    """Hermite smoothstep from 0 at ``lo`` to 1 at ``hi``.

    Used everywhere instead of a hard threshold so the teacher — and therefore
    the student — has no cliffs to reproduce.
    """
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def evidences(x: np.ndarray) -> dict[str, np.ndarray]:
    """Each named piece of evidence, in ``[0, 1]``, for a batch of features.

    ``x`` is ``(N, N_FEATURES)`` in the order of :data:`contract.FEATURES`.
    """
    hr_valid = x[:, IDX["hr_valid"]]
    rr_valid = x[:, IDX["rr_valid"]]

    # Respiratory regularity: ~2.5 brpm of spread is ordinary quiet wakefulness,
    # ~0.4 is the metronomic breathing of sleep onset. Inverted, so low spread
    # reads as high fatigue.
    rr_regularity = 1.0 - _smoothstep(x[:, IDX["rr_std"]], 0.4, 2.5)

    # Rate depression: 3 brpm below one's own baseline is a lot; above baseline
    # is evidence of the opposite and saturates at zero.
    rr_depression = _smoothstep(-x[:, IDX["rr_baseline_delta"]], 0.0, 3.0)

    # Cardiac deceleration: 8 bpm below baseline is a strong parasympathetic
    # signal in a resting subject.
    hr_deceleration = _smoothstep(-x[:, IDX["hr_baseline_delta"]], 0.0, 8.0)

    # Direction of travel, which leads the level. bpm/min.
    hr_trend = _smoothstep(-x[:, IDX["hr_slope"]], 0.0, 1.5)

    # Motion. ln(1 + 1000 * e) puts ordinary fidgeting near 3-5 and stillness
    # near 0; the band below ~2.5 is where someone has stopped moving.
    motion_quiescence = 1.0 - _smoothstep(x[:, IDX["motion_log_energy"]], 0.5, 3.5)
    motion_decay = _smoothstep(-x[:, IDX["motion_slope"]], 0.0, 1.0)
    motion_duty = 1.0 - _smoothstep(x[:, IDX["motion_active_fraction"]], 0.02, 0.35)

    # A saturating prior on time awake. Never absent, never decisive.
    time_on_task = _smoothstep(x[:, IDX["time_on_task_h"]], 0.0, 4.0)

    # A channel that is not there contributes nothing rather than contributing
    # zero — the difference matters, because a zeroed feature would otherwise
    # read as "rate far below baseline", i.e. maximal fatigue. This is the
    # concrete reason the validity bits are in the feature vector at all.
    neutral = 0.5
    ev = {
        "rr_regularity": np.where(rr_valid > 0.5, rr_regularity, neutral),
        "rr_depression": np.where(rr_valid > 0.5, rr_depression, neutral),
        "hr_deceleration": np.where(hr_valid > 0.5, hr_deceleration, neutral),
        "hr_trend": np.where(hr_valid > 0.5, hr_trend, neutral),
        "motion_quiescence": motion_quiescence,
        "motion_decay": motion_decay,
        "motion_duty": motion_duty,
        "time_on_task": time_on_task,
    }
    return ev


def score_and_sigma(x: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The rule's fatigue score in ``0..100`` and its spread, per row."""
    ev = evidences(x)
    total_weight = sum(WEIGHTS.values())

    score = np.zeros(x.shape[0], dtype=np.float64)
    for name, value in ev.items():
        score += WEIGHTS[name] * value
    score = 100.0 * score / total_weight

    # Disagreement between *channels*, each channel first collapsed to a single
    # verdict so that internal equivocation does not read as contradiction. A
    # channel with no valid reading is excluded rather than counted as neutral —
    # its absence is already paid for by the SIGMA_NO_* terms, and counting it
    # at 0.5 would manufacture disagreement out of a missing wire.
    #
    # This is the only term that can widen the distribution with every channel
    # present and valid, which is what it is for: low motion with a heart rate
    # running *above* baseline is not a tired person, it is an unexplained one.
    verdicts = []
    presence = []
    valid = {
        "respiratory": x[:, IDX["rr_valid"]] > 0.5,
        "cardiac": x[:, IDX["hr_valid"]] > 0.5,
        "motion": np.ones(x.shape[0], dtype=bool),
    }
    for channel, names in CHANNELS.items():
        channel_weight = sum(WEIGHTS[name] for name in names)
        verdict = sum(WEIGHTS[name] * ev[name] for name in names) / channel_weight
        verdicts.append(verdict)
        presence.append(valid[channel].astype(np.float64))
    stacked = np.stack(verdicts, axis=1)
    present = np.stack(presence, axis=1)

    count = present.sum(axis=1, keepdims=True)
    mean = (stacked * present).sum(axis=1, keepdims=True) / count
    variance = (present * (stacked - mean) ** 2).sum(axis=1) / count[:, 0]
    # Two channels at opposite extremes of [0, 1] have a standard deviation of
    # 0.5, so that is full disagreement. Below DISAGREEMENT_FLOOR nothing counts:
    # three noisy estimates of the same latent state always differ somewhat, and
    # charging for that ordinary spread would leave the device permanently
    # unsure and therefore permanently still. Only opposition should widen.
    raw = np.clip(np.sqrt(variance) / 0.5, 0.0, 1.0)
    disagreement = _smoothstep(raw, DISAGREEMENT_FLOOR, DISAGREEMENT_FULL)

    warmup_deficit = 1.0 - _smoothstep(x[:, IDX["time_on_task_h"]], 0.0, WARMUP_H)

    sigma = (
        SIGMA_BASE
        + SIGMA_NO_RR * (1.0 - x[:, IDX["rr_valid"]])
        + SIGMA_NO_HR * (1.0 - x[:, IDX["hr_valid"]])
        + SIGMA_DISAGREEMENT * disagreement
        + SIGMA_WARMUP * warmup_deficit
    )
    return score, np.minimum(sigma, SIGMA_MAX)


def target_distribution(x: np.ndarray) -> np.ndarray:
    """Soft label over the ordinal bins: a discretized Gaussian ``N(score, sigma)``.

    Training against a distribution rather than a scalar is what teaches the
    student to be uncertain in the right places. The runtime's confidence is the
    normalized entropy of exactly this shape, so a wide teacher target becomes a
    low confidence becomes a mat that does not move.
    """
    score, sigma = score_and_sigma(x)
    centers = np.asarray(BIN_CENTERS, dtype=np.float64)[None, :]
    z = (centers - score[:, None]) / sigma[:, None]
    logits = -0.5 * z * z
    logits -= logits.max(axis=1, keepdims=True)
    p = np.exp(logits)
    return p / p.sum(axis=1, keepdims=True)


def level_and_confidence(p: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Decode a bin distribution exactly the way the Rust runtime does.

    Kept here so the training script can report the numbers the device will
    actually produce, rather than a proxy that happens to correlate.
    """
    centers = np.asarray(BIN_CENTERS, dtype=np.float64)[None, :]
    level = (p * centers).sum(axis=1)
    entropy = -(p * np.log(np.clip(p, 1e-12, None))).sum(axis=1)
    confidence = 1.0 - entropy / np.log(p.shape[1])
    return level, confidence
