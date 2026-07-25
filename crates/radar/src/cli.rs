// SPDX-License-Identifier: Apache-2.0

//! The IWR6843's *other* UART: the 115 200-baud configuration CLI.
//!
//! The sensor enumerates two ports and they are not interchangeable. The first
//! is a line-oriented command interpreter at **115 200 baud** — the `mmwDemo:/>`
//! prompt — and the second is the binary TLV stream at **921 600 baud** that
//! [`RadarStream`](crate::RadarStream) reads. The sensor boots *idle*: until a
//! profile has been sent to the CLI port and its last line, `sensorStart`, has
//! been accepted, the data port produces nothing at all. Waiting for frames from
//! an unconfigured sensor looks exactly like broken wiring, and it is the first
//! thing to check when the data UART is silent.
//!
//! So the connect sequence is: open the CLI port, send the profile, *then* open
//! the data port. [`RadarCli::configure`] is that whole step.
//!
//! # Handshake
//!
//! The CLI is a terminal, not a protocol: it echoes what it is sent and answers
//! in prose. One exchange looks like
//!
//! ```text
//! mmwDemo:/>sensorStop
//! Ignored: Sensor is already stopped
//!
//! Done
//! ```
//!
//! `Done` terminates a command and is the only synchronisation there is — so
//! commands are sent one at a time, each awaiting its own `Done`, rather than
//! blasted at the port with a sleep between them. Everything before it is either
//! the echo, a blank line, or a note the sensor wanted to make (`Ignored: …`,
//! `Debug: Init Calibration Status = 0x1ffe`); notes are returned to the caller
//! rather than discarded, because the calibration status after `sensorStart` is
//! the one line worth having in the journal. A line starting with `Error` fails
//! the command and aborts the run: continuing past a rejected `profileCfg` would
//! start the sensor in some other configuration than the one asked for, and the
//! frames would look plausible.

use std::{fs, io, path::Path, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::{Instant, timeout, timeout_at},
};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::RadarError;

/// Baud rate of the configuration UART. Fixed by the demo firmware; the data
/// UART's 921 600 is the one that follows the profile.
pub const DEFAULT_CLI_BAUD_RATE: u32 = 115_200;

/// Default wait for a single command's `Done`. Generous because `sensorStart`
/// answers only after RF calibration, which is the slowest line in a profile by
/// an order of magnitude.
pub const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 5_000;

/// The prompt the CLI writes before every echoed command.
const PROMPT: &str = "mmwDemo:/>";

/// Factory Out-of-Box profile for the IWR6843ISK, shipped in this crate.
const BUILTIN_PROFILE: &str = include_str!("../profiles/out-of-box-6843isk.cfg");

const READ_BUFFER_LEN: usize = 512;

/// How long the CLI port must stay quiet before whatever it was saying (a boot
/// banner, the tail of someone else's session) is considered finished.
const DRAIN_QUIET: Duration = Duration::from_millis(150);

/// Configuration for opening and driving the IWR6843's CLI UART.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RadarCliConfig {
    /// Serial device path for the *configuration* port — the sensor's first tty,
    /// one below the data port (`/dev/ttyACM0`, `/dev/ttyUSB0`).
    pub cli_port: String,
    /// Baud rate of the CLI UART; [`DEFAULT_CLI_BAUD_RATE`].
    pub baud_rate: u32,
    /// A TI `.cfg` profile to send instead of the built-in Out-of-Box one.
    /// Required for any firmware other than the factory demo.
    #[serde(default)]
    pub profile_path: Option<String>,
    /// How long one command may take to answer `Done`.
    #[serde(default = "default_command_timeout_ms")]
    pub command_timeout_ms: u64,
}

impl Default for RadarCliConfig {
    fn default() -> Self {
        Self {
            // One below `RadarConfig::default().data_port`, matching how the two
            // ports enumerate.
            cli_port: "/dev/ttyUSB0".to_string(),
            baud_rate: DEFAULT_CLI_BAUD_RATE,
            profile_path: None,
            command_timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
        }
    }
}

