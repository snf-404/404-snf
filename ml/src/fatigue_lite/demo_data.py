"""Generate deterministic demonstration data; never use it for validation claims."""

from __future__ import annotations

import argparse
import csv
import random
from pathlib import Path

FIELDS = [
    "subject_id",
    "window_start",
    "heart_rate_bpm",
    "respiration_rate_bpm",
    "rms_radial_speed_mps",
    "moving_point_fraction",
    "short_term_energy_mps2",
    "long_term_energy_mps2",
    "baseline_heart_rate_bpm",
    "baseline_respiration_rate_bpm",
]


def generate(path: Path, subjects: int = 20, windows: int = 30, seed: int = 404) -> None:
    rng = random.Random(seed)
    rows: list[dict[str, object]] = []
    for subject in range(subjects):
        baseline_hr = rng.uniform(62.0, 82.0)
        baseline_rr = rng.uniform(12.0, 19.0)
        for window in range(windows):
            progression = window / max(windows - 1, 1)
            long_energy = rng.uniform(0.001, 0.008)
            short_energy = max(0.0, long_energy * (1.15 - 0.65 * progression + rng.gauss(0, 0.08)))
            row: dict[str, object] = {
                "subject_id": f"demo-{subject:03d}",
                "window_start": f"2026-01-01T{window // 60:02d}:{window % 60:02d}:00Z",
                "heart_rate_bpm": baseline_hr - 8.0 * progression + rng.gauss(0, 2.0),
                "respiration_rate_bpm": baseline_rr - 3.0 * progression + rng.gauss(0, 0.8),
                "rms_radial_speed_mps": max(0.002, 0.11 - 0.075 * progression + rng.gauss(0, 0.012)),
                "moving_point_fraction": min(1.0, max(0.0, 0.65 - 0.40 * progression + rng.gauss(0, 0.08))),
                "short_term_energy_mps2": short_energy,
                "long_term_energy_mps2": long_energy,
                "baseline_heart_rate_bpm": baseline_hr,
                "baseline_respiration_rate_bpm": baseline_rr,
            }
            rows.append(row)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=FIELDS)
        writer.writeheader()
        writer.writerows(rows)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("data/demo.csv"))
    parser.add_argument("--subjects", type=int, default=20)
    parser.add_argument("--windows", type=int, default=30)
    parser.add_argument("--seed", type=int, default=404)
    args = parser.parse_args()
    generate(args.output, args.subjects, args.windows, args.seed)
    print(f"demo data: {args.output}")


if __name__ == "__main__":
    main()
