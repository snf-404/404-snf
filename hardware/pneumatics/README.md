# Pneumatic actuation — BOM & control mapping

The **CA35** (`crates/app`) drives **two symmetric inflatable sections**, one per
Linux-owned timer channel, each switched by its own **FR120N** low-side MOSFET
module off a 24 V rail. The two are interchangeable — same part, same 40 Hz, same
duty — and the software drives them in lockstep.

> **There is no equilibrium state.** While a section runs, every period both
> inhales (line high, valve sealed, air pushed in) and exhales (line low, valve
> open, section pushing air out). The **duty is the intake/exhaust ratio**, not a
> throttle, and the _net_ of the two is what inflates or deflates:
>
> ```text
>   duty 0 ─────────── neutral ─────────── duty 100
>   all exhaust      in ≈ out           all intake
> ```
>
> Two things follow. The **neutral duty** — wherever a section's supply balances
> its valve's orifice — is a bench measurement, not a constant; everything in the
> control model is relative to it, so calibrate it first (inflate to a
> comfortable pressure, then sweep duty until the section neither firms up nor
> sags over ~30 s). Do both sections and confirm they land on the same number:
> the control model assumes one neutral for both, so if they differ, trim it in
> `crates/app/src/pneumatics.rs` rather than splitting the model. And **air is
> being moved continuously** whenever the mat is active, including while it is
> nominally holding — that is inherent to having no equilibrium, not a bug to
> optimize away.

> **Why the CA35 and not the CM33.** The board's RIF configuration reaches TIM4
> and TIM5 — the only PWM-capable timers whose channels land on the 40-pin
> connector — from the application processor only. 404-snf takes that as given
> rather than re-provisioning RIF, so the actuators hang off Linux. See
> [`Consortium.toml`](../../Consortium.toml).

**Optimized for a 72-hour hackathon:** off-the-shelf breakout modules, no custom
PCB, minimal soldering, parts that ship next-day / are in a typical maker bin.
Two sections is the baseline, not a stretch — it is what the two available timer
channels carry.

## Minimum viable BOM (two symmetric sections)

| #   | Function        | Part (hackathon pick)                                            | Interface    | ~Qty · Cost | Why this one                                                      |
| --- | --------------- | ---------------------------------------------------------------- | ------------ | ----------- | ----------------------------------------------------------------- |
| 1   | Section valve   | 24 V **2-way solenoid air valve**, _normally open_               | PWM 20–50 Hz | 2 · $6–10   | One per section; normally-open is what makes power loss fail safe |
| 2   | Driver          | **FR120N MOSFET trigger switch module**                          | PWM in       | 2 · $2 ea   | 100 V / 9.2 A part, so the 24 V rail is not a problem             |
| 3   | Air supply      | 24 V **diaphragm air pump** — _substituted, see below_           | on/off       | 1 · $10–18  | Shared by both sections through a T; see the note below           |
| 4   | Flyback diodes  | **SS34** or **1N5819**, one across each load                     | —            | 3 · $0.20   | Every load here is inductive; without these the MOSFETs die       |
| 5   | Pressure sensor | **Adafruit MPRLS breakout** (0–25 psi, I²C, STEMMA/Qwiic)        | I²C          | 1 · $12     | Plug-and-play I²C, no analog front-end to build (not wired yet)   |
| 6   | Power           | 24 V wall adapter **+ buck module** for the 5 V/3.3 V logic rail | —            | 1 · $4      | Module, not a designed regulator; isolates motor noise            |
| 7   | Pneumatics      | silicone tubing + T-fittings + two bladders/cuffs                | —            | — · $8      | Whatever's on the bench; a pair of BP cuffs is ready-made         |

**Bench total ≈ $55–70**, all modules, all solderless-friendly (Qwiic/JST + Dupont).

> **The pump has no timer channel left.** TIM4_CH2 and TIM5_CH1 are the only
> PWM-capable channels on the connector, and both now carry a section. The
> supply is therefore not something `crates/app` can modulate — it is either
> wired always-on with the mat, or switched through a third FR120N from a plain
> GPIO. Nothing in the software assumes it can throttle the pump; the sections'
> valves do all the metering. Two consequences to design around: the supply must
> have enough head for **both** sections at their fastest duty at once, and a
> pump that runs whenever the mat is powered is a pump whose noise floor is
> always present.

