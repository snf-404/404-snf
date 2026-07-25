"""Export the portable linear artifact to the ONNX contract consumed by Rust."""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import torch
from torch import nn

from .artifact import load_artifact
from .features import FEATURE_NAMES

INPUT_NAME = "features"
OUTPUT_NAME = "fatigue_score"
OPSET = 17


class OnnxFatigueModel(nn.Module):
    """Normalization and the seven-parameter model in one deployable graph."""

    def __init__(self, model: nn.Module, mean: torch.Tensor, scale: torch.Tensor) -> None:
        super().__init__()
        self.model = model
        self.register_buffer("mean", mean)
        self.register_buffer("scale", scale.clamp_min(1e-6))

    def forward(self, features: torch.Tensor) -> torch.Tensor:
        return self.model((features - self.mean) / self.scale).unsqueeze(1)


def export(model_path: Path, output_path: Path) -> None:
    model, mean, scale, _ = load_artifact(model_path)
    deploy = OnnxFatigueModel(model, mean, scale).eval()
    dummy = torch.zeros((1, len(FEATURE_NAMES)), dtype=torch.float32)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    torch.onnx.export(
        deploy,
        (dummy,),
        str(output_path),
        input_names=[INPUT_NAME],
        output_names=[OUTPUT_NAME],
        opset_version=OPSET,
        dynamo=False,
    )

    import onnx
    import onnxruntime

    onnx.checker.check_model(onnx.load(str(output_path)))
    session = onnxruntime.InferenceSession(
        str(output_path), providers=["CPUExecutionProvider"]
    )
    assert session.get_inputs()[0].name == INPUT_NAME
    assert list(session.get_inputs()[0].shape) == [1, len(FEATURE_NAMES)]
    assert session.get_outputs()[0].name == OUTPUT_NAME
    assert list(session.get_outputs()[0].shape) == [1, 1]

    samples = torch.tensor(
        [
            [0.0, 0.0, 0.2, 0.4, 0.0, 0.0],
            [1.0, 1.0, 0.9, 0.9, 0.8, 1.0],
            [-1.0, -1.0, 0.1, 0.1, -0.5, 1.0],
        ],
        dtype=torch.float32,
    )
    with torch.no_grad():
        expected = deploy(samples).numpy()
    maximum_error = 0.0
    for sample, wanted in zip(samples.numpy(), expected, strict=True):
        actual = session.run([OUTPUT_NAME], {INPUT_NAME: sample[None]})[0]
        maximum_error = max(maximum_error, float(np.abs(actual - wanted).max()))
    if maximum_error >= 1e-4:
        raise RuntimeError(f"ONNX and PyTorch differ by {maximum_error}")
    print(
        f"exported {output_path} ({output_path.stat().st_size} bytes); "
        f"max |ONNX-PyTorch|={maximum_error:.2e}"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, default=Path("out/fatigue-linear.json"))
    parser.add_argument("--output", type=Path, default=Path("out/fatigue.onnx"))
    args = parser.parse_args()
    export(args.model, args.output)


if __name__ == "__main__":
    main()
