// SPDX-License-Identifier: Apache-2.0

//! IPC message types for the `actuator` channel declared in `Consortium.toml`.
//!
//! Both directions use the default postcard codec, so every type derives
//! `serde::Serialize`/`Deserialize`. `IpcSafe` rejects pointers, references and
//! width-dependent integers (`usize`/`isize`) from crossing the core boundary.

use consortium_ipc::IpcSafe;
use serde::{Deserialize, Serialize};

/// Liveness heartbeat sent from the CA35 application to the CM33 safety
/// supervisor, once per control tick.
///
/// The CM33 treats the *arrival* of these messages as the deadman ping: if none
/// lands within its timeout, it assumes Linux has hung and cuts the pump rail.
/// `pump_enable` additionally lets the CA35 gate the rail explicitly (e.g. to
/// power the pneumatics down cleanly).
#[derive(Clone, Copy, Debug, IpcSafe, Deserialize, Serialize)]
pub struct PneumaticCommand {
    /// Monotonic heartbeat counter; wraps at `u32::MAX`.
    pub seq: u32,
    /// Whether the CA35 wants the pump power rail energized.
    pub pump_enable: bool,
}

/// Safety state reported from the CM33 supervisor back to the CA35.
#[derive(Clone, Copy, Debug, IpcSafe, Deserialize, Serialize)]
pub struct PneumaticStatus {
    /// Whether the pump power rail is currently energized by the interlock.
    pub rail_enabled: bool,
    /// True once the deadman has tripped (no heartbeat within the timeout);
    /// cleared automatically when heartbeats resume.
    pub tripped: bool,
    /// Monotonic sequence counter; wraps at `u32::MAX`.
    pub seq: u32,
}
