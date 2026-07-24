// SPDX-License-Identifier: Apache-2.0

/**
 * SNF Telemetry Protocol v1 — Web Bluetooth client codec.
 *
 * Mirrors the Rust wire codec in `crates/ble/src/protocol.rs`, byte for byte:
 * the same little-endian layout, the same `0xffff` / `0x7fff` sentinels, and the
 * same fragment-reassembly rules (`PROTOCOL.md` §6). Both sides must decode the
 * same golden vectors — do not reinterpret the layout here (`PROTOCOL.md` §14).
 *
 * Conventions (`PROTOCOL.md` §4): every multi-byte integer is little-endian;
 * rates are `u16` in `0.01 bpm` units with `0xffff` meaning unavailable;
 * coordinates are `i16` millimetres in the x-right / y-out / z-up frame; unknown
 * flags, message types, and trailing fields are ignored (§16). Decoders return
 * `null` for a field the device marked unavailable so the UI never shows a
 * sentinel as a real value.
 */

// ── GATT UUIDs (PROTOCOL.md §3) ──────────────────────────────────────────────
// All characteristics share the service's 128-bit base; only the last byte
// varies (`7b9f0001-6b44-4d2a-9f36-4040534e46xx`).

const snfUuid = (suffix: number): string =>
  `7b9f0001-6b44-4d2a-9f36-4040534e46${suffix.toString(16).padStart(2, '0')}`

/** Primary service grouping all SNF telemetry. */
export const SERVICE_UUID = snfUuid(0x00)
/** Protocol Info — 24-byte read of version, capabilities, limits. */
export const PROTOCOL_INFO_UUID = snfUuid(0x01)
/** Stream Control — write-with-response requests, indicated responses. */
export const STREAM_CONTROL_UUID = snfUuid(0x02)
/** Device Status — read + notify. */
export const DEVICE_STATUS_UUID = snfUuid(0x03)
/** Vitals — read + notify. */
export const VITALS_UUID = snfUuid(0x04)
/** Fatigue — read + notify (optional). */
export const FATIGUE_UUID = snfUuid(0x05)
/** Pose — notify. */
export const POSE_UUID = snfUuid(0x06)
/** Point Cloud — notify. */
export const POINT_CLOUD_UUID = snfUuid(0x07)

/** Local name advertised by the device. */
export const ADVERTISED_NAME = '404-SNF'

// ── Enumerations and bit flags ───────────────────────────────────────────────

/** Telemetry header message type (`PROTOCOL.md` §6). */
export enum MessageType {
  DeviceStatus = 0x10,
  Vitals = 0x20,
  Fatigue = 0x21,
  Pose = 0x30,
  PointCloud = 0x31,
  ControlResponse = 0x40,
}

/** Telemetry header flag bits (`PROTOCOL.md` §6). */
export const HeaderFlags = {
  MORE_FRAGMENTS: 1 << 0,
  SNAPSHOT: 1 << 1,
  DEGRADED: 1 << 2,
  STALE: 1 << 3,
} as const

/** Protocol Info capability bits (`PROTOCOL.md` §5). */
export const Capabilities = {
  VITALS: 1 << 0,
  FATIGUE: 1 << 1,
  POSE_3D: 1 << 2,
  POINT_CLOUD_3D: 1 << 3,
  MULTI_SUBJECT: 1 << 4,
  BATTERY_STATUS: 1 << 5,
  ENCRYPTION_REQUIRED: 1 << 6,
} as const

/** Vitals `status_flags` bits (`PROTOCOL.md` §7). */
export const VitalsFlags = {
  SUBJECT_TRACKED: 1 << 0,
  HEART_VALID: 1 << 1,
  RESPIRATION_VALID: 1 << 2,
  WARMING_UP: 1 << 3,
  MOTION_CONTAMINATED: 1 << 4,
  VENDOR_VALUE_INVALID: 1 << 5,
  RADAR_GAP: 1 << 6,
} as const

