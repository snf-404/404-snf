# SPDX-License-Identifier: Apache-2.0

"""Fit :class:`FatigueNet` to the analytic teacher and export it to ONNX.

    uv run train.py                      # train, evaluate, write out/fatigue.onnx
    uv run train.py --epochs 120         # longer
    uv run train.py --device cpu         # if MPS misbehaves

The loss is KL divergence against the teacher's *distribution*, not a regression
against its score. That distinction is the whole point of Stage A: the runtime
reads confidence out of the output's entropy and uses it to decide whether the
mat may move at all, so the spread has to be learned as carefully as the mean.
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import numpy as np
import torch
from torch import nn

from snf_net.contract import BIN_CENTERS, FEATURES, N_BINS
from snf_net.model import FatigueNet, parameter_count
from snf_net.rule import level_and_confidence, target_distribution
from snf_net.sessions import build_dataset

OUT = Path(__file__).parent / "out"


def pick_device(requested: str) -> torch.device:
    if requested != "auto":
        return torch.device(requested)
    if torch.backends.mps.is_available():
        return torch.device("mps")
    if torch.cuda.is_available():
        return torch.device("cuda")
    return torch.device("cpu")


def evaluate(
    model: FatigueNet, x: torch.Tensor, p_true: np.ndarray
) -> dict[str, float]:
    """Report the quantities the device cares about, not just the loss.

    ``level`` and ``confidence`` here are decoded exactly as
    ``crates/fatigue`` decodes them, so these numbers transfer.
    """
    model.eval()
    with torch.no_grad():
        level, confidence = model.decode(x)
    level = level.cpu().numpy().astype(np.float64)
    confidence = confidence.cpu().numpy().astype(np.float64)
    true_level, true_confidence = level_and_confidence(p_true)

    return {
        "level_mae": float(np.abs(level - true_level).mean()),
        "level_p95": float(np.percentile(np.abs(level - true_level), 95)),
        "confidence_mae": float(np.abs(confidence - true_confidence).mean()),
        # The three bands the runtime gates on. If the model systematically
        # shifts mass across .3 or .8 the gate changes behaviour even where the
        # level is right, so these are tracked separately from the MAE.
        "frac_below_0.3": float((confidence < 0.3).mean()),
        "frac_0.3_to_0.8": float(((confidence >= 0.3) & (confidence <= 0.8)).mean()),
        "frac_above_0.8": float((confidence > 0.8).mean()),
        "true_frac_below_0.3": float((true_confidence < 0.3).mean()),
        "true_frac_above_0.8": float((true_confidence > 0.8).mean()),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--epochs", type=int, default=80)
    parser.add_argument("--batch-size", type=int, default=1024)
    parser.add_argument("--lr", type=float, default=3e-3)
    parser.add_argument("--hidden", type=int, default=32)
    parser.add_argument("--train-sessions", type=int, default=140)
    parser.add_argument("--val-sessions", type=int, default=30)
    parser.add_argument("--minutes", type=float, default=45.0)
    parser.add_argument("--seed", type=int, default=20260725)
    parser.add_argument("--device", default="auto")
    args = parser.parse_args()

    torch.manual_seed(args.seed)
    device = pick_device(args.device)
    print(f"device: {device}")

    # ── Data ────────────────────────────────────────────────────────────────
    started = time.time()
    x_train = build_dataset(args.train_sessions, args.minutes, args.seed)
    x_val = build_dataset(args.val_sessions, args.minutes, args.seed + 1)
    print(
        f"simulated {args.train_sessions}+{args.val_sessions} sessions "
        f"-> {x_train.shape[0]} train / {x_val.shape[0]} val rows "
        f"in {time.time() - started:.1f}s"
    )

    p_train = target_distribution(x_train.astype(np.float64))
    p_val = target_distribution(x_val.astype(np.float64))

    mean = torch.from_numpy(x_train.mean(axis=0))
    std = torch.from_numpy(x_train.std(axis=0))

    xt = torch.from_numpy(x_train).to(device)
    pt = torch.from_numpy(p_train.astype(np.float32)).to(device)
    xv = torch.from_numpy(x_val).to(device)

    # ── Model ───────────────────────────────────────────────────────────────
    model = FatigueNet(mean, std, hidden=args.hidden).to(device)
    print(f"parameters: {parameter_count(model)}")

    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)
    schedule = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=args.epochs)
    # `batchmean` is the mathematically correct KL reduction; the default
    # `mean` divides by the class count too and silently scales the gradient.
    criterion = nn.KLDivLoss(reduction="batchmean")

    n = xt.shape[0]
    print(f"{'epoch':>5} {'loss':>10} {'level MAE':>10} {'conf MAE':>9}")
    for epoch in range(1, args.epochs + 1):
        model.train()
        permutation = torch.randperm(n, device=device)
        total = 0.0
        for start in range(0, n, args.batch_size):
            index = permutation[start : start + args.batch_size]
            logits = model(xt[index])
            loss = criterion(torch.log_softmax(logits, dim=-1), pt[index])

            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()
            total += loss.item() * index.shape[0]
        schedule.step()

        if epoch % 10 == 0 or epoch == 1 or epoch == args.epochs:
            metrics = evaluate(model, xv, p_val)
            print(
                f"{epoch:>5} {total / n:>10.6f} "
                f"{metrics['level_mae']:>10.3f} {metrics['confidence_mae']:>9.4f}"
            )

    # ── Report ──────────────────────────────────────────────────────────────
    metrics = evaluate(model, xv, p_val)
    print("\nvalidation:")
    for key, value in metrics.items():
        print(f"  {key:<22} {value:.4f}")

    OUT.mkdir(exist_ok=True)
    torch.save(
        {
            "state_dict": model.state_dict(),
            "hidden": args.hidden,
            "features": FEATURES,
            "bin_centers": BIN_CENTERS,
        },
        OUT / "fatigue.pt",
    )
    (OUT / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")
    print(f"\nwrote {OUT / 'fatigue.pt'}")

    # Export in the same run, so a checkpoint and its ONNX file can never
    # disagree about which weights they hold.
    from export import export

    export(model.cpu(), OUT / "fatigue.onnx", x_val[:256])


if __name__ == "__main__":
    main()
