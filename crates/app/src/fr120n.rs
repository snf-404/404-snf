// SPDX-License-Identifier: Apache-2.0

//! Driver for an **FR120N** low-side N-channel MOSFET switch module, used here
//! to drive the 24 V pneumatic pump and the vent solenoid from one PWM channel
//! each.
//!
//! This replaces the TB6612FNG the scaffold started with: that part tops out at
//! 13.5 V on its motor supply, and the pneumatics run on a 24 V rail. The
//! FR120N (100 V / 9.2 A N-channel MOSFET, sold as a solderless "trigger switch"
//! breakout) carries the rail comfortably — but it is a *switch*, not an
//! H-bridge, so the control surface collapses to a single line:
//!
//! | TB6612FNG (was)              | FR120N module (now)     |
//! | ---------------------------- | ----------------------- |
//! | `PWM` + `IN1`/`IN2` + `STBY` | one PWM/trigger input   |
//! | forward / reverse / brake    | on / off only           |
//! | coast vs. brake distinction  | none — the load coasts  |
//!
//! Both loads here are unidirectional (a diaphragm pump and a solenoid), so
//! nothing is lost. What *is* lost is the H-bridge's electrical brake, and with
//! it any way to stop the pump faster than its own friction — irrelevant for a
//! pump, worth knowing if this driver is reused for something with inertia.
//!
//! The module switches the **low side**: the load sits between +24 V and the
//! module's output, and the MOSFET interrupts its return path. Consequences that
//! matter when wiring:
//!
//! * the load never floats to ground when off — it floats to +24 V, so do not
//!   treat "off" as "safe to touch";
//! * both loads are inductive, so each needs a flyback diode across it
//!   (`SS34`/`1N5819`, cathode to +24 V). Many FR120N boards ship without one,
//!   and the MOSFET is what dies if it is missing;
//! * the 24 V and 3.3 V grounds must be common, or the gate sees no reference.
//!
//! # Gate drive
//!
//! The FR120N is **not** a logic-level MOSFET: its `V_GS(th)` is specified as a
//! 2–4 V range and its on-resistance is characterized at `V_GS = 10 V`. A module
//! that wires the trigger input straight to the gate will therefore be only
//! partially enhanced by the CA35's 3.3 V logic — it may pass a few hundred
//! milliamps and then heat, which looks exactly like an underpowered pump. Check
//! your board: if it has no gate driver or level shifter, either drive the gate
//! from 12 V through a small NPN stage, or confirm at the pump's actual stall
//! current that the module stays cool. See
//! `hardware/pneumatics/README.md` for the wiring and the parts.
//!
//! # PWM frequency
//!
//! A bare-gate module's slow switching also bounds the usable PWM frequency: the
//! MOSFET spends a large fraction of each edge in its linear region, and the
//! dissipation scales with the edge rate. 1 kHz (audible, but forgiving) is the
//! safe starting point on an unknown board; 20 kHz (inaudible) is fine only once
//! the module is known to switch cleanly.
//!
//! # Example
//!
//! ```ignore
//! use crate::fr120n::Fr120n;
//! use crate::sysfs_pwm::SysfsPwm;
//!
//! // Pump on TIM4_CH2 (connector pin 33), exported as pwmchipN channel 0.
//! let mut pump = Fr120n::new(SysfsPwm::new(0, 0, 1_000)?);
//! pump.set_percent(70)?;  // inflate
//! pump.off()?;            // coast to a stop
//! ```

use embedded_hal::pwm::SetDutyCycle;

/// A PWM update failed.
///
/// The one backend this driver targets — [`crate::sysfs_pwm::SysfsPwm`] — fails
/// only on a sysfs write, so the inner error is collapsed rather than carried
/// through as a generic parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("fr120n: pwm update failed")
    }
}

impl std::error::Error for Error {}

