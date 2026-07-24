// SPDX-License-Identifier: Apache-2.0

//! BLE upper layer for 404-snf.
//!
//! Implements the **SNF Telemetry Protocol v1** (`PROTOCOL.md`): the CA35
//! exposes a GATT peripheral that publishes vitals, fatigue, device status, and
//! (when models exist) pose and point-cloud telemetry, and accepts Stream
//! Control requests from a phone or Web Bluetooth client.
//!
//! The crate is split into a portable core and a platform backend:
//!
//! * [`protocol`] — the little-endian wire codec (headers, payloads, Stream
//!   Control). Pure and unit-tested on any host; the single Rust source of truth
//!   for the byte layout the TypeScript and native clients must match.
//! * [`fragment`] — splits a logical message into `ATT_MTU - 3` notifications.
//! * [`backend`] — the [`BleTransport`] trait and the [`Telemetry`] message enum
//!   the application publishes.
//! * [`bluez`] — the concrete backend: official BlueZ D-Bus bindings via the
//!   `bluer` crate. The pragmatic choice on OpenSTLinux, which runs
//!   `bluetoothd`. Linux-only, so the module is gated to `target_os = "linux"`;
//!   on a macOS dev host only the portable core and the trait compile.
//!
//! A future `trouble` backend (a `trouble-host` HCI-direct stack that *replaces*
//! BlueZ) would be an added module implementing the same trait, not a rewrite.

pub mod fragment;
pub mod protocol;
mod wire;

mod backend;

pub use backend::{BleError, BleTransport, Telemetry};

#[cfg(all(feature = "bluez", target_os = "linux"))]
pub mod bluez;

#[cfg(all(feature = "bluez", target_os = "linux"))]
pub use bluez::BluezPeripheral;
