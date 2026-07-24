// SPDX-License-Identifier: Apache-2.0

//! Backend-agnostic BLE peripheral interface.
//!
//! The CA35 application talks to this trait; the concrete backend (`bluez`
//! today, `trouble` in the future) is selected by crate feature. Keeping the
//! seam here means evaluating `trouble-host` later is an added module, not a
//! rewrite.
//!
//! The trait deals in *logical* SNF telemetry messages ([`Telemetry`]) and
//! Stream Control requests/responses; sequence numbering, timestamping, and
//! fragmentation are the backend's job. This mirrors the priority requirement in
//! `PROTOCOL.md` §13: the backend, not the caller, decides how competing streams
//! share a congested link.

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::protocol::{
    ControlRequest, ControlResponse, DEVICE_STATUS_UUID, DeviceStatus, FATIGUE_UUID, Fatigue,
    MessageType, POINT_CLOUD_UUID, POSE_UUID, PointCloud, Pose, VITALS_UUID, Vitals,
};

/// One logical telemetry message to publish. Each variant maps to exactly one
/// notify characteristic and one [`MessageType`]; the backend encodes the
/// payload, prepends the telemetry header, and fragments to the link MTU.
#[derive(Clone, Debug)]
pub enum Telemetry {
    Status(DeviceStatus),
    Vitals(Vitals),
    Fatigue(Fatigue),
    Pose(Pose),
    PointCloud(PointCloud),
}

impl Telemetry {
    /// The message type byte for this payload's telemetry header.
    pub fn message_type(&self) -> MessageType {
        match self {
            Telemetry::Status(_) => MessageType::DeviceStatus,
            Telemetry::Vitals(_) => MessageType::Vitals,
            Telemetry::Fatigue(_) => MessageType::Fatigue,
            Telemetry::Pose(_) => MessageType::Pose,
            Telemetry::PointCloud(_) => MessageType::PointCloud,
        }
    }

    /// UUID of the characteristic that notifies this payload.
    pub fn characteristic_uuid(&self) -> Uuid {
        match self {
            Telemetry::Status(_) => DEVICE_STATUS_UUID,
            Telemetry::Vitals(_) => VITALS_UUID,
            Telemetry::Fatigue(_) => FATIGUE_UUID,
            Telemetry::Pose(_) => POSE_UUID,
            Telemetry::PointCloud(_) => POINT_CLOUD_UUID,
        }
    }

    /// Encode the logical payload (without the telemetry header).
    pub fn encode_payload(&self) -> Vec<u8> {
        match self {
            Telemetry::Status(status) => status.encode(),
            Telemetry::Vitals(vitals) => vitals.encode(),
            Telemetry::Fatigue(fatigue) => fatigue.encode(),
            Telemetry::Pose(pose) => pose.encode(),
            Telemetry::PointCloud(cloud) => cloud.encode(),
        }
    }
}

/// Errors surfaced by a BLE backend.
#[derive(Debug)]
pub enum BleError {
    /// The backend could not reach the controller / adapter.
    Unavailable,
    /// The link MTU is too small to carry this message (see
    /// [`FragmentError::MtuTooSmall`](crate::fragment::FragmentError)). The
    /// backend should have already shed low-priority streams before this
    /// surfaces (`PROTOCOL.md` §14).
    LinkTooConstrained,
    /// A backend-specific failure, described by the message.
    Backend(String),
}

impl core::fmt::Display for BleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BleError::Unavailable => write!(f, "BLE controller unavailable"),
            BleError::LinkTooConstrained => write!(f, "BLE link MTU too small for message"),
            BleError::Backend(msg) => write!(f, "BLE backend error: {msg}"),
        }
    }
}

impl std::error::Error for BleError {}

/// A GATT peripheral that advertises the SNF telemetry service, notifies
/// telemetry, and delivers Stream Control requests to the application.
pub trait BleTransport {
    /// Start advertising and register the GATT service.
    fn start(&mut self) -> impl std::future::Future<Output = Result<(), BleError>> + Send;

    /// Publish one telemetry message to subscribed centrals.
    ///
    /// `flags` carries message-level telemetry-header flags (`SNAPSHOT`,
    /// `DEGRADED`, `STALE` from [`header_flags`](crate::protocol::header_flags));
    /// `MORE_FRAGMENTS` is managed by the fragmenter and must not be passed here.
    fn publish(
        &mut self,
        telemetry: Telemetry,
        flags: u8,
    ) -> impl std::future::Future<Output = Result<(), BleError>> + Send;

    /// Answer a Stream Control request. Delivered via Indicate on the Stream
    /// Control characteristic so the result is ATT-acknowledged.
    fn respond(
        &mut self,
        response: ControlResponse,
    ) -> impl std::future::Future<Output = Result<(), BleError>> + Send;

    /// Take the receiver of incoming Stream Control requests. Yields the channel
    /// once (subsequent calls return `None`); the application owns it thereafter
    /// and processes requests off the notify path.
    fn control_requests(&mut self) -> Option<mpsc::Receiver<ControlRequest>>;
}
