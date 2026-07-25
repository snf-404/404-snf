# SPDX-License-Identifier: Apache-2.0

"""Synthetic sessions, and the windowing that turns them into feature vectors.

The student is fit to the teacher, not to the truth, so this generator's job is
**coverage of the realistic manifold** — not correctness. Sampling the 14-D
feature box uniformly would spend most of the model's capacity on combinations
that cannot physically occur (a 40 bpm heart rate with a 4-brpm-per-minute
respiration slope), and leave it thin exactly where the device operates.

So instead: simulate a latent fatigue trajectory, generate plausible channels
conditioned on it, and run the *same windowing arithmetic the Rust extractor
uses* over the result. That gets the joint distribution roughly right, and it
gives a second benefit — the windowing here and the windowing in
``crates/bridge/src/features.rs`` can be checked against each other on the same
inputs.

Nothing here claims to be a physiological model. It is a sampler with the right
correlations and the right dropouts.
"""

from __future__ import annotations

import numpy as np

from .contract import (
    BASELINE_TAU_S,
    MOTION_ACTIVE_THRESHOLD,
    N_FEATURES,
    SAMPLE_HZ,
    WINDOW_S,
    IDX,
)


class SessionParams:
    """Per-subject constants, redrawn for every simulated session."""

    def __init__(self, rng: np.random.Generator):
        # Resting rates vary a lot between people; the model sees deviations
        # from a running baseline precisely so it does not have to care.
        self.hr_rest = rng.uniform(52.0, 82.0)
        self.rr_rest = rng.uniform(11.0, 18.0)
        # How far each channel travels between fully alert and fighting sleep.
        self.hr_span = rng.uniform(4.0, 14.0)
        self.rr_span = rng.uniform(1.5, 5.0)
        # Awake breathing irregularity, and how much of it survives sleep onset.
        self.rr_spread_awake = rng.uniform(1.6, 3.4)
        self.rr_spread_drowsy = rng.uniform(0.2, 0.7)
        # Estimator noise on the vendor rate outputs.
        self.hr_noise = rng.uniform(0.6, 2.2)
        self.rr_noise = rng.uniform(0.15, 0.6)
        # Dropout: how often the radar loses each channel, and for how long.
        self.hr_dropout = rng.uniform(0.0, 0.35)
        self.rr_dropout = rng.uniform(0.0, 0.30)
        self.motion_scale = rng.uniform(0.6, 1.8)


def latent_trajectory(n: int, rng: np.random.Generator) -> np.ndarray:
    """A fatigue trajectory in ``[0, 1]``, sampled at :data:`SAMPLE_HZ`.

    Mostly monotone — fatigue accumulates — but with arousals that knock it back
    down, because those are the transitions the device most needs to get right
    and a purely monotone ramp would never show them.
    """
    t = np.arange(n) / SAMPLE_HZ

    start = rng.uniform(0.0, 0.45)
    end = np.clip(start + rng.uniform(-0.1, 0.95), 0.0, 1.0)
    # A saturating rise, sometimes closer to linear, sometimes late-breaking.
    shape = rng.uniform(0.6, 2.5)
    ramp = start + (end - start) * (t / max(t[-1], 1.0)) ** shape

    # Slow wander, so the trajectory is not a clean curve the model can memorize.
    wander = np.cumsum(rng.normal(0.0, 0.012, size=n))
    wander -= wander.mean()

    trajectory = ramp + wander

    # Arousals: a few sharp drops with exponential recovery.
    for _ in range(rng.integers(0, 4)):
        onset = rng.integers(0, n)
        depth = rng.uniform(0.15, 0.55)
        tau = rng.uniform(30.0, 240.0) * SAMPLE_HZ
        decay = np.exp(-(np.arange(n) - onset) / tau)
        decay[: int(onset)] = 0.0
        trajectory -= depth * decay

    return np.clip(trajectory, 0.0, 1.0)


def simulate(n: int, rng: np.random.Generator) -> dict[str, np.ndarray]:
    """One session's raw channels, as the indicator layer would hand them over."""
    params = SessionParams(rng)
    fatigue = latent_trajectory(n, rng)

    # Heart rate: drifts down with fatigue, plus estimator noise.
    hr = params.hr_rest - params.hr_span * fatigue
    hr += rng.normal(0.0, params.hr_noise, size=n)

    # Respiration: drifts down, and — the part that actually carries the signal —
    # its sample-to-sample spread collapses as sleep approaches.
    rr_spread = (
        params.rr_spread_awake
        + (params.rr_spread_drowsy - params.rr_spread_awake) * fatigue
    )
    rr = params.rr_rest - params.rr_span * fatigue
    rr += rng.normal(0.0, 1.0, size=n) * rr_spread
    rr += rng.normal(0.0, params.rr_noise, size=n)

    # Motion energy, m²/s². Log-normal-ish, decaying with fatigue, with
    # occasional postural shifts that survive into stillness.
    base = np.exp(rng.normal(0.0, 0.7, size=n)) * params.motion_scale
    motion = base * 0.02 * (1.0 - 0.93 * fatigue)
    shifts = rng.random(n) < (0.004 * (1.0 - 0.5 * fatigue))
    motion[shifts] *= rng.uniform(8.0, 40.0, size=shifts.sum())
    motion = np.clip(motion, 0.0, None)

    # Dropouts, as runs rather than independent samples — a lost track stays
    # lost for a while, which is what makes the validity bits worth having.
    hr_valid = _dropout_mask(n, params.hr_dropout, rng)
    rr_valid = _dropout_mask(n, params.rr_dropout, rng)

    # Some sessions start mid-way through someone's day.
    t0_h = rng.uniform(0.0, 3.0) if rng.random() < 0.5 else 0.0

    return {
        "hr": hr,
        "rr": rr,
        "motion": motion,
        "hr_valid": hr_valid,
        "rr_valid": rr_valid,
        "t_h": t0_h + np.arange(n) / SAMPLE_HZ / 3600.0,
        "fatigue": fatigue,
    }