const fn default_command_timeout_ms() -> u64 {
    DEFAULT_COMMAND_TIMEOUT_MS
}

/// An ordered list of CLI commands — a TI `.cfg` file with the noise removed.
///
/// Two textual forms are accepted, because both are what people actually have:
/// a `.cfg` file (one command per line, `%` comments), and a **pasted session
/// transcript**, where the commands are the text after each `mmwDemo:/>` prompt
/// and everything else is the sensor talking back. The transcript form is
/// detected by the prompt's presence, so a transcript's `Done` and `Debug:`
/// lines can never be mistaken for commands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SensorProfile {
    commands: Vec<String>,
}

impl SensorProfile {
    /// The Out-of-Box profile compiled into this crate
    /// (`profiles/out-of-box-6843isk.cfg`).
    pub fn builtin() -> Self {
        Self::parse(BUILTIN_PROFILE)
    }

    /// Extract the commands from a `.cfg` file or a pasted CLI session.
    pub fn parse(text: &str) -> Self {
        let transcript = text.contains(PROMPT);
        let commands = text
            .lines()
            .filter_map(|line| {
                // In a transcript only prompted text was typed by a human;
                // everything else is the sensor's own output.
                let line = if transcript {
                    line.split_once(PROMPT)?.1
                } else {
                    line
                };
                let command = line.split(['%', '#']).next().unwrap_or(line).trim();
                (!command.is_empty()).then(|| command.to_string())
            })
            .collect();
        Self { commands }
    }

    /// Read a profile from disk.
    ///
    /// A file that parses to no commands is an error rather than an empty run:
    /// silently configuring nothing leaves the sensor idle, and the symptom
    /// (a data port that never speaks) points nowhere near the cause.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, RadarError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|error| RadarError::Profile {
            path: path.display().to_string(),
            error,
        })?;
        let profile = Self::parse(&text);
        if profile.is_empty() {
            return Err(RadarError::Profile {
                path: path.display().to_string(),
                error: io::Error::new(
                    io::ErrorKind::InvalidData,
                    "no configuration commands found",
                ),
            });
        }
        Ok(profile)
    }

    /// The commands, in the order they must be sent.
    pub fn commands(&self) -> &[String] {
        &self.commands
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// What one configuration run did, for the caller to log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigureReport {
    /// How many commands the sensor accepted.
    pub commands: usize,
    /// Every line the sensor produced that was neither an echo nor `Done`,
    /// prefixed with the command that provoked it — `sensorStart: Debug: Init
    /// Calibration Status = 0x1ffe`.
    pub notes: Vec<String>,
}

/// The sensor's configuration UART, open and ready to take commands.
pub struct RadarCli {
    session: CliSession<SerialStream>,
}

impl RadarCli {
    /// Open the CLI port described by `config`.
    pub fn open(config: &RadarCliConfig) -> Result<Self, RadarError> {
        let port = tokio_serial::new(&config.cli_port, config.baud_rate)
            .open_native_async()
            .map_err(|error| RadarError::Io(io::Error::other(error)))?;
        Ok(Self {
            session: CliSession::new(port, Duration::from_millis(config.command_timeout_ms)),
        })
    }

    /// The whole connect-time step: read the profile, open the port, send it.
    ///
    /// Returns once the sensor has accepted every command — which, for a profile
    /// ending in `sensorStart`, is the point at which the data UART begins to
    /// produce frames. The port is closed on return; the CLI holds no state that
    /// keeping it open would preserve.
    pub async fn configure(config: &RadarCliConfig) -> Result<ConfigureReport, RadarError> {
        let profile = match &config.profile_path {
            Some(path) => SensorProfile::load(path)?,
            None => SensorProfile::builtin(),
        };
        Self::open(config)?.apply(&profile).await
    }