/// One FR120N low-side switch channel, driven by a PWM output.
///
/// Construct from a PWM channel already configured and idle-low;
/// [`Fr120n::new`] performs no I/O, so the load stays off until the first
/// [`Fr120n::set_duty`]. The driver caches the last duty it wrote so callers can
/// ask what the load is doing without a sysfs read.
pub struct Fr120n<PWM> {
    pwm: PWM,
    duty: u16,
}

impl<PWM> Fr120n<PWM>
where
    PWM: SetDutyCycle,
{
    /// Bind the switch to its PWM output. Does no I/O.
    pub fn new(pwm: PWM) -> Self {
        Self { pwm, duty: 0 }
    }

    /// The duty value corresponding to 100 % (continuously on).
    pub fn max_duty(&self) -> u16 {
        self.pwm.max_duty_cycle()
    }

    /// The last duty written, in the same scale as [`Self::max_duty`].
    pub fn duty(&self) -> u16 {
        self.duty
    }

    /// Whether the load is being driven at all.
    pub fn is_on(&self) -> bool {
        self.duty > 0
    }

    /// Drive the load at raw `duty` (`0..=`[`max_duty`](Self::max_duty)).
    pub fn set_duty(&mut self, duty: u16) -> Result<(), Error> {
        self.pwm.set_duty_cycle(duty).map_err(|_| Error)?;
        self.duty = duty;
        Ok(())
    }

    /// Drive the load at `percent` (`0..=100`; higher values saturate).
    pub fn set_percent(&mut self, percent: u8) -> Result<(), Error> {
        let percent = u32::from(percent.min(100));
        let duty = (u32::from(self.max_duty()) * percent / 100) as u16;
        self.set_duty(duty)
    }

    /// Switch the load fully on (continuous conduction, no chopping).
    pub fn on(&mut self) -> Result<(), Error> {
        self.pwm.set_duty_cycle_fully_on().map_err(|_| Error)?;
        self.duty = self.max_duty();
        Ok(())
    }

    /// Switch the load off. The load coasts — there is no brake on a low-side
    /// switch.
    pub fn off(&mut self) -> Result<(), Error> {
        self.pwm.set_duty_cycle_fully_off().map_err(|_| Error)?;
        self.duty = 0;
        Ok(())
    }

    /// Consume the driver and return the owned PWM output.
    pub fn free(self) -> PWM {
        self.pwm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::pwm::{ErrorKind, ErrorType};

    /// A `SetDutyCycle` that just records what was written.
    struct FakePwm {
        max: u16,
        written: Vec<u16>,
    }

    #[derive(Debug)]
    struct FakeError;

    impl embedded_hal::pwm::Error for FakeError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    impl ErrorType for FakePwm {
        type Error = FakeError;
    }

    impl SetDutyCycle for FakePwm {
        fn max_duty_cycle(&self) -> u16 {
            self.max
        }

        fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
            self.written.push(duty);
            Ok(())
        }
    }

    fn switch() -> Fr120n<FakePwm> {
        Fr120n::new(FakePwm {
            max: 1000,
            written: Vec::new(),
        })
    }

    #[test]
    fn starts_off_and_does_no_io() {
        let switch = switch();
        assert_eq!(switch.duty(), 0);
        assert!(!switch.is_on());
        assert!(switch.free().written.is_empty());
    }

    #[test]
    fn percent_scales_onto_max_duty_and_saturates() {
        let mut switch = switch();
        switch.set_percent(70).unwrap();
        assert_eq!(switch.duty(), 700);
        switch.set_percent(200).unwrap();
        assert_eq!(switch.duty(), 1000);
        assert_eq!(switch.free().written, vec![700, 1000]);
    }

    #[test]
    fn on_and_off_track_the_cached_duty() {
        let mut switch = switch();
        switch.on().unwrap();
        assert!(switch.is_on());
        assert_eq!(switch.duty(), 1000);

        switch.off().unwrap();
        assert!(!switch.is_on());
        assert_eq!(switch.duty(), 0);
    }
}
