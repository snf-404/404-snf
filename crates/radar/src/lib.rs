// SPDX-License-Identifier: Apache-2.0

//! Pure-Rust IWR6843 UART transport, TLV parsing, and indicator extraction.

mod indicators;
mod parser;

pub use indicators::{
    ActivityTrend, GrossActivity, IndicatorConfig, IndicatorEngine, IndicatorSnapshot, RadarRoi,
};
#[cfg(feature = "vital-signs")]
pub use indicators::{VitalRateEstimate, VitalStatus};
#[cfg(feature = "vital-signs")]
pub use parser::VitalSignsReading;
pub use parser::{
    FrameHeader, MAGIC_WORD, ParseError, RadarFrame, RadarPoint, RadarProtocol, parse_frame,
    parse_frame_for,
};

use std::{error::Error, fmt, io};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use parser::{FRAME_HEADER_LEN, u32_at};

const DEFAULT_MAX_PACKET_LENGTH: usize = 1024 * 1024;
const READ_BUFFER_LEN: usize = 4096;

/// Configuration for opening the IWR6843 data UART.
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

/// Serial, framing, or packet parsing failure.
#[derive(Debug)]
pub enum RadarError {
    Io(io::Error),
    InvalidPacketLength { declared: usize, maximum: usize },
    Parse(ParseError),
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
        }
    }
}

impl Error for RadarError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::InvalidPacketLength { .. } => None,
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

/// Async source of reframed, parsed radar packets.
pub struct RadarStream {
    reader: RadarReader<SerialStream>,
}

impl RadarStream {
    /// Open the radar data UART described by `config`.
    pub fn open(config: RadarConfig) -> Result<Self, RadarError> {
        let serial = tokio_serial::new(&config.data_port, config.baud_rate)
            .open_native_async()
            .map_err(|error| RadarError::Io(io::Error::other(error)))?;
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
    decoder: FrameDecoder,
    protocol: RadarProtocol,
    read_buffer: [u8; READ_BUFFER_LEN],
}

impl<R> RadarReader<R>
where
    R: AsyncRead + Unpin,
{
    fn new(input: R, protocol: RadarProtocol, max_packet_length: usize) -> Self {
        Self {
            input,
            decoder: FrameDecoder::new(max_packet_length),
            protocol,
            read_buffer: [0; READ_BUFFER_LEN],
        }
    }

    async fn next_frame(&mut self) -> Result<Option<RadarFrame>, RadarError> {
        loop {
            if let Some(packet) = self.decoder.next_packet()? {
                return parse_frame_for(self.protocol, &packet)
                    .map(Some)
                    .map_err(RadarError::Parse);
            }

            let received = self.input.read(&mut self.read_buffer).await?;
            if received == 0 {
                return Ok(None);
            }
            self.decoder.push(&self.read_buffer[..received]);
        }
    }
}

struct FrameDecoder {
    bytes: Vec<u8>,
    max_packet_length: usize,
}

impl FrameDecoder {
    fn new(max_packet_length: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_packet_length: max_packet_length.max(FRAME_HEADER_LEN),
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn next_packet(&mut self) -> Result<Option<Vec<u8>>, RadarError> {
        let Some(magic_offset) = self
            .bytes
            .windows(MAGIC_WORD.len())
            .position(|candidate| candidate == MAGIC_WORD)
        else {
            let retained = self.bytes.len().min(MAGIC_WORD.len() - 1);
            if self.bytes.len() > retained {
                self.bytes.drain(..self.bytes.len() - retained);
            }
            return Ok(None);
        };
        if magic_offset != 0 {
            self.bytes.drain(..magic_offset);
        }
        if self.bytes.len() < FRAME_HEADER_LEN {
            return Ok(None);
        }

        let declared = usize::try_from(u32_at(&self.bytes, 12)).unwrap_or(usize::MAX);
        if declared < FRAME_HEADER_LEN || declared > self.max_packet_length {
            self.bytes.drain(..1);
            return Err(RadarError::InvalidPacketLength {
                declared,
                maximum: self.max_packet_length,
            });
        }
        if self.bytes.len() < declared {
            return Ok(None);
        }

        Ok(Some(self.bytes.drain(..declared).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use tokio::io::{AsyncWriteExt, duplex};

    #[test]
    fn decoder_handles_noise_split_magic_and_multiple_packets() {
        let first = make_empty_frame(1);
        let second = make_empty_frame(2);
        let mut decoder = FrameDecoder::new(1024);
        decoder.push(&[0xaa, 0xbb]);
        decoder.push(&first[..5]);
        assert!(decoder.next_packet().unwrap().is_none());
        decoder.push(&first[5..]);
        decoder.push(&second);

        assert_eq!(decoder.next_packet().unwrap().unwrap(), first);
        assert_eq!(decoder.next_packet().unwrap().unwrap(), second);
        assert!(decoder.next_packet().unwrap().is_none());
    }

    #[test]
    fn decoder_rejects_bad_length_then_resynchronizes() {
        let mut corrupt = make_empty_frame(1);
        corrupt[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        let valid = make_empty_frame(2);
        let mut decoder = FrameDecoder::new(1024);
        decoder.push(&corrupt);
        decoder.push(&valid);

        assert!(matches!(
            decoder.next_packet(),
            Err(RadarError::InvalidPacketLength { .. })
        ));
        let recovered = decoder.next_packet().unwrap().unwrap();
        assert_eq!(u32_at(&recovered, 20), 2);
    }

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
