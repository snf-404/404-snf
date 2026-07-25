# snf-app — CA35 Linux endpoint

The application-processor half of 404-snf, and where essentially everything runs.
It reads the IWR6843 over its USB virtual COM port with `tokio-serial` (default
`/dev/ttyACM1`), runs indicators → ONNX fatigue → BLE, and drives the
pneumatics: **two symmetric sections**, one on `TIM4_CH2` and one on `TIM5_CH1`,
each through its own FR120N MOSFET module and Linux sysfs PWM.

The CM33's USART6 front-end is reachable over the `radar` IPC channel but is not
the default source — this binary only pings it once at start-up as a link check.

The CM33 holds no actuator line, so there is no independent interlock behind the
pneumatics. Fail-safety comes from a normally-open vent valve plus the
`MAX_INFLATE` ceiling in the control loop — see
[`hardware/pneumatics/README.md`](../../hardware/pneumatics/README.md).

## Fatigue model

`snf-fatigue` runs a small ONNX graph through `ort`, loaded from
`[fatigue] model_path` (default `/opt/snf/fatigue.onnx`). It is built by
[`ml/`](../../ml/README.md) — `just ml-train --data data/recordings.csv`, then
`just ml-deploy root@board`.

The graph emits one `0..100` fatigue score. Confidence is derived independently
from rate-channel availability and personal-baseline warm-up, then decides how
much of the verdict reaches the pneumatics (`snf_bridge::confidence`):
below 0.30 the verdict is withheld entirely and published with `LOW_CONFIDENCE`;
between 0.30 and 0.80 the level is scaled by a smooth logistic, so less certainty
means a gentler command; above 0.80 it passes through untouched.

Without a model — or in a build without the `ort` feature — the crate returns a
zero-confidence stub, which is below the action floor. Such a build publishes
telemetry and never actuates, which is the right behaviour for one that cannot
see.

## Configuration — `Repose.toml`

Where the hardware is on a particular board is read at start-up from a
`Repose.toml` in **the directory holding the binary** — `/opt/snf/app/Repose.toml`
under the shipped systemd unit — not the working directory. It covers the radar
tty/baud/protocol, each section's `pwmchip` index and channel plus the shared
PWM frequency, the ONNX model path and revision, and the BlueZ adapter.

[`Repose.toml`](Repose.toml) in this crate is the checked-in template, with every
key at its compiled-in default and the reasoning beside it. Copy it to the board
and delete what you are not changing:

```bash
scp crates/app/Repose.toml root@board:/opt/snf/app/Repose.toml
```

The two failure modes are deliberately different. **No file** is normal: the
binary logs the path it looked at and runs on the defaults, exactly as it did
before the file existed. **A file it cannot honour** — an unknown key, a
`pwm_hz` of zero, an empty path — is logged with its line number and stops
start-up, because a misspelt key quietly reverting to a default is the failure
you find on the mat rather than in the journal.

Everything there is a wiring fact. The bench-calibrated control values (neutral
duty, charge ceiling, per-mode budgets) are not configurable and still live in
`InflationParams` in
[`crates/bridge/src/inflation.rs`](../bridge/src/inflation.rs).

## Build

This crate is **not** built by plain `cargo build` and is excluded from the
workspace. `csti build` generates `src/consortium.gen.rs` (UIO bring-up, IPC
transceivers, defmt console) from `../../Consortium.toml`, then cross-compiles
for `aarch64-unknown-linux-gnu`:

```bash
csti build --manifest ../../Consortium.toml
```

`src/consortium.gen.rs` is generated and git-ignored; `src/main.rs` `include!`s
it. Until the pipeline runs, this crate will not compile on its own — that is
expected for the scaffold.
