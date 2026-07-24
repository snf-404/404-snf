// SPDX-License-Identifier: Apache-2.0

//! SNF Telemetry Protocol v1 wire codec.
//!
//! Pure, allocation-light encoders and decoders for the byte layouts defined in
//! `PROTOCOL.md`. This module is the single Rust source of truth for the wire
//! format; the BlueZ backend ([`crate::bluez`]) only frames and ships the bytes
//! these functions produce, and the fragmenter ([`crate::fragment`]) only slices
//! them. Nothing here touches BlueZ, so it builds and is unit-tested on any host
//! (including the macOS dev box, where `bluer` is unavailable).
//!
//! Conventions, all from `PROTOCOL.md` §4:
//!
//! * every multi-byte integer is little-endian;
//! * rates are `u16` in units of `0.01 bpm`, with `0xffff` meaning unavailable;
//! * coordinates are `i16` millimetres in the x-right / y-out / z-up frame;
//! * reserved fields are written as zero and ignored on receipt.
//!
//! The `*_LEN` constants and the golden-vector tests pin the exact byte counts
//! so a layout change cannot pass silently — the TypeScript/native decoders must
//! be updated against the same vectors (`PROTOCOL.md` §17 step 6).

use uuid::Uuid;

use crate::wire::{Reader, Writer};

/// Major protocol version carried in every header (`PROTOCOL.md` §5, §6).
pub const PROTOCOL_MAJOR: u8 = 1;
/// Minor protocol version. Incremented only for additive changes.
pub const PROTOCOL_MINOR: u8 = 0;
/// Length of the unified telemetry header, repeated on every fragment.
pub const TELEMETRY_HEADER_LEN: u8 = 16;
/// Coordinate frame id `1`: x-right, y-out (away from radar), z-up.
pub const COORDINATE_FRAME_XRIGHT_YOUT_ZUP: u8 = 1;
/// Sentinel for an unavailable `u16` rate field (`0.01 bpm` units).
pub const RATE_UNAVAILABLE: u16 = 0xffff;
/// Sentinel for an unknown subject / tracking id.
pub const SUBJECT_UNKNOWN: u16 = 0xffff;

/// Builds a v1 SNF Telemetry UUID from the low 16 bits of the base
/// `7b9f0001-6b44-4d2a-9f36-4040534e46xx` (`PROTOCOL.md` §3). All SNF
/// characteristics share the service's 128-bit base; only the last byte varies.
const fn snf_uuid(suffix: u8) -> Uuid {
    Uuid::from_bytes([
        0x7b, 0x9f, 0x00, 0x01, 0x6b, 0x44, 0x4d, 0x2a, 0x9f, 0x36, 0x40, 0x40, 0x53, 0x4e, 0x46,
        suffix,
    ])
}

/// Primary service that groups all SNF telemetry.
pub const SERVICE_UUID: Uuid = snf_uuid(0x00);
/// Protocol Info — 24-byte read of version, capabilities, limits.
pub const PROTOCOL_INFO_UUID: Uuid = snf_uuid(0x01);
/// Stream Control — write-with-response requests, indicated responses.
pub const STREAM_CONTROL_UUID: Uuid = snf_uuid(0x02);
/// Device Status — uptime, drop counts, errors.
pub const DEVICE_STATUS_UUID: Uuid = snf_uuid(0x03);
/// Vitals — heart rate, respiration, motion quality.
pub const VITALS_UUID: Uuid = snf_uuid(0x04);
/// Fatigue — optional fatigue-model output.
pub const FATIGUE_UUID: Uuid = snf_uuid(0x05);
/// Pose — 3D joints of the tracked subject.
pub const POSE_UUID: Uuid = snf_uuid(0x06);
/// Point Cloud — downsampled 3D points.
pub const POINT_CLOUD_UUID: Uuid = snf_uuid(0x07);

/// Local name advertised by the peripheral (`PROTOCOL.md` §3).
pub const ADVERTISED_NAME: &str = "404-SNF";

