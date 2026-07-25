"""Interpretable fatigue-proxy training package."""

from .formula import empirical_fatigue_score
from .model import FatigueLinearModel

__all__ = ["FatigueLinearModel", "empirical_fatigue_score"]