### As built: the pump is a manual one

The 24 V pump above was **missed in the parts order**, so the bench build uses a
small everyday hand/foot pump instead, operated by a person. That is a real
deviation, not a simplification, and it is worth being precise about what it does
and does not invalidate — the substitution sits exactly where the design already
had no control authority.

What still holds, unchanged:

- **Everything the software does.** The sections' valves do all the metering, and
  they are still on their own timer channels at 40 Hz. `crates/app` never had a
  handle on the supply, so there is nothing to stub out or `#[cfg]` away — the
  code that runs on the bench is the code that ships.
- **The whole control model.** Duty-as-ratio, the piecewise modes, hysteresis,
  the budgets and the charge ledger are all defined relative to a supply that is
  simply _present_. A hand pump is a supply that is present.
- **Fail-safety.** It arguably improves: the normally-open valves still vent on
  any loss of control, and now there is no powered source that can keep pushing
  air into a section while the software is wedged.

What does not, and what to expect on the bench:

- **Supply pressure is neither constant nor sustained.** It sags between strokes
  and stops entirely when whoever is pumping stops. Since duty selects a _ratio_
  and not a flow, every kPa/s in the model scales with whatever the supply is
  doing at that moment — so inflation timings will not match the model's
  budgets, and they will vary stroke to stroke.
- **`neutral_duty` cannot be calibrated this way.** The calibration in the note
  above is a steady-state measurement: sweep duty until a section neither firms
  up nor sags. With a hand pump there is no steady state to find. Whatever
  number the bench produces is a placeholder against a placeholder, and it must
  be re-derived once a real pump is in line.
- **Anything time-based is demonstrative only.** The 3–5 s cradle formation and
  the 10/5/15 s budgets assume a supply that delivers continuously. Treat a
  bench run as showing the _shape_ of the behaviour — that the modes sequence,
  that hysteresis holds through a look-up, that the breath alternates — not its
  timing.
- **`max_charge` is not being exercised.** The charge ledger bounds
  `∫ (duty − neutral) dt`, which is a proxy for delivered air only when supply is
  roughly constant. Under a hand pump it bounds nothing physical, so the ceiling
  is currently untested. Re-derive it against the real pump **before** the first
  powered run — that ceiling is most of the over-inflation protection, and a
  hand-pumped bench cannot tell you whether the number is right.

The fix is a purchase, not a redesign: put the 24 V pump in line, wire it
always-on or behind a third FR120N, and the first three bullets above resolve
themselves. Then redo `neutral_duty` and `max_charge`.

### Why FR120N and not the TB6612FNG

The scaffold started on a **TB6612FNG** dual H-bridge. Its motor supply is rated
to **13.5 V**, and the pneumatics run on **24 V** — so it is out, regardless of
current headroom. The FR120N is a plain N-channel MOSFET (100 V, 9.2 A) sold as a
solderless "trigger switch" breakout, which carries the rail comfortably.

What changes in the control surface:

| TB6612FNG (was)                   | FR120N module (now)            |
| --------------------------------- | ------------------------------ |
| `PWM` + `IN1`/`IN2` + `STBY`      | one PWM/trigger input per load |
| forward / reverse / brake / coast | on / off only                  |
| one chip drives two loads         | one module per load — buy two  |

Every load here is a unidirectional solenoid, so nothing useful is lost. What
_is_ lost is the H-bridge's electrical brake, which a valve armature has no use
for.

### Three things to get right before powering up