/// Telemetry message type, the second byte of every telemetry header.
///
/// The discriminants are the on-wire values; unknown types must be ignored by
/// receivers (`PROTOCOL.md` §16), so decoders map unknown bytes to `None`
/// rather than erroring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    DeviceStatus = 0x10,
    Vitals = 0x20,
    Fatigue = 0x21,
    Pose = 0x30,
    PointCloud = 0x31,
    ControlResponse = 0x40,
}

impl MessageType {
    /// The on-wire byte for this message type.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Telemetry header flag bits (`PROTOCOL.md` §6). Combined with `|`.
pub mod header_flags {
    /// More fragments of this logical message follow.
    pub const MORE_FRAGMENTS: u8 = 1 << 0;
    /// Response to a one-shot snapshot request.
    pub const SNAPSHOT: u8 = 1 << 1;
    /// Displayable, but quality is degraded.
    pub const DEGRADED: u8 = 1 << 2;
    /// Carried value is the last good one, not a fresh frame.
    pub const STALE: u8 = 1 << 3;
}

/// Protocol Info capability bits (`PROTOCOL.md` §5). Combined with `|`.
pub mod capabilities {
    pub const VITALS: u32 = 1 << 0;
    pub const FATIGUE: u32 = 1 << 1;
    pub const POSE_3D: u32 = 1 << 2;
    pub const POINT_CLOUD_3D: u32 = 1 << 3;
    pub const MULTI_SUBJECT: u32 = 1 << 4;
    pub const BATTERY_STATUS: u32 = 1 << 5;
    pub const ENCRYPTION_REQUIRED: u32 = 1 << 6;
}

/// Vitals `status_flags` bits (`PROTOCOL.md` §7). Combined with `|`.
pub mod vitals_flags {
    pub const SUBJECT_TRACKED: u16 = 1 << 0;
    pub const HEART_VALID: u16 = 1 << 1;
    pub const RESPIRATION_VALID: u16 = 1 << 2;
    pub const WARMING_UP: u16 = 1 << 3;
    pub const MOTION_CONTAMINATED: u16 = 1 << 4;
    pub const VENDOR_VALUE_INVALID: u16 = 1 << 5;
    pub const RADAR_GAP: u16 = 1 << 6;
}

/// Fatigue `status_flags` bits (`PROTOCOL.md` §8). Combined with `|`.
pub mod fatigue_flags {
    pub const VALID: u16 = 1 << 0;
    pub const WARMING_UP: u16 = 1 << 1;
    pub const INSUFFICIENT_INPUT: u16 = 1 << 2;
}

/// Stream mask bits shared by Stream Control and Device Status
/// (`PROTOCOL.md` §12). Combined with `|`.
pub mod streams {
    pub const STATUS: u16 = 1 << 0;
    pub const VITALS: u16 = 1 << 1;
    pub const FATIGUE: u16 = 1 << 2;
    pub const POSE: u16 = 1 << 3;
    pub const POINT_CLOUD: u16 = 1 << 4;
}

// ── Protocol Info (fixed 24-byte read) ───────────────────────────────────────

/// Fixed length of the Protocol Info characteristic value.
pub const PROTOCOL_INFO_LEN: usize = 24;

/// Contents of the Protocol Info read (`PROTOCOL.md` §5).
///
/// `boot_id` changes on every boot so clients can detect a restart and flush
/// per-sequence filter state; `build_id` is the firmware build (`0` = unknown).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolInfo {
    pub capabilities: u32,
    pub max_point_count: u16,
    pub max_pose_joints: u8,
    pub max_subjects: u8,
    pub boot_id: u32,
    pub build_id: u32,
}

impl ProtocolInfo {
    /// Encode the fixed 24-byte value. Protocol Info has no telemetry header.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(PROTOCOL_INFO_LEN);
        w.bytes(b"SNF1")
            .u8(PROTOCOL_MAJOR)
            .u8(PROTOCOL_MINOR)
            .u8(TELEMETRY_HEADER_LEN)
            .u8(COORDINATE_FRAME_XRIGHT_YOUT_ZUP)
            .u32(self.capabilities)
            .u16(self.max_point_count)
            .u8(self.max_pose_joints)
            .u8(self.max_subjects)
            .u32(self.boot_id)
            .u32(self.build_id);
        debug_assert_eq!(w.len(), PROTOCOL_INFO_LEN);
        w.into_vec()
    }
}

