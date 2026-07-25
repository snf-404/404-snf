const MAGIC_WORD = new Uint8Array([0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0x08, 0x07])
const FRAME_HEADER_LENGTH = 40
const TLV_HEADER_LENGTH = 8
const MAX_PACKET_LENGTH = 1024 * 1024
const COMPRESSED_POINTS_TLV = 1020
const TRACKED_TARGETS_TLV = 1010
const VITAL_SIGNS_TLV = 1040

export type RadarPoint = {
  x: number
  y: number
  z: number
  velocity: number
  snrDb: number
}

export type RadarVitalSigns = {
  subjectId: number
  rangeBin: number
  breathingDeviation: number
  heartRateBpm: number
  breathingRateBpm: number
  heartWaveform: number[]
  breathWaveform: number[]
}

export type RadarTarget = {
  id: number
  x: number
  y: number
  z: number
  confidence: number
}

export type RadarSerialFrame = {
  receivedAt: number
  frameNumber: number
  declaredPointCount: number
  points: RadarPoint[]
  targets: RadarTarget[]
  vitals: RadarVitalSigns[]
}

export type RadarWebSerialConnection = {
  close: () => Promise<void>
}

type RadarWebSerialHandlers = {
  onFrame: (frame: RadarSerialFrame) => void
  onError: (error: unknown) => void
}

let configuredCliPort: SerialPort | null = null

function readWithTimeout(
  reader: ReadableStreamDefaultReader<Uint8Array>,
): Promise<ReadableStreamReadResult<Uint8Array>> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(new Error('serialWrongCliPort'))
    }, 4000)
    void reader.read().then(
      (result) => {
        window.clearTimeout(timer)
        resolve(result)
      },
      (error: unknown) => {
        window.clearTimeout(timer)
        reject(error instanceof Error ? error : new Error(String(error)))
      },
    )
  })
}

function profileCommands(profile: string): string[] {
  return profile
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('%') && !line.startsWith('#'))
}

class SerialLineReader {
  private pending = ''
  private readonly decoder = new TextDecoder()

  async next(reader: ReadableStreamDefaultReader<Uint8Array>): Promise<string> {
    for (;;) {
      const lineEnd = this.pending.indexOf('\n')
      if (lineEnd >= 0) {
        const line = this.pending.slice(0, lineEnd).trim()
        this.pending = this.pending.slice(lineEnd + 1)
        return line
      }
      const { value, done } = await readWithTimeout(reader)
      if (done) throw new Error('serialClosed')
      this.pending += this.decoder.decode(value, { stream: true })
    }
  }
}

function loadRadarProfile(): Promise<string> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open('GET', `${import.meta.env.BASE_URL}vital_signs_ISK_2m.cfg`)
    request.addEventListener('load', () => {
      if (request.status >= 200 && request.status < 300) resolve(request.responseText)
      else reject(new Error('radarConfiguration'))
    })
    request.addEventListener('error', () => {
      reject(new Error('radarConfiguration'))
    })
    request.send()
  })
}

async function configureRadar(port: SerialPort, profile: string): Promise<void> {
  await port.open({ baudRate: 115200, bufferSize: 4096 })
  const readable = port.readable
  const writable = port.writable
  if (readable === null || writable === null) throw new Error('serialUnavailable')
  const reader = readable.getReader()
  const writer = writable.getWriter()
  const lines = new SerialLineReader()
  const encoder = new TextEncoder()
  try {
    for (const command of profileCommands(profile)) {
      await writer.write(encoder.encode(`${command}\n`))
      for (;;) {
        const response = await lines.next(reader)
        if (response === 'Done') break
        if (response.startsWith('Error')) throw new Error('radarConfiguration')
      }
    }
  } finally {
    writer.releaseLock()
    await reader.cancel().catch(() => undefined)
    reader.releaseLock()
    await port.close()
  }
}

function matchesMagic(bytes: Uint8Array, offset: number): boolean {
  return MAGIC_WORD.every((byte, index) => bytes[offset + index] === byte)
}

