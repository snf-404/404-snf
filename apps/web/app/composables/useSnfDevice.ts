/// <reference types="web-bluetooth" />
// SPDX-License-Identifier: Apache-2.0

/**
 * Web Bluetooth connection to a 404-SNF device, decoding SNF Telemetry
 * Protocol v1 (`PROTOCOL.md`, `~/utils/protocol`).
 *
 * On connect it reads Protocol Info, then subscribes to Device Status, Vitals,
 * and — when the device advertises the `FATIGUE` capability — Fatigue. Every
 * notification and the connect-time status read are fed through a single
 * {@link TelemetryReassembler}, so fragmented messages are reassembled and
 * boot-id changes flush stale state (§6, §14). Vitals freshness is tracked per
 * §14: `live` under 2 s, `stale` under 5 s, `lost` beyond that.
 *
 * Web Bluetooth is client-only and Chromium-only; on an unsupported browser
 * (notably iOS Safari, §14) `state` is `unsupported` and {@link connect} is a
 * no-op. Requires a secure context and a user gesture to open the chooser.
 */

import { computed, onScopeDispose, readonly, ref, shallowRef } from 'vue'

import {
  Capabilities,
  DEVICE_STATUS_UUID,
  FATIGUE_UUID,
  MessageType,
  PROTOCOL_INFO_UUID,
  SERVICE_UUID,
  TelemetryReassembler,
  VITALS_UUID,
  decodeDeviceStatus,
  decodeFatigue,
  decodeProtocolInfo,
  decodeVitals,
  type DeviceStatus,
  type Fatigue,
  type ProtocolInfo,
  type Vitals,
} from '~/utils/protocol'

/** High-level connection state for UI. */
export type SnfConnectionState =
  | 'unsupported'
  | 'idle'
  | 'connecting'
  | 'connected'
  | 'disconnected'
  | 'error'

/** Freshness of the vitals stream (`PROTOCOL.md` §14). */
export type VitalsQuality = 'none' | 'live' | 'stale' | 'lost'

const VITALS_STALE_MS = 2000
const VITALS_LOST_MS = 5000
/** How often the freshness clock ticks; fine enough for the 2 s / 5 s bands. */
const FRESHNESS_TICK_MS = 500

