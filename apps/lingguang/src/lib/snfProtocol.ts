export const SNF_UUIDS = {
  service: '7b9f0001-6b44-4d2a-9f36-4040534e4600',
  protocolInfo: '7b9f0001-6b44-4d2a-9f36-4040534e4601',
  streamControl: '7b9f0001-6b44-4d2a-9f36-4040534e4602',
  deviceStatus: '7b9f0001-6b44-4d2a-9f36-4040534e4603',
  vitals: '7b9f0001-6b44-4d2a-9f36-4040534e4604',
  pointCloud: '7b9f0001-6b44-4d2a-9f36-4040534e4607',
} as const

export const SnfCapability = {
  vitals: 1 << 0,
  pointCloud: 1 << 3,
} as const

export const SnfStream = {
  status: 1 << 0,
  vitals: 1 << 1,
  pointCloud: 1 << 4,
} as const

export const SnfStatusFlag = {
  subjectTracked: 1 << 0,
  heartValid: 1 << 1,
  respirationValid: 1 << 2,
  warmingUp: 1 << 3,
  motionContaminated: 1 << 4,
  radarGap: 1 << 6,
} as const

const HEADER_LENGTH = 16
const PROTOCOL_MAJOR = 1
const UNAVAILABLE_U16 = 0xffff
const MAX_LOGICAL_PAYLOAD = 8192

export type ProtocolInfo = {
  capabilities: number
  bootId: number
  buildId: number
  maxPointCount: number
  maxPoseJoints: number
  maxSubjects: number
}

export type DeviceStatus = {
  uptimeSeconds: number
  activeStreams: number
  lastError: number
  radarGapCount: number
  processorTemperature: number | null
}

export type Vitals = {
  subjectId: number | null
  statusFlags: number
  heartRate: number | null
  respirationRate: number | null
  heartConfidence: number
  respirationConfidence: number
  activityConfidence: number
  stale: boolean
  degraded: boolean
}

export type PointCloud = {
  subjectId: number | null
  points: Float32Array
}

type DecodedMessage = {
  messageType: number
  flags: number
  sequence: number
  timestampMs: number
  payload: DataView
}

type PendingMessage = {
  sequence: number
  flags: number
  timestampMs: number
  createdAt: number
  bytes: Uint8Array
  covered: Uint8Array
  coveredCount: number
}

function viewBytes(view: DataView, offset = 0): Uint8Array {
  return new Uint8Array(view.buffer, view.byteOffset + offset, view.byteLength - offset)
}

export function decodeProtocolInfo(value: DataView): ProtocolInfo {
  if (value.byteLength !== 24) throw new Error('invalidProtocolDescriptor')
  const magic = String.fromCharCode(...viewBytes(value).slice(0, 4))
  if (magic !== 'SNF1' || value.getUint8(4) !== PROTOCOL_MAJOR || value.getUint8(6) !== 16) {
    throw new Error('incompatibleProtocol')
  }
  return {
    capabilities: value.getUint32(8, true),
    maxPointCount: value.getUint16(12, true),
    maxPoseJoints: value.getUint8(14),
    maxSubjects: value.getUint8(15),
    bootId: value.getUint32(16, true),
    buildId: value.getUint32(20, true),
  }
}

export class TelemetryAssembler {
  private readonly pending = new Map<number, PendingMessage>()

