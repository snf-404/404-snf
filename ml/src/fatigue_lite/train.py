"""Train a deterministic ridge logistic-linear model."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import torch

from .artifact import save_artifact
from .data import load_csv, split_indices
from .features import FEATURE_NAMES, feature_matrix
from .model import FatigueLinearModel


def _metrics(prediction: torch.Tensor, target: torch.Tensor) -> dict[str, float | None]:
    error = prediction - target
    mae = error.abs().mean().item()
    rmse = error.square().mean().sqrt().item()
    denominator = ((target - target.mean()) ** 2).sum()
    r2 = None if denominator <= 1e-12 else (1.0 - error.square().sum() / denominator).item()
    return {"mae": mae, "rmse": rmse, "r2": r2}


def train(
    data_path: Path,
    output_path: Path,
    ridge: float = 0.1,
    validation_fraction: float = 0.2,
    seed: int = 404,
) -> dict:
    if ridge < 0.0:
        raise ValueError("ridge must be non-negative")
    if not 0.0 < validation_fraction < 1.0:
        raise ValueError("validation_fraction must be between 0 and 1")
    torch.manual_seed(seed)
    torch.use_deterministic_algorithms(True)

    rows, labels, label_source = load_csv(data_path)
    train_indices, validation_indices = split_indices(rows, validation_fraction, seed)
    x = torch.tensor(feature_matrix(rows), dtype=torch.float64)
    y_score = torch.tensor(labels, dtype=torch.float64).clamp(0.5, 99.5)
    y_logit = torch.logit(y_score / 100.0)

    x_train = x[train_indices]
    mean = x_train.mean(dim=0)
    scale = x_train.std(dim=0, unbiased=False).clamp_min(1e-6)
    x_standardized = (x - mean) / scale

    # Closed-form ridge regression on the logit target. The last column is the
    # intercept and is intentionally not regularized.
    design = torch.cat(
        [x_standardized[train_indices], torch.ones((len(train_indices), 1), dtype=torch.float64)],
        dim=1,
    )
    penalty = torch.eye(design.shape[1], dtype=torch.float64) * ridge
    penalty[-1, -1] = 0.0
    coefficients = torch.linalg.solve(
        design.T @ design + penalty, design.T @ y_logit[train_indices]
    )

    model = FatigueLinearModel(len(FEATURE_NAMES))
    with torch.no_grad():
        model.linear.weight.copy_(coefficients[:-1].float().unsqueeze(0))
        model.linear.bias.copy_(coefficients[-1:].float())
    model.eval()

    with torch.no_grad():
        standardized = x_standardized.float()
        prediction = model(standardized)
    train_metrics = _metrics(prediction[train_indices], y_score[train_indices].float())
    validation_metrics = _metrics(
        prediction[validation_indices], y_score[validation_indices].float()
    )
    metadata = {
        "label_source": label_source,
        "data_rows": len(rows),
        "train_rows": len(train_indices),
        "validation_rows": len(validation_indices),
        "ridge": ridge,
        "seed": seed,
        "train_metrics": train_metrics,
        "validation_metrics": validation_metrics,
        "warning": "Engineering fatigue proxy; not a medical diagnosis.",
    }
    save_artifact(output_path, model, mean.float(), scale.float(), metadata)
    return metadata


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True, help="Input window-level CSV")
    parser.add_argument("--output", type=Path, default=Path("out/fatigue-linear.json"))
    parser.add_argument("--onnx-output", type=Path, default=Path("out/fatigue.onnx"))
    parser.add_argument("--ridge", type=float, default=0.1)
    parser.add_argument("--validation-fraction", type=float, default=0.2)
    parser.add_argument("--seed", type=int, default=404)
    return parser


def main() -> None:
    args = _parser().parse_args()
    metrics = train(args.data, args.output, args.ridge, args.validation_fraction, args.seed)
    print(json.dumps(metrics, indent=2))
    print(f"artifact: {args.output}")
    from .export import export

    export(args.output, args.onnx_output)


if __name__ == "__main__":
    main()
