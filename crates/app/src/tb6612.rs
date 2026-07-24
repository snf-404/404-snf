// SPDX-License-Identifier: Apache-2.0

//! Driver for the Toshiba **TB6612FNG** dual H-bridge, used here to drive the
//! pneumatic pump from one PWM channel plus two direction lines and the shared
//! standby line.
//!
//! The driver is generic over the embedded-hal 1.0 traits
//! [`SetDutyCycle`](embedded_hal::pwm::SetDutyCycle) and
//! [`OutputPin`](embedded_hal::digital::OutputPin), so the exact same code runs
//! against the CA35's Linux bindings ([`crate::sysfs_pwm::SysfsPwm`] +
//! `linux_embedded_hal::CdevPin`) today, or a bare-metal CM33 timer/GPIO binding
//! if the pump ever moves onto the real-time core.
//!
//! One TB6612FNG channel (A or B) is an H-bridge with three control inputs —
//! `PWMx` (speed) and `INx1`/`INx2` (direction) — gated by the chip-wide `STBY`
//! standby line. This type models one such channel together with the standby
//! pin, which is everything the single-zone pump needs. A second channel (e.g. a
//! second zone, or the vent valve wired to channel B) reuses the same pattern;
//! if both channels share one chip, hold a single `STBY` pin and give each
//! channel its own `PWM`/`IN1`/`IN2`.
//!
//! The pump is unidirectional, so in practice only [`Tb6612::forward`] and
//! [`Tb6612::coast`]/[`Tb6612::set_duty`] are exercised; the reverse and brake
//! paths are provided because the part supports them.
//!
//! # Truth table (per channel, from the TB6612FNG datasheet)
//!
//! | IN1 | IN2 | PWM | STBY | Output        |
//! | --- | --- | --- | ---- | ------------- |
//! | H   | H   |  X  |  H   | short brake   |
//! | L   | H   |  H  |  H   | reverse (CCW) |
//! | L   | H   |  L  |  H   | short brake   |
//! | H   | L   |  H  |  H   | forward (CW)  |
//! | H   | L   |  L  |  H   | short brake   |
//! | L   | L   |  X  |  H   | stop (coast)  |
//! | X   | X   |  X  |  L   | standby (Hi-Z)|
//!
//! # Example (CA35 / Linux)
//!
//! ```ignore
//! use linux_embedded_hal::CdevPin;
//! use linux_embedded_hal::gpio_cdev::{Chip, LineRequestFlags};
//!
//! use crate::sysfs_pwm::SysfsPwm;
//! use crate::tb6612::Tb6612;
//!
//! // Pump PWM: a Linux-owned timer exposed as pwmchip0 channel 0, 20 kHz.
//! let pwm = SysfsPwm::new(0, 0, 20_000)?;
//!
//! // TB6612 control lines on /dev/gpiochip0 (offsets are board-specific).
//! let mut chip = Chip::new("/dev/gpiochip0")?;
//! let ain1 = CdevPin::new(chip.get_line(5)?.request(LineRequestFlags::OUTPUT, 0, "snf-pump-ain1")?)?;
//! let ain2 = CdevPin::new(chip.get_line(6)?.request(LineRequestFlags::OUTPUT, 0, "snf-pump-ain2")?)?;
//! let stby = CdevPin::new(chip.get_line(13)?.request(LineRequestFlags::OUTPUT, 0, "snf-tb6612-stby")?)?;
//!
//! let mut pump = Tb6612::new(pwm, ain1, ain2, stby);
//! pump.enable()?;                     // release standby
//! pump.forward(pump.max_duty() / 2)?; // ~50 % — inflate the bladder
//! // ...close the pressure loop against the sensor here...
//! pump.coast()?;                      // pump off (venting handled elsewhere)
//! ```

use embedded_hal::digital::OutputPin;
use embedded_hal::pwm::SetDutyCycle;

/// Rotation direction of a TB6612 channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `IN1 = H`, `IN2 = L`.
    Forward,
    /// `IN1 = L`, `IN2 = H`.
    Reverse,
}

/// A control-line failure. The two backends this driver targets both have
/// infallible writes in practice, so the underlying error is collapsed to which
/// kind of line failed rather than carrying the (differing) inner error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A direction or standby GPIO write failed.
    Pin,
    /// A PWM duty-cycle update failed.
    Pwm,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Pin => f.write_str("tb6612: gpio write failed"),
            Error::Pwm => f.write_str("tb6612: pwm update failed"),
        }
    }
}