// ── Telemetry header (16 bytes, repeated per fragment) ───────────────────────

/// The unified 16-byte header that prefixes every telemetry fragment
/// (`PROTOCOL.md` §6). `total_payload_len` and `fragment_offset` describe the
/// fragment's place in the full logical payload; the [`crate::fragment`] module
/// fills them in, callers of the payload encoders below never do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TelemetryHeader {
    pub message_type: MessageType,
    pub flags: u8,
    pub sequence: u32,
    pub timestamp_ms: u32,
    pub total_payload_len: u16,
    pub fragment_offset: u16,
}

impl TelemetryHeader {
    /// Write the 16 header bytes into `w`.
    pub fn write(&self, w: &mut Writer) {
        w.u8(PROTOCOL_MAJOR)
            .u8(self.message_type.as_u8())
            .u8(self.flags)
            .u8(TELEMETRY_HEADER_LEN)
            .u32(self.sequence)
            .u32(self.timestamp_ms)
            .u16(self.total_payload_len)
            .u16(self.fragment_offset);
    }
}

// ── Device Status (fixed 20-byte payload, 0x10) ──────────────────────────────

/// Fixed length of a Device Status payload.
pub const DEVICE_STATUS_LEN: usize = 20;
/// `battery_mv` value meaning "not provided".
pub const BATTERY_MV_UNAVAILABLE: u16 = 0xffff;
/// `processor_temp_x100_c` value meaning "not provided".
pub const TEMP_UNAVAILABLE: i16 = 0x7fff;

/// Device Status payload (`PROTOCOL.md` §11). Drop counters saturate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceStatus {
    pub uptime_s: u32,
    pub active_streams: u16,
    pub last_error: u16,
    pub dropped_pose_frames: u16,
    pub dropped_point_frames: u16,
    pub radar_gap_count: u16,
    pub battery_mv: u16,
    pub processor_temp_x100_c: i16,
}

impl DeviceStatus {
    /// Encode the fixed 20-byte payload (no telemetry header).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(DEVICE_STATUS_LEN);
        w.u32(self.uptime_s)
            .u16(self.active_streams)
            .u16(self.last_error)
            .u16(self.dropped_pose_frames)
            .u16(self.dropped_point_frames)
            .u16(self.radar_gap_count)
            .u16(self.battery_mv)
            .i16(self.processor_temp_x100_c)
            .zeros(2);
        debug_assert_eq!(w.len(), DEVICE_STATUS_LEN);
        w.into_vec()
    }
}

// ── Vitals (fixed 24-byte payload, 0x20) ─────────────────────────────────────

/// Fixed length of a Vitals payload.
pub const VITALS_LEN: usize = 24;

/// Vitals payload (`PROTOCOL.md` §7).
///
/// Status precedes value: when e.g. `MOTION_CONTAMINATED` is set the last stable
/// BPM may still be sent, with the header's `STALE` flag, and the UI must show a
/// quality warning rather than treat it as a fresh reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vitals {
    pub subject_id: u16,
    pub status_flags: u16,
    /// BPM × 100, or [`RATE_UNAVAILABLE`].
    pub heart_rate_x100: u16,
    /// BPM × 100, or [`RATE_UNAVAILABLE`].
    pub respiration_rate_x100: u16,
    pub heart_confidence: u8,
    pub respiration_confidence: u8,
    pub activity_confidence: u8,
    /// Mean squared radial velocity × 1_000_000, in `µm²/s²`.
    pub motion_energy_um2_s2: u32,
    pub rms_speed_mm_s: u16,
    /// Fraction of moving points, `0..=32767`.
    pub moving_fraction_q15: u16,
    pub range_bin: u16,
    /// Vendor breathing deviation × 256.
    pub breathing_deviation_q8_8: i16,
}

