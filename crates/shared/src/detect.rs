// SPDX-License-Identifier: Apache-2.0

//! `no_std` IWR6843 packet framing and TLV parsing, for the CM33.
//!
//! `snf-radar` already parses these packets, but it is a `std` crate built
//! around `Vec` — it cannot run on the CM33. This module is the subset the
//! firmware needs: reassemble packets from a UART byte stream
//! ([`PacketAssembler`]), then reduce one packet to the fixed-size
//! [`RadarReport`] that crosses the IPC boundary ([`parse_report`]). No
//! allocation, no floating-point library calls.
//!
//! It lives beside the IPC types on purpose. The report *is* the wire format, so
//! keeping its producer next to its definition means the two cannot drift, and
//! it stays testable with a plain `cargo test -p snf-shared` on the host.
//!
//! Only the Out-of-Box demo's Cartesian point TLV (type 1) is decoded. Any other
//! TLV — side info, range profiles, the Vital Signs firmware's compressed
//! spherical points — is skipped by length, which is why frames still parse when
//! the sensor is configured to emit extras.

use crate::{RADAR_REPORT_POINTS, RadarPointFixed, RadarReport};

/// Byte sequence that starts every IWR6843 UART packet.
pub const MAGIC_WORD: [u8; 8] = [0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0x08, 0x07];

/// Radial speed at or above which a point counts toward
/// [`RadarReport::moving_points`].
pub const MOVING_MM_S: i32 = 50;

const FRAME_HEADER_LEN: usize = 40;
const TLV_HEADER_LEN: usize = 8;
const POINT_LEN: usize = 16;
const DETECTED_POINTS: u32 = 1;

/// Why a packet could not be reduced to a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectError {
    /// Shorter than a frame header.
    TooShort,
    /// Does not start with [`MAGIC_WORD`].
    InvalidMagic,
    /// The header's declared length disagrees with the buffer's.
    LengthMismatch { declared: usize, actual: usize },
}

/// Reassembles whole IWR6843 packets from an arbitrarily chunked byte stream.
///
/// `N` bounds the largest packet accepted; a header declaring more is treated as
/// corruption and the assembler resynchronizes from the next magic word. Size it
/// for the sensor's configuration — an Out-of-Box frame with 96 points and side
/// info is under 2 KiB.
pub struct PacketAssembler<const N: usize> {
    buffer: [u8; N],
    /// Bytes held in `buffer`.
    len: usize,
    /// Declared total packet length, known once the header is complete.
    declared: Option<usize>,
    /// How many leading bytes of [`MAGIC_WORD`] have matched while searching.
    matched: usize,
}

impl<const N: usize> Default for PacketAssembler<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> PacketAssembler<N> {
    /// An empty assembler, searching for the next magic word.
    pub const fn new() -> Self {
        const {
            assert!(
                N >= FRAME_HEADER_LEN,
                "PacketAssembler needs room for at least one frame header"
            );
        }
        Self {
            buffer: [0; N],
            len: 0,
            declared: None,
            matched: 0,
        }
    }

    /// Discard any partial packet and resume searching for a magic word.
    ///
    /// Call this after a known gap in the stream (a UART overrun), so the tail of
    /// a truncated packet is not spliced onto the bytes that follow it — joined
    /// bytes can pass the length check and parse as a plausible but wrong frame.
    pub fn reset(&mut self) {
        self.len = 0;
        self.declared = None;
        self.matched = 0;
    }

    /// Feed received bytes, invoking `on_packet` once per complete packet.
    pub fn push(&mut self, bytes: &[u8], mut on_packet: impl FnMut(&[u8])) {
        for &byte in bytes {
            if let Some(end) = self.push_byte(byte) {
                on_packet(&self.buffer[..end]);
                self.reset();
            }
        }
    }

    /// Absorb one byte; returns the packet length when one just completed.
    fn push_byte(&mut self, byte: u8) -> Option<usize> {
        // Still hunting for the magic word: nothing is buffered yet.
        if self.len == 0 {
            if byte == MAGIC_WORD[self.matched] {
                self.matched += 1;
                if self.matched == MAGIC_WORD.len() {
                    self.buffer[..MAGIC_WORD.len()].copy_from_slice(&MAGIC_WORD);
                    self.len = MAGIC_WORD.len();
                    self.matched = 0;
                }
            } else {
                // Restart the match, allowing this byte to be a fresh start.
                self.matched = usize::from(byte == MAGIC_WORD[0]);
            }
            return None;
        }

        self.buffer[self.len] = byte;
        self.len += 1;

        if self.len == FRAME_HEADER_LEN {
            let declared = u32_at(&self.buffer, 12) as usize;
            if declared < FRAME_HEADER_LEN || declared > N {
                // Implausible length: this was not really a packet start.
                self.reset();
                return None;
            }
            self.declared = Some(declared);
            if declared == FRAME_HEADER_LEN {
                return Some(declared);
            }
        }

        match self.declared {
            Some(declared) if self.len >= declared => Some(declared),
            _ => None,
        }
    }
}

