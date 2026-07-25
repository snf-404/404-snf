import type { TranslationKey } from '@/i18n/resources'

export const SNF_ERROR_TRANSLATIONS = {
  bluetoothUnsupported: 'error.bluetoothUnsupported',
  gattUnsupported: 'error.gattUnsupported',
  streamConfiguration: 'error.streamConfiguration',
  invalidProtocolDescriptor: 'error.invalidProtocolDescriptor',
  incompatibleProtocol: 'error.incompatibleProtocol',
  serialUnsupported: 'error.serialUnsupported',
  serialUnavailable: 'error.serialUnavailable',
  serialClosed: 'error.serialClosed',
  radarConfiguration: 'error.radarConfiguration',
  connectionCancelled: 'error.connectionCancelled',
  devicePermissionDenied: 'error.devicePermissionDenied',
  serialPortBusy: 'error.serialPortBusy',
  serialOpenFailed: 'error.serialOpenFailed',
  serialWrongCliPort: 'error.serialWrongCliPort',
  serialWrongDataPort: 'error.serialWrongDataPort',
  serialSamePort: 'error.serialSamePort',
  bluetoothConnection: 'error.bluetoothConnection',
} as const satisfies Record<string, TranslationKey>

export type SnfErrorCode = keyof typeof SNF_ERROR_TRANSLATIONS

export function getSnfErrorTranslation(error: unknown): TranslationKey {
  if (error instanceof Error && error.message in SNF_ERROR_TRANSLATIONS) {
    return SNF_ERROR_TRANSLATIONS[error.message as SnfErrorCode]
  }
  return 'notice.connectionFailed'
}
