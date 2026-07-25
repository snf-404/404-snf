import { describe, expect, it } from 'vitest'

import { parseRadarSerialFrame } from './radarWebSerial'

const MAGIC_WORD = [0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0x08, 0x07]

describe('Web Serial radar frame parsing', () => {
  it('keeps tracker targets when a frame has no compressed point cloud', () => {
    const packet = new Uint8Array(40 + 8 + 112)
    packet.set(MAGIC_WORD)
    const view = new DataView(packet.buffer)
    view.setUint32(12, packet.length, true)
    view.setUint32(20, 42, true)
    view.setUint32(28, 0, true)
    view.setUint32(32, 1, true)
    view.setUint32(40, 1010, true)
    view.setUint32(44, 112, true)
    view.setUint32(48, 7, true)
    view.setFloat32(52, 0.5, true)
    view.setFloat32(56, 1.25, true)
    view.setFloat32(60, -0.25, true)
    view.setFloat32(48 + 108, 0.9, true)

    const frame = parseRadarSerialFrame(packet)

    expect(frame.points).toEqual([])
    expect(frame.targets).toEqual([expect.objectContaining({ id: 7, x: 0.5, y: 1.25, z: -0.25 })])
  })
})
