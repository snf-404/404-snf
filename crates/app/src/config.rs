// SPDX-License-Identifier: Apache-2.0

//! `Repose.toml` — the deployment's answer to "where is the hardware".
//!
//! Everything in here is a *wiring* fact, not a tuning one: which tty the
//! IWR6843 enumerated on, which `/sys/class/pwm/pwmchipN` each pneumatic section
//! landed behind, where the ONNX model sits, which BlueZ adapter to claim. None
//! of it is knowable at compile time and all of it can change without a single
//! line of this repository changing — a different USB cable renumbers the radar
//! port, a kernel bump renumbers the pwmchips (see
//! `hardware/pneumatics/README.md`). Baking them into the binary meant a full
//! `csti` cross-build to correct a number the board could have told you.
//!
//! The file is read from **the directory holding the executable**, not the
//! working directory: under `dist/systemd/consortium-app.service` that is
//! `/opt/snf/app/Repose.toml`, next to `/opt/snf/app/snf-app`. `crates/app/Repose.toml`
//! is the checked-in template, every key at its compiled-in default.
//!
//! # Missing versus wrong
//!
//! The two failures are deliberately not treated alike:
//!
//! * **No file** is normal — a board that wants the defaults should not need to
//!   carry a copy of them. [`load`] logs the path it looked at and returns
//!   [`ReposeConfig::default`], which is exactly the behaviour this binary had
//!   before the file existed.
//! * **A file that does not say what its author meant** is not normal. Unknown
//!   keys are rejected rather than ignored (`deny_unknown_fields`) and
//!   [`ReposeConfig::validate`] rejects values that cannot work, both as hard
//!   errors that stop start-up. A `pwm_hz` misspelt into oblivion, silently
//!   reverting to 40 Hz, is a worse morning than a service that refuses to come
//!   up and says why.
//!
//! Values that are well-formed but wrong for the board need nothing here: a
//! nonexistent pwmchip already surfaces as an `io::Error` from
//! [`Pneumatics::open`](crate::pneumatics::Pneumatics::open), which `main`
//! degrades to telemetry-only, and a bad tty already stops
//! `RadarStream::open`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use snf_radar::{RadarConfig, RadarProtocol};

use crate::pneumatics::PneumaticConfig;

/// File name looked for beside the executable.
pub const FILE_NAME: &str = "Repose.toml";

/// PWM carrier range the valve armature can actually follow, per
/// [`SECTION_PWM_HZ`](crate::pneumatics::SECTION_PWM_HZ)'s reasoning. Outside it
/// the duty stops meaning a flow ratio — but a bench with different valves is a
/// real reason to leave the band, so this only warns.
const ADVISED_PWM_HZ: std::ops::RangeInclusive<u32> = 20..=50;

/// The whole of the deployment's configuration.
///
/// Every section and every key is optional; anything omitted keeps the value
/// compiled into this binary. That is what lets a board carry a two-line file
/// correcting one pwmchip index instead of a full copy of the defaults.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ReposeConfig {
    pub radar: RadarSection,
    pub pneumatics: PneumaticsSection,
    pub fatigue: FatigueSection,
    pub ble: BleSection,
}

/// `[radar]` — the IWR6843's data UART.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RadarSection {
    /// Serial device path for the data port. `/dev/ttyACM1` with the sensor on
    /// an XDS110 ISK/BoosterPack (the CLI port enumerates first, the data port
    /// second); `/dev/ttyUSB1` behind a bare FTDI cable. Check `dmesg`.
    pub data_port: String,
    pub baud_rate: u32,
    /// Must match the firmware flashed on the sensor: `"out-of-box"` or
    /// `"vital-signs"`.
    pub protocol: RadarProtocol,
    /// Hard allocation bound for a declared UART packet.
    pub max_packet_length: usize,
}

impl Default for RadarSection {
    fn default() -> Self {
        // Derived from the type this section feeds, so the two cannot drift —
        // except `data_port`, where the radar crate's dev-host default
        // (`/dev/ttyUSB1`) is not this board's.
        let RadarConfig {
            baud_rate,
            protocol,
            max_packet_length,
            ..
        } = RadarConfig::default();
        Self {
            data_port: "/dev/ttyACM1".to_string(),
            baud_rate,
            protocol,
            max_packet_length,
        }
    }
}