impl Vitals {
    /// Encode the fixed 24-byte payload (no telemetry header).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(VITALS_LEN);
        w.u16(self.subject_id)
            .u16(self.status_flags)
            .u16(self.heart_rate_x100)
            .u16(self.respiration_rate_x100)
            .u8(self.heart_confidence)
            .u8(self.respiration_confidence)
            .u8(self.activity_confidence)
            .zeros(1)
            .u32(self.motion_energy_um2_s2)
            .u16(self.rms_speed_mm_s)
            .u16(self.moving_fraction_q15)
            .u16(self.range_bin)
            .i16(self.breathing_deviation_q8_8);
        debug_assert_eq!(w.len(), VITALS_LEN);
        w.into_vec()
    }
}

// ── Fatigue (fixed 12-byte payload, 0x21) ────────────────────────────────────

/// Fixed length of a Fatigue payload.
pub const FATIGUE_LEN: usize = 12;

/// Fatigue payload (`PROTOCOL.md` §8). Published only when the `FATIGUE`
/// capability is advertised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fatigue {
    /// `0..=100`.
    pub level: u8,
    /// `0..=100`.
    pub confidence: u8,
    pub status_flags: u16,
    /// Model version or short hash.
    pub model_revision: u32,
}

impl Fatigue {
    /// Encode the fixed 12-byte payload (no telemetry header).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(FATIGUE_LEN);
        w.u8(self.level)
            .u8(self.confidence)
            .u16(self.status_flags)
            .u32(self.model_revision)
            .zeros(4);
        debug_assert_eq!(w.len(), FATIGUE_LEN);
        w.into_vec()
    }
}

// ── Pose (8-byte header + 8 bytes/joint, 0x30) ───────────────────────────────

/// Pose skeleton model id (`PROTOCOL.md` §9). New models get new ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PoseModel {
    Coco17 = 1,
    BlazePose33 = 2,
}

/// Pose `pose_flags` bits (`PROTOCOL.md` §9). Combined with `|`.
pub mod pose_flags {
    pub const TRACKED: u8 = 1 << 0;
    pub const INFERRED: u8 = 1 << 1;
    pub const PARTIAL: u8 = 1 << 2;
}

/// One 3D joint, 8 bytes on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Joint {
    pub joint_id: u8,
    /// `0..=100`.
    pub confidence: u8,
    pub x_mm: i16,
    pub y_mm: i16,
    pub z_mm: i16,
}

/// Pose payload (`PROTOCOL.md` §9): fixed 8-byte header plus one entry per
/// joint. The `POSE_3D` capability must be cleared when no pose model is loaded
/// rather than sending an all-zero skeleton.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pose {
    pub subject_id: u16,
    pub model: PoseModel,
    pub pose_flags: u8,
    pub joints: Vec<Joint>,
}

impl Pose {
    /// Encode the header and joints. Payload length is `8 + 8 * joint_count`.
    pub fn encode(&self) -> Vec<u8> {
        let joint_count = self.joints.len().min(u8::MAX as usize) as u8;
        let mut w = Writer::with_capacity(8 + 8 * joint_count as usize);
        w.u16(self.subject_id)
            .u8(self.model as u8)
            .u8(joint_count)
            .u8(COORDINATE_FRAME_XRIGHT_YOUT_ZUP)
            .u8(self.pose_flags)
            .zeros(2);
        for joint in self.joints.iter().take(joint_count as usize) {
            w.u8(joint.joint_id)
                .u8(joint.confidence)
                .i16(joint.x_mm)
                .i16(joint.y_mm)
                .i16(joint.z_mm);
        }
        w.into_vec()
    }
}

// ── Point Cloud (8-byte header + 8 bytes/point, 0x31) ────────────────────────

/// The only point format defined in v1.
pub const POINT_FORMAT_V1: u8 = 1;
/// `snr_half_db` value meaning "unknown".
pub const SNR_UNKNOWN: u8 = 0xff;