/** Fatigue `status_flags` bits (`PROTOCOL.md` §8). */
export const FatigueFlags = {
  VALID: 1 << 0,
  WARMING_UP: 1 << 1,
  INSUFFICIENT_INPUT: 1 << 2,
} as const

/** Stream mask bits, shared by Stream Control and Device Status (§12). */
export const StreamMask = {
  STATUS: 1 << 0,
  VITALS: 1 << 1,
  FATIGUE: 1 << 2,
  POSE: 1 << 3,
  POINT_CLOUD: 1 << 4,
} as const

/** Stream Control opcodes (`PROTOCOL.md` §12). */
export enum ControlOpcode {
  SetStreams = 0x01,
  SetSubject = 0x02,
  RequestSnapshot = 0x03,
  Ping = 0x04,
}

/** Control Response result codes (`PROTOCOL.md` §12). */
export enum ControlResult {
  Success = 0,
  Unsupported = 1,
  Invalid = 2,
  Busy = 3,
  Denied = 4,
}

/** Pose skeleton model ids (`PROTOCOL.md` §9). */
export enum PoseModel {
  Coco17 = 1,
  BlazePose33 = 2,
}

// Sentinels and fixed sizes, matching the Rust constants.
const PROTOCOL_MAJOR = 1
const RATE_UNAVAILABLE = 0xffff
const SUBJECT_UNKNOWN = 0xffff
const BATTERY_MV_UNAVAILABLE = 0xffff
const TEMP_UNAVAILABLE = 0x7fff
const SNR_UNKNOWN = 0xff

const TELEMETRY_HEADER_LEN = 16
const PROTOCOL_INFO_LEN = 24
const DEVICE_STATUS_LEN = 20
const VITALS_LEN = 24
const FATIGUE_LEN = 12
const CONTROL_HEADER_LEN = 8
const CONTROL_RESPONSE_LEN = 10
const PING_MAX_ECHO = 16

// ── Protocol Info (fixed 24-byte read, PROTOCOL.md §5) ───────────────────────

export interface ProtocolInfo {
  major: number
  minor: number
  telemetryHeaderLen: number
  coordinateFrame: number
  capabilities: number
  maxPointCount: number
  maxPoseJoints: number
  maxSubjects: number
  /** Changes on every boot; a new value means the device restarted (§14). */
  bootId: number
  /** Firmware build id; `0` means unknown. */
  buildId: number
}

/**
 * Decode the Protocol Info read. Throws if the magic or length is wrong, or if
 * `major` is a version this client cannot parse (§16 — the caller should show an
 * upgrade prompt rather than continue).
 */
export function decodeProtocolInfo(data: DataView): ProtocolInfo {
  if (data.byteLength < PROTOCOL_INFO_LEN) {
    throw new Error(`Protocol Info too short: ${data.byteLength} bytes`)
  }
  const magic = String.fromCharCode(
    data.getUint8(0),
    data.getUint8(1),
    data.getUint8(2),
    data.getUint8(3),
  )
  if (magic !== 'SNF1') {
    throw new Error(`Protocol Info magic mismatch: ${JSON.stringify(magic)}`)
  }
  const major = data.getUint8(4)
  if (major !== PROTOCOL_MAJOR) {
    throw new Error(`unsupported protocol major ${major}; this client speaks v${PROTOCOL_MAJOR}`)
  }
  return {
    major,
    minor: data.getUint8(5),
    telemetryHeaderLen: data.getUint8(6),
    coordinateFrame: data.getUint8(7),
    capabilities: data.getUint32(8, true),
    maxPointCount: data.getUint16(12, true),
    maxPoseJoints: data.getUint8(14),
    maxSubjects: data.getUint8(15),
    bootId: data.getUint32(16, true),
    buildId: data.getUint32(20, true),
  }
}

// ── Telemetry header (16 bytes, repeated per fragment, PROTOCOL.md §6) ────────

export interface TelemetryHeader {
  protocolMajor: number
  messageType: number
  flags: number
  headerLen: number
  sequence: number
  timestampMs: number
  totalPayloadLen: number
  fragmentOffset: number
}

