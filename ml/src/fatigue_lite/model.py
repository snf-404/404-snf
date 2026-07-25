"""Six-feature logistic-linear fatigue regressor."""

from __future__ import annotations

import torch
from torch import nn


class FatigueLinearModel(nn.Module):
    """A 7-parameter model (six weights plus one bias)."""

    def __init__(self, input_features: int = 6) -> None:
        super().__init__()
        self.linear = nn.Linear(input_features, 1)

    def forward(self, features: torch.Tensor) -> torch.Tensor:
        return 100.0 * torch.sigmoid(self.linear(features)).squeeze(-1)

    def logits(self, features: torch.Tensor) -> torch.Tensor:
        return self.linear(features).squeeze(-1)

