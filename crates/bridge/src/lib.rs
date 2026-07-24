// SPDX-License-Identifier: Apache-2.0

//! Wiring layer between the domain crates and the BLE transport.
//!
//! `snf-radar` produces [`IndicatorSnapshot`](snf_radar::IndicatorSnapshot)s and
//! `snf-fatigue` produces [`FatigueLevel`](snf_fatigue::FatigueLevel)s; `snf-ble`
//! speaks the SNF Telemetry Protocol v1 wire types. This crate is the pure,
//! host-testable glue between them, so the CA35 application ([`snf-app`]) stays a
//! thin orchestration loop that opens devices, calls these functions, and ships
//! the bytes.
//!
//! Three concerns live here:
//!
//! * [`map`] — pure conversions: an indicator snapshot to a BLE [`Vitals`] record
//!   (with quality flags), a snapshot to [`FatigueFeatures`], a fatigue verdict
//!   to a BLE [`Fatigue`] record.
//! * [`Accounting`] — the running counters behind the Device Status message
//!   (uptime, dropped frames, radar gaps).
//! * [`control`] — the Stream Control state machine: apply a client
//!   [`ControlRequest`](snf_ble::protocol::ControlRequest), clamp it to what this
//!   build supports, and produce the [`ControlResponse`](snf_ble::protocol::ControlResponse).
//!
//! Nothing here does I/O or touches BlueZ; every function is deterministic and
//! unit-tested against the protocol's scaling and sentinel rules.

mod accounting;
pub mod control;
pub mod map;

pub use accounting::Accounting;
pub use control::StreamState;
pub use map::{MappedVitals, fatigue_features, fatigue_telemetry, vitals};
