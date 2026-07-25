import { ChevronRight } from 'lucide-react'
import { AnimatePresence, motion } from 'motion/react'
import { useState, type ReactNode } from 'react'

import { Alert } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { Separator } from '@/components/ui/separator'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { useSnfTelemetry } from '@/hooks/useSnfTelemetry'
import { useTranslation } from '@/i18n'
import type { Locale } from '@/i18n'
import type { TranslationKey } from '@/i18n/resources'
import { SNF_ERROR_TRANSLATIONS } from '@/lib/snfErrors'
import { DeviceConnectionDialog } from '@/repose/DeviceConnectionDialog'

const cardShadow = 'shadow-[0_6px_18px_rgba(45,70,125,.06),inset_0_1px_0_rgba(255,255,255,.75)]'

function StatusDot({ ok }: { ok: boolean }) {
  return (
    <motion.span
      className={`size-2 rounded-full ${ok ? 'bg-[#5278e8] shadow-[0_0_6px_rgba(82,120,232,.4)]' : 'bg-[#d94f4f] shadow-[0_0_6px_rgba(200,60,60,.4)]'}`}
      animate={{ opacity: ok ? [0.7, 1, 0.7] : [0.6, 0.9, 0.6] }}
      transition={{ duration: 2.5, repeat: Infinity }}
    />
  )
}

function SettingsCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card className={`mb-3 bg-white/75 px-[18px] py-1.5 backdrop-blur-xl ${cardShadow}`}>
      <CardHeader className="py-3 pb-2">
        <CardTitle className="text-[11px] font-normal tracking-[.12em] text-[#9ba8bb]">
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  )
}

function DeviceRow({
  label,
  sub,
  status,
  ok,
  onClick,
  testId,
  last = false,
}: {
  label: string
  sub: string
  status: string
  ok: boolean
  onClick: () => void
  testId: string
  last?: boolean
}) {
  return (
    <>
      <button
        type="button"
        data-testid={testId}
        onClick={onClick}
        className="flex w-full items-center justify-between py-[13px] text-left"
      >
        <div>
          <div className="text-sm text-[#43516a]">{label}</div>
          <div className="mt-0.5 text-[11px] text-[#9ba8bb]">{sub}</div>
        </div>
        <div className="flex items-center gap-2">
          <span className={`text-xs ${ok ? 'text-[#5278e8]' : 'text-[#c43e3e]'}`}>{status}</span>
          <StatusDot ok={ok} />
          <ChevronRight size={16} className="text-[#9ba8bb]" />
        </div>
      </button>
      {!last && <Separator />}
    </>
  )
}