function magicOffset(bytes: Uint8Array): number {
  for (let offset = 0; offset <= bytes.length - MAGIC_WORD.length; offset += 1) {
    if (matchesMagic(bytes, offset)) return offset
  }
  return -1
}

function joinBytes(left: Uint8Array, right: Uint8Array): Uint8Array<ArrayBuffer> {
  const joined = new Uint8Array(new ArrayBuffer(left.length + right.length))
  joined.set(left)
  joined.set(right, left.length)
  return joined
}

function parseCompressedPoints(payload: DataView): RadarPoint[] {
  if (payload.byteLength < 20 || (payload.byteLength - 20) % 8 !== 0) return []
  const elevationUnit = payload.getFloat32(0, true)
  const azimuthUnit = payload.getFloat32(4, true)
  const dopplerUnit = payload.getFloat32(8, true)
  const rangeUnit = payload.getFloat32(12, true)
  const snrUnit = payload.getFloat32(16, true)
  const points: RadarPoint[] = []
  for (let offset = 20; offset < payload.byteLength; offset += 8) {
    const elevation = payload.getInt8(offset) * elevationUnit
    const azimuth = payload.getInt8(offset + 1) * azimuthUnit
    const range = payload.getUint16(offset + 4, true) * rangeUnit
    const horizontalRange = range * Math.cos(elevation)
    points.push({
      x: horizontalRange * Math.sin(azimuth),
      y: horizontalRange * Math.cos(azimuth),
      z: range * Math.sin(elevation),
      velocity: payload.getInt16(offset + 2, true) * dopplerUnit,
      snrDb: payload.getUint16(offset + 6, true) * snrUnit,
    })
  }
  return points
}

function parseVitalSigns(payload: DataView): RadarVitalSigns | null {
  if (payload.byteLength !== 136) return null
  const samples = (offset: number) =>
    Array.from({ length: 15 }, (_, index) => payload.getFloat32(offset + index * 4, true))
  return {
    subjectId: payload.getUint16(0, true),
    rangeBin: payload.getUint16(2, true),
    breathingDeviation: payload.getFloat32(4, true),
    heartRateBpm: payload.getFloat32(8, true),
    breathingRateBpm: payload.getFloat32(12, true),
    heartWaveform: samples(16),
    breathWaveform: samples(76),
  }
}

function parseTargets(payload: DataView): RadarTarget[] {
  if (payload.byteLength % 112 !== 0) return []
  return Array.from({ length: payload.byteLength / 112 }, (_, index) => {
    const offset = index * 112
    return {
      id: payload.getUint32(offset, true),
      x: payload.getFloat32(offset + 4, true),
      y: payload.getFloat32(offset + 8, true),
      z: payload.getFloat32(offset + 12, true),
      confidence: payload.getFloat32(offset + 108, true),
    }
  })
}

export function parseRadarSerialFrame(packet: Uint8Array): RadarSerialFrame {
  if (packet.length < FRAME_HEADER_LENGTH || !matchesMagic(packet, 0)) {
    throw new Error('invalid radar frame header')
  }
  const view = new DataView(packet.buffer, packet.byteOffset, packet.byteLength)
  if (view.getUint32(12, true) !== packet.length) throw new Error('radar packet length mismatch')
  const tlvCount = view.getUint32(32, true)
  if (tlvCount > 128) throw new Error('invalid radar TLV count')
  const points: RadarPoint[] = []
  const targets: RadarTarget[] = []
  const vitals: RadarVitalSigns[] = []
  let offset = FRAME_HEADER_LENGTH
  for (let index = 0; index < tlvCount; index += 1) {
    if (offset + TLV_HEADER_LENGTH > packet.length) throw new Error('truncated radar TLV header')
    const type = view.getUint32(offset, true)
    const payloadLength = view.getUint32(offset + 4, true)
    const payloadStart = offset + TLV_HEADER_LENGTH
    const payloadEnd = payloadStart + payloadLength
    if (payloadEnd > packet.length) throw new Error(`truncated radar TLV ${String(type)}`)
    const payload = new DataView(packet.buffer, packet.byteOffset + payloadStart, payloadLength)
    if (type === COMPRESSED_POINTS_TLV) points.push(...parseCompressedPoints(payload))
    if (type === TRACKED_TARGETS_TLV) targets.push(...parseTargets(payload))
    if (type === VITAL_SIGNS_TLV) {
      const reading = parseVitalSigns(payload)
      if (reading !== null) vitals.push(reading)
    }
    offset = payloadEnd
  }
  return {
    receivedAt: Date.now(),
    frameNumber: view.getUint32(20, true),
    declaredPointCount: view.getUint32(28, true),
    points,
    targets,
    vitals,
  }
}

