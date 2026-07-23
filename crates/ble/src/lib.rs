// SPDX-License-Identifier: Apache-2.0

//! BLE upper layer for 404-snf.
//!
//! Exposes the fatigue state as a GATT peripheral so the phone / Web Bluetooth
//! frontend can subscribe. The concrete stack is chosen at compile time behind
//! the [`BleTransport`] trait:
//!
//! * `bluez` (default) — official BlueZ D-Bus bindings via the `bluer` crate.
//!   The pragmatic choice on OpenSTLinux, which runs `bluetoothd`. Linux-only,
//!   so the backend module is gated to `target_os = "linux"`; on a macOS dev
//!   host only the trait compiles.
//! * `trouble` (not implemented) — a future `trouble-host` backend driving an
//!   HCI controller directly. On Linux this *replaces* BlueZ rather than layering
//!   on it, so the two are mutually exclusive.
//!
//! Scaffold only: no advertising, services, or characteristics are registered.

mod backend;

pub use backend::{BleError, BleTransport, FatigueReport};

#[cfg(all(feature = "bluez", target_os = "linux"))]
pub mod bluez;

#[cfg(all(feature = "bluez", target_os = "linux"))]
pub use bluez::BluezPeripheral;
