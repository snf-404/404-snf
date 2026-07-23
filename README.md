# 404-snf

A fatigue-sensing device on the **STM32MP257** — and a **landing evaluation of
the [consortium](../consortium) framework** (a real multi-core application built
to validate consortium's async inter-processor primitives in practice).

The device senses a person with a **TI IWR6843** mmWave radar, classifies
**fatigue level** with an **ONNX** model, drives **pneumatic actuators** through
the **CM33** real-time core, and exposes state over **BLE** to a phone /
Web-Bluetooth frontend.

> **Scaffold only.** Structure, manifests, crate skeletons, stub types, and READMEs
> are in place; there is no real implementation yet. The firm inputs are: STM32MP257
> following consortium's melt-pot example, **no TEE and no HMI**, tokio-serial + `cxx`
> for the radar, BlueZ for BLE, pneumatic control on the MCU, and an ONNX fatigue
> pipeline.

## Architecture

```
        IWR6843 ──serial──►  CA35 (Linux, tokio)                 CM33 (embassy)
                             ├─ snf-radar   (tokio-serial + cxx→TI mmWave SDK)
                             ├─ snf-fatigue (ONNX via consortium-ort)
                             ├─ snf-ble     (BlueZ / bluer) ──BLE──► apps/www (Nuxt)
                             └─ snf-app  ◄── actuator IPC channel ──► snf-mcu
                                                                       (pump PWM,
                                                                        valves, pressure)
```

Cross-core messaging uses consortium's typed IPC over shared memory, declared in
[`Consortium.toml`](Consortium.toml) (`actuator` channel; no `optee` endpoint,
no `[hmi]`).

## Layout

| Path | What |
| --- | --- |
| `crates/shared` | `snf-shared` — `IpcSafe` IPC message types |
| `crates/radar` | `snf-radar` — IWR6843: tokio-serial + `cxx` bridge to the TI mmWave SDK |
| `crates/ble` | `snf-ble` — BlueZ GATT peripheral via `bluer` |
| `crates/fatigue` | `snf-fatigue` — ONNX pipeline on `consortium-ort` |
| `crates/app` | `snf-app` — CA35 Linux endpoint (excluded from workspace, pipeline-built) |
| `crates/mcu` | `snf-mcu` — CM33 pneumatic firmware (excluded from workspace, pipeline-built) |
| `packages/` | JS/TS libraries (Vite+ / `vp`) — `@snf/protocol` |
| `apps/www` | Nuxt + Web Bluetooth frontend (**scaffolded externally**, not here) |
| `models/` | 3D models (enclosure, bladder housings, radar mount) |
| `hardware/pneumatics` | pump/valve/sensor BOM + CM33 pin mapping |

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

```bash
# Host-checkable libraries (radar cxx + fatigue ONNX are feature-gated off by default).
cargo check --workspace

# Endpoints: generate consortium.gen.rs + cross-compile app and mcu.
csti build --manifest Consortium.toml    # or: just build

# JavaScript workspace (Vite+).
vp install
vp dev
```

See [`justfile`](justfile) for the task shortcuts. Licensed under Apache-2.0.