/// Reduce one complete packet to a [`RadarReport`], leaving `seq` at 0.
///
/// Malformed TLVs stop the walk rather than failing the frame: a partially
/// decoded frame with a correct point count is more useful to the indicator
/// engine than no frame, and the frame header's own `num_detected` still shows
/// what was really out there.
pub fn parse_report(packet: &[u8]) -> Result<RadarReport, DetectError> {
    if packet.len() < FRAME_HEADER_LEN {
        return Err(DetectError::TooShort);
    }
    if packet[..MAGIC_WORD.len()] != MAGIC_WORD {
        return Err(DetectError::InvalidMagic);
    }
    let declared = u32_at(packet, 12) as usize;
    if declared != packet.len() {
        return Err(DetectError::LengthMismatch {
            declared,
            actual: packet.len(),
        });
    }

    let mut report = RadarReport::empty(0);
    report.fresh = true;
    report.frame_number = u32_at(packet, 20);
    report.num_detected = u32_at(packet, 28).min(u32::from(u16::MAX)) as u16;
    let num_tlvs = u32_at(packet, 32);

    let mut offset = FRAME_HEADER_LEN;
    let mut parsed: u32 = 0;
    let mut moving: u32 = 0;
    let mut speed_sum: u64 = 0;
    let mut nearest = i32::MAX;

    for _ in 0..num_tlvs {
        let available = declared - offset;
        if available < TLV_HEADER_LEN {
            break;
        }
        let tlv_type = u32_at(packet, offset);
        let payload_len = u32_at(packet, offset + 4) as usize;
        let Some(total) = payload_len.checked_add(TLV_HEADER_LEN) else {
            break;
        };
        if total > available {
            break;
        }

        if tlv_type == DETECTED_POINTS {
            let payload = &packet[offset + TLV_HEADER_LEN..offset + total];
            for record in payload.chunks_exact(POINT_LEN) {
                let point = RadarPointFixed {
                    x_mm: mm(f32_at(record, 0)),
                    y_mm: mm(f32_at(record, 4)),
                    z_mm: mm(f32_at(record, 8)),
                    velocity_mm_s: mm(f32_at(record, 12)),
                };

                let speed = i32::from(point.velocity_mm_s).unsigned_abs();
                speed_sum += u64::from(speed);
                if speed as i32 >= MOVING_MM_S {
                    moving += 1;
                }
                // Only points in front of the sensor bound "nearest"; the
                // firmware occasionally emits small negative ranges.
                if point.y_mm > 0 {
                    nearest = nearest.min(i32::from(point.y_mm));
                }

                if (parsed as usize) < RADAR_REPORT_POINTS {
                    report.points[parsed as usize] = point;
                } else {
                    report.truncated = true;
                }
                parsed += 1;
            }
        }

        offset += total;
    }

    report.num_points = (parsed as usize).min(RADAR_REPORT_POINTS) as u8;
    report.moving_points = moving.min(u32::from(u8::MAX)) as u8;
    if nearest != i32::MAX {
        report.nearest_mm = nearest.min(i32::from(u16::MAX)) as u16;
    }
    if parsed > 0 {
        report.mean_speed_mm_s = (speed_sum / u64::from(parsed)).min(u64::from(u16::MAX)) as u16;
    }

    Ok(report)
}

