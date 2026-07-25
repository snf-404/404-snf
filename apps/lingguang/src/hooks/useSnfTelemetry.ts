import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import {
  configureRadarWebSerial,
  openRadarDataWebSerial,
  type RadarSerialFrame,
  type RadarWebSerialConnection,
} from '@/lib/radarWebSerial'
import { SNF_ERROR_TRANSLATIONS, type SnfErrorCode } from '@/lib/snfErrors'
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

type ConnectionState =
  | 'idle'
  | 'connecting'
  | 'selecting-data'
  | 'connected'
  | 'reconnecting'
  | 'unsupported'
type QualityLabel = 'calibrating' | 'interference' | 'excellent' | 'good' | 'low'
type ConnectionMethod = 'bluetooth' | 'serial'

const EMPTY_POINTS = new Float32Array(0)
const HISTORY_LENGTH = 58

function normalizeHistory(values: number[]): number[] {
  if (values.length < 2) return []
  const min = Math.min(...values)
  const max = Math.max(...values)
  const span = Math.max(max - min, 0.01)
  return values.map((value) => ((value - min) / span) * 1.6 - 0.8)
}

function eventValue(event: Event): DataView | null {
  const characteristic = event.target as BluetoothRemoteGATTCharacteristic | null
  return characteristic?.value ?? null
}

function connectionErrorCode(error: unknown, fallback: SnfErrorCode): SnfErrorCode {
  if (error instanceof Error && error.message in SNF_ERROR_TRANSLATIONS) {
    return error.message as SnfErrorCode
  }
  if (error instanceof DOMException) {
    if (error.name === 'NotFoundError') return 'connectionCancelled'
    if (error.name === 'SecurityError') return 'devicePermissionDenied'
    if (error.name === 'InvalidStateError' || error.name === 'NetworkError') {
      return 'serialPortBusy'
    }
  }
  return fallback
}

