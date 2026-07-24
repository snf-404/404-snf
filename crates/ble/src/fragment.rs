// SPDX-License-Identifier: Apache-2.0

//! Splitting a logical telemetry message into ATT-sized notifications.
//!
//! A single logical payload (a Vitals record, a pose skeleton, a point-cloud
//! frame) may not fit in one BLE notification, so it is fragmented: each
//! fragment repeats the 16-byte [`TelemetryHeader`](crate::protocol::TelemetryHeader)
//! and carries a slice of the payload identified by `fragment_offset`, with the
//! `MORE_FRAGMENTS` flag set on all but the last (`PROTOCOL.md` §6). The client
//! reassembles by `(message_type, sequence)`.
//!
//! This is pure and host-testable: it turns bytes into framed bytes and never
//! touches BlueZ.

use crate::protocol::{MessageType, TELEMETRY_HEADER_LEN, TelemetryHeader, header_flags};

/// Why a message could not be fragmented for a given link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FragmentError {
    /// `att_mtu` leaves no room for even one payload byte after the ATT opcode
    /// (`3` bytes) and the 16-byte telemetry header. The caller should raise the
    /// MTU (or drop this stream) rather than emit useless header-only frames.
    MtuTooSmall,
    /// The logical payload is larger than `fragment_offset` (a `u16`) can
    /// address. Practically unreachable for v1 payloads, but checked so an
    /// oversized point cloud fails loudly instead of wrapping the offset.
    PayloadTooLarge,
}

/// Per-fragment overhead: the ATT notification header the controller adds.
const ATT_NOTIFY_OVERHEAD: usize = 3;

/// Maximum payload bytes carriable per fragment on a link with `att_mtu`.
///
/// Returns `None` if the header does not fit. At the default ATT MTU of 23 this
/// is 4 bytes, which is legal but inefficient — clients should negotiate a
/// larger MTU (`PROTOCOL.md` §6).
pub fn max_fragment_payload(att_mtu: usize) -> Option<usize> {
    att_mtu
        .checked_sub(ATT_NOTIFY_OVERHEAD)?
        .checked_sub(TELEMETRY_HEADER_LEN as usize)
        .filter(|&n| n > 0)
}

/// Fragment `payload` into ready-to-notify frames.
///
/// `base_flags` carries message-level flags (`SNAPSHOT`, `DEGRADED`, `STALE`);
/// `MORE_FRAGMENTS` is managed here and must not be set by the caller. Each
/// returned `Vec` is one complete notification value (header + fragment) no
/// longer than `att_mtu - 3`.
///
/// A zero-length payload still yields one header-only frame so a client sees the
/// sequence advance; in practice every v1 payload has a fixed non-empty size.
pub fn fragment(
    message_type: MessageType,
    sequence: u32,
    timestamp_ms: u32,
    base_flags: u8,
    payload: &[u8],
    att_mtu: usize,
) -> Result<Vec<Vec<u8>>, FragmentError> {
    let chunk = max_fragment_payload(att_mtu).ok_or(FragmentError::MtuTooSmall)?;
    let total = u16::try_from(payload.len()).map_err(|_| FragmentError::PayloadTooLarge)?;

    let mut frames = Vec::new();
    let mut offset = 0usize;
    loop {
        let end = (offset + chunk).min(payload.len());
        let is_last = end >= payload.len();
        let flags = base_flags
            | if is_last {
                0
            } else {
                header_flags::MORE_FRAGMENTS
            };

        let mut frame = Vec::with_capacity(TELEMETRY_HEADER_LEN as usize + (end - offset));
        let mut writer = crate::wire::Writer::default();
        TelemetryHeader {
            message_type,
            flags,
            sequence,
            timestamp_ms,
            total_payload_len: total,
            fragment_offset: offset as u16,
        }
        .write(&mut writer);
        frame.extend_from_slice(&writer.into_vec());
        frame.extend_from_slice(&payload[offset..end]);
        frames.push(frame);

        if is_last {
            break;
        }
        offset = end;
    }
    Ok(frames)
}

