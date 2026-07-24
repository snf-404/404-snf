import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import {
  decodeDeviceStatus,
  decodePointCloud,
  decodeProtocolInfo,
  decodeVitals,
  makeSetStreamsRequest,
  SnfCapability,
  SnfStatusFlag,
  SnfStream,
  SNF_UUIDS,
  TelemetryAssembler,
  type DeviceStatus,
  type ProtocolInfo,
  type Vitals,
} from '@/lib/snfProtocol'

type ConnectionState = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'unsupported'

const EMPTY_POINTS = new Float32Array(0)
const HISTORY_LENGTH = 58

function eventValue(event: Event): DataView | null {
  const characteristic = event.target as BluetoothRemoteGATTCharacteristic | null
  return characteristic?.value ?? null
}

function normalizeHistory(values: number[]): number[] {
  if (values.length < 2) return []
  const min = Math.min(...values)
  const max = Math.max(...values)
  const span = Math.max(max - min, 0.01)
  return values.map((value) => ((value - min) / span) * 1.6 - 0.8)
}

export function useSnfTelemetry(paused: boolean) {
  const [connectionState, setConnectionState] = useState<ConnectionState>('idle')
  const [protocolInfo, setProtocolInfo] = useState<ProtocolInfo | null>(null)
  const [status, setStatus] = useState<DeviceStatus | null>(null)
  const [vitals, setVitals] = useState<Vitals | null>(null)
  const [points, setPoints] = useState<Float32Array>(EMPTY_POINTS)
  const [lastVitalsAt, setLastVitalsAt] = useState(0)
  const [heartHistory, setHeartHistory] = useState<number[]>([])
  const [respirationHistory, setRespirationHistory] = useState<number[]>([])
  const [breathingPhase, setBreathingPhase] = useState(0)
  const [clock, setClock] = useState(Date.now())
  const [error, setError] = useState('')

  const deviceRef = useRef<BluetoothDevice | null>(null)
  const controlRef = useRef<BluetoothRemoteGATTCharacteristic | null>(null)
  const assemblerRef = useRef(new TelemetryAssembler())
  const manualDisconnectRef = useRef(false)
  const reconnectAttemptRef = useRef(0)
  const reconnectTimerRef = useRef<number | null>(null)
  const requestIdRef = useRef(0)
  const capabilitiesRef = useRef(0)
  const pausedRef = useRef(paused)
  const scheduleReconnectRef = useRef<(device: BluetoothDevice) => void>(() => undefined)
  pausedRef.current = paused

  const consume = useCallback((value: DataView) => {
    const message = assemblerRef.current.push(value)
    if (message === null) return
    if (message.messageType === 0x10) {
      const decoded = decodeDeviceStatus(message)
      if (decoded !== null) setStatus(decoded)
      return
    }
    if (message.messageType === 0x20) {
      const decoded = decodeVitals(message)
      if (decoded === null) return
      setVitals(decoded)
      setLastVitalsAt(Date.now())
      if (decoded.heartRate !== null && (decoded.statusFlags & SnfStatusFlag.heartValid) !== 0) {
        setHeartHistory((values) => [...values, decoded.heartRate as number].slice(-HISTORY_LENGTH))
      }
      if (
        decoded.respirationRate !== null &&
        (decoded.statusFlags & SnfStatusFlag.respirationValid) !== 0
      ) {
        setRespirationHistory((values) =>
          [...values, decoded.respirationRate as number].slice(-HISTORY_LENGTH),
        )
      }
      return
    }
    if (message.messageType === 0x31) {
      const decoded = decodePointCloud(message)
      if (decoded !== null) setPoints(decoded.points)
    }
  }, [])

  const onNotification = useCallback(
    (event: Event) => {
      const value = eventValue(event)
      if (value !== null && !pausedRef.current) consume(value)
    },
    [consume],
  )

  const applyStreams = useCallback(async (isPaused: boolean) => {
    const control = controlRef.current
    if (control === null) return
    let mask = SnfStream.status
    if (!isPaused) {
      mask |= SnfStream.vitals
      if ((capabilitiesRef.current & SnfCapability.pointCloud) !== 0) mask |= SnfStream.pointCloud
    }
    requestIdRef.current = (requestIdRef.current + 1) & 0xffff
    await control.writeValueWithResponse(makeSetStreamsRequest(requestIdRef.current, mask))
  }, [])

  const openConnection = useCallback(
    async (device: BluetoothDevice) => {
      const server = device.gatt
      if (server === undefined) throw new Error('设备不支持 GATT')
      await server.connect()
      device.addEventListener(
        'gattserverdisconnected',
        () => {
          scheduleReconnectRef.current(device)
        },
        { once: true },
      )
      const service = await server.getPrimaryService(SNF_UUIDS.service)
      const protocolCharacteristic = await service.getCharacteristic(SNF_UUIDS.protocolInfo)
      const info = decodeProtocolInfo(await protocolCharacteristic.readValue())
      capabilitiesRef.current = info.capabilities
      setProtocolInfo(info)
      assemblerRef.current.clear()

      const control = await service.getCharacteristic(SNF_UUIDS.streamControl)
      const statusCharacteristic = await service.getCharacteristic(SNF_UUIDS.deviceStatus)
      const vitalsCharacteristic = await service.getCharacteristic(SNF_UUIDS.vitals)
      controlRef.current = control
      await control.startNotifications()
      await statusCharacteristic.startNotifications()
      await vitalsCharacteristic.startNotifications()
      statusCharacteristic.addEventListener('characteristicvaluechanged', onNotification)
      vitalsCharacteristic.addEventListener('characteristicvaluechanged', onNotification)
      consume(await statusCharacteristic.readValue())
      consume(await vitalsCharacteristic.readValue())

      if ((info.capabilities & SnfCapability.pointCloud) !== 0) {
        const pointCharacteristic = await service.getCharacteristic(SNF_UUIDS.pointCloud)
        await pointCharacteristic.startNotifications()
        pointCharacteristic.addEventListener('characteristicvaluechanged', onNotification)
      } else {
        setPoints(EMPTY_POINTS)
      }
      reconnectAttemptRef.current = 0
      setConnectionState('connected')
      setError('')
    },
    [consume, onNotification],
  )

  const scheduleReconnect = useCallback(
    (device: BluetoothDevice) => {
      if (manualDisconnectRef.current || reconnectAttemptRef.current >= 3) {
        setConnectionState('idle')
        return
      }
      setConnectionState('reconnecting')
      const delay = 1000 * 2 ** reconnectAttemptRef.current
      reconnectAttemptRef.current += 1
      reconnectTimerRef.current = window.setTimeout(() => {
        void openConnection(device).catch(() => {
          scheduleReconnect(device)
        })
      }, delay)
    },
    [openConnection],
  )
  scheduleReconnectRef.current = scheduleReconnect

  const connect = useCallback(async () => {
    if (navigator.bluetooth === undefined) {
      setConnectionState('unsupported')
      throw new Error('当前环境不支持蓝牙连接')
    }
    manualDisconnectRef.current = false
    setConnectionState('connecting')
    const device = await navigator.bluetooth.requestDevice({
      filters: [{ services: [SNF_UUIDS.service] }],
    })
    deviceRef.current = device
    await openConnection(device)
  }, [openConnection])

  const disconnect = useCallback(() => {
    manualDisconnectRef.current = true
    if (reconnectTimerRef.current !== null) window.clearTimeout(reconnectTimerRef.current)
    deviceRef.current?.gatt?.disconnect()
    controlRef.current = null
    assemblerRef.current.clear()
    setConnectionState('idle')
    setProtocolInfo(null)
    setStatus(null)
    setVitals(null)
    setPoints(EMPTY_POINTS)
    setLastVitalsAt(0)
  }, [])

  useEffect(() => {
    if (connectionState !== 'connected') return
    void applyStreams(paused).catch(() => {
      setError('数据流设置失败')
    })
  }, [applyStreams, connectionState, paused])

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now())
    }, 500)
    return () => {
      window.clearInterval(timer)
    }
  }, [])

  useEffect(() => {
    if (paused || vitals?.respirationRate === null || vitals?.respirationRate === undefined) return
    const respirationRate = vitals.respirationRate
    const timer = window.setInterval(() => {
      setBreathingPhase((phase) => phase + (Math.PI * 2 * respirationRate * 0.1) / 60)
    }, 100)
    return () => {
      window.clearInterval(timer)
    }
  }, [paused, vitals?.respirationRate])

  useEffect(
    () => () => {
      manualDisconnectRef.current = true
      if (reconnectTimerRef.current !== null) window.clearTimeout(reconnectTimerRef.current)
      deviceRef.current?.gatt?.disconnect()
    },
    [],
  )

  const age = lastVitalsAt === 0 ? Number.POSITIVE_INFINITY : clock - lastVitalsAt
  const hidden = age > 5000
  const stale = age > 2000 || vitals?.stale === true
  const warming = ((vitals?.statusFlags ?? 0) & SnfStatusFlag.warmingUp) !== 0
  const motion = ((vitals?.statusFlags ?? 0) & SnfStatusFlag.motionContaminated) !== 0
  const heartValid = ((vitals?.statusFlags ?? 0) & SnfStatusFlag.heartValid) !== 0
  const respirationValid =
    ((vitals?.statusFlags ?? 0) & SnfStatusFlag.respirationValid) !== 0 && !stale
  const hasSpatialData = points.length > 0

  const qualityLabel = warming
    ? '校准中'
    : motion || vitals?.degraded === true
      ? '受干扰'
      : (vitals?.respirationConfidence ?? 0) >= 80
        ? '优秀'
        : (vitals?.respirationConfidence ?? 0) >= 50
          ? '良好'
          : '偏低'

  return useMemo(
    () => ({
      connectionState,
      connected: connectionState === 'connected',
      connect,
      disconnect,
      error,
      heartRate: !hidden && heartValid ? (vitals?.heartRate ?? null) : null,
      respirationRate: !hidden && respirationValid ? (vitals?.respirationRate ?? null) : null,
      heartConfidence: vitals?.heartConfidence ?? 0,
      respirationConfidence: vitals?.respirationConfidence ?? 0,
      qualityLabel,
      motionLabel: motion ? '高' : '低',
      processorTemperature: status?.processorTemperature ?? null,
      points,
      hasSpatialData,
      breathingPhase,
      heartWave: normalizeHistory(heartHistory),
      breathWave: normalizeHistory(respirationHistory),
      subjectLabel: 'AUTO',
      protocolInfo,
      stale,
    }),
    [
      breathingPhase,
      connect,
      connectionState,
      disconnect,
      error,
      heartHistory,
      heartValid,
      hidden,
      motion,
      points,
      protocolInfo,
      qualityLabel,
      respirationHistory,
      respirationValid,
      stale,
      status?.processorTemperature,
      vitals,
    ],
  )
}