  push(value: DataView): DecodedMessage | null {
    if (value.byteLength < HEADER_LENGTH) return null
    const major = value.getUint8(0)
    const messageType = value.getUint8(1)
    const flags = value.getUint8(2)
    const headerLength = value.getUint8(3)
    const sequence = value.getUint32(4, true)
    const timestampMs = value.getUint32(8, true)
    const totalLength = value.getUint16(12, true)
    const fragmentOffset = value.getUint16(14, true)
    const fragment = viewBytes(value, HEADER_LENGTH)
    if (
      major !== PROTOCOL_MAJOR ||
      headerLength !== HEADER_LENGTH ||
      totalLength > MAX_LOGICAL_PAYLOAD ||
      fragmentOffset + fragment.length > totalLength
    ) {
      this.pending.delete(messageType)
      return null
    }

    const now = Date.now()
    const previous = this.pending.get(messageType)
    if (previous !== undefined && now - previous.createdAt > 500) this.pending.delete(messageType)
    let state = this.pending.get(messageType)
    if (state === undefined || state.sequence !== sequence) {
      state = {
        sequence,
        flags,
        timestampMs,
        createdAt: now,
        bytes: new Uint8Array(totalLength),
        covered: new Uint8Array(totalLength),
        coveredCount: 0,
      }
      this.pending.set(messageType, state)
    }
    if (state.bytes.length !== totalLength) {
      this.pending.delete(messageType)
      return null
    }
    for (let index = 0; index < fragment.length; index += 1) {
      const target = fragmentOffset + index
      const next = fragment[index]
      if (next === undefined) return null
      if (state.covered[target] === 1 && state.bytes[target] !== next) {
        this.pending.delete(messageType)
        return null
      }
      if (state.covered[target] === 0) {
        state.covered[target] = 1
        state.coveredCount += 1
      }
      state.bytes[target] = next
    }
    state.flags |= flags
    const moreFragments = (flags & 1) !== 0
    if (moreFragments || state.coveredCount !== totalLength) return null
    this.pending.delete(messageType)
    return {
      messageType,
      flags: state.flags,
      sequence,
      timestampMs,
      payload: new DataView(state.bytes.buffer),
    }
  }

  clear(): void {
    this.pending.clear()
  }
}

export function decodeDeviceStatus(message: DecodedMessage): DeviceStatus | null {
  if (message.messageType !== 0x10 || message.payload.byteLength < 20) return null
  const value = message.payload
  const rawTemperature = value.getInt16(16, true)
  return {
    uptimeSeconds: value.getUint32(0, true),
    activeStreams: value.getUint16(4, true),
    lastError: value.getUint16(6, true),
    radarGapCount: value.getUint16(12, true),
    processorTemperature: rawTemperature === 0x7fff ? null : rawTemperature / 100,
  }
}

export function decodeVitals(message: DecodedMessage): Vitals | null {
  if (message.messageType !== 0x20 || message.payload.byteLength < 24) return null
  const value = message.payload
  const subjectId = value.getUint16(0, true)
  const heartRate = value.getUint16(4, true)
  const respirationRate = value.getUint16(6, true)
  return {
    subjectId: subjectId === UNAVAILABLE_U16 ? null : subjectId,
    statusFlags: value.getUint16(2, true),
    heartRate: heartRate === UNAVAILABLE_U16 ? null : heartRate / 100,
    respirationRate: respirationRate === UNAVAILABLE_U16 ? null : respirationRate / 100,
    heartConfidence: value.getUint8(8),
    respirationConfidence: value.getUint8(9),
    activityConfidence: value.getUint8(10),
    degraded: (message.flags & (1 << 2)) !== 0,
    stale: (message.flags & (1 << 3)) !== 0,
  }
}

export function decodePointCloud(message: DecodedMessage): PointCloud | null {
  if (message.messageType !== 0x31 || message.payload.byteLength < 8) return null
  const value = message.payload
  const subjectId = value.getUint16(0, true)
  const count = value.getUint16(2, true)
  if (value.getUint8(4) !== 1 || value.byteLength !== 8 + count * 8) return null
  const points = new Float32Array(count * 3)
  for (let index = 0; index < count; index += 1) {
    const offset = 8 + index * 8
    points[index * 3] = value.getInt16(offset, true) / 1000
    points[index * 3 + 1] = value.getInt16(offset + 2, true) / 1000
    points[index * 3 + 2] = value.getInt16(offset + 4, true) / 1000
  }
  return { subjectId: subjectId === UNAVAILABLE_U16 ? null : subjectId, points }
}

export function makeSetStreamsRequest(requestId: number, streamMask: number): ArrayBuffer {
  const buffer = new ArrayBuffer(16)
  const value = new Uint8Array(buffer)
  const view = new DataView(value.buffer)
  value[0] = PROTOCOL_MAJOR
  value[1] = 0x01
  view.setUint16(2, requestId, true)
  view.setUint16(4, 8, true)
  view.setUint16(8, streamMask, true)
  value[10] = 2
  value[11] = 0
  value[12] = 5
  value[13] = 96
  return buffer
}