/** Decode the 16-byte header, or `null` if the buffer is too short. */
export function decodeTelemetryHeader(data: DataView): TelemetryHeader | null {
  if (data.byteLength < TELEMETRY_HEADER_LEN) return null
  return {
    protocolMajor: data.getUint8(0),
    messageType: data.getUint8(1),
    flags: data.getUint8(2),
    headerLen: data.getUint8(3),
    sequence: data.getUint32(4, true),
    timestampMs: data.getUint32(8, true),
    totalPayloadLen: data.getUint16(12, true),
    fragmentOffset: data.getUint16(14, true),
  }
}

// ── Payload decoders ─────────────────────────────────────────────────────────

/** Device Status (`PROTOCOL.md` §11). Unavailable values decode to `null`. */
export interface DeviceStatus {
  uptimeS: number
  activeStreams: number
  lastError: number
  droppedPoseFrames: number
  droppedPointFrames: number
  radarGapCount: number
  /** millivolts, or `null` if not provided. */
  batteryMv: number | null
  /** °C, or `null` if not provided. */
  processorTempC: number | null
}

export function decodeDeviceStatus(payload: DataView): DeviceStatus {
  if (payload.byteLength < DEVICE_STATUS_LEN) {
    throw new Error(`Device Status too short: ${payload.byteLength} bytes`)
  }
  const batteryMv = payload.getUint16(14, true)
  const tempRaw = payload.getInt16(16, true)
  return {
    uptimeS: payload.getUint32(0, true),
    activeStreams: payload.getUint16(4, true),
    lastError: payload.getUint16(6, true),
    droppedPoseFrames: payload.getUint16(8, true),
    droppedPointFrames: payload.getUint16(10, true),
    radarGapCount: payload.getUint16(12, true),
    batteryMv: batteryMv === BATTERY_MV_UNAVAILABLE ? null : batteryMv,
    processorTempC: tempRaw === TEMP_UNAVAILABLE ? null : tempRaw / 100,
  }
}

/**
 * Vitals (`PROTOCOL.md` §7). `heartRateBpm` / `respirationRateBpm` are `null`
 * when the device sent the unavailable sentinel. Status precedes value: check
 * the `statusFlags` (e.g. `MOTION_CONTAMINATED`) and the header's `STALE` flag
 * before trusting a rate as a fresh measurement.
 */
export interface Vitals {
  subjectId: number | null
  statusFlags: number
  heartRateBpm: number | null
  respirationRateBpm: number | null
  heartConfidence: number
  respirationConfidence: number
  activityConfidence: number
  /** Mean squared radial velocity, in `(m/s)²`. */
  motionEnergyM2S2: number
  rmsSpeedMps: number
  /** Fraction of moving points, `0..=1`. */
  movingFraction: number
  rangeBin: number
  breathingDeviation: number
}

export function decodeVitals(payload: DataView): Vitals {
  if (payload.byteLength < VITALS_LEN) {
    throw new Error(`Vitals too short: ${payload.byteLength} bytes`)
  }
  const subjectId = payload.getUint16(0, true)
  const heart = payload.getUint16(4, true)
  const respiration = payload.getUint16(6, true)
  return {
    subjectId: subjectId === SUBJECT_UNKNOWN ? null : subjectId,
    statusFlags: payload.getUint16(2, true),
    heartRateBpm: heart === RATE_UNAVAILABLE ? null : heart / 100,
    respirationRateBpm: respiration === RATE_UNAVAILABLE ? null : respiration / 100,
    heartConfidence: payload.getUint8(8),
    respirationConfidence: payload.getUint8(9),
    activityConfidence: payload.getUint8(10),
    // offset 11 reserved
    motionEnergyM2S2: payload.getUint32(12, true) / 1_000_000,
    rmsSpeedMps: payload.getUint16(16, true) / 1000,
    movingFraction: payload.getUint16(18, true) / 32767,
    rangeBin: payload.getUint16(20, true),
    breathingDeviation: payload.getInt16(22, true) / 256,
  }
}