/// Metres to millimetres, saturating. Float-to-int `as` saturates in Rust, so a
/// `NaN` or absurd coordinate clamps instead of wrapping.
fn mm(metres: f32) -> i16 {
    (metres * 1000.0) as i16
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_bits(u32_at(bytes, offset))
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec::Vec;

    use super::*;

    /// Build an Out-of-Box packet with `points` as `(x, y, z, velocity)` metres.
    fn frame(frame_number: u32, points: &[(f32, f32, f32, f32)]) -> Vec<u8> {
        let payload_len = points.len() * POINT_LEN;
        let total = FRAME_HEADER_LEN + TLV_HEADER_LEN + payload_len;
        let mut packet = std::vec![0u8; total];
        packet[..8].copy_from_slice(&MAGIC_WORD);
        packet[12..16].copy_from_slice(&(total as u32).to_le_bytes());
        packet[20..24].copy_from_slice(&frame_number.to_le_bytes());
        packet[28..32].copy_from_slice(&(points.len() as u32).to_le_bytes());
        packet[32..36].copy_from_slice(&1u32.to_le_bytes()); // one TLV

        packet[40..44].copy_from_slice(&DETECTED_POINTS.to_le_bytes());
        packet[44..48].copy_from_slice(&(payload_len as u32).to_le_bytes());
        for (index, (x, y, z, velocity)) in points.iter().enumerate() {
            let at = 48 + index * POINT_LEN;
            packet[at..at + 4].copy_from_slice(&x.to_le_bytes());
            packet[at + 4..at + 8].copy_from_slice(&y.to_le_bytes());
            packet[at + 8..at + 12].copy_from_slice(&z.to_le_bytes());
            packet[at + 12..at + 16].copy_from_slice(&velocity.to_le_bytes());
        }
        packet
    }

    fn collect(assembler: &mut PacketAssembler<4096>, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        assembler.push(bytes, |packet| packets.push(packet.to_vec()));
        packets
    }

    #[test]
    fn assembles_across_arbitrary_chunk_boundaries() {
        let packet = frame(7, &[(0.1, 1.0, 0.0, 0.2)]);
        let mut assembler = PacketAssembler::<4096>::new();

        let mut packets = Vec::new();
        for chunk in packet.chunks(3) {
            packets.extend(collect(&mut assembler, chunk));
        }

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0], packet);
    }

    #[test]
    fn skips_leading_noise_and_a_split_magic_word() {
        let packet = frame(1, &[]);
        let mut assembler = PacketAssembler::<4096>::new();

        // Noise that includes a false start on the first magic byte.
        assert!(collect(&mut assembler, &[0xff, 0x02, 0xff, 0x02]).is_empty());
        let packets = collect(&mut assembler, &packet);

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0], packet);
    }

    #[test]
    fn rejects_an_implausible_length_and_recovers() {
        let mut corrupt = frame(1, &[]);
        corrupt[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        let valid = frame(2, &[]);
        let mut assembler = PacketAssembler::<4096>::new();

        let mut packets = collect(&mut assembler, &corrupt);
        packets.extend(collect(&mut assembler, &valid));

        assert_eq!(packets.len(), 1);
        assert_eq!(parse_report(&packets[0]).unwrap().frame_number, 2);
    }

    #[test]
    fn parses_points_and_aggregates() {
        let packet = frame(
            42,
            &[
                (0.0, 2.000, 0.0, 0.010),  // still, far
                (0.5, 1.000, 0.1, -0.300), // moving, near
                (0.0, 1.500, 0.0, 0.100),  // moving
            ],
        );

        let report = parse_report(&packet).unwrap();

        assert!(report.fresh);
        assert_eq!(report.frame_number, 42);
        assert_eq!(report.num_detected, 3);
        assert_eq!(report.num_points, 3);
        assert!(!report.truncated);
        assert_eq!(report.moving_points, 2);
        assert_eq!(report.nearest_mm, 1000);
        // |10| + |-300| + |100| = 410, / 3 = 136
        assert_eq!(report.mean_speed_mm_s, 136);
        assert_eq!(report.points()[1].x_mm, 500);
        assert_eq!(report.points()[1].velocity_mm_s, -300);
    }

    #[test]
    fn truncates_points_but_still_counts_them_all() {
        let points: Vec<(f32, f32, f32, f32)> = (0..RADAR_REPORT_POINTS + 5)
            .map(|index| (0.0, 1.0 + index as f32, 0.0, 1.0))
            .collect();
        let packet = frame(1, &points);

        let report = parse_report(&packet).unwrap();

        assert!(report.truncated);
        assert_eq!(report.num_points, RADAR_REPORT_POINTS as u8);
        assert_eq!(report.num_detected, points.len() as u16);
        assert_eq!(report.moving_points, points.len() as u8);
        assert_eq!(report.nearest_mm, 1000);
    }

    #[test]
    fn empty_frame_reports_no_nearest_point() {
        let report = parse_report(&frame(9, &[])).unwrap();

        assert!(report.fresh);
        assert_eq!(report.num_points, 0);
        assert_eq!(report.nearest_mm, u16::MAX);
        assert_eq!(report.mean_speed_mm_s, 0);
    }

    #[test]
    fn rejects_a_length_that_disagrees_with_the_buffer() {
        let packet = frame(1, &[]);
        assert!(matches!(
            parse_report(&packet[..packet.len() - 1]),
            Err(DetectError::LengthMismatch { .. })
        ));
    }

    #[test]
    fn skips_unknown_tlvs_by_length() {
        // Rebuild the packet with a leading unknown TLV of 12 payload bytes.
        let base = frame(5, &[(0.0, 1.0, 0.0, 0.0)]);
        let mut packet = Vec::new();
        packet.extend_from_slice(&base[..FRAME_HEADER_LEN]);
        packet.extend_from_slice(&999u32.to_le_bytes());
        packet.extend_from_slice(&12u32.to_le_bytes());
        packet.extend_from_slice(&[0xcd; 12]);
        packet.extend_from_slice(&base[FRAME_HEADER_LEN..]);
        let total = packet.len() as u32;
        packet[12..16].copy_from_slice(&total.to_le_bytes());
        packet[32..36].copy_from_slice(&2u32.to_le_bytes());

        let report = parse_report(&packet).unwrap();

        assert_eq!(report.num_points, 1);
        assert_eq!(report.nearest_mm, 1000);
    }
}
