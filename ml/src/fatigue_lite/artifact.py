"""Portable model artifact I/O."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import torch

from .features import FEATURE_NAMES
from .model import FatigueLinearModel

SCHEMA_VERSION = 1


def save_artifact(
    path: Path,
    model: FatigueLinearModel,
    mean: torch.Tensor,
    scale: torch.Tensor,
    metadata: dict[str, Any],
) -> None:
    payload = {
        "schema_version": SCHEMA_VERSION,
        "model_type": "standardized-logistic-linear",
        "feature_names": list(FEATURE_NAMES),
        "mean": mean.tolist(),
        "scale": scale.tolist(),
        "weight": model.linear.weight.detach().cpu().flatten().tolist(),
        "bias": float(model.linear.bias.detach().cpu().item()),
        "output": {"name": "fatigue_score", "minimum": 0.0, "maximum": 100.0},
        "metadata": metadata,
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def load_artifact(path: Path) -> tuple[FatigueLinearModel, torch.Tensor, torch.Tensor, dict]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise ValueError(f"unsupported artifact schema: {payload.get('schema_version')}")
    if payload.get("feature_names") != list(FEATURE_NAMES):
        raise ValueError("artifact feature order does not match this package")
    model = FatigueLinearModel(len(FEATURE_NAMES))
    with torch.no_grad():
        model.linear.weight.copy_(torch.tensor([payload["weight"]], dtype=torch.float32))
        model.linear.bias.copy_(torch.tensor([payload["bias"]], dtype=torch.float32))
    model.eval()
    return (
        model,
        torch.tensor(payload["mean"], dtype=torch.float32),
        torch.tensor(payload["scale"], dtype=torch.float32),
        payload,
    )

