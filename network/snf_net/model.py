# SPDX-License-Identifier: Apache-2.0

"""The Stage-A network: normalization, a small MLP, five ordinal logits.

Kept deliberately tiny. At 2 Hz on a Cortex-A35 the compute budget is enormous
relative to this graph, so the size is chosen for what it has to represent — a
smooth 14-D → 5-way function — rather than for the hardware. A bigger network
would fit the teacher's noise, not its shape.

Every op here is one ONNX Runtime covers well on ARM CPU: ``Sub``, ``Div``,
``Gemm``, ``Tanh``. No LayerNorm, no attention, nothing that needs a contrib
operator set.
"""

from __future__ import annotations

import torch
from torch import nn

from .contract import N_BINS, N_FEATURES


class Normalize(nn.Module):
    """Standardization as a layer, so the statistics ship inside the graph.

    The alternative — normalizing in Rust — means two artifacts that have to be
    updated together, and a silent, plausible-looking failure the first time
    someone retrains without copying the new constants across. Buffers are
    exported as initializers, so this costs 28 floats in the file and nothing at
    runtime.
    """

    def __init__(self, mean: torch.Tensor, std: torch.Tensor):
        super().__init__()
        self.register_buffer("mean", mean.float())
        # Guard against a constant feature in the training set producing a
        # division by zero for the rest of the model's life.
        self.register_buffer("std", std.float().clamp_min(1e-3))

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return (x - self.mean) / self.std


class FatigueNet(nn.Module):
    """14 features in, 5 ordinal logits out.

    ``Tanh`` rather than ``ReLU``: the job is to reproduce a smooth teacher, and
    a piecewise-linear activation reproduces it with visible kinks. Those kinks
    land in the commanded duty, where the inflation controller's derivative term
    then amplifies them.
    """

    def __init__(self, mean: torch.Tensor, std: torch.Tensor, hidden: int = 32):
        super().__init__()
        self.normalize = Normalize(mean, std)
        self.net = nn.Sequential(
            nn.Linear(N_FEATURES, hidden),
            nn.Tanh(),
            nn.Linear(hidden, hidden),
            nn.Tanh(),
            nn.Linear(hidden, N_BINS),
        )

    def forward(self, features: torch.Tensor) -> torch.Tensor:
        return self.net(self.normalize(features))

    @torch.no_grad()
    def decode(self, features: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
        """Level and confidence, decoded the way the Rust runtime decodes them.

        Not part of the exported graph — the runtime does this itself, on the
        logits — but having it here lets training report the numbers the device
        will actually emit instead of a training-only proxy.
        """
        from .contract import BIN_CENTERS

        p = torch.softmax(self(features), dim=-1)
        centers = torch.tensor(BIN_CENTERS, device=p.device, dtype=p.dtype)
        level = (p * centers).sum(dim=-1)
        entropy = -(p * p.clamp_min(1e-12).log()).sum(dim=-1)
        confidence = 1.0 - entropy / torch.log(torch.tensor(float(N_BINS)))
        return level, confidence


def parameter_count(model: nn.Module) -> int:
    return sum(p.numel() for p in model.parameters())