export function useSnfDevice() {
  const supported =
    import.meta.client && typeof navigator !== 'undefined' && 'bluetooth' in navigator

  const state = ref<SnfConnectionState>(supported ? 'idle' : 'unsupported')
  const error = ref<string | null>(null)
  const deviceName = ref<string | null>(null)

  const info = shallowRef<ProtocolInfo | null>(null)
  const status = shallowRef<DeviceStatus | null>(null)
  const vitals = shallowRef<Vitals | null>(null)
  const fatigue = shallowRef<Fatigue | null>(null)

  const vitalsUpdatedAt = ref<number | null>(null)
  // A ticking clock so freshness recomputes without a new notification.
  const nowTick = ref(0)

  const vitalsQuality = computed<VitalsQuality>(() => {
    if (vitalsUpdatedAt.value === null) return 'none'
    void nowTick.value // establish reactive dependency on the clock
    const age = performance.now() - vitalsUpdatedAt.value
    if (age < VITALS_STALE_MS) return 'live'
    if (age < VITALS_LOST_MS) return 'stale'
    return 'lost'
  })

  // Per-connection transient state.
  let device: BluetoothDevice | null = null
  let reassembler: TelemetryReassembler | null = null
  const subscribed: BluetoothRemoteGATTCharacteristic[] = []
  let freshnessTimer: ReturnType<typeof setInterval> | null = null
  let onDisconnected: (() => void) | null = null

  /** Route a reassembled message to the matching decoder and reactive ref. */
  function ingest(value: DataView) {
    if (!reassembler) return
    const message = reassembler.push(value)
    if (!message) return
    switch (message.messageType) {
      case MessageType.DeviceStatus:
        status.value = decodeDeviceStatus(message.payload)
        break
      case MessageType.Vitals:
        vitals.value = decodeVitals(message.payload)
        vitalsUpdatedAt.value = performance.now()
        break
      case MessageType.Fatigue:
        fatigue.value = decodeFatigue(message.payload)
        break
      // Unknown / unsubscribed types are ignored (§16).
    }
  }

  function handleNotification(event: Event) {
    const characteristic = event.target as BluetoothRemoteGATTCharacteristic
    if (characteristic.value) ingest(characteristic.value)
  }

  async function subscribe(service: BluetoothRemoteGATTService, uuid: string): Promise<void> {
    const characteristic = await service.getCharacteristic(uuid)
    characteristic.addEventListener('characteristicvaluechanged', handleNotification)
    await characteristic.startNotifications()
    subscribed.push(characteristic)
  }

  function startFreshnessClock() {
    stopFreshnessClock()
    freshnessTimer = setInterval(() => {
      nowTick.value = (nowTick.value + 1) % Number.MAX_SAFE_INTEGER
    }, FRESHNESS_TICK_MS)
  }

  function stopFreshnessClock() {
    if (freshnessTimer !== null) {
      clearInterval(freshnessTimer)
      freshnessTimer = null
    }
  }

  /** Prompt for a device, connect, read Protocol Info, and subscribe. */
  async function connect(): Promise<void> {
    if (!supported) return
    if (state.value === 'connecting' || state.value === 'connected') return

    error.value = null
    state.value = 'connecting'
    try {
      device = await navigator.bluetooth.requestDevice({
        filters: [{ services: [SERVICE_UUID] }],
        optionalServices: [SERVICE_UUID],
      })
      deviceName.value = device.name ?? null

      onDisconnected = () => {
        state.value = 'disconnected'
        vitalsUpdatedAt.value = null
      }
      device.addEventListener('gattserverdisconnected', onDisconnected)

      const server = await device.gatt!.connect()
      const service = await server.getPrimaryService(SERVICE_UUID)

      reassembler = new TelemetryReassembler()

      // Protocol Info is a bare 24-byte read (no telemetry header, §5). Read it
      // first so capabilities and boot id are known before subscribing.
      const infoValue = await (await service.getCharacteristic(PROTOCOL_INFO_UUID)).readValue()
      const protocolInfo = decodeProtocolInfo(infoValue)
      info.value = protocolInfo
      reassembler.observeBootId(protocolInfo.bootId)

      // Device Status and Vitals are always present; Fatigue only if advertised.
      await subscribe(service, DEVICE_STATUS_UUID)
      await subscribe(service, VITALS_UUID)
      if (protocolInfo.capabilities & Capabilities.FATIGUE) {
        await subscribe(service, FATIGUE_UUID)
      }

      // Connect-time snapshot of Device Status (§11); framed like a notification,
      // so it goes through the reassembler too.
      const statusChar = subscribed.find((c) => c.uuid === DEVICE_STATUS_UUID)
      if (statusChar) {
        try {
          ingest(await statusChar.readValue())
        } catch {
          // Status read is optional; the notification will follow.
        }
      }

      startFreshnessClock()
      state.value = 'connected'
    } catch (cause) {
      // A user cancelling the chooser throws; treat that as returning to idle
      // rather than a hard error.
      if (cause instanceof DOMException && cause.name === 'NotFoundError') {
        state.value = device?.gatt?.connected ? 'connected' : 'idle'
      } else {
        error.value = cause instanceof Error ? cause.message : String(cause)
        state.value = 'error'
      }
      if (state.value !== 'connected') teardown()
    }
  }

  /** Remove listeners, stop notifications, and drop the connection. */
  function teardown() {
    for (const characteristic of subscribed) {
      characteristic.removeEventListener('characteristicvaluechanged', handleNotification)
      // Best-effort; the link may already be gone.
      characteristic.stopNotifications().catch(() => {})
    }
    subscribed.length = 0
    stopFreshnessClock()
    if (device && onDisconnected) {
      device.removeEventListener('gattserverdisconnected', onDisconnected)
    }
    if (device?.gatt?.connected) device.gatt.disconnect()
    onDisconnected = null
    reassembler = null
  }

  /** Disconnect and return to the idle state. */
  function disconnect() {
    teardown()
    if (state.value !== 'unsupported') state.value = 'idle'
    vitalsUpdatedAt.value = null
  }

  onScopeDispose(teardown)

  return {
    supported,
    state: readonly(state),
    error: readonly(error),
    deviceName: readonly(deviceName),
    info: readonly(info),
    status: readonly(status),
    vitals: readonly(vitals),
    fatigue: readonly(fatigue),
    vitalsQuality,
    connect,
    disconnect,
  }
}
