# Pneumatic actuation — BOM & control mapping

The CM33 core (`crates/mcu`) closes a pressure loop over an inflatable bladder:
it drives a pump and a vent valve, and reads bladder pressure.

**Optimized for a 72-hour hackathon:** off-the-shelf breakout modules, no custom
PCB, minimal soldering, parts that ship next-day / are in a typical maker bin.
Start single-zone; add zones later by repeating the valve + driver channel.

## Minimum viable BOM (single zone)

| # | Function | Part (hackathon pick) | Interface | ~Qty · Cost | Why this one |
| - | --- | --- | --- | --- | --- |
| 1 | Air pump | 6 V mini **diaphragm air pump** (blood-pressure-cuff type, e.g. Skoocom / "KPM27C" class) | PWM | 1 · $5–8 | Ubiquitous on Amazon; runs off a MOSFET channel |
| 2 | Vent valve | 6 V **2-way solenoid air valve** (BP-monitor release valve) | on/off | 1 · $3–5 | Same ecosystem as the pump; tiny, cheap |
| 3 | Driver | **Dual TB6612FNG breakout** (SparkFun/Pololu) *or* 2× **IRF520 MOSFET module** | GPIO + PWM | 1 · $3–8 | Logic-level, no soldering IC, one board drives pump **and** valve |
| 4 | Pressure sensor | **Adafruit MPRLS breakout** (0–25 psi, I²C, STEMMA/Qwiic) | I²C | 1 · $12 | Plug-and-play I²C, no analog front-end to build |
| 5 | Power | 6–12 V wall adapter or LiPo **+ MP1584 buck module** for the pump rail | — | 1 · $2 | Module, not a designed regulator; isolates motor noise |
| 6 | Pneumatics | silicone tubing + T-fitting + bladder/cuff/balloon | — | — · $5 | Whatever's on the bench; a BP cuff is a ready bladder |

**Bench total ≈ $30–40**, all modules, all solderless-friendly (Qwiic/JST + Dupont).

### Deliberately dropped for the hackathon
- **Bare driver ICs** (DRV8871 / TPL7407L) → replaced by the TB6612 / MOSFET
  module so there's no IC-level PCB work.
- **Current sense** (INA219/INA240) → skip; detect problems in software via the
  pressure trace instead.
- **Designed buck** (TPS54331) → use the MP1584 module.

### If time permits (stretch)
- Second/third zone: add one MOSFET channel + one solenoid per zone; bump
  `PneumaticCommand.actuator_mask`.
- Latching solenoid + driver for lower idle power (battery runtime).
- Proportional valve for smooth pressure control instead of bang-bang venting.

## CM33 peripheral mapping

Reflected in `Consortium.toml` `[peripheral.*]` (owner = `cm33`):

| Signal | STM32MP257 peripheral | Pin (placeholder) | Drives |
| --- | --- | --- | --- |
| Pump PWM | `TIM3_CH1` | `PA6` | TB6612 `PWMA` / MOSFET-module gate |
| Vent valve | GPIO | TBD | TB6612 `AIN`/second MOSFET module |
| Pressure read | `I2C1` (SDA `PB0`, SCL `PB1`) | — | MPRLS breakout |

## Control sketch

`PneumaticCommand.target_pressure_kpa` sets the loop setpoint; the CM33 bang-bang
(or PI) controls pump PWM against the MPRLS reading, opens the vent valve to
deflate, and reports `PneumaticStatus { pressure_kpa, pump_on, seq }` back to the
CA35. Not implemented yet — scaffold only.