impl From<RadarSection> for RadarConfig {
    fn from(section: RadarSection) -> Self {
        Self {
            data_port: section.data_port,
            baud_rate: section.baud_rate,
            protocol: section.protocol,
            max_packet_length: section.max_packet_length,
        }
    }
}

/// `[pneumatics]` — which sysfs PWM channel backs each of the two sections.
///
/// The chip indices are assigned in probe order and are **not** stable across
/// kernel or device-tree changes; `hardware/pneumatics/README.md` has the
/// `readlink` loop that re-derives them on a live board.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct PneumaticsSection {
    /// Carrier frequency, in Hz. Both sections run it — two halves breathing at
    /// different rates is exactly the beat frequency a person would notice — so
    /// there is one key, not two.
    pub pwm_hz: u32,
    /// `TIM4_CH2` · `PA1` · connector pin 33.
    pub section_a: SectionChannel,
    /// `TIM5_CH1` · `PH8` · connector pin 31.
    pub section_b: SectionChannel,
}

/// One section's `/sys/class/pwm/pwmchip{chip}/pwm{channel}`.
///
/// `pwm-stm32` numbers channels by the **timer's own** channel index — `CH1` is
/// `pwm0`, `CH2` is `pwm1` — regardless of how many the device tree exposes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionChannel {
    pub chip: u32,
    pub channel: u32,
}

impl Default for PneumaticsSection {
    fn default() -> Self {
        let PneumaticConfig {
            pwm_hz,
            section_a_chip,
            section_a_channel,
            section_b_chip,
            section_b_channel,
        } = PneumaticConfig::default();
        Self {
            pwm_hz,
            section_a: SectionChannel {
                chip: section_a_chip,
                channel: section_a_channel,
            },
            section_b: SectionChannel {
                chip: section_b_chip,
                channel: section_b_channel,
            },
        }
    }
}

impl From<PneumaticsSection> for PneumaticConfig {
    fn from(section: PneumaticsSection) -> Self {
        Self {
            pwm_hz: section.pwm_hz,
            section_a_chip: section.section_a.chip,
            section_a_channel: section.section_a.channel,
            section_b_chip: section.section_b.chip,
            section_b_channel: section.section_b.channel,
        }
    }
}

/// `[fatigue]` — the ONNX model on the deployed image.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct FatigueSection {
    pub model_path: String,
    /// Reported in the Fatigue payload's `model_revision`, so a client can tell
    /// two models apart. Bump it whenever `model_path` is replaced.
    pub revision: u32,
}

impl Default for FatigueSection {
    fn default() -> Self {
        Self {
            model_path: "/opt/snf/fatigue.onnx".to_string(),
            // 1 was the stub; 2/3 were earlier graphs; 4 is the six-feature
            // logistic-linear model exported by `ml/`.
            revision: 4,
        }
    }
}

/// `[ble]` — which BlueZ adapter to advertise on.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct BleSection {
    /// `hci0`, `hci1`, … Omitted (or absent) means BlueZ's default adapter,
    /// which is what a board with one radio wants.
    pub adapter: Option<String>,
}

