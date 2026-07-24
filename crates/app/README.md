# snf-app — CA35 Linux endpoint

The application-processor half of 404-snf, and where essentially everything runs.
It reads the IWR6843 over its USB virtual COM port with `tokio-serial`
(`RADAR_DATA_PORT`, default `/dev/ttyACM1`), runs indicators → ONNX fatigue →
BLE, and drives the pneumatics: pump on `TIM4_CH2`, vent valve on `TIM5_CH1`,
both through FR120N MOSFET modules and Linux sysfs PWM.

The CM33's USART6 front-end is reachable over the `radar` IPC channel but is not
the default source — this binary only pings it once at start-up as a link check.

The CM33 holds no actuator line, so there is no independent interlock behind the
pneumatics. Fail-safety comes from a normally-open vent valve plus the
`MAX_INFLATE` ceiling in the control loop — see
[`hardware/pneumatics/README.md`](../../hardware/pneumatics/README.md).

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
