# Pneumatic actuation — BOM & control mapping

The **CA35** (`crates/app`) drives an inflatable bladder: a pump inflates it and a
solenoid vents it, both switched by **FR120N** low-side MOSFET modules off a 24 V
rail, both PWM'd from Linux-owned timers.

> **Why the CA35 and not the CM33.** The board's RIF configuration reaches TIM4
> and TIM5 — the only PWM-capable timers whose channels land on the 40-pin
> connector — from the application processor only. 404-snf takes that as given
> rather than re-provisioning RIF, so the actuators hang off Linux. See
> [`Consortium.toml`](../../Consortium.toml).

**Optimized for a 72-hour hackathon:** off-the-shelf breakout modules, no custom
PCB, minimal soldering, parts that ship next-day / are in a typical maker bin.
Start single-zone; add zones later by repeating the valve + driver channel.

## Minimum viable BOM (single zone)

| #   | Function        | Part (hackathon pick)                                            | Interface | ~Qty · Cost | Why this one                                                    |
| --- | --------------- | ---------------------------------------------------------------- | --------- | ----------- | --------------------------------------------------------------- |
| 1   | Air pump        | 24 V **diaphragm air pump**                                      | PWM       | 1 · $10–18  | Enough head to inflate a cuff-sized bladder in a few seconds    |
| 2   | Vent valve      | 24 V **2-way solenoid air valve**, _normally open_               | on/off    | 1 · $6–10   | Normally-open is what makes power loss fail safe — see below    |
| 3   | Driver          | 2× **FR120N MOSFET trigger switch module**                       | PWM in    | 2 · $2 ea   | 100 V / 9.2 A part, so the 24 V rail is not a problem           |
| 4   | Flyback diodes  | **SS34** or **1N5819**, one across each load                     | —         | 2 · $0.20   | Both loads are inductive; without these the MOSFETs die         |
| 5   | Pressure sensor | **Adafruit MPRLS breakout** (0–25 psi, I²C, STEMMA/Qwiic)        | I²C       | 1 · $12     | Plug-and-play I²C, no analog front-end to build (not wired yet) |
| 6   | Power           | 24 V wall adapter **+ buck module** for the 5 V/3.3 V logic rail | —         | 1 · $4      | Module, not a designed regulator; isolates motor noise          |
| 7   | Pneumatics      | silicone tubing + T-fitting + bladder/cuff/balloon               | —         | — · $5      | Whatever's on the bench; a BP cuff is a ready bladder           |

**Bench total ≈ $45–55**, all modules, all solderless-friendly (Qwiic/JST + Dupont).

### Why FR120N and not the TB6612FNG

The scaffold started on a **TB6612FNG** dual H-bridge. Its motor supply is rated
to **13.5 V**, and the pneumatics run on **24 V** — so it is out, regardless of
current headroom. The FR120N is a plain N-channel MOSFET (100 V, 9.2 A) sold as a
solderless "trigger switch" breakout, which carries the rail comfortably.

What changes in the control surface:

| TB6612FNG (was)                    | FR120N module (now)            |
| ---------------------------------- | ------------------------------ |
| `PWM` + `IN1`/`IN2` + `STBY`       | one PWM/trigger input per load |
| forward / reverse / brake / coast  | on / off only                  |
| one chip drives pump **and** valve | one module per load — buy two  |

Both loads are unidirectional, so nothing useful is lost. What _is_ lost is the
H-bridge's electrical brake; a diaphragm pump stops on its own friction, so that
does not matter here.

### Two things to get right before powering up

1. **Gate drive.** The FR120N is **not** a logic-level MOSFET: `V_GS(th)` is
   specified as a 2–4 V range and its on-resistance is characterized at
   `V_GS = 10 V`. A module that wires its trigger input straight to the gate will
   be only partially enhanced by the CA35's 3.3 V logic — it will pass a few
   hundred milliamps and then get hot, which on the bench looks exactly like an
   underpowered pump. Check your board. If it has no gate driver or level
   shifter, either drive the gate from 12 V through a small NPN stage, or verify
   at the pump's real stall current that the module stays cool.
