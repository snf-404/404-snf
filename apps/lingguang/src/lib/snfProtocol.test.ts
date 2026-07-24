import { describe, expect, it } from 'vitest'

import { decodeProtocolInfo, decodeVitals, SnfStatusFlag, TelemetryAssembler } from './snfProtocol'

function fragment(sequence: number, offset: number, payload: Uint8Array, total: number): DataView {
  const bytes = new Uint8Array(16 + payload.length)
  const view = new DataView(bytes.buffer)
  bytes[0] = 1
  bytes[1] = 0x20
  bytes[2] = offset + payload.length < total ? 1 : 0
  bytes[3] = 16
  view.setUint32(4, sequence, true)
  view.setUint32(8, 1234, true)
  view.setUint16(12, total, true)
  view.setUint16(14, offset, true)
  bytes.set(payload, 16)
  return view
}

describe('SNF BLE v1 decoding', () => {
  it('validates protocol info', () => {
    const bytes = new Uint8Array(24)
    bytes.set([0x53, 0x4e, 0x46, 0x31, 1, 0, 16, 1])
    const view = new DataView(bytes.buffer)
    view.setUint32(8, 1, true)
    view.setUint32(16, 0x12345678, true)
    expect(decodeProtocolInfo(view)).toMatchObject({ capabilities: 1, bootId: 0x12345678 })
  })

  it('reassembles fragmented vitals and preserves unavailable values', () => {
    const payload = new Uint8Array(24)
    const view = new DataView(payload.buffer)
    view.setUint16(0, 0xffff, true)
    view.setUint16(2, SnfStatusFlag.subjectTracked | SnfStatusFlag.respirationValid, true)
    view.setUint16(4, 0xffff, true)
    view.setUint16(6, 1550, true)
    payload[9] = 91

    const assembler = new TelemetryAssembler()
    expect(assembler.push(fragment(7, 0, payload.slice(0, 4), payload.length))).toBeNull()
    const message = assembler.push(fragment(7, 4, payload.slice(4), payload.length))
    expect(message).not.toBeNull()
    const vitals = message === null ? null : decodeVitals(message)
    expect(vitals).toMatchObject({
      subjectId: null,
      heartRate: null,
      respirationRate: 15.5,
      respirationConfidence: 91,
    })
  })
})
