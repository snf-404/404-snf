"""CSV loading and leakage-safe splitting."""

from __future__ import annotations

import csv
import random
from pathlib import Path

from .features import REQUIRED_COLUMNS
from .formula import empirical_fatigue_score


def load_csv(path: Path) -> tuple[list[dict[str, str]], list[float], str]:
    with path.open(encoding="utf-8-sig", newline="") as stream:
        reader = csv.DictReader(stream)
        missing = sorted(set(REQUIRED_COLUMNS) - set(reader.fieldnames or ()))
        if missing:
            raise ValueError(f"CSV missing columns: {', '.join(missing)}")
        rows = list(reader)
    if not rows:
        raise ValueError("CSV contains no data rows")

    has_labels = all(row.get("fatigue_score", "").strip() for row in rows)
    if has_labels:
        labels = [float(row["fatigue_score"]) for row in rows]
        if any(not 0.0 <= value <= 100.0 for value in labels):
            raise ValueError("fatigue_score values must be in [0, 100]")
        label_source = "measured"
    else:
        labels = [empirical_fatigue_score(row) for row in rows]
        label_source = "empirical-formula"
    return rows, labels, label_source


def split_indices(
    rows: list[dict[str, str]], validation_fraction: float, seed: int
) -> tuple[list[int], list[int]]:
    """Split whole subjects when subject_id is available; otherwise split rows."""

    rng = random.Random(seed)
    subjects = [row.get("subject_id", "").strip() for row in rows]
    unique_subjects = sorted(set(subjects) - {""})
    if len(unique_subjects) >= 2 and all(subjects):
        rng.shuffle(unique_subjects)
        validation_count = min(
            len(unique_subjects) - 1,
            max(1, round(len(unique_subjects) * validation_fraction)),
        )
        validation_subjects = set(unique_subjects[:validation_count])
        train = [
            index for index, subject in enumerate(subjects) if subject not in validation_subjects
        ]
        validation = [
            index for index, subject in enumerate(subjects) if subject in validation_subjects
        ]
    else:
        indices = list(range(len(rows)))
        rng.shuffle(indices)
        validation_count = min(len(rows) - 1, max(1, round(len(rows) * validation_fraction)))
        validation = sorted(indices[:validation_count])
        train = sorted(indices[validation_count:])
    if not train or not validation:
        raise ValueError("at least two rows are required for train/validation splitting")
    return train, validation