/// One point in format 1, 8 bytes on the wire.
///
/// Out-of-range points must be dropped before sending, not saturated
/// (`PROTOCOL.md` §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloudPoint {
    pub x_mm: i16,
    pub y_mm: i16,
    pub z_mm: i16,
    /// Radial velocity in units of `0.02 m/s`; positive = away from radar.
    pub radial_velocity_2cm_s: i8,
    /// SNR in units of `0.5 dB`, or [`SNR_UNKNOWN`].
    pub snr_half_db: u8,
}

/// Point Cloud payload (`PROTOCOL.md` §10). Off by default; downsampling is the
/// sender's responsibility and must be spatial, never a prefix of the parser's
/// array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointCloud {
    pub subject_id: u16,
    pub points: Vec<CloudPoint>,
}

impl PointCloud {
    /// Encode the header and points. Payload length is `8 + 8 * point_count`.
    pub fn encode(&self) -> Vec<u8> {
        let point_count = self.points.len().min(u16::MAX as usize) as u16;
        let mut w = Writer::with_capacity(8 + 8 * point_count as usize);
        w.u16(self.subject_id)
            .u16(point_count)
            .u8(POINT_FORMAT_V1)
            .u8(COORDINATE_FRAME_XRIGHT_YOUT_ZUP)
            .zeros(2);
        for point in self.points.iter().take(point_count as usize) {
            w.i16(point.x_mm)
                .i16(point.y_mm)
                .i16(point.z_mm)
                .i8(point.radial_velocity_2cm_s)
                .u8(point.snr_half_db);
        }
        w.into_vec()
    }
}

// ── Stream Control (client write) ────────────────────────────────────────────

/// Length of the fixed Stream Control request header (`PROTOCOL.md` §12).
pub const CONTROL_HEADER_LEN: usize = 8;
/// Maximum bytes echoed back by a `PING` (`PROTOCOL.md` §12).
pub const PING_MAX_ECHO: usize = 16;

/// A decoded Stream Control request written by a client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlRequest {
    pub request_id: u16,
    pub op: ControlOp,
}

/// The opcode-specific body of a [`ControlRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlOp {
    /// `0x01` — set which streams are active and their rates.
    SetStreams(StreamSettings),
    /// `0x02` — pin a subject id, or `0xffff` to auto-select.
    SetSubject(u16),
    /// `0x03` — one-shot snapshot of the given streams.
    RequestSnapshot(u16),
    /// `0x04` — liveness check; the payload is echoed back.
    Ping(Vec<u8>),
}

/// `SET_STREAMS` payload (`PROTOCOL.md` §12). The device may lower a requested
/// rate but must never silently raise it; the effective values come back in the
/// Control Response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamSettings {
    pub stream_mask: u16,
    pub vitals_hz: u8,
    pub pose_hz: u8,
    pub point_cloud_hz: u8,
    pub max_points: u8,
}

/// Why a Stream Control request could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlParseError {
    /// Fewer than [`CONTROL_HEADER_LEN`] bytes, or a body shorter than its
    /// declared `payload_len`.
    Truncated,
    /// Header `protocol_major` did not match [`PROTOCOL_MAJOR`].
    VersionMismatch,
    /// The opcode byte is not one defined in v1.
    UnknownOpcode(u8),
}