/// Frame `payload` as a single unfragmented message: the 16-byte telemetry
/// header followed by the whole payload.
///
/// Used for a GATT **Read**, where the controller returns the entire value (a
/// long read if it exceeds the MTU) rather than a sequence of notifications, so
/// the header must still be present (`PROTOCOL.md` §6, §11) but `MORE_FRAGMENTS`
/// never applies. Notifications go through [`fragment`] instead.
pub fn frame_unfragmented(
    message_type: MessageType,
    sequence: u32,
    timestamp_ms: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let total = payload.len().min(u16::MAX as usize) as u16;
    let mut writer =
        crate::wire::Writer::with_capacity(TELEMETRY_HEADER_LEN as usize + payload.len());
    TelemetryHeader {
        message_type,
        flags: flags & !header_flags::MORE_FRAGMENTS,
        sequence,
        timestamp_ms,
        total_payload_len: total,
        fragment_offset: 0,
    }
    .write(&mut writer);
    writer.bytes(payload);
    writer.into_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{RATE_UNAVAILABLE, VITALS_LEN, Vitals};

    fn sample_vitals_payload() -> Vec<u8> {
        Vitals {
            subject_id: 1,
            status_flags: 0,
            heart_rate_x100: 7000,
            respiration_rate_x100: RATE_UNAVAILABLE,
            heart_confidence: 50,
            respiration_confidence: 0,
            activity_confidence: 0,
            motion_energy_um2_s2: 0,
            rms_speed_mm_s: 0,
            moving_fraction_q15: 0,
            range_bin: 0,
            breathing_deviation_q8_8: 0,
        }
        .encode()
    }

    #[test]
    fn single_fragment_when_mtu_is_ample() {
        let payload = sample_vitals_payload();
        // 247-byte MTU comfortably fits header + 24-byte payload.
        let frames = fragment(MessageType::Vitals, 5, 1000, 0, &payload, 247).unwrap();
        assert_eq!(frames.len(), 1);
        let frame = &frames[0];
        assert_eq!(frame.len(), TELEMETRY_HEADER_LEN as usize + VITALS_LEN);
        // Header: no MORE_FRAGMENTS, offset 0, total = 24.
        assert_eq!(frame[2] & header_flags::MORE_FRAGMENTS, 0);
        assert_eq!(
            u16::from_le_bytes([frame[12], frame[13]]),
            VITALS_LEN as u16
        );
        assert_eq!(u16::from_le_bytes([frame[14], frame[15]]), 0);
    }

    #[test]
    fn splits_and_reassembles_at_default_mtu() {
        let payload = sample_vitals_payload();
        // Default ATT MTU 23 => 4 payload bytes/fragment => ceil(24/4) = 6 frames.
        let att_mtu = 23;
        let chunk = max_fragment_payload(att_mtu).unwrap();
        assert_eq!(chunk, 4);
        let frames = fragment(MessageType::Vitals, 9, 2000, 0, &payload, att_mtu).unwrap();
        assert_eq!(frames.len(), 6);

        // Every fragment carries the same sequence/total; offsets are contiguous;
        // only the last clears MORE_FRAGMENTS. Reassembling yields the payload.
        let mut reassembled = vec![0u8; payload.len()];
        for (i, frame) in frames.iter().enumerate() {
            assert!(frame.len() <= att_mtu - ATT_NOTIFY_OVERHEAD);
            assert_eq!(
                u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]),
                9
            );
            assert_eq!(
                u16::from_le_bytes([frame[12], frame[13]]),
                payload.len() as u16
            );
            let offset = u16::from_le_bytes([frame[14], frame[15]]) as usize;
            let more = frame[2] & header_flags::MORE_FRAGMENTS != 0;
            assert_eq!(more, i != frames.len() - 1);
            let data = &frame[TELEMETRY_HEADER_LEN as usize..];
            reassembled[offset..offset + data.len()].copy_from_slice(data);
        }
        assert_eq!(reassembled, payload);
    }

    #[test]
    fn frames_read_value_as_single_message_with_header() {
        let payload = sample_vitals_payload();
        let frame = frame_unfragmented(
            MessageType::DeviceStatus,
            3,
            500,
            header_flags::STALE,
            &payload,
        );
        assert_eq!(frame.len(), TELEMETRY_HEADER_LEN as usize + payload.len());
        // Header carries the payload but never MORE_FRAGMENTS on a read.
        assert_eq!(frame[1], MessageType::DeviceStatus as u8);
        assert_eq!(frame[2] & header_flags::MORE_FRAGMENTS, 0);
        assert_eq!(frame[2] & header_flags::STALE, header_flags::STALE);
        assert_eq!(
            u16::from_le_bytes([frame[12], frame[13]]),
            payload.len() as u16
        );
        assert_eq!(u16::from_le_bytes([frame[14], frame[15]]), 0);
        assert_eq!(&frame[TELEMETRY_HEADER_LEN as usize..], &payload[..]);
    }

    #[test]
    fn rejects_mtu_with_no_room_for_payload() {
        // MTU 19 => 19 - 3 - 16 = 0 payload bytes.
        assert_eq!(max_fragment_payload(19), None);
        assert_eq!(
            fragment(MessageType::Vitals, 0, 0, 0, &[1, 2, 3], 19),
            Err(FragmentError::MtuTooSmall)
        );
    }
}
