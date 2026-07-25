# network/ — the Stage-A fatigue model

Produces `out/fatigue.onnx`, the graph `crates/fatigue` loads on the CA35.

```bash
just net-train          # simulate, fit, evaluate, export, verify
just net-deploy BOARD   # scp the result to /opt/snf/fatigue.onnx
```

Training runs on the **macOS host** (`uv`, MPS) — not in the Linux container.
The container is for compiling Rust; there is no GPU in it and no reason to put
PyTorch there. Only the 8 KB `.onnx` crosses over.

## What this is, and what it is not

There is no labelled fatigue data, so this model does not learn what fatigue is.
It learns a smooth approximation of **a rule we wrote down** (`snf_net/rule.py`)
from the cardiorespiratory literature. That is Stage A of a two-stage plan, and
it buys three things a hand-coded `if` ladder in Rust would not:

1. **The ONNX contract becomes real and exercised end to end.** Swapping in a
   model trained on actual recordings later is then a file copy and a `revision`
   bump in `Repose.toml` — no Rust change, no redeploy of the binary.
2. **The output is smooth.** A threshold ladder produces cliffs in the fatigue
   level, and `InflationController`'s derivative term amplifies cliffs into
   commanded duty. Every threshold in the rule is a Hermite smoothstep for this
   reason, and the network uses `Tanh` rather than `ReLU` so it does not
   reintroduce kinks while approximating them.
3. **Uncertainty falls out of the same function as the score** — see below.

What it is _not_ is evidence about anyone's actual fatigue. The numbers in
`out/metrics.json` measure how well the student reproduces the teacher. They say
nothing about whether the teacher is right. Treat the level as a plausible,
well-behaved index; do not treat it as a measurement.

## The pieces

| File                  | Role                                                                                                                                                             |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `snf_net/contract.py` | The interface: 14 features, 5 bins, window sizes. **Mirrored** in `crates/bridge/src/features.rs` and `crates/fatigue/src/lib.rs`; the three must move together. |
| `snf_net/rule.py`     | The teacher. Emits a _distribution_, not a number.                                                                                                               |
| `snf_net/sessions.py` | Session simulator + the windowing that mirrors the Rust extractor.                                                                                               |
| `snf_net/model.py`    | Normalization layer + 2×32 MLP + 5 logits. ~1.7k parameters.                                                                                                     |
| `train.py`            | KL fit, evaluation, checkpoint, then export.                                                                                                                     |
| `export.py`           | ONNX export at opset 17, verified against PyTorch.                                                                                                               |

## Why bins and not a regressed level

The head emits five ordinal logits. The runtime takes the **softmax expectation**
for the level and the **normalized entropy** for the confidence.

That matters because confidence is load-bearing: `snf_bridge::confidence` uses it
to decide whether the pneumatics may move at all. A separately-predicted
confidence head is a number the model is free to make up, and nothing ties it to
the level. Deriving both from one distribution means they cannot disagree — a
model that spreads its mass necessarily reports a middling level _and_ low
confidence, and the device correctly declines to act on it.

So the teacher's spread is trained as carefully as its mean. `rule.py` widens
`sigma` when:

- a channel is missing (respiration costs more than heart rate — it carries the
  most weight),
- the **channels disagree** — near-zero motion with a heart rate running above
  baseline is not a tired person, it is an unexplained one,
- the session is young and the personal baselines have not converged.

The constants are calibrated against what the runtime does with the result. With
bin centres 25 apart, confidence ≈ 0.99 at `sigma` 7, ≈ 0.8 at 13, ≈ 0.5 at 23,
≈ 0.3 at 34 — and the device's gates sit at 0.30 and 0.80. So:

| Situation                           | `sigma` | confidence | device does                         |
| ----------------------------------- | ------: | ---------: | ----------------------------------- |
| both channels, agreeing, warmed up  |       7 |      ~0.99 | acts in full                        |
| heart rate missing                  |      18 |      ~0.65 | acts, scaled down                   |
| respiration missing                 |      25 |      ~0.45 | acts, well scaled down              |
| both missing, or flat contradiction |     36+ |      <0.30 | does not act, sets `LOW_CONFIDENCE` |

## Why synthetic sessions rather than sampling the feature box

Uniform sampling over 14 dimensions spends most of the model's capacity on
combinations that cannot physically occur — a 40 bpm heart rate alongside a
4 breath/min-per-minute respiration slope — and leaves it thin where the device
actually operates. `sessions.py` instead simulates a latent fatigue trajectory,
generates plausible channels from it (including dropouts as _runs_, because a
lost track stays lost), and runs the same windowing the Rust side uses.

The generator's job is coverage of the realistic manifold, not correctness: the
student is fit to the teacher, and the teacher is evaluated pointwise.

## Deliberately absent: heart-rate variability

HRV is the textbook drowsiness marker, and it is not here. Real HRV needs
beat-to-beat intervals; the TI vital-signs demo hands over a smoothed _rate_ at
2 Hz. The `hr_std` feature is mostly estimator noise, and it is weighted
accordingly — it informs the spread, not the score. Adding HRV means changing
what the radar layer extracts, not what this model consumes.

## Retraining

The feature contract is pinned by tests on both sides:

- `crates/fatigue` — `input_order_matches_the_training_contract` pins field
  order, and `FatigueModel::load` checks the graph's input width and tensor
  names at start-up, so a stale model is a start-up error rather than
  silently-wrong verdicts.
- `crates/bridge/src/features.rs` — the window and baseline constants are _not_
  `Repose.toml` configuration, because they are baked into the weights.
  Changing one without retraining feeds the model a distribution it has never
  seen.

If you change `contract.py`, change both Rust mirrors in the same commit and
bump `[fatigue] revision` in `Repose.toml`.
