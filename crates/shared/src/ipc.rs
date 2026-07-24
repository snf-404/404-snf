// SPDX-License-Identifier: Apache-2.0

//! IPC message types for the `radar` channel declared in `Consortium.toml`.
//!
//! Both directions use the default postcard codec, so every type derives
//! `serde::Serialize`/`Deserialize`. `IpcSafe` rejects pointers, references and
//! width-dependent integers (`usize`/`isize`) from crossing the core boundary.
//!
//! The CM33 owns USART6, parses the IWR6843's TLV frames (see [`crate::detect`])
//! and reports one [`RadarReport`] per frame. Raw bytes never cross the boundary:
//! a frame's point cloud is ~2 KB of `f32`, while the report is a fixed ~400 B of
//! millimetre-resolution integers that the CA35's indicator engine can use
//! directly.

use consortium_ipc::IpcSafe;
use serde::{Deserialize, Serialize};

/// Points carried in one [`RadarReport`].
///
/// 32 is both plenty for the region-of-interest indicators and the largest array
/// length `serde` implements `Deserialize` for. A frame with more detections
/// still reports its true count in [`RadarReport::num_detected`] and sets
/// [`RadarReport::truncated`].
pub const RADAR_REPORT_POINTS: usize = 32;

/// One detected point, in millimetres and millimetres per second.
///
/// The sensor emits `f32` metres; the CM33 scales to integers because at 1 mm
/// resolution an `i16` still spans ±32.7 m — far past this radar's useful range —
/// and postcard encodes the result in a fraction of the bytes.
#[derive(Clone, Copy, Debug, Default, IpcSafe, Deserialize, Serialize)]
pub struct RadarPointFixed {
    /// Lateral offset from the sensor's boresight.
    pub x_mm: i16,
    /// Downrange distance from the antenna plane.
    pub y_mm: i16,
    /// Height relative to the sensor.
    pub z_mm: i16,
    /// Radial velocity; positive is away from the sensor.
    pub velocity_mm_s: i16,
}

/// One parsed radar frame, reported CM33 → CA35.
#[derive(Clone, Copy, Debug, IpcSafe, Deserialize, Serialize)]
pub struct RadarReport {
    /// Monotonic report counter; wraps at `u32::MAX`.
    pub seq: u32,
    /// Whether this report carries a frame at all. `false` is the CM33 saying
    /// "nothing new since you last asked" — the radar is quiet, unpowered, or
    /// still mid-frame — and every field below is meaningless.
    pub fresh: bool,
    /// The sensor's own frame counter, straight from the frame header.
    pub frame_number: u32,
    /// Points the frame header declared, which may exceed [`Self::num_points`].
    pub num_detected: u16,
    /// Points actually carried in [`Self::points`].
    pub num_points: u8,
    /// Of those, how many are moving (see [`crate::detect::MOVING_MM_S`]).
    pub moving_points: u8,
    /// Closest downrange distance among the carried points, or `u16::MAX` when
    /// the frame is empty.
    pub nearest_mm: u16,
    /// Mean absolute radial speed across the carried points.
    pub mean_speed_mm_s: u16,
    /// The frame had more points than [`RADAR_REPORT_POINTS`]; the extras were
    /// dropped, but the aggregates above cover every point that was parsed.
    pub truncated: bool,
    /// The CM33's UART ring overflowed or a line error was seen, so at least one
    /// frame was lost before this one.
    pub overrun: bool,
    /// Frames parsed but never delivered since the last report — the CA35 pulled
    /// slower than the sensor produced. Reset each time it is reported.
    pub dropped: u16,
    /// The detections. Only the first [`Self::num_points`] are valid.
    pub points: [RadarPointFixed; RADAR_REPORT_POINTS],
}

impl RadarReport {
    /// A report carrying no frame.
    pub const fn empty(seq: u32) -> Self {
        Self {
            seq,
            fresh: false,
            frame_number: 0,
            num_detected: 0,
            num_points: 0,
            moving_points: 0,
            nearest_mm: u16::MAX,
            mean_speed_mm_s: 0,
            truncated: false,
            overrun: false,
            dropped: 0,
            points: [RadarPointFixed {
                x_mm: 0,
                y_mm: 0,
                z_mm: 0,
                velocity_mm_s: 0,
            }; RADAR_REPORT_POINTS],
        }
    }

    /// The valid detections.
    pub fn points(&self) -> &[RadarPointFixed] {
        &self.points[..(self.num_points as usize).min(RADAR_REPORT_POINTS)]
    }
}

/// Stream gate and liveness ping sent from the CA35 down to the CM33.
///
/// The CM33 only reads the UART while the last `RadarControl` it saw had
/// `streaming` set, which lets the CA35 quiet the sensor path when nothing
/// downstream can consume a frame.
#[derive(Clone, Copy, Debug, IpcSafe, Deserialize, Serialize)]
pub struct RadarControl {
    /// Monotonic counter; wraps at `u32::MAX`.
    pub seq: u32,
    /// Whether the CA35 wants frames.
    pub streaming: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_carries_no_points() {
        let report = RadarReport::empty(3);
        assert_eq!(report.seq, 3);
        assert!(!report.fresh);
        assert!(report.points().is_empty());
        assert_eq!(report.nearest_mm, u16::MAX);
    }

    #[test]
    fn points_are_bounded_by_the_array_even_if_the_count_lies() {
        let mut report = RadarReport::empty(0);
        report.num_points = 255;
        assert_eq!(report.points().len(), RADAR_REPORT_POINTS);
    }
}