    /// Send every command in `profile`, each awaiting its own `Done`.
    pub async fn apply(&mut self, profile: &SensorProfile) -> Result<ConfigureReport, RadarError> {
        self.session.apply(profile).await
    }

    /// Send one command and wait for its `Done`, returning the sensor's notes.
    pub async fn send(&mut self, command: &str) -> Result<Vec<String>, RadarError> {
        self.session.send(command).await
    }
}

/// The line handshake, over any byte stream so it can be tested without a tty.
struct CliSession<T> {
    port: T,
    /// Bytes read past the end of the last complete line. The CLI leaves its
    /// prompt un-terminated after `Done`, so there is always a remainder.
    pending: Vec<u8>,
    timeout: Duration,
}

/// How one response line bears on the command in flight.
enum Response {
    /// `Done` — the command finished.
    Finished,
    /// The sensor refused the command.
    Rejected,
    /// Something the sensor said along the way, worth keeping.
    Note,
    /// The echo, the prompt, or a blank line.
    Noise,
}

impl<T> CliSession<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn new(port: T, timeout: Duration) -> Self {
        Self {
            port,
            pending: Vec::new(),
            timeout,
        }
    }

    /// Send every command in `profile`, each awaiting its own `Done`.
    async fn apply(&mut self, profile: &SensorProfile) -> Result<ConfigureReport, RadarError> {
        // Whatever the port was saying before this run — a boot banner, the tail
        // of a previous session — is not an answer to anything sent here.
        self.drain().await;

        let mut report = ConfigureReport::default();
        for command in profile.commands() {
            let notes = self.send(command).await?;
            report.commands += 1;
            report
                .notes
                .extend(notes.into_iter().map(|note| format!("{command}: {note}")));
        }
        Ok(report)
    }

    /// Discard anything already waiting on the port, up to [`DRAIN_QUIET`] of
    /// silence.
    async fn drain(&mut self) {
        let mut buffer = [0; READ_BUFFER_LEN];
        while let Ok(Ok(read)) = timeout(DRAIN_QUIET, self.port.read(&mut buffer)).await {
            if read == 0 {
                break;
            }
        }
        self.pending.clear();
    }

    async fn send(&mut self, command: &str) -> Result<Vec<String>, RadarError> {
        self.port.write_all(command.as_bytes()).await?;
        self.port.write_all(b"\n").await?;
        self.port.flush().await?;

        // One deadline for the whole command, not per read: a sensor dribbling
        // one byte at a time is as stuck as one saying nothing.
        let deadline = Instant::now() + self.timeout;
        let mut notes = Vec::new();
        loop {
            let line = self.next_line(deadline, command).await?;
            match classify(&line, command) {
                Response::Finished => return Ok(notes),
                Response::Rejected => {
                    return Err(RadarError::CommandRejected {
                        command: command.to_string(),
                        response: line,
                    });
                }
                Response::Note => notes.push(line),
                Response::Noise => {}
            }
        }
    }

    /// The next `\n`-terminated line, trimmed, or why there will not be one.
    async fn next_line(&mut self, deadline: Instant, command: &str) -> Result<String, RadarError> {
        loop {
            if let Some(index) = self.pending.iter().position(|&byte| byte == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=index).collect();
                return Ok(String::from_utf8_lossy(&line).trim().to_string());
            }

            let mut buffer = [0; READ_BUFFER_LEN];
            let read = timeout_at(deadline, self.port.read(&mut buffer))
                .await
                .map_err(|_| RadarError::CommandTimeout {
                    command: command.to_string(),
                    timeout: self.timeout,
                })??;
            if read == 0 {
                return Err(RadarError::CommandClosed {
                    command: command.to_string(),
                });
            }
            self.pending.extend_from_slice(&buffer[..read]);
        }
    }
}

