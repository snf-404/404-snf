// SPDX-License-Identifier: Apache-2.0

//! Minimal Linux **sysfs PWM** adapter implementing [`SetDutyCycle`].
//!
//! On the CA35 the pneumatics are driven by the two Linux-owned timers that
//! reach the DK's 40-pin connector — `TIM4_CH2` on `PA1` (pump) and `TIM5_CH1`
//! on `PH8` (vent valve) — whose `pwm-stm32` providers appear under
//! `/sys/class/pwm/pwmchipN/`. This wraps one exported channel of such a chip
//! and presents it to [`crate::fr120n`] through the embedded-hal 1.0
//! [`SetDutyCycle`] trait, so the FR120N driver is identical whether its PWM
//! comes from here or from a bare-metal timer.
//!
//! The 16-bit duty value required by [`SetDutyCycle`] is mapped linearly onto the
//! channel period in nanoseconds: `0` is fully off, [`u16::MAX`] is fully on.
//!
//! sysfs semantics this adapter respects:
//! * a channel must be `export`ed before its attributes exist;
//! * `duty_cycle` must never exceed `period`, so period changes always drop the
//!   duty to zero first;
//! * the channel is `enable`d on construction and disabled + unexported on drop.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use embedded_hal::pwm::{ErrorKind, ErrorType, SetDutyCycle};

/// Full-scale duty value presented to [`SetDutyCycle`]; `0..=DUTY_MAX` maps
/// linearly onto `0..=period_ns`.
const DUTY_MAX: u16 = u16::MAX;

/// One exported PWM channel at `/sys/class/pwm/pwmchip{chip}/pwm{channel}`.
pub struct SysfsPwm {
    /// `/sys/class/pwm/pwmchip{chip}`.
    chip_dir: PathBuf,
    /// `{chip_dir}/pwm{channel}`.
    dir: PathBuf,
    /// Channel index within the chip (the value written to `export`).
    channel: u32,
    /// Configured period; the denominator of the duty mapping.
    period_ns: u32,
}

impl SysfsPwm {
    /// Export channel `channel` of `pwmchip{chip}` and configure it for
    /// `frequency_hz`, active-high, enabled at 0 % duty (idle low).
    ///
    /// If the channel is already exported (e.g. left over from a previous run)
    /// its attributes are reused rather than re-exported.
    pub fn new(chip: u32, channel: u32, frequency_hz: u32) -> io::Result<Self> {
        assert!(frequency_hz > 0, "PWM frequency must be non-zero");

        let chip_dir = PathBuf::from(format!("/sys/class/pwm/pwmchip{chip}"));
        let dir = chip_dir.join(format!("pwm{channel}"));

        if !dir.exists() {
            write_attr(&chip_dir.join("export"), &channel.to_string())?;
            // Exporting is handled by the kernel synchronously, but the per-channel
            // attribute files are populated by a udev event that can lag briefly.
            wait_for_dir(&dir)?;
        }

        let period_ns = (1_000_000_000u64 / u64::from(frequency_hz)) as u32;

        let pwm = Self {
            chip_dir,
            dir,
            channel,
            period_ns,
        };

        // Order matters: duty must be <= period at all times, so clear any stale
        // duty before (re)programming the period.
        pwm.write("duty_cycle", "0")?;
        pwm.write("period", &period_ns.to_string())?;
        pwm.write("polarity", "normal")?;
        pwm.write("enable", "1")?;

        Ok(pwm)
    }

    /// Reprogram the output frequency, dropping the duty to zero first so the
    /// kernel never rejects a `period < duty_cycle` transition.
    pub fn set_frequency(&mut self, frequency_hz: u32) -> io::Result<()> {
        assert!(frequency_hz > 0, "PWM frequency must be non-zero");
        self.write("duty_cycle", "0")?;
        self.period_ns = (1_000_000_000u64 / u64::from(frequency_hz)) as u32;
        self.write("period", &self.period_ns.to_string())
    }

    /// Enable the channel output.
    pub fn enable(&self) -> io::Result<()> {
        self.write("enable", "1")
    }

    /// Disable the channel output (drives the pin to its inactive level).
    pub fn disable(&self) -> io::Result<()> {
        self.write("enable", "0")
    }

    fn write(&self, attr: &str, value: &str) -> io::Result<()> {
        write_attr(&self.dir.join(attr), value)
    }
}

impl Drop for SysfsPwm {
    fn drop(&mut self) {
        // Best-effort: quiet the output and release the channel.
        let _ = self.write("enable", "0");
        let _ = write_attr(&self.chip_dir.join("unexport"), &self.channel.to_string());
    }
}

/// Error writing a sysfs PWM attribute.
#[derive(Debug)]
pub struct SysfsPwmError(io::Error);

impl std::fmt::Display for SysfsPwmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sysfs pwm write failed: {}", self.0)
    }
}

impl std::error::Error for SysfsPwmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl embedded_hal::pwm::Error for SysfsPwmError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

impl ErrorType for SysfsPwm {
    type Error = SysfsPwmError;
}

impl SetDutyCycle for SysfsPwm {
    fn max_duty_cycle(&self) -> u16 {
        DUTY_MAX
    }

    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
        // Linear map of the 16-bit duty onto the period in nanoseconds.
        let ns = (u64::from(self.period_ns) * u64::from(duty) / u64::from(DUTY_MAX)) as u32;
        self.write("duty_cycle", &ns.to_string())
            .map_err(SysfsPwmError)
    }
}

/// Write `value` to a sysfs attribute file, truncating it.
fn write_attr(path: &Path, value: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).open(path)?;
    file.write_all(value.as_bytes())
}

/// Poll for a freshly exported channel directory to appear (~up to 100 ms).
fn wait_for_dir(dir: &Path) -> io::Result<()> {
    for _ in 0..20 {
        if dir.exists() {
            return Ok(());
        }
        sleep(Duration::from_millis(5));
    }
    if dir.exists() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("pwm channel {} did not appear after export", dir.display()),
        ))
    }
}
