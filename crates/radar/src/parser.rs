// SPDX-License-Identifier: Apache-2.0

//! Rust-side frame model and the parse entry point.
//!
//! The Rust code handles framing and everything above it; the raw TLV decode is
//! the only part handed to C/C++ (under the `sdk` feature). This module is the
//! seam between the two.

use serde::{Deserialize, Serialize};

/// A parsed radar frame: the fields the rest of 404-snf actually consumes.
///
/// Placeholder shape — will grow point-cloud, range-doppler, and vital-sign
/// fields as the pipeline firms up.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RadarFrame {
    /// Frame sequence number from the radar header.
    pub frame_number: u32,
    /// Number of detected points in this frame.
    pub num_detected_points: u16,
    /// Estimated breathing rate, breaths per minute (0.0 if unavailable).
    pub breathing_rate_bpm: f32,
    /// Estimated heart rate, beats per minute (0.0 if unavailable).
    pub heart_rate_bpm: f32,
}

/// Parse one validated frame buffer into a [`RadarFrame`].
///
/// Under the `sdk` feature this delegates the raw TLV decode to the TI mmWave
/// SDK via the `cxx` bridge; otherwise it returns a stub. Framing and validation
/// happen in Rust before this is called.
pub fn parse_frame(_frame: &[u8]) -> RadarFrame {
    #[cfg(feature = "sdk")]
    {
        let raw = crate::ffi::ffi::parse_mmwave_frame(_frame);
        RadarFrame {
            frame_number: raw.frame_number,
            num_detected_points: raw.num_detected_points,
            breathing_rate_bpm: raw.breathing_rate_bpm,
            heart_rate_bpm: raw.heart_rate_bpm,
        }
    }

    #[cfg(not(feature = "sdk"))]
    {
        RadarFrame::default()
    }
}
