// SPDX-License-Identifier: Apache-2.0

//! Pure-Rust IWR6843 UART framing, TLV parsing, and indicator extraction.
//!
//! On 404-snf the radar's data UART hangs off USART6, which the board's RIF
//! configuration reaches only from the CM33 — so on the CA35 the bytes arrive
//! over the `radar` IPC channel, not from a local serial port. The framing is
//! therefore transport-independent: push bytes into a [`RadarDecoder`] from
//! wherever they came and pull [`RadarFrame`]s back out.
//!
//! [`RadarStream`] (feature `serial`, on by default) is the same decoder behind
//! a local `tokio-serial` port. It is what you want on a dev host with the
//! sensor on USB, and it is not used in the on-target data path.
//!
//! The sensor's *second* UART — the 115 200-baud `mmwDemo:/>` CLI — is
//! [`RadarCli`]. It is not optional: the IWR6843 boots idle and the data port
//! stays silent until a profile ending in `sensorStart` has been sent there, so
//! [`RadarCli::configure`] runs before [`RadarStream::open`].

#[cfg(feature = "serial")]
mod cli;
mod decoder;
mod indicators;
mod parser;

#[cfg(feature = "serial")]
pub use cli::{
    ConfigureReport, DEFAULT_CLI_BAUD_RATE, DEFAULT_COMMAND_TIMEOUT_MS, RadarCli, RadarCliConfig,
    SensorProfile,
};
pub use decoder::{DEFAULT_MAX_PACKET_LENGTH, RadarDecoder};
pub use indicators::{
    ActivityTrend, GrossActivity, IndicatorConfig, IndicatorEngine, IndicatorSnapshot, RadarRoi,
};
#[cfg(feature = "vital-signs")]
pub use indicators::{VitalRateEstimate, VitalStatus};
#[cfg(feature = "vital-signs")]
pub use parser::VitalSignsReading;
pub use parser::{
    FrameHeader, MAGIC_WORD, ParseError, ProcessingStats, RadarFrame, RadarPoint, RadarProtocol,
    RangeProfile, TemperatureStats, parse_frame, parse_frame_for,
};

use std::{error::Error, fmt, io, time::Duration};

use parser::FRAME_HEADER_LEN;

/// Serial, framing, packet parsing, or sensor-configuration failure.
#[derive(Debug)]
pub enum RadarError {
    Io(io::Error),
    InvalidPacketLength {
        declared: usize,
        maximum: usize,
    },
    Parse(ParseError),
    /// A configuration profile could not be read, or held no commands.
    Profile {
        path: String,
        error: io::Error,
    },
    /// The sensor's CLI answered a configuration command with an error. The run
    /// stops here: a sensor started under a partially applied profile produces
    /// frames that look plausible and mean something else.
    CommandRejected {
        command: String,
        response: String,
    },
    /// A configuration command produced no `Done` in time.
    CommandTimeout {
        command: String,
        timeout: Duration,
    },
    /// The CLI port reached end-of-stream part-way through a command.
    CommandClosed {
        command: String,
    },
}

impl fmt::Display for RadarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "radar UART error: {error}"),
            Self::InvalidPacketLength { declared, maximum } => write!(
                f,
                "radar packet declares invalid length {declared}; allowed range is {FRAME_HEADER_LEN}..={maximum}"
            ),
            Self::Parse(error) => write!(f, "radar packet parse error: {error}"),
            Self::Profile { path, error } => {
                write!(f, "radar configuration profile {path}: {error}")
            }
            Self::CommandRejected { command, response } => write!(
                f,
                "radar rejected configuration command `{command}`: {response}"
            ),
            Self::CommandTimeout { command, timeout } => write!(
                f,
                "radar configuration command `{command}` produced no `Done` within {} ms",
                timeout.as_millis()
            ),
            Self::CommandClosed { command } => write!(
                f,
                "radar CLI port closed while running configuration command `{command}`"
            ),
        }
    }
}

impl Error for RadarError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) | Self::Profile { error, .. } => Some(error),
            Self::Parse(error) => Some(error),
            Self::InvalidPacketLength { .. }
            | Self::CommandRejected { .. }
            | Self::CommandTimeout { .. }
            | Self::CommandClosed { .. } => None,
        }
    }
}

impl From<io::Error> for RadarError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ParseError> for RadarError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

#[cfg(feature = "serial")]
mod serial {
    use serde::{Deserialize, Serialize};
    use tokio::io::{AsyncRead, AsyncReadExt};
    use tokio_serial::{SerialPortBuilderExt, SerialStream};

    use crate::{DEFAULT_MAX_PACKET_LENGTH, RadarDecoder, RadarError, RadarFrame, RadarProtocol};

    const READ_BUFFER_LEN: usize = 4096;