impl ControlRequest {
    /// Parse a client-written Stream Control value.
    ///
    /// Validates the 8-byte header, then the opcode body against its expected
    /// size. `payload_len` from the header bounds the body so trailing bytes
    /// from a larger ATT write do not leak into an opcode payload.
    pub fn parse(buf: &[u8]) -> Result<Self, ControlParseError> {
        let mut r = Reader::new(buf);
        let major = r.u8().ok_or(ControlParseError::Truncated)?;
        if major != PROTOCOL_MAJOR {
            return Err(ControlParseError::VersionMismatch);
        }
        let opcode = r.u8().ok_or(ControlParseError::Truncated)?;
        let request_id = r.u16().ok_or(ControlParseError::Truncated)?;
        let payload_len = r.u16().ok_or(ControlParseError::Truncated)? as usize;
        let _reserved = r.u16().ok_or(ControlParseError::Truncated)?;
        if r.remaining() < payload_len {
            return Err(ControlParseError::Truncated);
        }
        // Confine the body to its declared length.
        let body = &buf[CONTROL_HEADER_LEN..CONTROL_HEADER_LEN + payload_len];

        let op = match opcode {
            0x01 => {
                let mut b = Reader::new(body);
                let stream_mask = b.u16().ok_or(ControlParseError::Truncated)?;
                let vitals_hz = b.u8().ok_or(ControlParseError::Truncated)?;
                let pose_hz = b.u8().ok_or(ControlParseError::Truncated)?;
                let point_cloud_hz = b.u8().ok_or(ControlParseError::Truncated)?;
                let max_points = b.u8().ok_or(ControlParseError::Truncated)?;
                ControlOp::SetStreams(StreamSettings {
                    stream_mask,
                    vitals_hz,
                    pose_hz,
                    point_cloud_hz,
                    max_points,
                })
            }
            0x02 => {
                let mut b = Reader::new(body);
                ControlOp::SetSubject(b.u16().ok_or(ControlParseError::Truncated)?)
            }
            0x03 => {
                let mut b = Reader::new(body);
                ControlOp::RequestSnapshot(b.u16().ok_or(ControlParseError::Truncated)?)
            }
            0x04 => {
                let echo = &body[..body.len().min(PING_MAX_ECHO)];
                ControlOp::Ping(echo.to_vec())
            }
            other => return Err(ControlParseError::UnknownOpcode(other)),
        };
        Ok(Self { request_id, op })
    }
}

/// Result code of a Control Response (`PROTOCOL.md` §12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ControlResult {
    Success = 0,
    Unsupported = 1,
    Invalid = 2,
    Busy = 3,
    Denied = 4,
}

/// Length of the fixed Control Response payload core (`PROTOCOL.md` §12).
pub const CONTROL_RESPONSE_LEN: usize = 10;

/// Control Response payload (`message_type` `0x40`, `PROTOCOL.md` §12).
///
/// Sent via Indicate so the command result is ATT-acknowledged. The `effective_*`
/// fields report what the device actually applied. For a `PING`, the echoed
/// bytes are appended after the 10-byte core (`echo`); other opcodes leave it
/// empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlResponse {
    pub request_id: u16,
    /// Opcode being answered.
    pub opcode: u8,
    pub result: ControlResult,
    pub effective_stream_mask: u16,
    pub effective_vitals_hz: u8,
    pub effective_pose_hz: u8,
    pub effective_point_cloud_hz: u8,
    pub effective_max_points: u8,
    /// `PING` echo, appended after the core; empty for every other opcode.
    pub echo: Vec<u8>,
}

impl ControlResponse {
    /// Encode the payload (no telemetry header; the caller frames it as a
    /// `ControlResponse` message).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(CONTROL_RESPONSE_LEN + self.echo.len());
        w.u16(self.request_id)
            .u8(self.opcode)
            .u8(self.result as u8)
            .u16(self.effective_stream_mask)
            .u8(self.effective_vitals_hz)
            .u8(self.effective_pose_hz)
            .u8(self.effective_point_cloud_hz)
            .u8(self.effective_max_points)
            .bytes(&self.echo);
        w.into_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_uuids_match_protocol_spec() {
        // The base is `7b9f0001-...-4040534e4600`; suffixes 0x00..=0x07.
        assert_eq!(
            SERVICE_UUID,
            Uuid::parse_str("7b9f0001-6b44-4d2a-9f36-4040534e4600").unwrap()
        );
        assert_eq!(
            PROTOCOL_INFO_UUID,
            Uuid::parse_str("7b9f0001-6b44-4d2a-9f36-4040534e4601").unwrap()
        );
        assert_eq!(
            POINT_CLOUD_UUID,
            Uuid::parse_str("7b9f0001-6b44-4d2a-9f36-4040534e4607").unwrap()
        );
    }

