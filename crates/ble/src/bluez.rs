// SPDX-License-Identifier: Apache-2.0

//! BlueZ backend (`bluer`). Linux-only.
//!
//! Registers a GATT peripheral advertising the 404-snf fatigue service and
//! notifies the fatigue characteristic on each [`publish`](BleTransport::publish).
//!
//! Scaffold: opens a BlueZ session but does not yet build the advertisement or
//! GATT application. The commented outline shows where `bluer::adv::Advertisement`
//! and `bluer::gatt::local::Application` go.

use uuid::Uuid;

use crate::backend::{BleError, BleTransport, FatigueReport};

/// 128-bit UUID of the 404-snf fatigue GATT service.
pub const FATIGUE_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_5f04_0000_1000_8000_00805f9b34fb);
/// 128-bit UUID of the fatigue-level characteristic (notify).
pub const FATIGUE_LEVEL_CHAR_UUID: Uuid = Uuid::from_u128(0x0000_5f05_0000_1000_8000_00805f9b34fb);

/// BlueZ-backed fatigue peripheral.
pub struct BluezPeripheral {
    adapter_name: Option<String>,
    session: Option<bluer::Session>,
}

impl BluezPeripheral {
    /// Create a peripheral, optionally pinned to a specific adapter (e.g.
    /// `hci0`); `None` uses the default adapter.
    pub fn new(adapter_name: Option<String>) -> Self {
        Self {
            adapter_name,
            session: None,
        }
    }
}

impl BleTransport for BluezPeripheral {
    async fn start(&mut self) -> Result<(), BleError> {
        let session = bluer::Session::new()
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;

        // Outline of the real bring-up:
        //   let adapter = match &self.adapter_name {
        //       Some(name) => session.adapter(name),
        //       None => session.default_adapter().await,
        //   }?;
        //   adapter.set_powered(true).await?;
        //   let _adv = adapter.advertise(Advertisement { .. }).await?;
        //   let _app = adapter.serve_gatt_application(Application { .. }).await?;
        let _ = &self.adapter_name;

        self.session = Some(session);
        Ok(())
    }

    async fn publish(&mut self, _report: FatigueReport) -> Result<(), BleError> {
        // Real impl: write `_report` into the notify characteristic's value and
        // signal subscribed centrals.
        if self.session.is_none() {
            return Err(BleError::Unavailable);
        }
        Ok(())
    }
}
