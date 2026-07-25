import { AnimatePresence, motion } from 'motion/react'
import { useState } from 'react'

import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTranslation } from '@/i18n'
import type { TranslationKey } from '@/i18n/resources'

import { RecordTab } from './RecordTab'

type TrendView = 'today' | 'longTerm' | 'records'
type SegmentType = 'work' | 'fatigue' | 'cradle' | 'wrap' | 'reset'

const segments: Array<{ type: SegmentType; minutes: number }> = [
  { type: 'work', minutes: 55 },
  { type: 'fatigue', minutes: 7 },
  { type: 'cradle', minutes: 12 },
  { type: 'reset', minutes: 2 },
  { type: 'work', minutes: 52 },
  { type: 'fatigue', minutes: 5 },
  { type: 'cradle', minutes: 3 },
  { type: 'reset', minutes: 1 },
  { type: 'work', minutes: 54 },
  { type: 'fatigue', minutes: 8 },
  { type: 'wrap', minutes: 7 },
  { type: 'reset', minutes: 2 },
  { type: 'work', minutes: 52 },
]

const segmentClasses: Record<SegmentType, string> = {
  work: 'bg-[#5278e8]/15',
  fatigue: 'bg-[#c08820]/45',
  cradle: 'bg-[#5278e8]/50',
  wrap: 'bg-[#5278e8]/75',
  reset: 'bg-[#4664a0]/20',
}

const segmentLabels: Record<SegmentType, TranslationKey> = {
  work: 'trends.timeline.work',
  fatigue: 'trends.timeline.fatigue',
  cradle: 'trends.timeline.cradle',
  wrap: 'trends.timeline.wrap',
  reset: 'trends.timeline.reset',
}

const glassCard =
  'border-[#4664a0]/10 bg-white/75 shadow-[0_6px_18px_rgba(45,70,125,0.06)] backdrop-blur-xl'