/// Why the configuration could not be used.
#[derive(Debug)]
pub enum ConfigError {
    /// The executable's own path could not be resolved, so there is no
    /// directory to look in.
    Locate(io::Error),
    /// The file exists but could not be read.
    Read { path: PathBuf, error: io::Error },
    /// Not valid TOML, or a key this binary does not know.
    Parse {
        path: PathBuf,
        error: toml::de::Error,
    },
    /// Parsed, but a value cannot work.
    Invalid { path: PathBuf, reason: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locate(error) => {
                write!(
                    f,
                    "cannot locate the executable to find {FILE_NAME}: {error}"
                )
            }
            Self::Read { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Parse { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Invalid { path, reason } => write!(f, "{}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Locate(error) | Self::Read { error, .. } => Some(error),
            Self::Parse { error, .. } => Some(error),
            Self::Invalid { .. } => None,
        }
    }
}

impl ReposeConfig {
    /// Reject values that cannot produce a working system.
    ///
    /// Only the cases where continuing would panic or would obviously fail
    /// later; anything a board can legitimately do differently is left alone.
    fn validate(&self) -> Result<(), String> {
        if self.radar.data_port.is_empty() {
            return Err("radar.data_port is empty".to_string());
        }
        if self.radar.baud_rate == 0 {
            return Err("radar.baud_rate must be non-zero".to_string());
        }
        if self.radar.max_packet_length == 0 {
            return Err("radar.max_packet_length must be non-zero".to_string());
        }
        // `SysfsPwm::new` asserts this; catching it here turns a panic partway
        // through bring-up into a start-up error naming the key.
        if self.pneumatics.pwm_hz == 0 {
            return Err("pneumatics.pwm_hz must be non-zero".to_string());
        }
        if self.fatigue.model_path.is_empty() {
            return Err("fatigue.model_path is empty".to_string());
        }
        Ok(())
    }

    /// Log anything unusual but permitted, so it appears in the journal next to
    /// whatever it goes on to cause.
    fn warn_unusual(&self) {
        let pwm_hz = self.pneumatics.pwm_hz;
        if !ADVISED_PWM_HZ.contains(&pwm_hz) {
            tracing::warn!(
                "snf-app: pneumatics.pwm_hz = {pwm_hz} is outside the {}–{} Hz band the valves \
                 can follow; duty may no longer map monotonically to net flow",
                ADVISED_PWM_HZ.start(),
                ADVISED_PWM_HZ.end(),
            );
        }
    }

    /// One line naming every resolved wiring fact, so the journal answers "what
    /// did it actually use" without anyone having to guess which file won.
    pub fn log_summary(&self) {
        tracing::info!(
            "snf-app: radar {} @ {} ({:?}); sections pwmchip{}/pwm{} + pwmchip{}/pwm{} @ {} Hz; \
             model {} rev {}; ble adapter {}",
            self.radar.data_port,
            self.radar.baud_rate,
            self.radar.protocol,
            self.pneumatics.section_a.chip,
            self.pneumatics.section_a.channel,
            self.pneumatics.section_b.chip,
            self.pneumatics.section_b.channel,
            self.pneumatics.pwm_hz,
            self.fatigue.model_path,
            self.fatigue.revision,
            self.ble.adapter.as_deref().unwrap_or("<default>"),
        );
    }
}

/// Where [`load`] will look: `Repose.toml` beside the running executable.
pub fn path() -> Result<PathBuf, ConfigError> {
    let exe = std::env::current_exe().map_err(ConfigError::Locate)?;
    let dir = exe.parent().ok_or_else(|| {
        ConfigError::Locate(io::Error::other(format!(
            "executable path {} has no parent directory",
            exe.display()
        )))
    })?;
    Ok(dir.join(FILE_NAME))
}

/// Read the configuration, falling back to the compiled-in defaults when there
/// is no file — but never when there is one this binary cannot honour.
pub fn load() -> Result<ReposeConfig, ConfigError> {
    let path = path()?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            tracing::info!(
                "snf-app: no {} at {}; using compiled-in defaults",
                FILE_NAME,
                path.display()
            );
            let config = ReposeConfig::default();
            config.warn_unusual();
            return Ok(config);
        }
        Err(error) => return Err(ConfigError::Read { path, error }),
    };
    parse(&path, &text).inspect(|config| {
        tracing::info!("snf-app: loaded {}", path.display());
        config.warn_unusual();
    })
}

