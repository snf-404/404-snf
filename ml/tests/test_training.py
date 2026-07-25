import json

from fatigue_lite.artifact import load_artifact
from fatigue_lite.demo_data import generate
from fatigue_lite.export import export
from fatigue_lite.train import train


def test_training_is_deterministic_and_writes_portable_artifact(tmp_path):
    data = tmp_path / "demo.csv"
    first = tmp_path / "first.json"
    second = tmp_path / "second.json"
    generate(data, subjects=6, windows=8)
    metrics = train(data, first, seed=7)
    train(data, second, seed=7)
    assert json.loads(first.read_text()) == json.loads(second.read_text())
    assert metrics["validation_metrics"]["mae"] < 1.0
    model, mean, scale, payload = load_artifact(first)
    assert model.linear.weight.numel() == 6
    assert mean.shape == scale.shape == (6,)
    assert payload["metadata"]["label_source"] == "empirical-formula"

    onnx = tmp_path / "fatigue.onnx"
    export(first, onnx)
    assert onnx.stat().st_size < 20_000
