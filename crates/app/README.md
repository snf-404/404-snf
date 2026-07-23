# snf-app — CA35 Linux endpoint

The application-processor half of 404-snf. Orchestrates radar → fatigue → BLE and
exchanges `PneumaticCommand`/`PneumaticStatus` with the CM33 over the `actuator`
IPC channel.

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
