// SPDX-License-Identifier: Apache-2.0

//! `cxx` bridge to the TI mmWave SDK TLV parser (compiled under the `sdk`
//! feature). This is the *only* C/C++ surface in 404-snf; everything else is
//! Rust. See `cxx/include/mmwave_shim.h` and `cxx/src/mmwave_shim.cpp`.

#[cxx::bridge]
pub mod ffi {
    /// POD result of decoding one frame. Mirrors the fields
    /// [`crate::RadarFrame`] cares about; kept trivially copyable so it can
    /// cross the FFI boundary by value.
    struct MmwaveFrame {
        frame_number: u32,
        num_detected_points: u16,
        breathing_rate_bpm: f32,
        heart_rate_bpm: f32,
    }

    unsafe extern "C++" {
        include!("mmwave_shim.h");

        /// Decode a validated frame buffer using the TI mmWave SDK.
        ///
        /// Framing/validation is done in Rust before this is called.
        fn parse_mmwave_frame(frame: &[u8]) -> MmwaveFrame;
    }
}
