#!/usr/bin/env python3
"""Validate respiration estimation on the MIT pulse-wifi-sensing dataset."""

from __future__ import annotations

import _ctypes
import argparse
import ctypes
import pathlib
import sys
import tempfile

from validate_espectre_dataset import Result, build_detector, load_detector, new_state


def load_esp32_csv(path: pathlib.Path):
    rows = []
    with path.open(encoding="utf-8") as source:
        for line in source:
            if not line.startswith("CSI_DATA"):
                continue
            fields = line.split(",")
            if int(fields[5]) != 1:
                continue
            timestamp_us = int(fields[18])
            iq = [
                int(value) for value in line.split("[", 1)[1].split("]", 1)[0].split()
            ]
            if len(iq) == 128:
                rows.append((timestamp_us, iq))
    if not rows:
        raise ValueError(f"no HT CSI rows found in {path}")
    return rows


def evaluate(library, path: pathlib.Path):
    rows = load_esp32_csv(path)
    state = new_state(library)
    result = Result()
    first_timestamp = rows[0][0]
    outputs = []
    for timestamp_us, iq in rows:
        frame = (ctypes.c_int8 * 128)(*iq)
        library.csi_sensing_push(
            state,
            frame,
            128,
            timestamp_us - first_timestamp,
            -60,
            ctypes.byref(result),
        )
        outputs.append(
            (
                bool(result.motion),
                bool(result.breathing_valid),
                float(result.breathing_bpm),
                float(result.breathing_confidence),
            )
        )
    return outputs


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "dataset", type=pathlib.Path, help="Path to pulse-wifi-sensing/data/breathing"
    )
    parser.add_argument("--cc", default="gcc", help="Host C compiler")
    args = parser.parse_args()

    repo = pathlib.Path(__file__).resolve().parents[1]
    positive_path = args.dataset / "resp_6rpm.csv"
    negative_path = args.dataset / "resp_vacio.csv"
    if not positive_path.exists() or not negative_path.exists():
        parser.error("resp_6rpm.csv and resp_vacio.csv are required")

    suffix = ".dll" if sys.platform == "win32" else ".so"
    with tempfile.TemporaryDirectory(prefix="csi-breath-") as temporary:
        library_path = pathlib.Path(temporary) / f"csi_sensing{suffix}"
        build_detector(repo, library_path, args.cc)
        library = load_detector(library_path)

        positive = evaluate(library, positive_path)
        negative = evaluate(library, negative_path)
        positive_tail = positive[-1500:]
        negative_tail = negative[-1500:]
        valid_bpms = [sample[2] for sample in positive_tail if sample[1]]
        detection_rate = len(valid_bpms) / len(positive_tail)
        false_rate = sum(sample[1] for sample in negative_tail) / len(negative_tail)
        median_bpm = sorted(valid_bpms)[len(valid_bpms) // 2] if valid_bpms else 0.0

        print(f"paced 6 bpm: estimate={median_bpm:.2f} bpm valid={detection_rate:.3%}")
        print(f"empty room: false breathing={false_rate:.3%}")

        if sys.platform == "win32":
            _ctypes.FreeLibrary(library._handle)
        del library

    passed = (
        abs(median_bpm - 6.0) <= 1.0 and detection_rate >= 0.75 and false_rate <= 0.10
    )
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
