# SPDX-License-Identifier: Apache-2.0

"""ONNX export, plus the check that the exported graph still does what PyTorch did.

    uv run export.py            # re-export out/fatigue.pt without retraining

Export settings are chosen for the target rather than for convenience:

* **opset 17** — comfortably inside what the ``ort`` 2.0 / ONNX Runtime 1.18
  build in ``crates/fatigue`` supports on ``aarch64``.
* **fixed batch of 1, no dynamic axes** — the runtime infers one window per
  vitals notification. A dynamic axis would buy nothing and costs shape
  inference at load.
* **no quantization** — a ~1.6k-parameter graph at 2 Hz on a Cortex-A35 is free.
  Quantizing it would trade real accuracy for imaginary headroom.
* **logits, not softmax** — the runtime needs the distribution for its entropy
  anyway, and doing the softmax there keeps the numerically-sensitive part in
  one place next to the entropy that consumes it.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import torch

from snf_net.contract import N_BINS, N_FEATURES
from snf_net.model import FatigueNet

OPSET = 17
OUT = Path(__file__).parent / "out"


def export(model: FatigueNet, path: Path, sample: np.ndarray | None = None) -> None:
    """Write ``model`` to ``path`` and verify the result against PyTorch."""
    model.eval()
    dummy = torch.zeros(1, N_FEATURES)

    torch.onnx.export(
        model,
        (dummy,),
        str(path),
        input_names=["features"],
        output_names=["logits"],
        opset_version=OPSET,
        dynamo=False,
    )
    print(f"wrote {path} ({path.stat().st_size} bytes, opset {OPSET})")

    _verify(model, path, sample)


def _verify(model: FatigueNet, path: Path, sample: np.ndarray | None) -> None:
    """Run both graphs on the same inputs and insist they agree.

    Worth the twenty lines: an export that silently loses the normalization
    buffers, or reorders an output, produces a model that loads fine and then
    drives the mat off entirely plausible-looking numbers.
    """
    import onnx
    import onnxruntime

    onnx.checker.check_model(onnx.load(str(path)))

    session = onnxruntime.InferenceSession(
        str(path), providers=["CPUExecutionProvider"]
    )
    inputs = session.get_inputs()
    outputs = session.get_outputs()
    assert len(inputs) == 1, f"expected one input, got {[i.name for i in inputs]}"
    assert inputs[0].name == "features", inputs[0].name
    assert list(inputs[0].shape) == [1, N_FEATURES], inputs[0].shape
    assert list(outputs[0].shape) == [1, N_BINS], outputs[0].shape

    if sample is None:
        sample = np.zeros((4, N_FEATURES), dtype=np.float32)

    worst = 0.0
    for row in sample.astype(np.float32):
        got = session.run(None, {"features": row[None, :]})[0]
        with torch.no_grad():
            want = model(torch.from_numpy(row[None, :])).numpy()
        worst = max(worst, float(np.abs(got - want).max()))

    assert worst < 1e-4, f"ONNX and PyTorch disagree by {worst}"
    print(
        f"verified against PyTorch on {len(sample)} rows (max |Δlogit| = {worst:.2e})"
    )


def main() -> None:
    checkpoint = torch.load(OUT / "fatigue.pt", weights_only=False)
    model = FatigueNet(
        torch.zeros(N_FEATURES), torch.ones(N_FEATURES), hidden=checkpoint["hidden"]
    )
    model.load_state_dict(checkpoint["state_dict"])
    export(model, OUT / "fatigue.onnx")


if __name__ == "__main__":
    main()
