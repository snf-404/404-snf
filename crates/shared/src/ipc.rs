// SPDX-License-Identifier: Apache-2.0

//! IPC message types for the `actuator` channel declared in `Consortium.toml`.
//!
//! Both directions use the default postcard codec, so every type derives
//! `serde::Serialize`/`Deserialize`. `IpcSafe` rejects pointers, references and
//! width-dependent integers (`usize`/`isize`) from crossing the core boundary.

use consortium_ipc::IpcSafe;
use serde::{Deserialize, Serialize};

/// Command sent from the CA35 application to the CM33 pneumatic controller,
/// derived from the current fatigue verdict.
///
/// Placeholder shape — a real protocol will likely carry per-zone set-points and
/// a richer mode enum.
#[derive(Clone, Copy, Debug, IpcSafe, Deserialize, Serialize)]
pub struct PneumaticCommand {
    /// Target bladder pressure in kilopascals.
    pub target_pressure_kpa: u16,
    /// Bitmask selecting which actuator zones this command applies to.
    pub actuator_mask: u8,
    /// Control mode (0 = idle, 1 = hold, 2 = pulse, ... — to be defined).
    pub mode: u8,
}

/// Status reported from the CM33 pneumatic controller back to the CA35.
#[derive(Clone, Copy, Debug, IpcSafe, Deserialize, Serialize)]
pub struct PneumaticStatus {
    /// Measured bladder pressure in kilopascals.
    pub pressure_kpa: u16,
    /// Whether the pump is currently energized.
    pub pump_on: bool,
    /// Monotonic sequence counter; wraps at `u32::MAX`.
    pub seq: u32,
}
