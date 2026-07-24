// SPDX-License-Identifier: Apache-2.0

//! Running counters behind the Device Status message (`PROTOCOL.md` §11).
//!
//! The application updates these as it runs — a radar gap here, a dropped pose
//! frame there — and snapshots them into a [`DeviceStatus`] payload once per
//! status tick. Counters saturate rather than wrap so a client never sees a
//! drop count go backwards.

use std::time::Instant;

use snf_ble::protocol::{BATTERY_MV_UNAVAILABLE, DeviceStatus, TEMP_UNAVAILABLE};

/// Mutable device-health state, snapshotted into Device Status payloads.
#[derive(Debug)]
pub struct Accounting {
    start: Instant,
    /// Mirrors the Stream Control stream mask (`PROTOCOL.md` §12).
    pub active_streams: u16,
    pub last_error: u16,
    dropped_pose_frames: u16,
    dropped_point_frames: u16,
    radar_gap_count: u16,
    battery_mv: u16,
    processor_temp_x100_c: i16,
}

impl Accounting {
    /// Start accounting from now, with the given initial stream mask. Battery
    /// and temperature default to their "not provided" sentinels until a reading
    /// is supplied.
    pub fn new(active_streams: u16) -> Self {
        Self {
            start: Instant::now(),
            active_streams,
            last_error: 0,
            dropped_pose_frames: 0,
            dropped_point_frames: 0,
            radar_gap_count: 0,
            battery_mv: BATTERY_MV_UNAVAILABLE,
            processor_temp_x100_c: TEMP_UNAVAILABLE,
        }
    }

    /// Record a gap in the radar frame stream (a missed / late frame).
    pub fn note_radar_gap(&mut self) {
        self.radar_gap_count = self.radar_gap_count.saturating_add(1);
    }

    /// Record a pose frame dropped under backpressure (`PROTOCOL.md` §13).
    pub fn note_dropped_pose_frame(&mut self) {
        self.dropped_pose_frames = self.dropped_pose_frames.saturating_add(1);
    }

    /// Record a point-cloud frame dropped under backpressure.
    pub fn note_dropped_point_frame(&mut self) {
        self.dropped_point_frames = self.dropped_point_frames.saturating_add(1);
    }

    /// Supply a fresh battery reading in millivolts.
    pub fn set_battery_mv(&mut self, millivolts: u16) {
        self.battery_mv = millivolts;
    }

    /// Supply a fresh processor temperature in °C.
    pub fn set_processor_temp_c(&mut self, celsius: f32) {
        self.processor_temp_x100_c = (celsius * 100.0)
            .round()
            .clamp(i16::MIN as f32, (TEMP_UNAVAILABLE - 1) as f32)
            as i16;
    }

    /// Build the current Device Status payload. `uptime_s` saturates at
    /// `u32::MAX` (~136 years); the drop counters carry their saturated values.
    pub fn snapshot(&self) -> DeviceStatus {
        DeviceStatus {
            uptime_s: self.start.elapsed().as_secs().min(u32::MAX as u64) as u32,
            active_streams: self.active_streams,
            last_error: self.last_error,
            dropped_pose_frames: self.dropped_pose_frames,
            dropped_point_frames: self.dropped_point_frames,
            radar_gap_count: self.radar_gap_count,
            battery_mv: self.battery_mv,
            processor_temp_x100_c: self.processor_temp_x100_c,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snf_ble::protocol::streams;

    #[test]
    fn defaults_are_unavailable_sentinels() {
        let acc = Accounting::new(streams::STATUS | streams::VITALS);
        let status = acc.snapshot();
        assert_eq!(status.active_streams, streams::STATUS | streams::VITALS);
        assert_eq!(status.battery_mv, BATTERY_MV_UNAVAILABLE);
        assert_eq!(status.processor_temp_x100_c, TEMP_UNAVAILABLE);
        assert_eq!(status.radar_gap_count, 0);
    }

    #[test]
    fn counters_saturate_and_readings_apply() {
        let mut acc = Accounting::new(0);
        for _ in 0..5 {
            acc.note_radar_gap();
        }
        acc.note_dropped_pose_frame();
        acc.set_battery_mv(3700);
        acc.set_processor_temp_c(42.5);

        let status = acc.snapshot();
        assert_eq!(status.radar_gap_count, 5);
        assert_eq!(status.dropped_pose_frames, 1);
        assert_eq!(status.battery_mv, 3700);
        assert_eq!(status.processor_temp_x100_c, 4250);
    }

    #[test]
    fn temperature_never_hits_the_unavailable_sentinel() {
        let mut acc = Accounting::new(0);
        acc.set_processor_temp_c(1000.0); // absurdly hot
        assert!(acc.snapshot().processor_temp_x100_c < TEMP_UNAVAILABLE);
    }
}
