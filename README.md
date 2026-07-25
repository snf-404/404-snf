# 404-snf

A fatigue-sensing device on the **STM32MP257** — and a **landing evaluation of
the [consortium](../consortium) framework** (a real multi-core application built
to validate consortium's async inter-processor primitives in practice).

The device senses a person with a **TI IWR6843** mmWave radar, classifies
**fatigue level** with an **ONNX** model, drives **pneumatic actuators** from the
**CA35**, and exposes state over **BLE** to a phone / Web-Bluetooth frontend.

> **Scaffold only.** Structure, manifests, crate skeletons, stub types, and READMEs
> are in place; there is no complete product implementation yet. The firm inputs
> are: STM32MP257 following consortium's melt-pot example, **no TEE and no HMI**,
> pure-Rust tokio-serial radar handling, BlueZ for BLE, an ONNX fatigue pipeline,
> and the RIF-imposed placement of the pneumatics on the CA35 (TIM4/TIM5).

## Architecture

The IWR6843 connects over its **USB virtual COM port**, so both of its UARTs are
ordinary Linux ttys and the sensing pipeline is entirely on the CA35: the sensor
boots idle, `snf-radar` sends it a configuration profile over the 115 200-baud
CLI port at start-up, and only then does the 921 600-baud data port stream TLVs.
The actuators are there too, because the board's RIF configuration — which this
project deliberately does **not** re-provision — reaches **TIM4/TIM5, the only
PWM-capable timers that land on the 40-pin connector, from the AP alone**:

```
     IWR6843 ──cfg 115k2──►  CA35 (Linux, tokio)                 CM33 (embassy)
            ◄──data 921k6──  ├─ snf-radar   (pure-Rust UART + TLV indicators)
                             ├─ snf-fatigue (ONNX via consortium-ort)
                             ├─ snf-ble     (BlueZ / bluer) ──BLE──► apps/lingguang
                             └─ snf-app  ◄── actuator IPC channel ──► snf-mcu
                                                                       (pump PWM,
                                                                        valves, pressure)
```

The CM33 owns USART6 — RIF reaches that port from the CM33 alone — and carries a
`no_std` TLV parser that reports fixed-size `RadarReport`s over the `radar` IPC
channel, for a build where the sensor is wired to those pins instead of USB. With
virtual COM there is nothing on USART6 to read, so `snf-app` only uses the
channel as an **opt-in** start-up link check (`[mcu] link_check`, off by
default): a CM33 that never answers is not a degraded board, and the CA35 half
runs unchanged without it.

Cross-core messaging uses consortium's typed IPC over shared memory, declared in
[`Consortium.toml`](Consortium.toml) (`radar` channel; no `optee` endpoint, no
`[hmi]`).

The CM33 holds no actuator line, so there is no independent interlock behind the
pneumatics; fail-safety comes from a normally-open vent valve plus an inflation
ceiling in the control loop. See
[`hardware/pneumatics/README.md`](hardware/pneumatics/README.md).

## Layout

| Path                  | What                                                                          |
| --------------------- | ----------------------------------------------------------------------------- |
| `crates/shared`       | `snf-shared` — `IpcSafe` IPC message types                                    |
| `crates/radar`        | `snf-radar` — pure-Rust IWR6843 UART, TLV parser, and indicators              |
| `crates/ble`          | `snf-ble` — BlueZ GATT peripheral via `bluer`                                 |
| `crates/fatigue`      | `snf-fatigue` — ONNX pipeline on `consortium-ort`                             |
| `crates/app`          | `snf-app` — CA35 Linux endpoint (excluded from workspace, pipeline-built)     |
| `crates/mcu`          | `snf-mcu` — CM33 pneumatic firmware (excluded from workspace, pipeline-built) |
| `apps/lingguang`      | React + TypeScript Lingguang flash app                                        |
| `models/`             | 3D models (enclosure, bladder housings, radar mount)                          |
| `hardware/pneumatics` | pump/valve/sensor BOM + CM33 pin mapping                                      |
| `tools/deploy.sh`     | put a built `dist/` on a board: model, app, config, CM33 reload, systemd unit |

`crates/app` and `crates/mcu` are **excluded from the cargo workspace**: each
`include!`s a `consortium.gen.rs` emitted by `csti build` and cross-compiles into
its own `target/` — the same arrangement as consortium's melt-pot example.

## Dependencies

- **consortium** — path dependency on the sibling checkout at `../consortium`
  (see `[workspace.dependencies]` in `Cargo.toml`). Switching to a git dependency
  is confined to that block.
- **`csti`** — the consortium build CLI (from `../consortium/crates/consortium-cli`),
  needed to build the endpoints.
- Rust targets: `aarch64-unknown-linux-gnu` (CA35) and `thumbv8m.main-none-eabihf` (CM33).

## Build

Host checks run natively on macOS; anything Linux-specific (the CA35 app's
`bluer`/UIO links, the `csti` endpoint build, the mmWave SDK) runs in the dev
container.

```bash
# Host-checkable libraries. Radar vital signs are on by default (the sensors are
# flashed with that firmware); fatigue ONNX inference is still feature-gated off.
cargo check --workspace

# Repository-level JavaScript tooling (Vite+).
vp install

# Vite+ orchestrates npm ci plus the official Lingguang scaffold checks.
vp run -w check:lingguang
vp run -w dev:lingguang
```

### Dev container (Apple `container`)

Mirrors consortium's `Dockerfile.linux` approach: Apple `container` boots a
**native arm64 Linux VM**, so `aarch64-unknown-linux-gnu` builds natively and
`Consortium.toml`'s `sysroot = "/"` resolves to the image root — no Yocto SDK.
Because 404-snf path-depends on `../consortium`, the container mounts **both**
repos under `/work` (this repo at `/work/404-snf`, consortium at
`/work/consortium`).

```bash
just container-image          # build the image (Rust toolchains + build deps)
just container-csti-install   # install `csti` into the cargo volume (once)
just container-build          # csti build --manifest Consortium.toml  (app + mcu)

just snf cargo check --workspace   # run any command in the container
just container-shell               # interactive shell
just container-clean               # drop the cached build volumes
```

- **consortium submodules.** If `csti build` needs consortium's vendored sources
  (e.g. embassy), run `just init` / `just pac-init` in `../consortium` first.
- **DNS.** Behind a VPN with fake-ip DNS, set `SNF_LINUX_DNS` (default `1.1.1.1`).

See [`justfile`](justfile) for all task shortcuts. Licensed under Apache-2.0.
