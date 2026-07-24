# 404-snf

A fatigue-sensing device on the **STM32MP257** — and a **landing evaluation of
the [consortium](../consortium) framework** (a real multi-core application built
to validate consortium's async inter-processor primitives in practice).

The device senses a person with a **TI IWR6843** mmWave radar, classifies
**fatigue level** with an **ONNX** model, drives **pneumatic actuators** through
the **CM33** real-time core, and exposes state over **BLE** to a Lingguang flash
app.

> **Scaffold only.** Structure, manifests, crate skeletons, stub types, and READMEs
> are in place; there is no complete product implementation yet. The firm inputs
> are: STM32MP257 following consortium's melt-pot example, **no TEE and no HMI**,
> pure-Rust tokio-serial radar handling, BlueZ for BLE, pneumatic control on the
> MCU, and an ONNX fatigue pipeline.

## Architecture

```
        IWR6843 ──serial──►  CA35 (Linux, tokio)                 CM33 (embassy)
                             ├─ snf-radar   (pure-Rust UART + TLV indicators)
                             ├─ snf-fatigue (ONNX via consortium-ort)
                             ├─ snf-ble     (BlueZ / bluer) ──BLE──► apps/lingguang
                             └─ snf-app  ◄── actuator IPC channel ──► snf-mcu
                                                                       (pump PWM,
                                                                        valves, pressure)
```

Cross-core messaging uses consortium's typed IPC over shared memory, declared in
[`Consortium.toml`](Consortium.toml) (`actuator` channel; no `optee` endpoint,
no `[hmi]`).

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
# Host-checkable libraries (radar vitals + fatigue ONNX are feature-gated off by default).
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
