// SPDX-License-Identifier: Apache-2.0

/**
 * Shared protocol definitions for the 404-snf BLE fatigue service.
 *
 * The Nuxt + Web Bluetooth frontend (apps/www, scaffolded externally) imports
 * these UUIDs and the payload decoder so the wire format stays in one place.
 * Mirrors the Rust side in `crates/ble` (`FatigueReport`, `FATIGUE_*_UUID`).
 *
 * Scaffold: the byte layout below is a placeholder and must stay in lockstep
 * with the Rust encoder once that is implemented.
 */

/** GATT service UUID advertised by the device. */
export const FATIGUE_SERVICE_UUID = '00005f04-0000-1000-8000-00805f9b34fb'

/** Notify characteristic carrying the fatigue verdict. */
export const FATIGUE_LEVEL_CHAR_UUID = '00005f05-0000-1000-8000-00805f9b34fb'

/** Decoded fatigue verdict, mirroring the Rust `FatigueReport`. */
export interface FatigueReport {
  /** Fatigue level, 0 (alert) .. 100 (severely fatigued). */
  level: number
  /** Confidence, 0.0 .. 1.0. */
  confidence: number
  /** Monotonic sequence counter. */
  seq: number
}

/**
 * Decode a notify payload (little-endian: u8 level, f32 confidence, u32 seq).
 * Placeholder layout — keep in sync with the Rust encoder.
 */
export function decodeFatigueReport(data: DataView): FatigueReport {
  return {
    level: data.getUint8(0),
    confidence: data.getFloat32(1, true),
    seq: data.getUint32(5, true),
  }
}