1. **Valve PWM frequency: 20–50 Hz.** This is the one frequency here that is a
   constraint rather than a preference, and it runs opposite to the usual
   instinct. The valve is not being switched somewhere the solenoid _cannot_
   follow — it is being switched somewhere it **must** follow, because its duty
   is what sets the intake/exhaust ratio. The armature has to complete a full
   open-close cycle every period. Below ~20 Hz the pulsing is audible and
   palpable as individual clicks; above ~50 Hz the valve cannot keep up, sits
   part-open, and the mapping from duty to net flow quietly stops being
   monotonic — the loop still commands, the section just no longer responds the
   way the control model assumes. `crates/app/src/pneumatics.rs` ships 40 Hz on
   both channels; drop toward 20 Hz if your valves are slower, and keep the two
   the same — halves breathing at slightly different rates is exactly the beat
   frequency a person notices. Check it with a scope on the gate _and_ an ear on
   the valve: a solenoid that is tracking sounds like a buzz, one that is not
   sounds like a hiss.
2. **Gate drive.** The FR120N is **not** a logic-level MOSFET: `V_GS(th)` is
   specified as a 2–4 V range and its on-resistance is characterized at
   `V_GS = 10 V`. A module that wires its trigger input straight to the gate will
   be only partially enhanced by the CA35's 3.3 V logic — it will pass a few
   hundred milliamps and then get hot, which on the bench looks exactly like an
   underpowered actuator. Check your board. If it has no gate driver or level
   shifter, either drive the gate from 12 V through a small NPN stage, or verify
   at the load's real current that the module stays cool.
3. **Flyback diodes.** The module switches the low side, so when it turns off,
   the load's inductance drives its return node **up**, toward and past +24 V.
   Put an SS34/1N5819 across each load, cathode to +24 V. Many FR120N boards ship
   without one. This matters more than it did when the valves were parked: they
   now switch 40 times a second, on both channels, for as long as the mat is
   active — so a missing diode is not an occasional transient but a continuous
   one.

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

- Wire the MPRLS and close a real pressure loop around the open-loop duty model
  in `crates/bridge/src/inflation.rs`. This is the one that buys the most: it
  turns the neutral duty from a calibrated guess into a measurement, makes the
  breath net-neutral by construction instead of by symmetry, and retires the
  charge ceiling as the only over-inflation bound.
- Drive the sections **independently** — a cradle that rises under one forearm
  before the other, or a canopy that leans away from the window. The wiring
  already supports it and `Pneumatics` is already per-section; what has to change
  is `PneumaticState::Cycle` carrying a duty per section, and the control model
  gaining some notion of which side to favour.
- A third/fourth section, which is where this stops fitting: the connector has no
  more PWM-capable channels, so more sections mean an I²C or SPI PWM expander
  (PCA9685) rather than one more timer.
- Proportional valve, which would replace PWM-ing a bang-bang valve with an
  actual analog orifice — quieter, no armature cycling, and no upper frequency
  limit to respect.
- Latching solenoid for lower idle power — but note it is incompatible with the
  scheme above, which needs the valve to cycle continuously.

## CA35 peripheral mapping

Reflected in [`Consortium.toml`](../../Consortium.toml) `[peripheral.*]`
(owner = `ap`), and enabled in the device tree by
[`hardware/dts/`](../dts/README.md):

| Signal        | STM32MP257 peripheral | Pin   | Connector pin | sysfs           | Drives                              |
| ------------- | --------------------- | ----- | ------------- | --------------- | ----------------------------------- |
| Section A     | `TIM4_CH2`            | `PA1` | 33            | `pwmchip4/pwm1` | FR120N module #1 — valve A at 40 Hz |
| Section B     | `TIM5_CH1`            | `PH8` | 31            | `pwmchip8/pwm0` | FR120N module #2 — valve B at 40 Hz |
| Pressure read | `I2C2` (`PF0`/`PF2`)  | —     | 27 / 28       | —               | MPRLS breakout (not wired)          |

The two sections sit on **different timers**, so their phases are independent and
cannot be aligned from sysfs. Two identical duties therefore do not mean two
identical waveforms — the sections are the same _on average over a period_ but
free-run against each other within one. At 40 Hz that is a 25 ms window, far
below the pneumatics' own response, so it does not reach the air. It would matter
if these were ever driven fast enough for a single period to move meaningful
volume; they are not, and the 20–50 Hz ceiling keeps it that way.