    /// Configuration for opening the IWR6843 data UART locally.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct RadarConfig {
        /// Serial device path for the radar data port (for example `/dev/ttyUSB1`).
        pub data_port: String,
        /// Baud rate of the data UART.
        pub baud_rate: u32,
        /// Must match the firmware currently flashed on the sensor.
        #[serde(default)]
        pub protocol: RadarProtocol,
        /// Hard allocation bound for a declared UART packet.
        #[serde(default = "default_max_packet_length")]
        pub max_packet_length: usize,
    }

    impl Default for RadarConfig {
        fn default() -> Self {
            Self {
                data_port: "/dev/ttyUSB1".to_string(),
                baud_rate: 921_600,
                protocol: RadarProtocol::OutOfBox,
                max_packet_length: DEFAULT_MAX_PACKET_LENGTH,
            }
        }
    }

    const fn default_max_packet_length() -> usize {
        DEFAULT_MAX_PACKET_LENGTH
    }

    /// Async source of reframed, parsed radar packets from a local serial port.
    ///
    /// Bench/dev-host path only: on the STM32MP257 the radar UART belongs to the
    /// CM33, and the CA35 feeds a [`RadarDecoder`] from IPC chunks instead.
    pub struct RadarStream {
        reader: RadarReader<SerialStream>,
    }

    impl RadarStream {
        /// Open the radar data UART described by `config`.
        pub fn open(config: RadarConfig) -> Result<Self, RadarError> {
            let serial = tokio_serial::new(&config.data_port, config.baud_rate)
                .open_native_async()
                .map_err(|error| RadarError::Io(std::io::Error::other(error)))?;
            Ok(Self {
                reader: RadarReader::new(serial, config.protocol, config.max_packet_length),
            })
        }

        /// Await the next fully framed packet.
        ///
        /// Returns `Ok(None)` on end-of-stream. A malformed packet is consumed
        /// before its error is returned, so the next call can resynchronize.
        pub async fn next_frame(&mut self) -> Result<Option<RadarFrame>, RadarError> {
            self.reader.next_frame().await
        }
    }

    struct RadarReader<R> {
        input: R,
        decoder: RadarDecoder,
        read_buffer: [u8; READ_BUFFER_LEN],
    }

    impl<R> RadarReader<R>
    where
        R: AsyncRead + Unpin,
    {
        fn new(input: R, protocol: RadarProtocol, max_packet_length: usize) -> Self {
            Self {
                input,
                decoder: RadarDecoder::new(protocol, max_packet_length),
                read_buffer: [0; READ_BUFFER_LEN],
            }
        }

        async fn next_frame(&mut self) -> Result<Option<RadarFrame>, RadarError> {
            loop {
                if let Some(frame) = self.decoder.next_frame()? {
                    return Ok(Some(frame));
                }

                let received = self.input.read(&mut self.read_buffer).await?;
                if received == 0 {
                    return Ok(None);
                }
                self.decoder.push(&self.read_buffer[..received]);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::MAGIC_WORD;
        use std::{
            io,
            pin::Pin,
            task::{Context, Poll},
        };
        use tokio::io::{AsyncWriteExt, duplex};

        #[tokio::test]
        async fn async_reader_reassembles_chunks_and_reports_eof() {
            let packet = make_empty_frame(42);
            let (mut writer, input) = duplex(64);
            let expected = packet.clone();
            let writer_task = tokio::spawn(async move {
                writer.write_all(&expected[..13]).await.unwrap();
                writer.write_all(&expected[13..]).await.unwrap();
            });
            let mut reader = RadarReader::new(input, RadarProtocol::OutOfBox, 1024);

            let frame = reader.next_frame().await.unwrap().unwrap();
            assert_eq!(frame.frame_number(), 42);
            writer_task.await.unwrap();
            assert!(reader.next_frame().await.unwrap().is_none());
        }

        #[tokio::test]
        async fn async_reader_propagates_io_errors() {
            let mut reader = RadarReader::new(BrokenReader, RadarProtocol::OutOfBox, 1024);
            assert!(matches!(reader.next_frame().await, Err(RadarError::Io(_))));
        }

        struct BrokenReader;

        impl AsyncRead for BrokenReader {
            fn poll_read(
                self: Pin<&mut Self>,
                _context: &mut Context<'_>,
                _buffer: &mut tokio::io::ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                Poll::Ready(Err(io::Error::other("synthetic read failure")))
            }
        }

        fn make_empty_frame(frame_number: u32) -> Vec<u8> {
            let packet_length = 64_u32;
            let mut frame = vec![0; packet_length as usize];
            frame[..8].copy_from_slice(&MAGIC_WORD);
            frame[8..12].copy_from_slice(&0x03_06_00_00_u32.to_le_bytes());
            frame[12..16].copy_from_slice(&packet_length.to_le_bytes());
            frame[16..20].copy_from_slice(&0x000a_6843_u32.to_le_bytes());
            frame[20..24].copy_from_slice(&frame_number.to_le_bytes());
            frame
        }
    }
}

#[cfg(feature = "serial")]
pub use serial::{RadarConfig, RadarStream};
