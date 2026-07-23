# Pneumatic actuation — BOM & control mapping

The CM33 core (`crates/mcu`) closes a pressure loop over inflatable bladders: it
drives a pump and a bank of solenoid valves, and reads bladder pressure. This is
an **eval-grade reference BOM** — parts are to be finalized against the
mechanical design; substitute equivalents freely.

## Bill of materials

| # | Function | Suggested part | Interface | Notes |
| - | --- | --- | --- | --- |
| 1 | Air pump | 6–12 V micro **diaphragm pump** (e.g. Skoocom SC3101PM class) | PWM | Speed set by CM33 TIM3 PWM through the driver below |
| 2 | Pump driver | **TI DRV8871** (single brushed DC, 3.6 A) | 2× logic | IN1/IN2 from CM33; integral current limit + thermal shutdown |
| 3 | Deflate / vent valves | 3-way **solenoid valve**, 5–12 V (latching preferred for low power) | on/off | one per actuator zone |
| 4 | Valve driver | **TI TPL7407L** (7-ch low-side, integrated clamp) | GPIO | drives the solenoids from CM33 GPIO; replaces ULN2003 |
| 5 | Pressure sensor | **Honeywell MPRLS** or **ABP2** series (I²C gauge) | I²C | closed-loop pressure feedback on CM33 I2C1 |
| 6 | Pump current sense | **TI INA219** or **INA240** (optional) | I²C / analog | stall / leak detection |
| 7 | Pump rail supply | buck converter (e.g. **TI TPS54331**) + bulk caps | — | isolate motor transients from logic rail |
| 8 | Protection | flyback/clamp diodes on pump + valves | — | TPL7407L integrates valve clamps; add one across the pump |
| 9 | Pneumatics | manifold, tubing, quick-connects, bladders | — | mechanical; sized with the enclosure in `models/` |

## CM33 peripheral mapping

Reflected in `Consortium.toml` `[peripheral.*]` (owner = `cm33`):

| Signal | STM32MP257 peripheral | Pin (placeholder) | Drives |
| --- | --- | --- | --- |
| Pump PWM | `TIM3_CH1` | `PA6` | DRV8871 IN1 (IN2 tied for one-direction PWM) |
| Valve 0..n | GPIO | TBD | TPL7407L inputs |
| Pressure read | `I2C1` (SDA `PB0`, SCL `PB1`) | — | MPRLS / ABP2 |

## Control sketch

`PneumaticCommand.target_pressure_kpa` sets the loop setpoint; the CM33 PI-controls
pump PWM against the MPRLS reading, opens the vent valve to deflate, and reports
`PneumaticStatus { pressure_kpa, pump_on, seq }` back to the CA35. None of this
is implemented yet — scaffold only.