function TodayView() {
  const { t } = useTranslation('translation')
  const summary: Array<{ label: TranslationKey; value: string; note: TranslationKey }> = [
    { label: 'trends.today.focus', value: '3h 33m', note: 'trends.today.focusNote' },
    {
      label: 'trends.today.fatigue',
      value: t('common.minutesCount', { count: 3 }),
      note: 'trends.today.fatigueNote',
    },
    {
      label: 'trends.today.rest',
      value: t('common.minutesCount', { count: 2 }),
      note: 'trends.today.restNote',
    },
  ]
  const workSegments = [
    ['09:00 – 09:55', '55 min'],
    ['10:34 – 11:30', '56 min'],
    ['11:38 – 14:12', '2h 34m'],
    [`14:32 – ${t('trends.now')}`, t('common.inProgress')],
  ]

  return (
    <>
      <Card className={`mb-3 ${glassCard}`}>
        <CardHeader className="px-[18px] pb-3 pt-4">
          <CardTitle className="text-[11px] font-medium tracking-[.1em] text-[#9ba8bb]">
            {t('trends.today.timeline')}
          </CardTitle>
        </CardHeader>
        <CardContent className="px-[18px] pb-4">
          <div
            className="flex h-3.5 gap-0.5 overflow-hidden rounded-full"
            aria-label={t('trends.today.timeline')}
          >
            {segments.map((segment, index) => (
              <div
                key={`${segment.type}-${String(index)}`}
                className={segmentClasses[segment.type]}
                style={{ flex: segment.minutes }}
                title={t(segmentLabels[segment.type])}
              />
            ))}
          </div>
          <div className="mt-1.5 flex justify-between text-[10px] text-[#9ba8bb]">
            <span>09:00</span>
            <span className="text-[#5278e8]">{t('trends.now')} 14:32</span>
          </div>
          <div className="mt-3 flex flex-wrap gap-x-3 gap-y-2 border-t border-[#4664a0]/5 pt-3">
            {(Object.keys(segmentLabels) as SegmentType[]).map((type) => (
              <div key={type} className="flex items-center gap-1.5 text-[10px] text-[#7c8aa2]">
                <span className={`h-2 w-2 flex-none rounded-full ${segmentClasses[type]}`} />
                {t(segmentLabels[type])}
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      <div className="mb-3 grid grid-cols-3 gap-2">
        {summary.map((item) => (
          <Card key={item.label} className={glassCard}>
            <CardContent className="px-2 py-3 text-center">
              <div className="text-base font-medium text-[#17233d]">{item.value}</div>
              <div className="mt-1 text-[10px] text-[#7c8aa2]">{t(item.label)}</div>
              <div className="mt-0.5 text-[9px] text-[#9ba8bb]">{t(item.note)}</div>
            </CardContent>
          </Card>
        ))}
      </div>

      <Card className={`mb-3 ${glassCard}`}>
        <CardHeader className="px-[18px] pb-2 pt-4">
          <CardTitle className="text-[11px] font-medium tracking-[.1em] text-[#9ba8bb]">
            {t('trends.today.workSegments')}
          </CardTitle>
        </CardHeader>
        <CardContent className="px-[18px] pb-3">
          {workSegments.map(([time, duration], index) => (
            <div
              key={time}
              className="flex items-center justify-between border-b border-[#4664a0]/5 py-2.5 last:border-0"
            >
              <span className="text-[13px] text-[#43516a]">{time}</span>
              <div className="flex items-center gap-2">
                <span className="text-[11px] text-[#9ba8bb]">{duration}</span>
                {index === 2 && (
                  <Badge
                    variant="outline"
                    className="border-[#c08820]/20 bg-[#c08820]/5 text-[10px] text-[#9a6e10]"
                  >
                    {t('trends.today.longSession')}
                  </Badge>
                )}
              </div>
            </div>
          ))}
        </CardContent>
      </Card>
    </>
  )
}

function LongTermView() {
  const { t } = useTranslation('translation')
  const metrics: Array<{
    label: TranslationKey
    relation: TranslationKey
    value: string
    warning: boolean
  }> = [
    {
      label: 'trends.baseline.activity',
      relation: 'trends.baseline.below',
      value: '-18%',
      warning: true,
    },
    {
      label: 'trends.baseline.leaning',
      relation: 'trends.baseline.above',
      value: '+23%',
      warning: true,
    },
    {
      label: 'trends.baseline.workDuration',
      relation: 'trends.baseline.above',
      value: '+12 min',
      warning: true,
    },
    {
      label: 'status.metric.heartRate',
      relation: 'trends.baseline.near',
      value: '+2 BPM',
      warning: false,
    },
    {
      label: 'status.metric.respiration',
      relation: 'trends.baseline.near',
      value: t('common.stable'),
      warning: false,
    },
  ]
  const weekly = [2, 3, 1, 4, 2, 3, 8]

  return (
    <>
      <Card className={`mb-3 ${glassCard}`}>
        <CardHeader className="px-[18px] pb-2 pt-4">
          <CardTitle className="text-[11px] font-medium tracking-[.1em] text-[#9ba8bb]">
            {t('trends.baseline.title')}
          </CardTitle>
        </CardHeader>
        <CardContent className="px-[18px] pb-4">
          {metrics.map((metric) => (
            <div
              key={metric.label}
              className="flex items-center justify-between border-b border-[#4664a0]/5 py-2.5 last:border-0"
            >
              <div className="flex items-center gap-2">
                <span
                  className={`h-1.5 w-1.5 rounded-full ${metric.warning ? 'bg-[#c08820]' : 'bg-[#3a9a6a]'}`}
                />
                <div>
                  <div className="text-[13px] text-[#43516a]">{t(metric.label)}</div>
                  <div className="text-[10px] text-[#9ba8bb]">{t(metric.relation)}</div>
                </div>
              </div>
              <span
                className={
                  metric.warning ? 'text-[13px] text-[#9a6e10]' : 'text-[13px] text-[#7c8aa2]'
                }
              >
                {metric.value}
              </span>
            </div>
          ))}
          <div className="mt-2 rounded-xl border border-[#4664a0]/10 bg-[#4664a0]/[.03] px-3 py-2 text-[11px] leading-relaxed text-[#9ba8bb]">
            {t('trends.baseline.note')}
          </div>
        </CardContent>
      </Card>

      <Card className={`mb-3 ${glassCard}`}>
        <CardHeader className="px-[18px] pb-3 pt-4">
          <CardTitle className="text-[11px] font-medium tracking-[.1em] text-[#9ba8bb]">
            {t('trends.weekly.title')}
          </CardTitle>
        </CardHeader>
        <CardContent className="px-[18px] pb-4">
          <div className="flex h-20 items-end gap-2" aria-label={t('trends.weekly.title')}>
            {weekly.map((count, index) => (
              <div key={String(index)} className="flex flex-1 flex-col items-center gap-1.5">
                <motion.div
                  className={`w-full rounded-t ${index === weekly.length - 1 ? 'bg-[#5278e8]' : 'bg-[#5278e8]/30'}`}
                  initial={{ height: 0 }}
                  animate={{ height: Math.max(8, (count / 8) * 56) }}
                  transition={{ duration: 0.45, delay: index * 0.04 }}
                />
                <span
                  className={`text-[10px] ${index === weekly.length - 1 ? 'text-[#5278e8]' : 'text-[#9ba8bb]'}`}
                >
                  {t(
                    index === weekly.length - 1
                      ? 'trends.weekly.today'
                      : (`trends.weekly.day${String(index + 1)}` as TranslationKey),
                  )}
                </span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      <div className="rounded-2xl border border-[#4664a0]/10 bg-[#4664a0]/[.03] px-4 py-3 text-[11px] leading-relaxed text-[#9ba8bb]">
        {t('trends.disclaimer')}
      </div>
    </>
  )
}

export function TrendsTab() {
  const [view, setView] = useState<TrendView>('today')
  const { t } = useTranslation('translation')

  return (
    <div className="h-full overflow-y-auto px-5 pb-[72px]">
      <div className="pb-4 pt-5">
        <h2 className="m-0 text-[30px] font-medium tracking-[-.02em] text-[#17233d]">
          {t('trends.title')}
        </h2>
      </div>
      <Tabs
        value={view}
        onValueChange={(value) => {
          setView(value as TrendView)
        }}
      >
        <TabsList className="mb-4 grid w-full grid-cols-3 rounded-xl border border-[#4664a0]/10 bg-[#4664a0]/5 p-0.5">
          {(['today', 'longTerm', 'records'] as TrendView[]).map((item) => (
            <TabsTrigger
              key={item}
              value={item}
              data-testid={`trends-${item}`}
              className="rounded-[10px] py-2 text-[13px] text-[#7c8aa2] data-[state=active]:bg-[#5278e8] data-[state=active]:text-white data-[state=active]:shadow-none"
            >
              {t(`trends.tab.${item}` as TranslationKey)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      <AnimatePresence mode="wait">
        <motion.div
          key={view}
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -8 }}
          transition={{ duration: 0.2 }}
        >
          {view === 'today' && <TodayView />}
          {view === 'longTerm' && <LongTermView />}
          {view === 'records' && <RecordTab embedded />}
        </motion.div>
      </AnimatePresence>
    </div>
  )
}
