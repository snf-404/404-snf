// SPDX-License-Identifier: Apache-2.0

//! Shared, `IpcSafe` message types for 404-snf.
//!
//! These types are referenced by name from the generated `consortium.gen.rs`
//! modules in `crates/app` and `crates/mcu` (the `party.<name>.type` entries in
//! `Consortium.toml`). They must stay `no_std` and free of address-space-local
//! constructs so the same layout is valid on both the 64-bit CA35 and the
//! 32-bit CM33.
//!
//! Scaffold only: fields are placeholders and will change once the pneumatic
//! control protocol is firmed up.

#![no_std]

mod ipc;

pub use ipc::{PneumaticCommand, PneumaticStatus};
