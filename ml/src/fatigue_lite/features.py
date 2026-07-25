"""Feature engineering shared by training and inference."""

from __future__ import annotations

import math
from collections.abc import Mapping, Sequence

FEATURE_NAMES = (
    "heart_slowdown",
    "respiration_slowdown",
    "motion_quietness",
    "moving_point_quietness",
    "recent_motion_drop",
    "cardiorespiratory_agreement",
)

REQUIRED_COLUMNS = (
    "heart_rate_bpm",
    "respiration_rate_bpm",
    "rms_radial_speed_mps",
    "moving_point_fraction",
    "short_term_energy_mps2",
    "long_term_energy_mps2",
    "baseline_heart_rate_bpm",
    "baseline_respiration_rate_bpm",
)


def _finite(row: Mapping[str, object], name: str) -> float:
    try:
        value = float(row[name])
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{name} must be a finite number") from error
    if not math.isfinite(value):
        raise ValueError(f"{name} must be a finite number")
    return value


def _clip(value: float, low: float, high: float) -> float:
    return min(max(value, low), high)


def engineer_features(row: Mapping[str, object]) -> list[float]:
    """Convert one radar window to six bounded, dimensionless features.

    Positive slowdown/drop values indicate a change in the drowsiness direction.
    Personal baselines should come from a rested, awake calibration period.
    """

    heart = _finite(row, "heart_rate_bpm")
    respiration = _finite(row, "respiration_rate_bpm")
    rms_speed = max(_finite(row, "rms_radial_speed_mps"), 0.0)
    moving_fraction = _clip(_finite(row, "moving_point_fraction"), 0.0, 1.0)
    short_energy = max(_finite(row, "short_term_energy_mps2"), 0.0)
    long_energy = max(_finite(row, "long_term_energy_mps2"), 0.0)
    baseline_heart = _finite(row, "baseline_heart_rate_bpm")
    baseline_respiration = _finite(row, "baseline_respiration_rate_bpm")

    if not 30.0 <= heart <= 220.0 or not 30.0 <= baseline_heart <= 220.0:
        raise ValueError("heart rates must be in [30, 220] bpm")
    if not 4.0 <= respiration <= 60.0 or not 4.0 <= baseline_respiration <= 60.0:
        raise ValueError("respiration rates must be in [4, 60] bpm")

    heart_scale = max(0.15 * baseline_heart, 10.0)
    respiration_scale = max(0.20 * baseline_respiration, 3.0)
    heart_slowdown = _clip((baseline_heart - heart) / heart_scale, -2.0, 2.0)
    respiration_slowdown = _clip(
        (baseline_respiration - respiration) / respiration_scale, -2.0, 2.0
    )
    motion_quietness = 1.0 - _clip(rms_speed / 0.10, 0.0, 1.0)
    moving_point_quietness = 1.0 - moving_fraction
    recent_motion_drop = _clip(
        (long_energy - short_energy) / max(long_energy, 1e-6), -1.0, 1.0
    )
    agreement = heart_slowdown * respiration_slowdown

    return [
        heart_slowdown,
        respiration_slowdown,
        motion_quietness,
        moving_point_quietness,
        recent_motion_drop,
        agreement,
    ]


def feature_matrix(rows: Sequence[Mapping[str, object]]) -> list[list[float]]:
    return [engineer_features(row) for row in rows]