class RadarFrameDecoder {
  private bytes = new Uint8Array(0)

  push(chunk: Uint8Array): RadarSerialFrame[] {
    this.bytes = joinBytes(this.bytes, chunk)
    const frames: RadarSerialFrame[] = []
    while (this.bytes.length >= MAGIC_WORD.length) {
      const start = magicOffset(this.bytes)
      if (start < 0) {
        this.bytes = this.bytes.slice(-MAGIC_WORD.length + 1)
        break
      }
      if (start > 0) this.bytes = this.bytes.slice(start)
      if (this.bytes.length < 16) break
      const header = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength)
      const packetLength = header.getUint32(12, true)
      if (packetLength < FRAME_HEADER_LENGTH || packetLength > MAX_PACKET_LENGTH) {
        this.bytes = this.bytes.slice(1)
        continue
      }
      if (this.bytes.length < packetLength) break
      frames.push(parseRadarSerialFrame(this.bytes.slice(0, packetLength)))
      this.bytes = this.bytes.slice(packetLength)
    }
    return frames
  }
}

export async function configureRadarWebSerial(): Promise<void> {
  const serial = navigator.serial
  if (serial === undefined) throw new Error('serialUnsupported')
  const cliPort = await serial.requestPort()
  const profile = await loadRadarProfile()
  await configureRadar(cliPort, profile)
  configuredCliPort = cliPort
}

export async function openRadarDataWebSerial(
  handlers: RadarWebSerialHandlers,
): Promise<RadarWebSerialConnection> {
  const serial = navigator.serial
  if (serial === undefined) throw new Error('serialUnsupported')
  const dataPort = await serial.requestPort()
  if (dataPort === configuredCliPort) throw new Error('serialSamePort')
  await dataPort.open({ baudRate: 921600, bufferSize: 65536 })
  const readable = dataPort.readable
  if (readable === null) {
    await dataPort.close()
    throw new Error('serialUnavailable')
  }
  const reader = readable.getReader()
  const decoder = new RadarFrameDecoder()
  const state: { closing: boolean; resolveFirstFrame: (() => void) | null } = {
    closing: false,
    resolveFirstFrame: null,
  }
  const firstFrame = new Promise<void>((resolve) => {
    state.resolveFirstFrame = resolve
  })
  const closed = (async () => {
    try {
      for (;;) {
        const { value, done } = await reader.read()
        if (done) break
        for (const frame of decoder.push(value)) {
          state.resolveFirstFrame?.()
          state.resolveFirstFrame = null
          handlers.onFrame(frame)
        }
      }
    } catch (error) {
      if (!state.closing) handlers.onError(error)
    } finally {
      reader.releaseLock()
    }
  })()

  const connection: RadarWebSerialConnection = {
    close: async () => {
      state.closing = true
      await reader.cancel().catch(() => undefined)
      await closed
      await dataPort.close()
    },
  }
  try {
    await Promise.race([
      firstFrame,
      new Promise<never>((_resolve, reject) => {
        window.setTimeout(() => {
          reject(new Error('serialWrongDataPort'))
        }, 5000)
      }),
    ])
  } catch (error) {
    await connection.close()
    throw error
  }
  return connection
}