/// Parse and validate `text` as if it had been read from `path`. Split out from
/// [`load`] so the checked-in template can be tested without a filesystem.
fn parse(path: &Path, text: &str) -> Result<ReposeConfig, ConfigError> {
    let config: ReposeConfig = toml::from_str(text).map_err(|error| ConfigError::Parse {
        path: path.to_path_buf(),
        error,
    })?;
    config.validate().map_err(|reason| ConfigError::Invalid {
        path: path.to_path_buf(),
        reason,
    })?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use snf_radar::DEFAULT_MAX_PACKET_LENGTH;

    use super::*;
    use crate::pneumatics::SECTION_PWM_HZ;

    fn parse_str(text: &str) -> Result<ReposeConfig, ConfigError> {
        parse(Path::new("Repose.toml"), text)
    }

    /// The checked-in template is the documentation of the defaults, so it has
    /// to *be* the defaults. This is what catches a default changing in Rust
    /// without the template following.
    #[test]
    fn shipped_template_is_the_defaults() {
        let template = include_str!("../Repose.toml");
        assert_eq!(parse_str(template).unwrap(), ReposeConfig::default());
    }

    #[test]
    fn empty_file_is_the_defaults() {
        assert_eq!(parse_str("").unwrap(), ReposeConfig::default());
    }

    #[test]
    fn partial_file_leaves_the_rest_alone() {
        let config = parse_str("[radar]\ndata_port = \"/dev/ttyUSB1\"\n").unwrap();
        assert_eq!(config.radar.data_port, "/dev/ttyUSB1");
        assert_eq!(config.radar.baud_rate, RadarSection::default().baud_rate);
        assert_eq!(config.pneumatics, PneumaticsSection::default());
        assert_eq!(config.fatigue, FatigueSection::default());
    }

    /// A misspelt key must not read as "leave it at the default".
    #[test]
    fn unknown_keys_are_rejected() {
        for text in [
            "[pnuematics]\npwm_hz = 30\n",
            "[pneumatics]\npwm_hz_typo = 30\n",
            "[pneumatics]\nsection_a = { chip = 4, chanel = 1 }\n",
        ] {
            assert!(
                matches!(parse_str(text), Err(ConfigError::Parse { .. })),
                "expected a parse error for {text:?}"
            );
        }
    }

    /// `SysfsPwm::new` asserts a non-zero frequency, so this must never reach it.
    #[test]
    fn zero_pwm_hz_is_rejected() {
        assert!(matches!(
            parse_str("[pneumatics]\npwm_hz = 0\n"),
            Err(ConfigError::Invalid { .. })
        ));
    }

    #[test]
    fn empty_paths_are_rejected() {
        assert!(matches!(
            parse_str("[radar]\ndata_port = \"\"\n"),
            Err(ConfigError::Invalid { .. })
        ));
        assert!(matches!(
            parse_str("[fatigue]\nmodel_path = \"\"\n"),
            Err(ConfigError::Invalid { .. })
        ));
    }

    #[test]
    fn sections_convert_to_the_types_they_feed() {
        let config = parse_str(
            "[radar]\n\
             data_port = \"/dev/ttyUSB1\"\n\
             protocol = \"vital-signs\"\n\
             [pneumatics]\n\
             pwm_hz = 25\n\
             section_a = { chip = 6, channel = 0 }\n\
             section_b = { chip = 7, channel = 1 }\n",
        )
        .unwrap();

        let radar: RadarConfig = config.radar.into();
        assert_eq!(radar.data_port, "/dev/ttyUSB1");
        assert_eq!(radar.protocol, RadarProtocol::VitalSigns);
        assert_eq!(radar.max_packet_length, DEFAULT_MAX_PACKET_LENGTH);

        let pneumatics: PneumaticConfig = config.pneumatics.into();
        assert_eq!(
            pneumatics,
            PneumaticConfig {
                pwm_hz: 25,
                section_a_chip: 6,
                section_a_channel: 0,
                section_b_chip: 7,
                section_b_channel: 1,
            }
        );
    }

    /// The lookup is beside the *binary*, not the working directory — that is
    /// the whole contract, and it is the one part `parse` cannot cover.
    #[test]
    fn path_is_beside_the_executable() {
        let path = path().unwrap();
        assert!(path.is_absolute());
        assert_eq!(path.file_name().unwrap(), FILE_NAME);
        assert_eq!(
            path.parent().unwrap(),
            std::env::current_exe().unwrap().parent().unwrap()
        );
    }

    /// The missing-file branch: no `Repose.toml` beside the test binary, so this
    /// exercises the real `NotFound` path rather than a simulated one.
    #[test]
    fn a_missing_file_is_not_an_error() {
        let path = path().unwrap();
        if path.exists() {
            return; // someone put one beside the test binary; nothing to assert
        }
        assert_eq!(load().unwrap(), ReposeConfig::default());
    }

    /// The default carrier is the one the module doc argues for.
    #[test]
    fn default_pwm_hz_is_the_section_carrier() {
        assert_eq!(PneumaticsSection::default().pwm_hz, SECTION_PWM_HZ);
        assert!(ADVISED_PWM_HZ.contains(&SECTION_PWM_HZ));
    }
}
