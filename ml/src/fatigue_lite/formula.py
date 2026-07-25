"""Literature-informed weak-label formula.

This is an engineering prior for bootstrapping, not a validated diagnostic scale.
Replace its pseudo-labels with KSS/PVT or another protocol label as soon as data exists.
"""

from __future__ import annotations

import math
from collections.abc import Mapping, Sequence

from .features import engineer_features

# Logit coefficients. Motion is deliberately weaker than combined vital-sign evidence,
# because stillness by itself is not fatigue.
EMPIRICAL_INTERCEPT = -2.20
EMPIRICAL_WEIGHTS = (0.65, 0.85, 0.55, 0.35, 0.45, 0.35)


def _sigmoid(value: float) -> float:
    if value >= 0:
        return 1.0 / (1.0 + math.exp(-value))
    exp_value = math.exp(value)
    return exp_value / (1.0 + exp_value)


def empirical_fatigue_score(row: Mapping[str, object]) -> float:
    """Return a bounded 0..100 fatigue proxy from one aggregated radar window."""

    features = engineer_features(row)
    logit = EMPIRICAL_INTERCEPT + sum(
        weight * value for weight, value in zip(EMPIRICAL_WEIGHTS, features, strict=True)
    )
    return 100.0 * _sigmoid(logit)


def empirical_scores(rows: Sequence[Mapping[str, object]]) -> list[float]:
    return [empirical_fatigue_score(row) for row in rows]