export function useSnfTelemetry(paused: boolean) {
  const [connectionState, setConnectionState] = useState<ConnectionState>('idle')
  const [vitals, setVitals] = useState<Vitals | null>(null)
  const [points, setPoints] = useState<Float32Array>(EMPTY_POINTS)
  const [targetPoints, setTargetPoints] = useState<Float32Array>(EMPTY_POINTS)
  const [lastVitalsAt, setLastVitalsAt] = useState(0)
  const [heartHistory, setHeartHistory] = useState<number[]>([])
  const [respirationHistory, setRespirationHistory] = useState<number[]>([])
  const [breathingPhase, setBreathingPhase] = useState(0)
  const [clock, setClock] = useState(Date.now())
  const [error, setError] = useState<SnfErrorCode | ''>('')
  const [protocolInfo, setProtocolInfo] = useState<ProtocolInfo | null>(null)
  const [status, setStatus] = useState<DeviceStatus | null>(null)
  const [processorTemperature, setProcessorTemperature] = useState<number | null>(null)
  const [connectionMethod, setConnectionMethod] = useState<ConnectionMethod | null>(null)
  const serialConnectionRef = useRef<RadarWebSerialConnection | null>(null)
  const radarConfiguredRef = useRef(false)
  const bluetoothDeviceRef = useRef<BluetoothDevice | null>(null)
  const bluetoothControlRef = useRef<BluetoothRemoteGATTCharacteristic | null>(null)
  const assemblerRef = useRef(new TelemetryAssembler())
  const requestIdRef = useRef(0)
  const capabilitiesRef = useRef(0)
  const pausedRef = useRef(paused)
  pausedRef.current = paused

  const consumeFrame = useCallback((frame: RadarSerialFrame) => {
    if (pausedRef.current) return
    const reading = frame.vitals[0]
    if (reading !== undefined) {
      const hasSubject = reading.breathingDeviation > 0
      const heartValid = hasSubject && reading.heartRateBpm > 0
      const respirationValid =
        hasSubject && reading.breathingDeviation >= 0.02 && reading.breathingRateBpm > 0
      setVitals({
        subjectId: hasSubject ? reading.subjectId : null,
        statusFlags:
          (hasSubject ? SnfStatusFlag.subjectTracked : 0) |
          (heartValid ? SnfStatusFlag.heartValid : 0) |
          (respirationValid ? SnfStatusFlag.respirationValid : 0),
        heartRate: heartValid ? reading.heartRateBpm : null,
        respirationRate: respirationValid ? reading.breathingRateBpm : null,
        heartConfidence: heartValid ? 100 : 0,
        respirationConfidence: respirationValid ? 100 : 0,
        activityConfidence: hasSubject || frame.points.length > 0 ? 100 : 0,
        stale: false,
        degraded: false,
      })
      setLastVitalsAt(frame.receivedAt)
      setHeartHistory((values) => [...values, ...reading.heartWaveform].slice(-HISTORY_LENGTH))
      setRespirationHistory((values) =>
        [...values, ...reading.breathWaveform].slice(-HISTORY_LENGTH),
      )
    }
    setPoints(Float32Array.from(frame.points.flatMap((point) => [point.x, point.y, point.z])))
    setTargetPoints(
      Float32Array.from(frame.targets.flatMap((target) => [target.x, target.y, target.z])),
    )
  }, [])

  const consumeBluetooth = useCallback((value: DataView) => {
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

  const onBluetoothNotification = useCallback(
    (event: Event) => {
      const value = eventValue(event)
      if (value !== null && !pausedRef.current) consumeBluetooth(value)
    },
    [consumeBluetooth],
  )

  const applyBluetoothStreams = useCallback(async (isPaused: boolean) => {
    const control = bluetoothControlRef.current
    if (control === null) return
    let mask = SnfStream.status
    if (!isPaused) {
      mask |= SnfStream.vitals
      if ((capabilitiesRef.current & SnfCapability.pointCloud) !== 0) mask |= SnfStream.pointCloud
    }
    requestIdRef.current = (requestIdRef.current + 1) & 0xffff
    await control.writeValueWithResponse(makeSetStreamsRequest(requestIdRef.current, mask))
  }, [])

  const connectBluetooth = useCallback(async () => {
    if (navigator.bluetooth === undefined) {
      setConnectionState('unsupported')
      setError('bluetoothUnsupported')
      throw new Error('bluetoothUnsupported')
    }
    setConnectionState('connecting')
    setError('')
    try {
      const device = await navigator.bluetooth.requestDevice({
        filters: [{ services: [SNF_UUIDS.service] }],
      })
      const server = device.gatt
      if (server === undefined) throw new Error('gattUnsupported')
      await server.connect()
      bluetoothDeviceRef.current = device
      device.addEventListener(
        'gattserverdisconnected',
        () => {
          if (bluetoothDeviceRef.current !== null) setConnectionState('reconnecting')
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
      bluetoothControlRef.current = control
      await control.startNotifications()
      await statusCharacteristic.startNotifications()
      await vitalsCharacteristic.startNotifications()
      statusCharacteristic.addEventListener('characteristicvaluechanged', onBluetoothNotification)
      vitalsCharacteristic.addEventListener('characteristicvaluechanged', onBluetoothNotification)
      consumeBluetooth(await statusCharacteristic.readValue())
      consumeBluetooth(await vitalsCharacteristic.readValue())
      if ((info.capabilities & SnfCapability.pointCloud) !== 0) {
        const pointCharacteristic = await service.getCharacteristic(SNF_UUIDS.pointCloud)
        await pointCharacteristic.startNotifications()
        pointCharacteristic.addEventListener('characteristicvaluechanged', onBluetoothNotification)
      }
      setConnectionMethod('bluetooth')
      setConnectionState('connected')
    } catch (connectError) {
      setConnectionState('idle')
      setError(connectionErrorCode(connectError, 'bluetoothConnection'))
      throw connectError
    }
  }, [consumeBluetooth, onBluetoothNotification])

  const configureSerial = useCallback(async () => {
    if (navigator.serial === undefined) {
      setConnectionState('unsupported')
      setError('serialUnsupported')
      throw new Error('serialUnsupported')
    }
    setConnectionState('connecting')
    setError('')
    try {
      await configureRadarWebSerial()
      radarConfiguredRef.current = true
      setConnectionState('selecting-data')
    } catch (connectError) {
      setConnectionState('idle')
      setError(connectionErrorCode(connectError, 'radarConfiguration'))
      throw connectError
    }
  }, [])

  const connectSerialData = useCallback(async () => {
    if (!radarConfiguredRef.current) throw new Error('radarConfiguration')
    setConnectionState('connecting')
    setError('')
    try {
      serialConnectionRef.current = await openRadarDataWebSerial({
        onFrame: consumeFrame,
        onError: () => {
          setConnectionState('reconnecting')
          setError('streamConfiguration')
        },
      })
      setProtocolInfo({
        capabilities: SnfCapability.vitals | SnfCapability.pointCloud,
        maxPointCount: 1024,
        maxPoseJoints: 0,
        maxSubjects: 2,
        bootId: 0,
        buildId: 0,
      })
      setProcessorTemperature(null)
      setConnectionMethod('serial')
      setConnectionState('connected')
    } catch (connectError) {
      setConnectionState('selecting-data')
      setError(connectionErrorCode(connectError, 'serialOpenFailed'))
      throw connectError
    }
  }, [consumeFrame])

  const disconnect = useCallback(() => {
    const serialConnection = serialConnectionRef.current
    serialConnectionRef.current = null
    void serialConnection?.close()
    bluetoothDeviceRef.current?.gatt?.disconnect()
    bluetoothDeviceRef.current = null
    bluetoothControlRef.current = null
    assemblerRef.current.clear()
    setConnectionState('idle')
    setVitals(null)
    setPoints(EMPTY_POINTS)
    setTargetPoints(EMPTY_POINTS)
    setLastVitalsAt(0)
    setProtocolInfo(null)
    setStatus(null)
    setProcessorTemperature(null)
    setConnectionMethod(null)
    setError('')
    setHeartHistory([])
    setRespirationHistory([])
  }, [])

  useEffect(() => {
    if (connectionState !== 'connected' || connectionMethod !== 'bluetooth') return
    void applyBluetoothStreams(paused).catch(() => {
      setError('streamConfiguration')
    })
  }, [applyBluetoothStreams, connectionMethod, connectionState, paused])

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
      const serialConnection = serialConnectionRef.current
      serialConnectionRef.current = null
      void serialConnection?.close()
      bluetoothDeviceRef.current?.gatt?.disconnect()
      bluetoothDeviceRef.current = null
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
  const hasTrackedTargets = targetPoints.length > 0
  const hasPresence =
    hasSpatialData ||
    hasTrackedTargets ||
    (!hidden && ((vitals?.statusFlags ?? 0) & SnfStatusFlag.subjectTracked) !== 0)
  const qualityLabel: QualityLabel = warming
    ? 'calibrating'
    : motion || vitals?.degraded === true
      ? 'interference'
      : (vitals?.respirationConfidence ?? 0) >= 80
        ? 'excellent'
        : (vitals?.respirationConfidence ?? 0) >= 50
          ? 'good'
          : 'low'

  return useMemo(
    () => ({
      connectionState,
      connected: connectionState === 'connected',
      awaitingDataPort: connectionState === 'selecting-data',
      connectionMethod,
      connectBluetooth,
      configureSerial,
      connectSerialData,
      disconnect,
      error,
      heartRate: !hidden && heartValid ? (vitals?.heartRate ?? null) : null,
      respirationRate: !hidden && respirationValid ? (vitals?.respirationRate ?? null) : null,
      heartConfidence: vitals?.heartConfidence ?? 0,
      respirationConfidence: vitals?.respirationConfidence ?? 0,
      qualityLabel,
      motionLabel: motion ? 'high' : 'low',
      processorTemperature:
        connectionMethod === 'bluetooth'
          ? (status?.processorTemperature ?? null)
          : processorTemperature,
      points,
      targetPoints,
      hasSpatialData,
      hasTrackedTargets,
      hasPresence,
      breathingPhase,
      heartWave: normalizeHistory(heartHistory),
      breathWave: normalizeHistory(respirationHistory),
      subjectLabel: 'AUTO',
      protocolInfo,
      stale,
    }),
    [
      breathingPhase,
      connectBluetooth,
      connectSerialData,
      configureSerial,
      connectionMethod,
      connectionState,
      disconnect,
      error,
      heartHistory,
      heartValid,
      hidden,
      motion,
      points,
      targetPoints,
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