Each timer's `pwm-stm32` provider appears as **its own** `/sys/class/pwm/pwmchipN`.
Within a chip, channels are numbered by the **timer's own** channel index — `CH1`
is `pwm0`, `CH2` is `pwm1` — which is why section A lands on channel 1 and
section B on channel 0.

The chip indices come from probe order and are not stable across kernel or
device-tree changes. To re-identify them on a live board:

```bash
for chip in /sys/class/pwm/pwmchip*; do echo "$chip -> $(readlink -f "$chip/device")"; done
```

`40020000.timer` is TIM4 (section A); `40030000.timer` is TIM5 (section B).

The answer goes in `[pneumatics]` of `/opt/snf/app/Repose.toml` — the config file
`snf-app` reads from the directory holding the binary — not into Rust:

```toml
[pneumatics]
section_a = { chip = 4, channel = 1 }   # TIM4_CH2
section_b = { chip = 8, channel = 0 }   # TIM5_CH1
```

The values above are the defaults, so a board that enumerates this way needs no
file at all. `crates/app/Repose.toml` is the annotated template to copy from;
see [`crates/app/README.md`](../../crates/app/README.md) § Configuration.

## Fail-safe behaviour

Both section valves **must be normally open**: de-energized they vent their
section, energized they seal so pressure can build. That polarity is what makes
losing control fail safe, and it matters more in this design than it did in the
scaffold — the CM33 is busy owning the radar UART and holds **no actuator line**,
so there is no independent interlock behind the software. (The CM33 keeps USART6,
but that port carries no actuator line either.)

Cycling the valves rather than parking them makes that polarity do more work, not
less. A control loop that _stalls_ leaves the last duty running, and the sections
keep doing whatever that duty was doing. A control loop that _dies_ drops both
lines — and a dropped line is a fully open valve. The dangerous failure is
therefore the live-but-wrong loop, which is what the ceilings in
`crates/bridge/src/inflation.rs` are aimed at, rather than the dead one.

Symmetry adds one failure the single-bladder design did not have: a write that
lands on one section and not the other, which leaves the mat lopsided under
whatever is resting on it. `Pneumatics::set_state` treats a partial write as an
error, rolls both sections back to vent, and lets the caller stop actuating —
`crates/app` drops the pneumatics entirely on that path rather than keep poking a
half-working chip.

What covers what:

| Failure                    | What stops the inflation                                       |
| -------------------------- | -------------------------------------------------------------- |
| Fatigue pinned high        | Per-mode budgets, then the `max_charge` ledger → neutral duty  |
| Fatigue verdicts stop      | Verdict timeout: neutral at 3 s, full vent at 6 s              |
| One section fails to write | `set_state` vents both and reports; the app stops actuating    |
| Clean exit / IPC loss      | The app vents explicitly before returning                      |
| Panic that unwinds         | `Pneumatics`/`SysfsPwm` `Drop`: vent, disable, unexport        |
| `SIGKILL`, app crash       | Channels unexported by the kernel; pins idle low → valves open |
| Board power loss           | Valves de-energize → both sections vent                        |

The one gap is a `SIGKILL` landing mid-inflation, which leaves the sections at
whatever pressure they had reached until the process is restarted or power drops.
The per-mode budgets and `max_charge` bound how much air that can be.

## Control sketch

The fatigue level does not set the duty directly. `crates/bridge/src/inflation.rs`
maps it through a piecewise model — a sigmoid micro-rise (`Nudge`), an
inverted-U that forms the bowl in 3–5 s and then breathes (`Cradle`), and a
constant slow deployment that must be explicitly asked for (`Canopy`) — with a
hysteresis loop between them so a brief look-up does not collapse the structure.
`crates/app` does no deciding: it feeds verdicts in, takes one duty out every
100 ms, and writes it to both sections.

It is open-loop throughout, which is why the ceilings exist. Adding the MPRLS
turns it into a real setpoint loop — the mode model choosing a _pressure_ rather
than a duty, with a PI loop underneath — and none of the driver layer has to
change to get there.
