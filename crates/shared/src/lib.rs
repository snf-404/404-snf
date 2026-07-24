// SPDX-License-Identifier: Apache-2.0

//! Shared, `IpcSafe` message types for 404-snf.
//!
//! These types are referenced by name from the generated `consortium.gen.rs`
//! modules in `crates/app` and `crates/mcu` (the `party.<name>.type` entries in
//! `Consortium.toml`). They must stay `no_std` and free of address-space-local
//! constructs so the same layout is valid on both the 64-bit CA35 and the
//! 32-bit CM33.
//!
//! The one channel carries the radar: the CM33 owns USART6, parses the IWR6843's
//! TLV frames with [`detect`], and reports one [`RadarReport`] per frame; the
//! CA35 gates it with [`RadarControl`]. Actuation does not appear here — the
//! pneumatics hang off TIM4/TIM5 on the CA35 and never cross the core boundary.

#![no_std]

pub mod detect;
mod ipc;

pub use ipc::{RADAR_REPORT_POINTS, RadarControl, RadarPointFixed, RadarReport};
