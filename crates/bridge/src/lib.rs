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
//! Six concerns live here:
//!
//! * [`map`] — pure conversions: an indicator snapshot to a BLE [`Vitals`] record
//!   (with quality flags), a fatigue verdict to a BLE [`Fatigue`] record.
//! * [`features`] — the rolling window and personal baselines behind the fatigue
//!   model's input. Stateful, unlike [`map`], because the model reads trends
//!   rather than instants.
//! * [`confidence`] — how much of a verdict is allowed to reach the actuators.
//!   Low confidence scales the level down and, low enough, withholds it
//!   entirely.
//! * [`Accounting`] — the running counters behind the Device Status message
//!   (uptime, dropped frames, radar gaps).
//! * [`control`] — the Stream Control state machine: apply a client
//!   [`ControlRequest`](snf_ble::protocol::ControlRequest), clamp it to what this
//!   build supports, and produce the [`ControlResponse`](snf_ble::protocol::ControlResponse).
//! * [`inflation`] — the fatigue → inflation-speed model: which deformation mode
//!   a fatigue level calls for, how fast to get there, and every ceiling that
//!   bounds it. The application turns its [`Actuation`] into pump and valve
//!   writes and nothing more.
//!
//! So the fatigue path reads: radar snapshot → [`features`] → the ONNX model →
//! [`confidence`] → [`inflation`] → actuator state. Each step is a separate,
//! separately-testable decision, and only the last one knows there is hardware.
//!
//! Nothing here does I/O or touches BlueZ; every function is deterministic and
//! unit-tested against the protocol's scaling and sentinel rules.

mod accounting;
pub mod confidence;
pub mod control;
pub mod features;
pub mod inflation;
pub mod map;

pub use accounting::Accounting;
pub use confidence::Trust;
pub use control::StreamState;
pub use features::FeatureExtractor;
pub use inflation::{
    Actuation, InflationCommand, InflationController, InflationMode, InflationParams,
};
pub use map::{MappedVitals, fatigue_telemetry, vitals};