/** Fatigue (`PROTOCOL.md` §8). */
export interface Fatigue {
  /** `0..=100`. */
  level: number
  /** `0..=100`. */
  confidence: number
  statusFlags: number
  modelRevision: number
}

export function decodeFatigue(payload: DataView): Fatigue {
  if (payload.byteLength < FATIGUE_LEN) {
    throw new Error(`Fatigue too short: ${payload.byteLength} bytes`)
  }
  return {
    level: payload.getUint8(0),
    confidence: payload.getUint8(1),
    statusFlags: payload.getUint16(2, true),
    modelRevision: payload.getUint32(4, true),
    // offset 8 reserved
  }
}

/** One 3D joint (`PROTOCOL.md` §9). */
export interface Joint {
  jointId: number
  confidence: number
  xMm: number
  yMm: number
  zMm: number
}

/** Pose skeleton (`PROTOCOL.md` §9). */
export interface Pose {
  subjectId: number
  modelId: number
  coordinateFrame: number
  poseFlags: number
  joints: Joint[]
}

export function decodePose(payload: DataView): Pose {
  if (payload.byteLength < 8) {
    throw new Error(`Pose header too short: ${payload.byteLength} bytes`)
  }
  const jointCount = payload.getUint8(3)
  const expected = 8 + jointCount * 8
  if (payload.byteLength < expected) {
    throw new Error(`Pose truncated: ${payload.byteLength} < ${expected} bytes`)
  }
  const joints: Joint[] = []
  for (let i = 0; i < jointCount; i++) {
    const o = 8 + i * 8
    joints.push({
      jointId: payload.getUint8(o),
      confidence: payload.getUint8(o + 1),
      xMm: payload.getInt16(o + 2, true),
      yMm: payload.getInt16(o + 4, true),
      zMm: payload.getInt16(o + 6, true),
    })
  }
  return {
    subjectId: payload.getUint16(0, true),
    modelId: payload.getUint8(2),
    coordinateFrame: payload.getUint8(4),
    poseFlags: payload.getUint8(5),
    joints,
  }
}

/** One point in format 1 (`PROTOCOL.md` §10). */
export interface CloudPoint {
  xMm: number
  yMm: number
  zMm: number
  /** Radial velocity in m/s; positive = away from radar. */
  radialVelocityMps: number
  /** SNR in dB, or `null` if unknown. */
  snrDb: number | null
}

/** Point cloud (`PROTOCOL.md` §10). */
export interface PointCloud {
  subjectId: number | null
  pointFormat: number
  coordinateFrame: number
  points: CloudPoint[]
}

export function decodePointCloud(payload: DataView): PointCloud {
  if (payload.byteLength < 8) {
    throw new Error(`Point Cloud header too short: ${payload.byteLength} bytes`)
  }
  const pointCount = payload.getUint16(2, true)
  const expected = 8 + pointCount * 8
  if (payload.byteLength < expected) {
    throw new Error(`Point Cloud truncated: ${payload.byteLength} < ${expected} bytes`)
  }
  const subjectId = payload.getUint16(0, true)
  const points: CloudPoint[] = []
  for (let i = 0; i < pointCount; i++) {
    const o = 8 + i * 8
    const snr = payload.getUint8(o + 7)
    points.push({
      xMm: payload.getInt16(o, true),
      yMm: payload.getInt16(o + 2, true),
      zMm: payload.getInt16(o + 4, true),
      radialVelocityMps: payload.getInt8(o + 6) * 0.02,
      snrDb: snr === SNR_UNKNOWN ? null : snr * 0.5,
    })
  }
  return {
    subjectId: subjectId === SUBJECT_UNKNOWN ? null : subjectId,
    pointFormat: payload.getUint8(4),
    coordinateFrame: payload.getUint8(5),
    points,
  }
}

/** Control Response payload (`PROTOCOL.md` §12). */
export interface ControlResponse {
  requestId: number
  opcode: number
  result: ControlResult
  effectiveStreamMask: number
  effectiveVitalsHz: number
  effectivePoseHz: number
  effectivePointCloudHz: number
  effectiveMaxPoints: number
  /** `PING` echo, empty for other opcodes. */
  echo: Uint8Array
}

