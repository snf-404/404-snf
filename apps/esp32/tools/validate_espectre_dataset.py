#!/usr/bin/env python3
"""Run the deployed C detector against ESPectre's public ESP32-C5 recordings."""

from __future__ import annotations

import argparse
import ctypes
import _ctypes
import pathlib
import subprocess
import sys
import tempfile

import numpy as np


class Result(ctypes.Structure):
    _fields_ = [
        ("stage", ctypes.c_int),
        ("calibration_percent", ctypes.c_uint8),
        ("motion", ctypes.c_bool),
        ("motion_score", ctypes.c_float),
        ("motion_threshold", ctypes.c_float),
        ("breathing_valid", ctypes.c_bool),
        ("breathing_bpm", ctypes.c_float),
        ("breathing_confidence", ctypes.c_float),
        ("rssi", ctypes.c_int8),
        ("accepted_frames", ctypes.c_uint32),
        ("rejected_frames", ctypes.c_uint32),
    ]


def build_detector(repo: pathlib.Path, output: pathlib.Path, compiler: str) -> None:
    source = repo / "components" / "csi_sensing" / "csi_sensing.c"
    include = repo / "components" / "csi_sensing" / "include"
    command = [
        compiler,
        "-shared",
        "-O2",
        "-std=c17",
        f"-I{include}",
        str(source),
        "-o",
        str(output),
        "-lm",
    ]
    subprocess.run(command, check=True)


def load_detector(library_path: pathlib.Path):
    library = ctypes.CDLL(str(library_path))
    library.csi_sensing_instance_size.restype = ctypes.c_size_t
    library.csi_sensing_init.argtypes = [ctypes.c_void_p]
    library.csi_sensing_push.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_int8),
        ctypes.c_size_t,
        ctypes.c_int64,
        ctypes.c_int8,
        ctypes.POINTER(Result),
    ]
    library.csi_sensing_push.restype = ctypes.c_bool
    return library


def new_state(library):
    state = ctypes.create_string_buffer(library.csi_sensing_instance_size())
    library.csi_sensing_init(state)
    return state


def feed(library, state, frames: np.ndarray, start_index: int = 0):
    result = Result()
    decisions: list[bool] = []
    scores: list[float] = []
    for index, frame in enumerate(np.ascontiguousarray(frames, dtype=np.int8)):
        pointer = frame.ctypes.data_as(ctypes.POINTER(ctypes.c_int8))
        library.csi_sensing_push(
            state,
            pointer,
            frame.size,
            (start_index + index) * 10_000,
            -50,
            ctypes.byref(result),
        )
        decisions.append(bool(result.motion))
        scores.append(float(result.motion_score))
    return result, decisions, scores


def load_frames(path: pathlib.Path) -> np.ndarray:
    with np.load(path) as recording:
        frames = recording["csi_data"]
    if frames.ndim != 2 or frames.shape[1] < 128:
        raise ValueError(f"unsupported CSI shape in {path}: {frames.shape}")
    return frames[:, :128]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "dataset", type=pathlib.Path, help="Path to espectre/micro-espectre/data"
    )
    parser.add_argument("--cc", default="gcc", help="Host C compiler")
    args = parser.parse_args()

    repo = pathlib.Path(__file__).resolve().parents[1]
    baseline_files = sorted((args.dataset / "baseline").glob("baseline_c5_*.npz"))
    movement_files = sorted((args.dataset / "movement").glob("movement_c5_*.npz"))
    long_files = sorted((args.dataset / "test").glob("test_c5_*.npz"))
    if not baseline_files or not movement_files:
        parser.error("ESP32-C5 baseline/movement files were not found")

    suffix = ".dll" if sys.platform == "win32" else ".so"
    with tempfile.TemporaryDirectory(prefix="csi-sensing-") as temporary:
        library_path = pathlib.Path(temporary) / f"csi_sensing{suffix}"
        build_detector(repo, library_path, args.cc)
        library = load_detector(library_path)

        false_hits = 0
        idle_decisions = 0
        motion_hits = 0
        motion_decisions = 0
        for baseline_path, movement_path in zip(baseline_files, movement_files):
            state = new_state(library)
            baseline = load_frames(baseline_path)
            movement = load_frames(movement_path)
            result, idle, _ = feed(library, state, baseline)
            result, active, _ = feed(library, state, movement, len(baseline))
            idle = idle[1000:]
            active = active[100:]
            pair_fp = sum(idle) / max(len(idle), 1)
            pair_recall = sum(active) / max(len(active), 1)
            print(
                f"  {baseline_path.stem}: recall={pair_recall:.3%} "
                f"false_positive={pair_fp:.3%} threshold={result.motion_threshold:.8f}"
            )
            false_hits += sum(idle)
            idle_decisions += len(idle)
            motion_hits += sum(active)
            motion_decisions += len(active)

        false_positive_rate = false_hits / max(idle_decisions, 1)
        recall = motion_hits / max(motion_decisions, 1)
        print(
            f"paired C5 recordings: recall={recall:.3%} false_positive={false_positive_rate:.3%}"
        )

        if long_files:
            frames = load_frames(long_files[0])
            state = new_state(library)
            long_result, decisions, scores = feed(library, state, frames)
            transition = 3320
            idle = decisions[1000:transition]
            active = decisions[transition + 100 :]
            false_runs: list[tuple[int, int]] = []
            run_start = None
            for offset, decision in enumerate(idle):
                if decision and run_start is None:
                    run_start = 1000 + offset
                elif not decision and run_start is not None:
                    false_runs.append((run_start, 1000 + offset - 1))
                    run_start = None
            if run_start is not None:
                false_runs.append((run_start, transition - 1))
            print(
                "long C5 recording: "
                f"recall={sum(active) / max(len(active), 1):.3%} "
                f"false_positive={sum(idle) / max(len(idle), 1):.3%} "
                f"threshold={long_result.motion_threshold:.8f} "
                f"idle_p99={np.percentile(scores[1000:transition], 99):.8f}"
            )
            if false_runs:
                print(f"  idle motion runs: {false_runs}")

        if sys.platform == "win32":
            _ctypes.FreeLibrary(library._handle)
        del library

    return 0 if recall >= 0.80 and false_positive_rate <= 0.10 else 1


if __name__ == "__main__":
    raise SystemExit(main())
