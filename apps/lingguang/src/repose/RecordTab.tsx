import { motion } from 'motion/react'

import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { useTranslation } from '@/i18n'
import type { TranslationKey } from '@/i18n/resources'

type RecordItem = {
  id: string
  dateKey: TranslationKey
  time: string
  duration: string
  topStateKey: TranslationKey
  stateCode: 'WRAP' | 'CRADLE' | 'NUDGE'
  endReasonKey: TranslationKey
  timeline: Array<{ stateKey: TranslationKey; duration: string }>
}

const records: RecordItem[] = [
  {
    id: '1',
    dateKey: 'records.today',
    time: '14:32',
    duration: '06:32',
    topStateKey: 'records.deepWrap',
    stateCode: 'WRAP',
    endReasonKey: 'records.bodyLeft',
    timeline: [
      { stateKey: 'records.nudge', duration: '1:18' },
      { stateKey: 'records.support', duration: '2:05' },
      { stateKey: 'records.deepWrap', duration: '3:09' },
    ],
  },
  {
    id: '2',
    dateKey: 'records.today',
    time: '10:15',
    duration: '03:47',
    topStateKey: 'records.cradleSupport',
    stateCode: 'CRADLE',
    endReasonKey: 'records.manualExit',
    timeline: [
      { stateKey: 'records.nudge', duration: '1:02' },
      { stateKey: 'records.support', duration: '2:45' },
    ],
  },
  {
    id: '3',
    dateKey: 'records.yesterday',
    time: '16:08',
    duration: '08:14',
    topStateKey: 'records.deepWrap',
    stateCode: 'WRAP',
    endReasonKey: 'records.bodyLeft',
    timeline: [
      { stateKey: 'records.nudge', duration: '2:11' },
      { stateKey: 'records.support', duration: '3:14' },
      { stateKey: 'records.deepWrap', duration: '2:49' },
    ],
  },
  {
    id: '4',
    dateKey: 'records.yesterday',
    time: '11:30',
    duration: '01:55',
    topStateKey: 'records.nudgeReminder',
    stateCode: 'NUDGE',
    endReasonKey: 'records.manualExit',
    timeline: [{ stateKey: 'records.nudge', duration: '1:55' }],
  },
]

const stateColor: Record<RecordItem['stateCode'], string> = {
  WRAP: '#5278e8',
  CRADLE: '#6b8ef0',
  NUDGE: '#a8bfef',
}
const glassShadow = 'shadow-[0_6px_18px_rgba(45,70,125,.06),inset_0_1px_0_rgba(255,255,255,.75)]'

function RecordCard({ record, delay }: { record: RecordItem; delay: number }) {
  const { t } = useTranslation('translation')
  return (
    <motion.div
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.4, delay, ease: 'easeOut' }}
    >
      <Card className={`mb-2.5 bg-white/75 backdrop-blur-xl ${glassShadow}`}>
        <CardContent className="p-[18px]">
          <div className="mb-3.5 flex items-start justify-between">
            <div>
              <div className="mb-1 flex items-end gap-1.5">
                <span className="text-[28px] font-light leading-none text-[#17233d]">
                  {record.duration}
                </span>
                <span className="pb-0.5 text-xs text-[#9ba8bb]">{t('common.minute')}</span>
              </div>
              <div className="text-xs text-[#9ba8bb]">
                {t(record.dateKey)} · {record.time}
              </div>
            </div>
            <div className="flex flex-col items-end gap-1">
              <Badge
                className="px-[11px] py-1 text-[11px]"
                style={{ background: stateColor[record.stateCode] }}
              >
                {t(record.topStateKey)}
              </Badge>
              <span className="text-[10px] tracking-[.06em] text-[#9ba8bb]">
                {t('records.peakState')}
              </span>
            </div>
          </div>
          <div className="mb-3.5 flex gap-1.5">
            {record.timeline.map((item) => (
              <div
                key={`${item.stateKey}-${item.duration}`}
                className="flex-1 rounded-xl border border-[rgba(70,100,160,.08)] bg-[rgba(240,246,255,.8)] px-2 py-2"
              >
                <div className="mb-0.5 text-[10px] text-[#9ba8bb]">{t(item.stateKey)}</div>
                <div className="text-[13px] text-[#43516a]">{item.duration}</div>
              </div>
            ))}
          </div>
          <div className="flex items-center justify-between">
            <span className="text-xs text-[#9ba8bb]">{t(record.endReasonKey)}</span>
            <Badge variant="secondary" className="gap-1 border-0 bg-transparent px-0 text-[11px]">
              <span className="text-[8px]">●</span>
              {t('records.resetNormally')}
            </Badge>
          </div>
        </CardContent>
      </Card>
    </motion.div>
  )
}

export function RecordTab() {
  const { t } = useTranslation('translation')
  const summaries: Array<{ labelKey: TranslationKey; value: string }> = [
    { labelKey: 'records.todayCount', value: t('common.minutesCount', { count: 2 }) },
    { labelKey: 'records.totalDuration', value: '10:19' },
    { labelKey: 'records.peakState', value: t('records.deepWrap') },
  ]
  return (
    <div className="h-full overflow-y-auto px-5 pb-[72px]">
      <div className="py-5 pb-4">
        <h2 className="m-0 text-[30px] font-medium text-[#17233d]">{t('records.title')}</h2>
      </div>
      <div className="mb-3.5 grid grid-cols-3 gap-2">
        {summaries.map((item) => (
          <Card
            key={item.labelKey}
            className={`bg-white/75 text-center backdrop-blur-xl ${glassShadow}`}
          >
            <CardContent className="px-2 py-3">
              <div className="mb-1 text-[15px] font-medium text-[#17233d]">{item.value}</div>
              <div className="text-[10px] text-[#9ba8bb]">{t(item.labelKey)}</div>
            </CardContent>
          </Card>
        ))}
      </div>
      <Card className="mb-4 rounded-[14px] bg-[rgba(70,100,160,.04)] shadow-none">
        <CardContent className="px-3.5 py-2.5 text-xs leading-relaxed text-[#9ba8bb]">
          {t('records.disclaimer')}
        </CardContent>
      </Card>
      {records.map((record, index) => (
        <RecordCard key={record.id} record={record} delay={index * 0.06} />
      ))}
    </div>
  )
}
