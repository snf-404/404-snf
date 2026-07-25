import * as Dialog from '@radix-ui/react-dialog'
import { Bluetooth, Cable, ChevronRight, X } from 'lucide-react'
import { useState } from 'react'

import { Alert } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import type { useSnfTelemetry } from '@/hooks/useSnfTelemetry'
import { useTranslation } from '@/i18n'
import { SNF_ERROR_TRANSLATIONS } from '@/lib/snfErrors'

type DeviceKind = 'radar' | 'pneumatic'

type DeviceConnectionDialogProps = {
  device: DeviceKind | null
  open: boolean
  onOpenChange: (open: boolean) => void
  telemetry: ReturnType<typeof useSnfTelemetry>
}

export function DeviceConnectionDialog({
  device,
  open,
  onOpenChange,
  telemetry,
}: DeviceConnectionDialogProps) {
  const [busy, setBusy] = useState(false)
  const { t } = useTranslation('translation')

  const run = async (action: () => Promise<void>, closeWhenDone = false) => {
    setBusy(true)
    try {
      await action()
      if (closeWhenDone) onOpenChange(false)
    } catch {
      // The hook exposes a precise, localized error code inside this dialog.
    } finally {
      setBusy(false)
    }
  }

  const title =
    device === 'pneumatic'
      ? t('device.connection.pneumaticTitle')
      : telemetry.awaitingDataPort
        ? t('device.connection.dataTitle')
        : t('device.connection.chooseTitle')
  const description =
    device === 'pneumatic'
      ? t('device.connection.pneumaticDescription')
      : telemetry.awaitingDataPort
        ? t('device.connection.dataDescription')
        : t('device.connection.chooseDescription')

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-[#17233d]/25 backdrop-blur-sm" />
        <Dialog.Content className="device-dialog-content fixed bottom-0 left-1/2 z-50 w-full max-w-[430px] -translate-x-1/2 rounded-t-[24px] border border-white/70 bg-[#f8faff] px-5 pt-5 shadow-[0_-16px_60px_rgba(30,50,90,.18)]">
          <div className="pr-10">
            <Dialog.Title className="text-xl font-medium text-[#17233d]">{title}</Dialog.Title>
            <Dialog.Description className="mt-1.5 text-[13px] leading-relaxed text-[#7c8aa2]">
              {description}
            </Dialog.Description>
          </div>
          <Dialog.Close asChild>
            <Button
              data-testid="close-device-dialog"
              variant="unstyled"
              size="icon"
              className="absolute right-4 top-4 rounded-full bg-white/80 text-[#7c8aa2]"
              aria-label={t('common.close')}
            >
              <X size={18} />
            </Button>
          </Dialog.Close>

          {device === 'radar' && telemetry.error !== '' && (
            <Alert variant="destructive" className="mt-4 text-[13px]">
              {t(SNF_ERROR_TRANSLATIONS[telemetry.error])}
            </Alert>
          )}

          <div className="mt-5 space-y-2.5">
            {device === 'pneumatic' && (
              <Button
                data-testid="close-pneumatic-dialog"
                className="w-full"
                onClick={() => {
                  onOpenChange(false)
                }}
              >
                {t('common.close')}
              </Button>
            )}

            {device === 'radar' && telemetry.connected && (
              <>
                <div className="rounded-[16px] border border-[#5278e8]/15 bg-[#5278e8]/5 px-4 py-3 text-sm text-[#43516a]">
                  {t('device.connection.connectedVia', {
                    method:
                      telemetry.connectionMethod === 'bluetooth'
                        ? t('device.connection.bluetooth')
                        : t('device.connection.usb'),
                  })}
                </div>
                <Button
                  data-testid="disconnect-radar"
                  variant="outline"
                  className="w-full"
                  onClick={() => {
                    telemetry.disconnect()
                    onOpenChange(false)
                  }}
                >
                  {t('device.connection.disconnect')}
                </Button>
              </>
            )}

            {device === 'radar' && !telemetry.connected && telemetry.awaitingDataPort && (
              <Button
                data-testid="select-radar-data-port"
                disabled={busy}
                className="h-auto min-h-14 w-full justify-between py-3"
                onClick={() => {
                  void run(telemetry.connectSerialData, true)
                }}
              >
                <span className="flex items-center gap-3 text-left">
                  <Cable size={20} />
                  <span>
                    <span className="block">{t('device.connection.selectData')}</span>
                    <span className="block text-[11px] font-normal opacity-75">COM20 · 921600</span>
                  </span>
                </span>
                <ChevronRight size={17} />
              </Button>
            )}

            {device === 'radar' && !telemetry.connected && !telemetry.awaitingDataPort && (
              <>
                <Button
                  data-testid="connect-radar-bluetooth"
                  disabled={busy}
                  variant="outline"
                  className="h-auto min-h-[66px] w-full justify-between bg-white py-3"
                  onClick={() => {
                    void run(telemetry.connectBluetooth, true)
                  }}
                >
                  <span className="flex items-center gap-3 text-left">
                    <Bluetooth size={21} className="text-[#5278e8]" />
                    <span>
                      <span className="block text-[#43516a]">
                        {t('device.connection.bluetooth')}
                      </span>
                      <span className="block text-[11px] font-normal text-[#9ba8bb]">
                        {t('device.connection.bluetoothDescription')}
                      </span>
                    </span>
                  </span>
                  <ChevronRight size={17} />
                </Button>
                <Button
                  data-testid="configure-radar-serial"
                  disabled={busy}
                  variant="outline"
                  className="h-auto min-h-[66px] w-full justify-between bg-white py-3"
                  onClick={() => {
                    void run(telemetry.configureSerial)
                  }}
                >
                  <span className="flex items-center gap-3 text-left">
                    <Cable size={21} className="text-[#5278e8]" />
                    <span>
                      <span className="block text-[#43516a]">{t('device.connection.usb')}</span>
                      <span className="block text-[11px] font-normal text-[#9ba8bb]">
                        {t('device.connection.usbDescription')}
                      </span>
                    </span>
                  </span>
                  <ChevronRight size={17} />
                </Button>
              </>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