export function decodeControlResponse(payload: DataView): ControlResponse {
  if (payload.byteLength < CONTROL_RESPONSE_LEN) {
    throw new Error(`Control Response too short: ${payload.byteLength} bytes`)
  }
  return {
    requestId: payload.getUint16(0, true),
    opcode: payload.getUint8(2),
    result: payload.getUint8(3) as ControlResult,
    effectiveStreamMask: payload.getUint16(4, true),
    effectiveVitalsHz: payload.getUint8(6),
    effectivePoseHz: payload.getUint8(7),
    effectivePointCloudHz: payload.getUint8(8),
    effectiveMaxPoints: payload.getUint8(9),
    echo: new Uint8Array(
      payload.buffer,
      payload.byteOffset + CONTROL_RESPONSE_LEN,
      payload.byteLength - CONTROL_RESPONSE_LEN,
    ),
  }
}

// ── Stream Control request encoders (client → device, PROTOCOL.md §12) ───────

function encodeControlRequest(
  opcode: ControlOpcode,
  requestId: number,
  body: Uint8Array,
): Uint8Array {
  const out = new Uint8Array(CONTROL_HEADER_LEN + body.length)
  const view = new DataView(out.buffer)
  view.setUint8(0, PROTOCOL_MAJOR)
  view.setUint8(1, opcode)
  view.setUint16(2, requestId, true)
  view.setUint16(4, body.length, true)
  // offset 6 reserved
  out.set(body, CONTROL_HEADER_LEN)
  return out
}

export interface StreamSettings {
  streamMask: number
  vitalsHz: number
  poseHz: number
  pointCloudHz: number
  maxPoints: number
}

/** Build a `SET_STREAMS` request value (`PROTOCOL.md` §12). */
export function encodeSetStreams(requestId: number, settings: StreamSettings): Uint8Array {
  const body = new Uint8Array(8)
  const view = new DataView(body.buffer)
  view.setUint16(0, settings.streamMask, true)
  view.setUint8(2, settings.vitalsHz)
  view.setUint8(3, settings.poseHz)
  view.setUint8(4, settings.pointCloudHz)
  view.setUint8(5, settings.maxPoints)
  // offset 6 reserved
  return encodeControlRequest(ControlOpcode.SetStreams, requestId, body)
}

/** Build a `SET_SUBJECT` request; pass `null` (→ `0xffff`) to auto-select. */
export function encodeSetSubject(requestId: number, subjectId: number | null): Uint8Array {
  const body = new Uint8Array(2)
  new DataView(body.buffer).setUint16(0, subjectId ?? SUBJECT_UNKNOWN, true)
  return encodeControlRequest(ControlOpcode.SetSubject, requestId, body)
}

/** Build a `REQUEST_SNAPSHOT` request over the given stream mask. */
export function encodeRequestSnapshot(requestId: number, streamMask: number): Uint8Array {
  const body = new Uint8Array(2)
  new DataView(body.buffer).setUint16(0, streamMask, true)
  return encodeControlRequest(ControlOpcode.RequestSnapshot, requestId, body)
}

/** Build a `PING` request; the payload (≤16 bytes) is echoed back. */
export function encodePing(requestId: number, payload: Uint8Array = new Uint8Array()): Uint8Array {
  return encodeControlRequest(ControlOpcode.Ping, requestId, payload.slice(0, PING_MAX_ECHO))
}

// ── Fragment reassembly (PROTOCOL.md §6, §14) ────────────────────────────────

/** A fully reassembled logical message handed to the application. */
export interface ReassembledMessage {
  messageType: number
  sequence: number
  timestampMs: number
  flags: number
  payload: DataView
}

/** How long an incomplete message may wait for its remaining fragments (§6). */
const REASSEMBLY_TIMEOUT_MS = 500

interface Pending {
  sequence: number
  total: number
  buf: Uint8Array
  filled: Uint8Array // 1 where a byte has been written; detects overlap
  filledCount: number
  sawLast: boolean
  flags: number
  timestampMs: number
  updatedAt: number
}

