// SPDX-License-Identifier: Apache-2.0

//! IWR6843 mmWave radar upper layer.
//!
//! Responsibilities split, per the 404-snf design:
//!
//! * **Transport & orchestration — Rust.** [`RadarStream`] owns the radar's data
//!   UART via `tokio-serial`, drives the config/data state machine, reframes the
//!   byte stream, and hands frames downstream. All of this is Rust.
//! * **Raw TLV parsing — C/C++ (via `cxx`).** Only the innermost step, turning a
//!   validated frame buffer into POD detection/vitals structs, is delegated to
//!   the TI mmWave SDK through the [`ffi`] bridge (compiled under the `sdk`
//!   feature). Without `sdk`, [`parse_frame`] returns a stub.
//!
//! Scaffold only: no real serial I/O, framing, or feature extraction yet.

mod parser;

#[cfg(feature = "sdk")]
mod ffi;

pub use parser::{RadarFrame, parse_frame};

use serde::{Deserialize, Serialize};

/// Configuration for opening the IWR6843 data UART.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RadarConfig {
    /// Serial device path for the radar data port (e.g. `/dev/ttyUSB1`).
    pub data_port: String,
    /// Baud rate of the data UART (IWR6843 mmw demo default is 921_600).
    pub baud_rate: u32,
}

impl Default for RadarConfig {
    fn default() -> Self {
        Self {
            data_port: "/dev/ttyUSB1".to_string(),
            baud_rate: 921_600,
        }
    }
}

/// Async source of parsed radar frames.
///
/// Wraps the tokio-serial reader plus the Rust-side reframing state machine.
/// Stubbed for now.
pub struct RadarStream {
    _config: RadarConfig,
}

impl RadarStream {
    /// Open the radar data UART described by `config`.
    ///
    /// Stub: does not touch hardware yet.
    pub fn open(config: RadarConfig) -> Self {
        Self { _config: config }
    }

    /// Await the next fully-framed radar frame.
    ///
    /// Stub: real implementation will read from `tokio-serial`, reassemble the
    /// magic-word-delimited frame, then call [`parse_frame`].
    pub async fn next_frame(&mut self) -> Option<RadarFrame> {
        None
    }
}
