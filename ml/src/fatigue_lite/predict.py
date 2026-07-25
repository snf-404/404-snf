"""Run fatigue-proxy inference from a CSV file."""

from __future__ import annotations

import argparse
import csv
from pathlib import Path

import torch

from .artifact import load_artifact
from .features import feature_matrix


def predict(model_path: Path, data_path: Path, output_path: Path) -> None:
    model, mean, scale, _ = load_artifact(model_path)
    with data_path.open(encoding="utf-8-sig", newline="") as stream:
        rows = list(csv.DictReader(stream))
    features = torch.tensor(feature_matrix(rows), dtype=torch.float32)
    with torch.no_grad():
        scores = model((features - mean) / scale).tolist()
    fieldnames = list(rows[0]) + ["predicted_fatigue_score"]
    with output_path.open("w", encoding="utf-8", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fieldnames)
        writer.writeheader()
        for row, score in zip(rows, scores, strict=True):
            writer.writerow({**row, "predicted_fatigue_score": f"{score:.4f}"})


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    predict(args.model, args.data, args.output)
    print(f"predictions: {args.output}")


if __name__ == "__main__":
    main()