    #[test]
    fn protocol_info_golden_vector() {
        let info = ProtocolInfo {
            capabilities: capabilities::VITALS | capabilities::FATIGUE,
            max_point_count: 96,
            max_pose_joints: 17,
            max_subjects: 1,
            boot_id: 0x0102_0304,
            build_id: 0,
        };
        let bytes = info.encode();
        assert_eq!(bytes.len(), PROTOCOL_INFO_LEN);
        #[rustfmt::skip]
        assert_eq!(
            bytes,
            vec![
                b'S', b'N', b'F', b'1', // magic
                1,                      // major
                0,                      // minor
                16,                     // telemetry_header_len
                1,                      // coordinate_frame
                0x03, 0x00, 0x00, 0x00, // capabilities = VITALS | FATIGUE
                96, 0x00,               // max_point_count
                17,                     // max_pose_joints
                1,                      // max_subjects
                0x04, 0x03, 0x02, 0x01, // boot_id (LE)
                0x00, 0x00, 0x00, 0x00, // build_id
            ]
        );
    }

    #[test]
    fn telemetry_header_golden_vector() {
        let mut w = Writer::with_capacity(TELEMETRY_HEADER_LEN as usize);
        TelemetryHeader {
            message_type: MessageType::Vitals,
            flags: header_flags::STALE,
            sequence: 0x1122_3344,
            timestamp_ms: 0x00AA_BBCC,
            total_payload_len: VITALS_LEN as u16,
            fragment_offset: 0,
        }
        .write(&mut w);
        let bytes = w.into_vec();
        assert_eq!(bytes.len(), TELEMETRY_HEADER_LEN as usize);
        #[rustfmt::skip]
        assert_eq!(
            bytes,
            vec![
                1,                      // protocol_major
                0x20,                   // message_type = Vitals
                0b1000,                 // flags = STALE
                16,                     // header_len
                0x44, 0x33, 0x22, 0x11, // sequence (LE)
                0xCC, 0xBB, 0xAA, 0x00, // timestamp_ms (LE)
                24, 0x00,               // total_payload_len
                0x00, 0x00,             // fragment_offset
            ]
        );
    }

    #[test]
    fn vitals_golden_vector() {
        let vitals = Vitals {
            subject_id: 7,
            status_flags: vitals_flags::SUBJECT_TRACKED | vitals_flags::HEART_VALID,
            heart_rate_x100: 7250, // 72.50 bpm
            respiration_rate_x100: RATE_UNAVAILABLE,
            heart_confidence: 90,
            respiration_confidence: 0,
            activity_confidence: 40,
            motion_energy_um2_s2: 1_000_000,
            rms_speed_mm_s: 12,
            moving_fraction_q15: 16_384,
            range_bin: 44,
            breathing_deviation_q8_8: -256,
        };
        let bytes = vitals.encode();
        assert_eq!(bytes.len(), VITALS_LEN);
        #[rustfmt::skip]
        assert_eq!(
            bytes,
            vec![
                0x07, 0x00,             // subject_id
                0x03, 0x00,             // status_flags
                0x52, 0x1C,             // heart_rate_x100 = 7250
                0xFF, 0xFF,             // respiration_rate_x100 = unavailable
                90,                     // heart_confidence
                0,                      // respiration_confidence
                40,                     // activity_confidence
                0,                      // reserved
                0x40, 0x42, 0x0F, 0x00, // motion_energy_um2_s2 = 1_000_000
                12, 0x00,               // rms_speed_mm_s
                0x00, 0x40,             // moving_fraction_q15 = 16384
                44, 0x00,               // range_bin
                0x00, 0xFF,             // breathing_deviation_q8_8 = -256
            ]
        );
    }