/**
 * Reassembles fragmented telemetry notifications keyed by
 * `(bootId, messageType, sequence)` (`PROTOCOL.md` §6, §14).
 *
 * Feed every notification through {@link push}; it returns a
 * {@link ReassembledMessage} once a logical message is complete, or `null`
 * while more fragments are outstanding or a fragment was dropped. One in-flight
 * message is tracked per message type: a newer sequence, a 500 ms stall, an
 * out-of-bounds offset, or an overlapping fragment discards the partial message
 * (§6 rule 4–5). Call {@link reset} when the device's `bootId` changes to flush
 * all filter state (§14).
 */
export class TelemetryReassembler {
  private pending = new Map<number, Pending>()
  private bootId: number | null = null

  /**
   * Note the device's current `bootId`. A change flushes every partial message
   * (and signals the caller to clear its own per-sequence history, §14).
   * Returns `true` if the boot id changed.
   */
  observeBootId(bootId: number): boolean {
    if (this.bootId === bootId) return false
    this.bootId = bootId
    this.pending.clear()
    return true
  }

  /** Drop all in-flight reassembly state. */
  reset(): void {
    this.pending.clear()
  }

  /**
   * Ingest one notification value (header + fragment). Returns the completed
   * message, or `null` if incomplete or dropped.
   *
   * `now` defaults to `performance.now()`; pass an explicit clock in tests.
   */
  push(data: DataView, now: number = performance.now()): ReassembledMessage | null {
    const header = decodeTelemetryHeader(data)
    if (!header || header.protocolMajor !== PROTOCOL_MAJOR) return null

    const fragment = new Uint8Array(
      data.buffer,
      data.byteOffset + TELEMETRY_HEADER_LEN,
      data.byteLength - TELEMETRY_HEADER_LEN,
    )
    const { messageType, sequence, fragmentOffset, totalPayloadLen } = header

    // Reject fragments that would write outside the declared payload (§6 rule 4).
    if (fragmentOffset + fragment.length > totalPayloadLen) {
      this.pending.delete(messageType)
      return null
    }

    let entry = this.pending.get(messageType)
    const stale = entry !== undefined && now - entry.updatedAt > REASSEMBLY_TIMEOUT_MS
    // Start fresh on a new sequence, first sight, or a timed-out partial (§6 rule 4).
    if (entry === undefined || entry.sequence !== sequence || stale) {
      entry = {
        sequence,
        total: totalPayloadLen,
        buf: new Uint8Array(totalPayloadLen),
        filled: new Uint8Array(totalPayloadLen),
        filledCount: 0,
        sawLast: false,
        flags: header.flags,
        timestampMs: header.timestampMs,
        updatedAt: now,
      }
      this.pending.set(messageType, entry)
    }

    // Overlap or a total-length disagreement is a conflict: discard (§6 rule 4).
    if (entry.total !== totalPayloadLen) {
      this.pending.delete(messageType)
      return null
    }
    for (let i = 0; i < fragment.length; i++) {
      if (entry.filled[fragmentOffset + i]) {
        this.pending.delete(messageType)
        return null
      }
    }

    entry.buf.set(fragment, fragmentOffset)
    for (let i = 0; i < fragment.length; i++) entry.filled[fragmentOffset + i] = 1
    entry.filledCount += fragment.length
    entry.updatedAt = now
    // The header's non-MORE_FRAGMENTS flags come from the terminating fragment.
    if ((header.flags & HeaderFlags.MORE_FRAGMENTS) === 0) {
      entry.sawLast = true
      entry.flags = header.flags
      entry.timestampMs = header.timestampMs
    }

    // Complete once the last fragment is in and every byte is covered (§6 rule 3).
    if (entry.sawLast && entry.filledCount === entry.total) {
      this.pending.delete(messageType)
      return {
        messageType,
        sequence,
        timestampMs: entry.timestampMs,
        flags: entry.flags,
        payload: new DataView(entry.buf.buffer),
      }
    }
    return null
  }
}
