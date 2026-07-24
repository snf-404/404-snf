# snf-mcu — CM33 firmware

The real-time half of 404-snf: a **USART6 radar front-end**.

> **Not the default data path.** The IWR6843 is wired over its USB virtual COM
> port, so the radar is a Linux tty and `snf-app` reads it with `tokio-serial`.
> This firmware is what runs when the sensor is wired to `PF13`/`PF14` instead —
> RIF reaches USART6 from the CM33 alone, so that path has to live here. In the
> virtual-COM configuration `snf-app` only pings the channel once at start-up as
> a link check, and this core parks with `streaming = false`.

The board's RIF configuration reaches USART6 only from the CM33, and reaches
TIM4/TIM5 only from the CA35. 404-snf takes that split as given rather than
re-provisioning it, so this core holds no actuator line — the pneumatics run
entirely on Linux (see [`crates/app`](../app)).

The firmware parses on-core. Bytes off USART6 go through `PacketAssembler` to
recover whole packets, then `parse_report` reduces each packet's TLVs to a
fixed-size `RadarReport`: up to 32 detections in millimetres, plus the frame's
aggregates (nearest range, moving-point count, mean speed). Both live in
[`snf-shared`](../shared)'s `detect` module — the `no_std` subset of `snf-radar`,
kept beside the wire type so the two cannot drift, and host-testable with
`cargo test -p snf-shared`.

Raw point clouds never cross the IPC boundary: a frame is ~2 KB of `f32`, the
report is ~400 B of integers the CA35's indicator engine can use directly.

The transfer is a strict pull — the CA35 sends a `RadarControl`, this core
answers with the newest report or one marked `fresh = false`. Backpressure lands
in the UART ring, which is what it is for; `RadarReport::dropped` and
`::overrun` say when a frame was lost anyway.

## What is generated and what is not

`csti` emits bring-up for I2C and GPIO blocks only, so the `[peripheral.usart6]`
entry in `Consortium.toml` is **declarative**: it records ownership, but no
`context.peripherals.usart6` handle appears. `main.rs` constructs the
`BufferedUart` by hand from the chip map's base (`0x4022_0000`) and interrupt
(`136`), the same way the `IPCC1_RX` vector already hardcodes the validated IPCC1
base. The pins are already muxed to USART6 for this core, so nothing here
configures them.

One value is worth checking against your board before blaming the parser:
**`USART6_KERNEL_CLOCK_HZ`** is the reset default (HSI, 64 MHz). If the clock
tree reparents `ck_ker_usart6` before the CM33 starts, the baud divider is wrong
and every byte arrives as a framing error.

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
