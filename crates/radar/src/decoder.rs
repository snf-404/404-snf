// SPDX-License-Identifier: Apache-2.0

//! Transport-independent framing of the IWR6843 UART byte stream.
//!
//! [`RadarDecoder`] is a push decoder: feed it bytes from wherever they came —
//! a local serial port ([`crate::RadarStream`]) or the CM33 relay's IPC chunks —
//! and pull whole [`RadarFrame`]s back out. It owns the resynchronization rules,
//! so nothing upstream needs to know how packets are delimited:
//!
//! * bytes before the next magic word are discarded, keeping only the trailing
//!   bytes that could still be a split magic word;
//! * a packet whose declared length is impossible is rejected and one byte is
//!   consumed, so the very next call rescans from the following offset;
//! * a partial packet is retained until the rest arrives.

use crate::{
    RadarError, RadarFrame, RadarProtocol,
    parser::{FRAME_HEADER_LEN, MAGIC_WORD, parse_frame_for, u32_at},
};

/// Default hard bound on a declared packet length, in bytes. A packet claiming
/// more than this is treated as corruption rather than trusted into an
/// allocation.
pub const DEFAULT_MAX_PACKET_LENGTH: usize = 1024 * 1024;

/// Reassembles IWR6843 UART packets from an arbitrarily chunked byte stream.
///
/// The decoder holds no I/O. A caller that knows its stream lost bytes (an IPC
/// sequence gap, a UART overrun) should call [`RadarDecoder::resync`] so a
/// truncated packet is not spliced onto the bytes that follow it.
pub struct RadarDecoder {
    bytes: Vec<u8>,
    protocol: RadarProtocol,
    max_packet_length: usize,
}

impl RadarDecoder {
    /// A decoder for `protocol`, bounding declared packet lengths at
    /// `max_packet_length` (clamped up to one frame header).
    pub fn new(protocol: RadarProtocol, max_packet_length: usize) -> Self {
        Self {
            bytes: Vec::new(),
            protocol,
            max_packet_length: max_packet_length.max(FRAME_HEADER_LEN),
        }
    }

    /// A decoder for `protocol` with [`DEFAULT_MAX_PACKET_LENGTH`].
    pub fn with_protocol(protocol: RadarProtocol) -> Self {
        Self::new(protocol, DEFAULT_MAX_PACKET_LENGTH)
    }

    /// Append received bytes to the reassembly buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Bytes currently held pending a complete packet.
    pub fn buffered(&self) -> usize {
        self.bytes.len()
    }

    /// Drop the reassembly buffer after a known gap in the stream.
    ///
    /// Without this, the tail of a packet truncated by the gap would be joined
    /// to whatever arrives next, and the joined bytes could pass the length
    /// check and parse as a plausible but wrong frame.
    pub fn resync(&mut self) {
        self.bytes.clear();
    }

    /// Take the next fully framed, parsed packet, if one is buffered.
    ///
    /// Returns `Ok(None)` when more bytes are needed. A malformed packet is
    /// consumed before its error is returned, so calling again resynchronizes.
    pub fn next_frame(&mut self) -> Result<Option<RadarFrame>, RadarError> {
        let Some(packet) = self.next_packet()? else {
            return Ok(None);
        };
        parse_frame_for(self.protocol, &packet)
            .map(Some)
            .map_err(RadarError::Parse)
    }

    /// Take the next complete packet's bytes, magic word included.
    fn next_packet(&mut self) -> Result<Option<Vec<u8>>, RadarError> {
        let Some(magic_offset) = self
            .bytes
            .windows(MAGIC_WORD.len())
            .position(|candidate| candidate == MAGIC_WORD)
        else {
            // No magic word yet: keep only what could still be a split one.
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

    /// A minimal well-formed packet with no TLVs.
    pub(crate) fn make_empty_frame(frame_number: u32) -> Vec<u8> {
        let packet_length = 64_u32;
        let mut frame = vec![0; packet_length as usize];
        frame[..8].copy_from_slice(&MAGIC_WORD);
        frame[8..12].copy_from_slice(&0x03_06_00_00_u32.to_le_bytes());
        frame[12..16].copy_from_slice(&packet_length.to_le_bytes());
        frame[16..20].copy_from_slice(&0x000a_6843_u32.to_le_bytes());
        frame[20..24].copy_from_slice(&frame_number.to_le_bytes());
        frame
    }

    fn decoder() -> RadarDecoder {
        RadarDecoder::new(RadarProtocol::OutOfBox, 1024)
    }

    #[test]
    fn handles_noise_split_magic_and_multiple_packets() {
        let first = make_empty_frame(1);
        let second = make_empty_frame(2);
        let mut decoder = decoder();
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
    fn rejects_bad_length_then_resynchronizes() {
        let mut corrupt = make_empty_frame(1);
        corrupt[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        let valid = make_empty_frame(2);
        let mut decoder = decoder();
        decoder.push(&corrupt);
        decoder.push(&valid);

        assert!(matches!(
            decoder.next_packet(),
            Err(RadarError::InvalidPacketLength { .. })
        ));
        let recovered = decoder.next_packet().unwrap().unwrap();
        assert_eq!(u32_at(&recovered, 20), 2);
    }

    #[test]
    fn parses_frames_pushed_in_arbitrary_chunks() {
        let packet = make_empty_frame(42);
        let mut decoder = decoder();
        for chunk in packet.chunks(7) {
            decoder.push(chunk);
        }

        let frame = decoder.next_frame().unwrap().unwrap();
        assert_eq!(frame.frame_number(), 42);
        assert!(decoder.next_frame().unwrap().is_none());
    }

    #[test]
    fn resync_drops_a_truncated_packet_instead_of_splicing_it() {
        let truncated = make_empty_frame(1);
        let whole = make_empty_frame(2);
        let mut decoder = decoder();
        decoder.push(&truncated[..30]);
        assert!(decoder.next_frame().unwrap().is_none());
        assert_eq!(decoder.buffered(), 30);

        decoder.resync();
        decoder.push(&whole);

        let frame = decoder.next_frame().unwrap().unwrap();
        assert_eq!(frame.frame_number(), 2);
    }
}