function SettingRow({
  label,
  sub,
  value,
  toggle,
  onToggle,
  chevron,
  last = false,
}: {
  label: string
  sub?: string
  value?: string
  toggle?: boolean
  onToggle?: (value: boolean) => void
  chevron?: boolean
  last?: boolean
}) {
  return (
    <>
      <div className="flex min-h-12 items-center justify-between py-2">
        <div>
          <div className="text-sm text-[#43516a]">{label}</div>
          {sub !== undefined && sub !== '' && (
            <div className="mt-0.5 max-w-[235px] text-[11px] leading-snug text-[#9ba8bb]">
              {sub}
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {value !== undefined && value !== '' && (
            <span className="text-[13px] text-[#9ba8bb]">{value}</span>
          )}
          {toggle !== undefined && onToggle !== undefined && (
            <Switch
              data-testid="auto-adjust-toggle"
              checked={toggle}
              onCheckedChange={onToggle}
              aria-label={label}
            />
          )}
          {chevron === true && <ChevronRight size={16} className="text-[#9ba8bb]" />}
        </div>
      </div>
      {!last && <Separator />}
    </>
  )
}

interface DeviceTabProps {
  telemetry: ReturnType<typeof useSnfTelemetry>
  paused: boolean
  onTogglePaused: () => void
}

export function DeviceTab({ telemetry, paused, onTogglePaused }: DeviceTabProps) {
  const [autoAdjust, setAutoAdjust] = useState(true)
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false)
  const [selectedDevice, setSelectedDevice] = useState<'radar' | 'pneumatic' | null>(null)
  const { t, i18n } = useTranslation('translation')
  const locale: Locale = i18n.resolvedLanguage === 'en-US' ? 'en-US' : 'zh-CN'

  return (
    <div className="h-full overflow-y-auto px-5 pb-[72px]">
      <div className="py-5 pb-4">
        <h2 className="m-0 text-[30px] font-medium text-[#17233d]">{t('device.title')}</h2>
      </div>

      <SettingsCard title={t('language.title')}>
        <Tabs
          value={locale}
          onValueChange={(value) => {
            if (value === 'zh-CN' || value === 'en-US') i18n.changeLanguage(value)
          }}
        >
          <TabsList
            className="grid grid-cols-2 gap-1.5 py-2 pb-2.5"
            aria-label={t('language.title')}
          >
            {(['zh-CN', 'en-US'] as Locale[]).map((option) => (
              <TabsTrigger
                key={option}
                value={option}
                data-testid={`locale-${option}`}
                className="min-h-[38px] rounded-[11px] border border-[rgba(70,100,160,.1)] bg-[rgba(240,246,255,.68)] text-[13px] text-[#7c8aa2] data-[state=active]:border-[rgba(82,120,232,.34)] data-[state=active]:bg-[#5278e8]/10 data-[state=active]:text-[#5278e8]"
              >
                {option === 'zh-CN' ? '中文' : 'English'}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      </SettingsCard>

      <SettingsCard title={t('device.connection.title')}>
        <DeviceRow
          label={t('device.connection.radar')}
          sub={t('device.connection.serialHint')}
          status={t(telemetry.connected ? 'common.connected' : 'common.disconnected')}
          ok={telemetry.connected}
          testId="open-radar-connection"
          onClick={() => {
            setSelectedDevice('radar')
          }}
        />
        <DeviceRow
          label={t('device.connection.pneumatic')}
          sub={t('device.connection.airCoreReserved')}
          status={t('common.pending')}
          ok={false}
          testId="open-pneumatic-connection"
          onClick={() => {
            setSelectedDevice('pneumatic')
          }}
          last
        />
      </SettingsCard>

      <SettingsCard title={t('device.stream.title')}>
        <SettingRow
          label={t('device.stream.data')}
          sub={t(
            telemetry.connected
              ? 'device.stream.vitalsAndSpatial'
              : 'device.stream.availableAfterConnection',
          )}
          value={t(paused ? 'common.paused' : 'common.live')}
          last
        />
        <Button
          data-testid="toggle-stream"
          disabled={!telemetry.connected}
          onClick={onTogglePaused}
          variant="outline"
          className="my-1.5 w-full bg-[rgba(240,246,255,.82)]"
        >
          {t(paused ? 'device.stream.resume' : 'device.stream.pause')}
        </Button>
      </SettingsCard>
      {telemetry.error !== '' && (
        <Alert variant="destructive" className="mb-3 text-[13px]">
          {t(SNF_ERROR_TRANSLATIONS[telemetry.error])}
        </Alert>
      )}

      <SettingsCard title={t('device.auto.title')}>
        <SettingRow
          label={t('device.auto.title')}
          sub={t(autoAdjust ? 'device.auto.enabledDescription' : 'device.auto.pausedDescription')}
          toggle={autoAdjust}
          onToggle={setAutoAdjust}
        />
        <SettingRow
          label={t('device.auto.sensitivity')}
          value={t('device.auto.standard')}
          chevron
        />
        <SettingRow
          label={t('device.auto.maxRestDuration')}
          value={t('device.auto.fifteenMinutes')}
          chevron
          last
        />
      </SettingsCard>

      <AnimatePresence>
        {!autoAdjust && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
          >
            <Alert variant="warning" className="mb-3 text-[13px] leading-relaxed">
              {t('device.auto.pausedNotice')}
            </Alert>
          </motion.div>
        )}
      </AnimatePresence>

      <SettingsCard title={t('device.calibration.title')}>
        <SettingRow label={t('device.calibration.recalibrate')} chevron />
        <SettingRow
          label={t('device.calibration.safetyTest')}
          sub={t('device.calibration.safetyTestDescription')}
          chevron
        />
        <SettingRow label={t('device.calibration.status')} value={t('common.normal')} last />
      </SettingsCard>
      <SettingsCard title={t('device.privacy.title')}>
        <SettingRow
          label={t('device.privacy.storage')}
          sub={t('device.privacy.localOnly')}
          chevron
        />
        <SettingRow label={t('device.privacy.clearHistory')} chevron last />
      </SettingsCard>

      <Card className="mb-3 bg-[rgba(240,246,255,.6)] px-[18px] py-1.5 backdrop-blur-xl">
        <CardHeader className="py-3 pb-2">
          <CardTitle className="text-[11px] font-normal tracking-[.12em] text-[rgba(70,100,160,.35)]">
            {t('device.developer.title')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Collapsible open={diagnosticsOpen} onOpenChange={setDiagnosticsOpen}>
            <CollapsibleTrigger asChild>
              <Button
                type="button"
                data-testid="toggle-engineering-diagnostics"
                variant="unstyled"
                size="unstyled"
                className="flex w-full justify-between border-b border-[rgba(70,100,160,.06)] py-[13px] text-left text-sm text-[#7c8aa2]"
              >
                {t('device.developer.diagnostics')}
                <ChevronRight
                  size={16}
                  className={`transition-transform ${diagnosticsOpen ? 'rotate-90' : ''}`}
                />
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="overflow-hidden data-[state=open]:animate-accordion-down data-[state=closed]:animate-accordion-up">
              <div className="py-3 pb-1.5">
                <div className="mb-2.5 rounded-[10px] border border-amber-600/20 bg-amber-600/5 px-3 py-2 text-[11px] leading-relaxed text-[#9a6e10]">
                  {t('device.developer.notice')}
                </div>
                {(
                  [
                    'device.developer.testNudge',
                    'device.developer.testCradle',
                    'device.developer.testWrap',
                    'device.developer.deflate',
                    'device.developer.reset',
                  ] as TranslationKey[]
                ).map((command, index) => (
                  <Button
                    key={command}
                    type="button"
                    data-testid={`engineering-command-${String(index)}`}
                    variant="outline"
                    className="mb-1.5 block w-full bg-white/90 text-left"
                  >
                    {t(command)}
                  </Button>
                ))}
              </div>
            </CollapsibleContent>
          </Collapsible>
        </CardContent>
      </Card>
      <div className="py-2 pb-1 text-center text-[11px] tracking-[.04em] text-[#9ba8bb]">
        {t('device.footer')}
      </div>
      <DeviceConnectionDialog
        device={selectedDevice}
        open={selectedDevice !== null}
        onOpenChange={(open) => {
          if (!open) setSelectedDevice(null)
        }}
        telemetry={telemetry}
      />
    </div>
  )
}