2. **Flyback diodes.** The module switches the low side, so when it turns off,
   the load's inductance drives its return node **up**, toward and past +24 V.
   Put an SS34/1N5819 across each load, cathode to +24 V. Many FR120N boards ship
   without one.

Also worth knowing while probing: because these are low-side switches, an "off"
load does not sit at ground — it floats at +24 V. And the 24 V and logic grounds
must be common, or the gates have no reference.

### Deliberately dropped for the hackathon

- **Bare driver ICs** (DRV8871 / TPL7407L) → the FR120N module needs no IC-level
  PCB work.
- **Current sense** (INA219/INA240) → skip; detect problems in software via the
  pressure trace instead.
- **Designed buck** (TPS54331) → use a module.

### If time permits (stretch)

- Wire the MPRLS and close a real pressure loop instead of the open-loop
  inflate-then-hold timer in `crates/app`.
- Second/third zone: one more FR120N module + solenoid per zone.
- Latching solenoid for lower idle power (battery runtime).
- Proportional valve for smooth pressure control instead of bang-bang venting.

## CA35 peripheral mapping

Reflected in [`Consortium.toml`](../../Consortium.toml) `[peripheral.*]`
(owner = `ap`), and enabled in the device tree by
[`hardware/dts/`](../dts/README.md):

| Signal        | STM32MP257 peripheral | Pin   | Connector pin | Drives                     |
| ------------- | --------------------- | ----- | ------------- | -------------------------- |
| Pump PWM      | `TIM4_CH2`            | `PA1` | 33            | FR120N module #1 trigger   |
| Vent valve    | `TIM5_CH1`            | `PH8` | 31            | FR120N module #2 trigger   |
| Pressure read | `I2C2` (`PF0`/`PF2`)  | —     | 27 / 28       | MPRLS breakout (not wired) |

Each timer's `pwm-stm32` provider appears as **its own** `/sys/class/pwm/pwmchipN`
with one channel, so the two loads are told apart by _chip_ index, not channel
index. Those indices come from probe order and are not stable across kernel or
device-tree changes — `PneumaticConfig` in `crates/app/src/pneumatics.rs` carries
them so they can be corrected without touching the control logic. To identify
them on a live board:

```bash
for chip in /sys/class/pwm/pwmchip*; do echo "$chip -> $(readlink -f "$chip/device")"; done
```

`40020000.timer` is TIM4 (pump); `40030000.timer` is TIM5 (valve).

## Fail-safe behaviour

The vent valve **must be normally open**: de-energized it vents the bladder,
energized it seals so the pump can build pressure. That polarity is what makes
losing control fail safe, and it matters more in this design than it did in the
scaffold — the CM33 is busy owning the radar UART and holds **no actuator line**,
so there is no independent interlock behind the software. (The CM33 keeps USART6,
but that port carries no actuator line either.)

What covers what:

| Failure                      | What stops the inflation                                       |
| ---------------------------- | -------------------------------------------------------------- |
| Fatigue alert stuck asserted | `MAX_INFLATE` (6 s) ceiling in `crates/app`, then hold         |
| Clean exit / IPC loss        | The app vents explicitly before returning                      |
| Panic that unwinds           | `Pneumatics`/`SysfsPwm` `Drop`: vent, disable, unexport        |
| `SIGKILL`, app crash         | Channels unexported by the kernel; pins idle low → valve opens |
| Board power loss             | Valve de-energizes → bladder vents                             |

The one gap is a `SIGKILL` landing mid-inflation, which leaves the bladder at
whatever pressure it had reached until the process is restarted or power drops.
`MAX_INFLATE` bounds how much air that can be.

## Control sketch

`crates/app` runs open-loop today: when the fatigue verdict crosses
`FATIGUE_ALERT_LEVEL`, it seals the valve and runs the pump at
`PUMP_DUTY_PERCENT` for up to `MAX_INFLATE`, then holds; when the verdict falls
back, it vents. Adding the MPRLS turns that into a real setpoint loop —
bang-bang or PI against the measured pressure — without changing the driver
layer.