/// Classify one response line against the command that is in flight.
fn classify(line: &str, command: &str) -> Response {
    // A line can arrive with the previous command's un-terminated prompt glued
    // to its front, so the prompt is stripped here rather than in the reader.
    let text = line
        .rsplit_once(PROMPT)
        .map_or(line, |(_, rest)| rest)
        .trim();
    if text.is_empty() || text == command {
        return Response::Noise;
    }
    if text.eq_ignore_ascii_case("done") {
        return Response::Finished;
    }
    let lowered = text.to_ascii_lowercase();
    if lowered.starts_with("error")
        || lowered.contains("not recognized as a cli command")
        || lowered.contains("not a valid cli command")
    {
        return Response::Rejected;
    }
    Response::Note
}

#[cfg(test)]
mod tests {
    use std::{
        pin::Pin,
        sync::{Arc, Mutex},
        task::{Context, Poll},
    };

    use tokio::io::{DuplexStream, duplex};

    use super::*;

    #[test]
    fn parses_a_cfg_file_ignoring_comments_and_blanks() {
        let profile = SensorProfile::parse(
            "% a comment\n\
             \n\
             sensorStop\n\
             flushCfg   % trailing note\n\
             # another comment style\n\
             \tdfeDataOutputMode 1\t\n",
        );

        assert_eq!(
            profile.commands(),
            ["sensorStop", "flushCfg", "dfeDataOutputMode 1"]
        );
    }

    /// The form a person is most likely to have: text copied out of a terminal.
    /// The sensor's own replies must not survive as commands.
    #[test]
    fn parses_a_pasted_session_transcript() {
        let profile = SensorProfile::parse(
            "mmwDemo:/>sensorStop\n\
             Ignored: Sensor is already stopped\n\
             \n\
             Done\n\
             \n\
             mmwDemo:/>flushCfg\n\
             Done\n\
             \n\
             mmwDemo:/>sensorStart\n\
             Debug: Init Calibration Status = 0x1ffe\n\
             \n\
             Done\n",
        );

        assert_eq!(
            profile.commands(),
            ["sensorStop", "flushCfg", "sensorStart"]
        );
    }

    /// Re-sending must be safe on a sensor that is already streaming, and the
    /// data UART stays silent until the last line runs.
    #[test]
    fn builtin_profile_stops_flushes_and_starts() {
        let profile = SensorProfile::builtin();
        let commands = profile.commands();

        assert_eq!(commands[0], "sensorStop");
        assert_eq!(commands[1], "flushCfg");
        assert_eq!(commands.last().unwrap(), "sensorStart");
        assert!(commands.iter().any(|c| c.starts_with("profileCfg ")));
        assert!(!profile.is_empty());
    }

    #[tokio::test]
    async fn waits_for_done_past_the_echo_and_keeps_the_notes() {
        let (port, sensor) = duplex(256);
        let seen = spawn_sensor(sensor, |command| match command {
            "sensorStop" => vec!["Ignored: Sensor is already stopped".to_string()],
            _ => Vec::new(),
        });
        let mut session = CliSession::new(port, Duration::from_secs(2));

        assert_eq!(
            session.send("sensorStop").await.unwrap(),
            ["Ignored: Sensor is already stopped"]
        );
        assert!(session.send("flushCfg").await.unwrap().is_empty());
        assert_eq!(*seen.lock().unwrap(), ["sensorStop", "flushCfg"]);
    }

    #[tokio::test]
    async fn a_rejected_command_is_an_error_naming_it() {
        let (port, sensor) = duplex(256);
        spawn_sensor(sensor, |_| vec!["Error -3".to_string()]);
        let mut session = CliSession::new(port, Duration::from_secs(2));

        let error = session.send("profileCfg 0 60").await.unwrap_err();
        let RadarError::CommandRejected { command, response } = error else {
            panic!("expected a rejection, got {error:?}");
        };
        assert_eq!(command, "profileCfg 0 60");
        assert_eq!(response, "Error -3");
    }