def _dropout_mask(n: int, rate: float, rng: np.random.Generator) -> np.ndarray:
    """Validity as runs: ``rate`` is the long-run fraction of lost samples."""
    mask = np.ones(n)
    if rate <= 0.0:
        return mask
    position = 0
    while position < n:
        good = int(rng.exponential(max(1.0, (1.0 - rate) * 240.0)))
        position += good
        bad = int(rng.exponential(max(1.0, rate * 240.0)))
        mask[position : position + bad] = 0.0
        position += bad
    return mask[:n]


def windowed_features(session: dict[str, np.ndarray]) -> np.ndarray:
    """Turn raw channels into feature rows, mirroring the Rust extractor.

    One row per sample once the window has filled. Both the baseline EWMA and
    the window statistics ignore invalid samples, which is the same rule
    ``FeatureExtractor`` follows.
    """
    n = session["hr"].shape[0]
    window = int(WINDOW_S * SAMPLE_HZ)
    if n <= window:
        return np.zeros((0, N_FEATURES), dtype=np.float32)

    alpha = 1.0 - np.exp(-1.0 / (BASELINE_TAU_S * SAMPLE_HZ))

    hr_baseline = np.full(n, np.nan)
    rr_baseline = np.full(n, np.nan)
    hr_b = rr_b = np.nan
    for i in range(n):
        if session["hr_valid"][i] > 0.5:
            hr_b = (
                session["hr"][i]
                if np.isnan(hr_b)
                else hr_b + alpha * (session["hr"][i] - hr_b)
            )
        if session["rr_valid"][i] > 0.5:
            rr_b = (
                session["rr"][i]
                if np.isnan(rr_b)
                else rr_b + alpha * (session["rr"][i] - rr_b)
            )
        hr_baseline[i] = hr_b
        rr_baseline[i] = rr_b

    log_motion = np.log1p(1000.0 * session["motion"])

    rows = np.zeros((n - window, N_FEATURES), dtype=np.float32)
    minutes = np.arange(window) / SAMPLE_HZ / 60.0
    for out, end in enumerate(range(window, n)):
        sl = slice(end - window, end)
        row = np.zeros(N_FEATURES)

        hr_ok = session["hr_valid"][sl] > 0.5
        rr_ok = session["rr_valid"][sl] > 0.5
        # A window needs to be *mostly* present before its spread and slope mean
        # anything. Eight scattered samples can span four seconds, and a
        # least-squares slope over a four-second baseline is pure noise with a
        # huge magnitude — which the rule would then read as a violent trend.
        # Below half a window the channel reports invalid, exactly as the Rust
        # extractor does.
        minimum = window // 2
        hr_present = hr_ok.sum() >= minimum and not np.isnan(hr_baseline[end - 1])
        rr_present = rr_ok.sum() >= minimum and not np.isnan(rr_baseline[end - 1])

        if hr_present:
            values = session["hr"][sl][hr_ok]
            row[IDX["hr_bpm"]] = values[-1]
            row[IDX["hr_baseline_delta"]] = values[-1] - hr_baseline[end - 1]
            row[IDX["hr_slope"]] = _slope(minutes[hr_ok], values)
            row[IDX["hr_std"]] = values.std()
            row[IDX["hr_valid"]] = 1.0

        if rr_present:
            values = session["rr"][sl][rr_ok]
            row[IDX["rr_bpm"]] = values[-1]
            row[IDX["rr_baseline_delta"]] = values[-1] - rr_baseline[end - 1]
            row[IDX["rr_slope"]] = _slope(minutes[rr_ok], values)
            row[IDX["rr_std"]] = values.std()
            row[IDX["rr_valid"]] = 1.0

        motion_window = log_motion[sl]
        row[IDX["motion_log_energy"]] = motion_window[-1]
        row[IDX["motion_slope"]] = _slope(minutes, motion_window)
        row[IDX["motion_active_fraction"]] = float(
            (session["motion"][sl] > MOTION_ACTIVE_THRESHOLD).mean()
        )
        row[IDX["time_on_task_h"]] = session["t_h"][end - 1]

        rows[out] = row

    return rows


def _slope(x: np.ndarray, y: np.ndarray) -> float:
    """Least-squares slope of ``y`` against ``x``; 0 when it is not defined."""
    if x.shape[0] < 2:
        return 0.0
    x_mean = x.mean()
    denominator = ((x - x_mean) ** 2).sum()
    if denominator <= 0.0:
        return 0.0
    return float(((x - x_mean) * (y - y.mean())).sum() / denominator)


def build_dataset(sessions: int, minutes: float, seed: int) -> np.ndarray:
    """Feature rows pooled across ``sessions`` independent simulated sessions."""
    rng = np.random.default_rng(seed)
    n = int(minutes * 60.0 * SAMPLE_HZ)
    blocks = []
    for _ in range(sessions):
        blocks.append(windowed_features(simulate(n, rng)))
    return np.concatenate(blocks, axis=0)