impl std::error::Error for Error {}

/// One TB6612FNG H-bridge channel plus the chip-wide standby line.
///
/// Construct with pins already configured as outputs at their inactive level
/// (`IN1 = IN2 = STBY = Low`, i.e. the chip in standby); [`Tb6612::new`] performs
/// no I/O.
pub struct Tb6612<PWM, IN1, IN2, STBY> {
    pwm: PWM,
    in1: IN1,
    in2: IN2,
    stby: STBY,
}

impl<PWM, IN1, IN2, STBY> Tb6612<PWM, IN1, IN2, STBY>
where
    PWM: SetDutyCycle,
    IN1: OutputPin,
    IN2: OutputPin,
    STBY: OutputPin,
{
    /// Bind the channel to its PWM output and control lines. Does no I/O; the
    /// chip stays in whatever state the pins were left in (expected: standby).
    pub fn new(pwm: PWM, in1: IN1, in2: IN2, stby: STBY) -> Self {
        Self {
            pwm,
            in1,
            in2,
            stby,
        }
    }

    /// The duty value corresponding to 100 % (fully on).
    pub fn max_duty(&self) -> u16 {
        self.pwm.max_duty_cycle()
    }

    /// Release standby (`STBY = H`), allowing the H-bridge to drive its outputs.
    pub fn enable(&mut self) -> Result<(), Error> {
        self.stby.set_high().map_err(|_| Error::Pin)
    }

    /// Enter standby (`STBY = L`): both outputs go high-impedance regardless of
    /// the IN/PWM lines.
    pub fn standby(&mut self) -> Result<(), Error> {
        self.stby.set_low().map_err(|_| Error::Pin)
    }

    /// Set the direction lines for `dir` without touching the duty cycle.
    fn set_direction(&mut self, dir: Direction) -> Result<(), Error> {
        match dir {
            Direction::Forward => {
                self.in1.set_high().map_err(|_| Error::Pin)?;
                self.in2.set_low().map_err(|_| Error::Pin)?;
            }
            Direction::Reverse => {
                self.in1.set_low().map_err(|_| Error::Pin)?;
                self.in2.set_high().map_err(|_| Error::Pin)?;
            }
        }
        Ok(())
    }

    /// Drive in `dir` at raw `duty` (`0..=`[`max_duty`](Self::max_duty)).
    pub fn drive(&mut self, dir: Direction, duty: u16) -> Result<(), Error> {
        self.set_direction(dir)?;
        self.set_duty(duty)
    }

    /// Drive forward at raw `duty`. For the pump this is "inflate".
    pub fn forward(&mut self, duty: u16) -> Result<(), Error> {
        self.drive(Direction::Forward, duty)
    }

    /// Drive reverse at raw `duty`.
    pub fn reverse(&mut self, duty: u16) -> Result<(), Error> {
        self.drive(Direction::Reverse, duty)
    }

    /// Update only the duty cycle, keeping the current direction.
    pub fn set_duty(&mut self, duty: u16) -> Result<(), Error> {
        self.pwm.set_duty_cycle(duty).map_err(|_| Error::Pwm)
    }

    /// Set direction and speed as a percentage (`0..=100`).
    pub fn set_speed_percent(&mut self, dir: Direction, percent: u8) -> Result<(), Error> {
        self.set_direction(dir)?;
        self.pwm
            .set_duty_cycle_percent(percent)
            .map_err(|_| Error::Pwm)
    }

    /// Short-brake the motor (`IN1 = IN2 = H`): the winding is shorted, stopping
    /// it quickly. PWM level is irrelevant in this state.
    pub fn brake(&mut self) -> Result<(), Error> {
        self.in1.set_high().map_err(|_| Error::Pin)?;
        self.in2.set_high().map_err(|_| Error::Pin)
    }

    /// Coast to a stop (`IN1 = IN2 = L`, duty 0): outputs are released. For the
    /// pump this is simply "off".
    pub fn coast(&mut self) -> Result<(), Error> {
        self.in1.set_low().map_err(|_| Error::Pin)?;
        self.in2.set_low().map_err(|_| Error::Pin)?;
        self.pwm
            .set_duty_cycle_fully_off()
            .map_err(|_| Error::Pwm)
    }

    /// Consume the driver and return the owned PWM output and pins.
    pub fn free(self) -> (PWM, IN1, IN2, STBY) {
        (self.pwm, self.in1, self.in2, self.stby)
    }
}
