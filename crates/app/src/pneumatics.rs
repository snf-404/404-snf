// SPDX-License-Identifier: Apache-2.0

//! The CA35-side pneumatic actuator pair: pump and vent valve.
//!
//! Both hang off Linux-owned timers, because TIM4 and TIM5 are the only
//! PWM-capable timers whose channels reach the DK's 40-pin connector and the
//! board's RIF configuration reaches them only from the AP:
//!
//! | Load       | Timer      | Pin  | Connector | Driver          |
//! | ---------- | ---------- | ---- | --------- | --------------- |
//! | Pump       | `TIM4_CH2` | `PA1`| pin 33    | FR120N module   |
//! | Vent valve | `TIM5_CH1` | `PH8`| pin 31    | FR120N module   |
//!
//! Each timer's `pwm-stm32` provider appears as its own `/sys/class/pwm/pwmchipN`
//! with a single channel, so the chip indices — not the channel indices — are
//! what distinguishes them. Those indices are assigned in probe order and are
//! **not** stable across kernel or device-tree changes; [`PneumaticConfig`]
//! carries them so they can be corrected without touching this logic. See
//! `hardware/pneumatics/README.md` for how to identify the two chips on a live
//! board.
//!
//! # Fail-safe
//!
//! The vent valve is assumed **normally open**: de-energized, the bladder vents
//! to atmosphere; energized, it seals so the pump can build pressure. That
//! polarity is what makes losing control fail safe, which matters here because
//! this design has no independent interlock — the CM33 is busy owning the radar
//! UART and holds no actuator line at all. So every path that ends the process
//! must end with both outputs low:
//!
//! * [`Pneumatics::set_state`] to [`PneumaticState::Vent`] on any shutdown;
//! * [`crate::sysfs_pwm::SysfsPwm`]'s `Drop` disables and unexports the channel,
//!   which covers a clean exit and a panic that unwinds;
//! * the pins idle low after the driver releases them, which covers a kill.
//!
//! A hard `SIGKILL` mid-inflation is the one case the software cannot cover; the
//! [`MAX_INFLATE`](crate::MAX_INFLATE) ceiling in the control loop bounds how
//! long a run can be inflating when that happens.

use std::io;

use crate::fr120n::{self, Fr120n};
use crate::sysfs_pwm::SysfsPwm;

/// Pump PWM frequency, in Hz.
///
/// 1 kHz is the conservative default for an FR120N breakout whose gate may be
/// driven straight from 3.3 V logic: the switching losses of a slow gate scale
/// with the edge rate, and 1 kHz keeps them negligible at the cost of an audible
/// whine. Raise toward 20 kHz (inaudible) only after confirming the module
/// switches cleanly at the pump's real current — see `fr120n`'s module docs.
pub const PUMP_PWM_HZ: u32 = 1_000;

/// Vent-valve PWM frequency, in Hz. The valve is only ever fully on or fully
/// off, so this just has to be somewhere the solenoid cannot follow.
pub const VALVE_PWM_HZ: u32 = 1_000;

/// Which `/sys/class/pwm/pwmchipN` backs each load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PneumaticConfig {
    /// `pwmchip` index of the TIM4 provider (pump).
    pub pump_chip: u32,
    /// Channel within the pump's chip. `TIM4_CH2` is the timer's second channel,
    /// but `pwm-stm32` numbers only the channels the device tree exposes, so a
    /// single-channel node is channel `0`.
    pub pump_channel: u32,
    /// `pwmchip` index of the TIM5 provider (vent valve).
    pub valve_chip: u32,
    /// Channel within the valve's chip.
    pub valve_channel: u32,
}

impl Default for PneumaticConfig {
    fn default() -> Self {
        Self {
            pump_chip: 0,
            pump_channel: 0,
            valve_chip: 1,
            valve_channel: 0,
        }
    }
}

/// What the pneumatics should be doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PneumaticState {
    /// Valve open, pump off: the bladder is venting. Also the resting state, and
    /// the state every shutdown path drives to.
    #[default]
    Vent,
    /// Valve sealed, pump running at the given percentage (`0..=100`).
    Inflate(u8),
    /// Valve sealed, pump off: hold whatever pressure is in the bladder.
    Hold,
}

/// The pump and vent valve as one unit, so the two are never left in a
/// contradictory combination (pumping into an open valve, or sealing with no way
/// to release).
pub struct Pneumatics {
    pump: Fr120n<SysfsPwm>,
    valve: Fr120n<SysfsPwm>,
    state: PneumaticState,
}

impl Pneumatics {
    /// Export and configure both PWM channels, leaving the pneumatics venting.
    pub fn open(config: PneumaticConfig) -> io::Result<Self> {
        let pump = SysfsPwm::new(config.pump_chip, config.pump_channel, PUMP_PWM_HZ)?;
        let valve = SysfsPwm::new(config.valve_chip, config.valve_channel, VALVE_PWM_HZ)?;

        Ok(Self {
            pump: Fr120n::new(pump),
            valve: Fr120n::new(valve),
            state: PneumaticState::Vent,
        })
    }

    /// The last state successfully applied.
    pub fn state(&self) -> PneumaticState {
        self.state
    }

    /// Drive the pair to `state`.
    ///
    /// Ordering is chosen so no intermediate combination can push air past a
    /// closing valve or run the pump into an open one: the pump always stops
    /// before the valve opens, and the valve always seals before the pump
    /// starts.
    pub fn set_state(&mut self, state: PneumaticState) -> Result<(), fr120n::Error> {
        match state {
            PneumaticState::Vent => {
                self.pump.off()?;
                self.valve.off()?;
            }
            PneumaticState::Hold => {
                self.pump.off()?;
                self.valve.on()?;
            }
            PneumaticState::Inflate(percent) => {
                self.valve.on()?;
                self.pump.set_percent(percent)?;
            }
        }
        self.state = state;
        Ok(())
    }

    /// Whether the pump is currently being driven.
    pub fn pump_running(&self) -> bool {
        self.pump.is_on()
    }
}

impl Drop for Pneumatics {
    fn drop(&mut self) {
        // Best-effort: vent before the channels are released. `SysfsPwm`'s own
        // `Drop` disables and unexports each channel right after, which drives
        // both pins low regardless of whether these writes landed.
        let _ = self.set_state(PneumaticState::Vent);
    }
}