    /// A command that never answers must not hang the whole start-up.
    #[tokio::test]
    async fn a_silent_sensor_times_out() {
        let (port, sensor) = duplex(256);
        let _held = sensor; // opened, never answers
        let mut session = CliSession::new(port, Duration::from_millis(50));

        assert!(matches!(
            session.send("sensorStart").await,
            Err(RadarError::CommandTimeout { .. })
        ));
    }

    /// End-of-stream is distinct from a timeout: the port took the command and
    /// then went away, which is a sensor that reset or a tty that vanished.
    #[tokio::test]
    async fn a_port_at_end_of_stream_is_an_error() {
        let mut session = CliSession::new(MuteSensor, Duration::from_secs(2));

        assert!(matches!(
            session.send("sensorStop").await,
            Err(RadarError::CommandClosed { .. })
        ));
    }

    /// The whole profile reaches the sensor, in order, and the run stops at the
    /// first refusal rather than starting the sensor half-configured.
    #[tokio::test]
    async fn apply_sends_every_command_and_stops_at_a_refusal() {
        let (port, sensor) = duplex(1024);
        let seen = spawn_sensor(sensor, |command| match command {
            "chirpCfg 0" => vec!["Error -1".to_string()],
            "sensorStart" => vec!["Debug: Init Calibration Status = 0x1ffe".to_string()],
            _ => Vec::new(),
        });
        let mut session = CliSession::new(port, Duration::from_secs(2));

        let report = session
            .apply(&SensorProfile::parse("sensorStop\nflushCfg\nsensorStart\n"))
            .await
            .unwrap();
        assert_eq!(report.commands, 3);
        assert_eq!(
            report.notes,
            ["sensorStart: Debug: Init Calibration Status = 0x1ffe"]
        );

        assert!(matches!(
            session
                .apply(&SensorProfile::parse("flushCfg\nchirpCfg 0\nsensorStart\n"))
                .await,
            Err(RadarError::CommandRejected { .. })
        ));
        assert_eq!(
            *seen.lock().unwrap(),
            [
                "sensorStop",
                "flushCfg",
                "sensorStart",
                "flushCfg",
                "chirpCfg 0"
            ]
        );
    }

    /// A port that accepts everything written to it and is already at
    /// end-of-stream for reads.
    struct MuteSensor;

    impl AsyncRead for MuteSensor {
        fn poll_read(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(())) // no bytes filled in: EOF
        }
    }

    impl AsyncWrite for MuteSensor {
        fn poll_write(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            buffer: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(buffer.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    /// A stand-in for the sensor's CLI: echo the command the way a terminal
    /// does, answer with `reply`'s lines, then leave an un-terminated prompt —
    /// the detail that makes the next echo arrive prompt-prefixed.
    fn spawn_sensor(
        mut port: DuplexStream,
        reply: impl Fn(&str) -> Vec<String> + Send + 'static,
    ) -> Arc<Mutex<Vec<String>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        tokio::spawn(async move {
            let mut pending: Vec<u8> = Vec::new();
            let mut buffer = [0; 128];
            while let Ok(read) = port.read(&mut buffer).await {
                if read == 0 {
                    break;
                }
                pending.extend_from_slice(&buffer[..read]);

                while let Some(index) = pending.iter().position(|&byte| byte == b'\n') {
                    let line: Vec<u8> = pending.drain(..=index).collect();
                    let command = String::from_utf8_lossy(&line).trim().to_string();
                    recorder.lock().unwrap().push(command.clone());

                    let mut response = format!("{command}\r\n");
                    for note in reply(&command) {
                        response.push_str(&note);
                        response.push_str("\r\n");
                    }
                    response.push_str("\r\nDone\r\n\r\n");
                    response.push_str(PROMPT);
                    if port.write_all(response.as_bytes()).await.is_err() {
                        return;
                    }
                }
            }
        });
        seen
    }
}
