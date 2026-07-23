# snf-mcu — CM33 firmware

The real-time half of 404-snf: the pneumatic actuation controller. Receives
`PneumaticCommand` from the CA35 over the `actuator` IPC channel, drives the pump
and solenoid valves, reads bladder pressure, and returns `PneumaticStatus`.

Hardware BOM and pin mapping: [`hardware/pneumatics/README.md`](../../hardware/pneumatics/README.md).

## Build

Excluded from the workspace and built by the pipeline, not plain `cargo build`.
`csti build` generates `src/consortium.gen.rs`, then cross-compiles for
`thumbv8m.main-none-eabihf` (target pinned in `.cargo/config.toml`):

```bash
csti build --manifest ../../Consortium.toml
```

`src/consortium.gen.rs` is generated and git-ignored; `src/main.rs` `include!`s
it. Until the pipeline runs, this crate will not compile on its own — expected
for the scaffold. `memory.x` mirrors the STM32MP2 vendor linker carveouts.