    #[test]
    fn fatigue_and_status_lengths_are_fixed() {
        let fatigue = Fatigue {
            level: 30,
            confidence: 80,
            status_flags: fatigue_flags::VALID,
            model_revision: 0xDEAD_BEEF,
        };
        assert_eq!(fatigue.encode().len(), FATIGUE_LEN);

        let status = DeviceStatus {
            uptime_s: 120,
            active_streams: streams::STATUS | streams::VITALS,
            last_error: 0,
            dropped_pose_frames: 0,
            dropped_point_frames: 0,
            radar_gap_count: 0,
            battery_mv: BATTERY_MV_UNAVAILABLE,
            processor_temp_x100_c: TEMP_UNAVAILABLE,
        };
        assert_eq!(status.encode().len(), DEVICE_STATUS_LEN);
    }

    #[test]
    fn pose_and_cloud_lengths_track_element_count() {
        let pose = Pose {
            subject_id: 1,
            model: PoseModel::Coco17,
            pose_flags: pose_flags::TRACKED,
            joints: vec![
                Joint {
                    joint_id: 0,
                    confidence: 90,
                    x_mm: 1,
                    y_mm: 2,
                    z_mm: 3,
                },
                Joint {
                    joint_id: 1,
                    confidence: 80,
                    x_mm: -1,
                    y_mm: -2,
                    z_mm: -3,
                },
            ],
        };
        assert_eq!(pose.encode().len(), 8 + 2 * 8);

        let cloud = PointCloud {
            subject_id: SUBJECT_UNKNOWN,
            points: vec![CloudPoint {
                x_mm: 100,
                y_mm: 1500,
                z_mm: -50,
                radial_velocity_2cm_s: -5,
                snr_half_db: SNR_UNKNOWN,
            }],
        };
        assert_eq!(cloud.encode().len(), 8 + 8);
    }

    #[test]
    fn parses_set_streams_request() {
        #[rustfmt::skip]
        let buf = vec![
            1,          // protocol_major
            0x01,       // opcode = SET_STREAMS
            0x2A, 0x00, // request_id = 42
            0x08, 0x00, // payload_len = 8
            0x00, 0x00, // reserved
            0x06, 0x00, // stream_mask = VITALS | FATIGUE
            2,          // vitals_hz
            0,          // pose_hz
            0,          // point_cloud_hz
            0,          // max_points
            0x00, 0x00, // reserved
        ];
        let req = ControlRequest::parse(&buf).unwrap();
        assert_eq!(req.request_id, 42);
        assert_eq!(
            req.op,
            ControlOp::SetStreams(StreamSettings {
                stream_mask: streams::VITALS | streams::FATIGUE,
                vitals_hz: 2,
                pose_hz: 0,
                point_cloud_hz: 0,
                max_points: 0,
            })
        );
    }

    #[test]
    fn ping_echo_is_bounded_and_round_trips_through_response() {
        #[rustfmt::skip]
        let buf = vec![
            1, 0x04, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00,
            b'p', b'o', b'n', b'g',
        ];
        let req = ControlRequest::parse(&buf).unwrap();
        let ControlOp::Ping(echo) = req.op else {
            panic!("expected ping");
        };
        assert_eq!(echo, b"pong");

        let resp = ControlResponse {
            request_id: req.request_id,
            opcode: 0x04,
            result: ControlResult::Success,
            effective_stream_mask: 0,
            effective_vitals_hz: 0,
            effective_pose_hz: 0,
            effective_point_cloud_hz: 0,
            effective_max_points: 0,
            echo,
        };
        let encoded = resp.encode();
        assert_eq!(encoded.len(), CONTROL_RESPONSE_LEN + 4);
        assert_eq!(&encoded[CONTROL_RESPONSE_LEN..], b"pong");
    }

    #[test]
    fn rejects_bad_version_and_unknown_opcode() {
        let bad_major = vec![2, 0x04, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            ControlRequest::parse(&bad_major),
            Err(ControlParseError::VersionMismatch)
        );

        let unknown = vec![1, 0x7F, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            ControlRequest::parse(&unknown),
            Err(ControlParseError::UnknownOpcode(0x7F))
        );

        let truncated = vec![1, 0x01, 0, 0, 8, 0, 0, 0, 1, 2, 3];
        assert_eq!(
            ControlRequest::parse(&truncated),
            Err(ControlParseError::Truncated)
        );
    }
}
