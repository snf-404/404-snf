// SPDX-License-Identifier: Apache-2.0

//! Backend-agnostic BLE peripheral interface.
//!
//! The CA35 application talks to this trait; the concrete backend (`bluez`
//! today, `trouble` in the future) is selected by crate feature. Keeping the
//! seam here means evaluating `trouble-host` later is an added module, not a
//! rewrite.

use serde::{Deserialize, Serialize};

/// The value 404-snf publishes to subscribers: the current fatigue verdict.
///
/// Placeholder shape, mirrored by the TypeScript definitions in
/// `apps/web/app/utils/protocol.ts` that the Web Bluetooth frontend consumes.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FatigueReport {
    /// Fatigue level, 0 (alert) .. 100 (severely fatigued).
    pub level: u8,
    /// Confidence of the estimate, 0.0 .. 1.0.
    pub confidence: f32,
    /// Monotonic sequence counter.
    pub seq: u32,
}

/// Errors surfaced by a BLE backend. Deliberately coarse for the scaffold.
#[derive(Debug)]
pub enum BleError {
    /// The backend could not reach the controller / adapter.
    Unavailable,
    /// A backend-specific failure, described by the message.
    Backend(String),
}

impl core::fmt::Display for BleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BleError::Unavailable => write!(f, "BLE controller unavailable"),
            BleError::Backend(msg) => write!(f, "BLE backend error: {msg}"),
        }
    }
}

impl std::error::Error for BleError {}

/// A GATT peripheral that advertises the fatigue service and pushes updates.
pub trait BleTransport {
    /// Start advertising and register the GATT service.
    fn start(&mut self) -> impl std::future::Future<Output = Result<(), BleError>> + Send;

    /// Publish a new fatigue report to subscribed centrals.
    fn publish(
        &mut self,
        report: FatigueReport,
    ) -> impl std::future::Future<Output = Result<(), BleError>> + Send;
}
