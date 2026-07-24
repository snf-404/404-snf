import { motion } from 'motion/react'

import { PointCloudView } from '@/components/PointCloudView'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Progress } from '@/components/ui/progress'
import type { useSnfTelemetry } from '@/hooks/useSnfTelemetry'
import { useTranslation } from '@/i18n'
import type { TranslationKey } from '@/i18n/resources'

interface SensingDetailPageProps {
  telemetry: ReturnType<typeof useSnfTelemetry>
  onClose: () => void
}

// ─── Design tokens — Ice Blue Light ───────────────────────────────────────────
const T = {
  cardBg: 'rgba(255,255,255,0.76)',
  cardBgEl: 'rgba(255,255,255,0.90)',
  cardSub: 'rgba(240,246,255,0.72)',
  border: 'rgba(70,100,160,0.10)',
  borderSub: 'rgba(70,100,160,0.06)',
  textPri: '#17233D',
  textSec: '#43516A',
  textMut: '#7C8AA2',
  textFaint: '#9BA8BB',
  brand: '#5278E8',
  brandMid: '#6B8EF0',
  brandMist: '#A8BFEF',
  warning: '#C08820',
  shadow: '0 12px 36px rgba(45,70,125,0.08), inset 0 1px 0 rgba(255,255,255,0.80)',
  shadowSm: '0 6px 18px rgba(45,70,125,0.06), inset 0 1px 0 rgba(255,255,255,0.75)',
}

function SLabel({ label }: { label: string }) {
  return (
    <div style={{ marginBottom: 8, paddingLeft: 2 }}>
      <span style={{ fontSize: 13, fontWeight: 500, color: T.textSec }}>{label}</span>
    </div>
  )
}

// ─── 1. Presence card (pale ice-blue bg — person visible with dark strokes) ────
function PresenceCard() {
  const { t } = useTranslation('translation')
  return (
    <Card
      style={{
        borderRadius: 20,
        background:
          'linear-gradient(145deg, rgba(230,240,255,0.88) 0%, rgba(215,232,255,0.82) 100%)',
        border: `1px solid rgba(70,100,160,0.10)`,
        overflow: 'hidden',
        position: 'relative',
        marginBottom: 14,
        boxShadow: T.shadow,
      }}
    >
      <div
        style={{
          position: 'absolute',
          inset: 0,
          background:
            'radial-gradient(ellipse at 30% 0%, rgba(158,187,255,0.20) 0%, transparent 55%)',
          pointerEvents: 'none',
        }}
      />
      <div style={{ display: 'flex', alignItems: 'center' }}>
        <svg viewBox="0 0 128 152" fill="none" style={{ width: 128, flexShrink: 0 }}>
          <defs>
            <linearGradient id="sdBody" x1="0.5" y1="0" x2="0.5" y2="1">
              <stop offset="0%" stopColor="rgba(130,165,220,0.22)" />
              <stop offset="100%" stopColor="rgba(150,185,235,0.07)" />
            </linearGradient>
            <radialGradient id="sdAura" cx="50%" cy="42%" r="52%">
              <stop offset="0%" stopColor="rgba(82,120,232,0.08)" />
              <stop offset="100%" stopColor="rgba(82,120,232,0)" />
            </radialGradient>
          </defs>
          <ellipse cx="64" cy="76" rx="58" ry="68" fill="url(#sdAura)" />
          <motion.ellipse
            cx="64"
            cy="76"
            rx="55"
            ry="64"
            stroke="rgba(82,120,210,0.10)"
            strokeWidth="0.8"
            fill="none"
            strokeDasharray="3 5"
            animate={{ opacity: [0.4, 0.88, 0.4] }}
            transition={{ duration: 5.5, repeat: Infinity }}
          />

          <circle
            cx="64"
            cy="28"
            r="14"
            fill="rgba(150,185,230,0.18)"
            stroke="rgba(82,120,210,0.30)"
            strokeWidth="1.5"
          />
          <rect x="60" y="42" width="8" height="13" rx="4" fill="rgba(120,160,220,0.12)" />
          <path d="M 36 60 Q 64 55 92 60 L 95 110 Q 64 116 33 110 Z" fill="url(#sdBody)" />
          <path
            d="M 22 60 Q 64 52 106 60"
            stroke="rgba(82,120,210,0.28)"
            strokeWidth="1.5"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M 22 60 Q 10 88 8 110"
            stroke="rgba(82,120,210,0.10)"
            strokeWidth="7"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M 22 60 Q 10 88 8 110"
            stroke="rgba(82,120,210,0.26)"
            strokeWidth="1.4"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M 106 60 Q 118 88 120 110"
            stroke="rgba(82,120,210,0.10)"
            strokeWidth="7"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M 106 60 Q 118 88 120 110"
            stroke="rgba(82,120,210,0.26)"
            strokeWidth="1.4"
            fill="none"
            strokeLinecap="round"
          />

          <path d="M 2 128 L 126 128 L 122 114 L 6 114 Z" fill="rgba(190,220,255,0.35)" />
          <path d="M 2 128 L 126 128 L 126 132 L 2 132 Z" fill="rgba(220,237,255,0.55)" />

          <motion.ellipse
            cx="12"
            cy="123"
            rx="18"
            ry="5.5"
            fill="rgba(82,120,232,0.22)"
            animate={{ opacity: [0.25, 0.55, 0.25] }}
            transition={{ duration: 3.5, repeat: Infinity }}
          />
          <motion.ellipse
            cx="116"
            cy="121"
            rx="18"
            ry="5.5"
            fill="rgba(82,120,232,0.18)"
            animate={{ opacity: [0.22, 0.5, 0.22] }}
            transition={{ duration: 3.5, repeat: Infinity, delay: 0.8 }}
          />

          {(
            [
              [38, 43, 0],
              [88, 38, 1.4],
              [57, 71, 2.1],
              [74, 86, 0.6],
            ] as [number, number, number][]
          ).map(([cx, cy, d]) => (
            <motion.circle
              key={`${String(cx)}-${String(cy)}`}
              cx={cx}
              cy={cy}
              r={1.2}
              fill="#A8BFEF"
              opacity={0}
              animate={{ opacity: [0, 0.5, 0], r: [1.2, 1.8, 1.2] }}
              transition={{
                duration: 2.8 + d * 0.2,
                repeat: Infinity,
                delay: d,
                ease: 'easeInOut',
              }}
            />
          ))}
        </svg>

        <div
          style={{
            flex: 1,
            padding: '18px 16px 18px 6px',
            display: 'flex',
            flexDirection: 'column',
            gap: 10,
          }}
        >
          {(
            [
              { label: 'status.metric.presence', value: 'common.detected', color: T.brand },
              { label: 'sensing.presence.quality', value: 'common.good', color: T.brand },
              { label: 'sensing.presence.distance', value: 'common.moderate', color: T.brandMid },
              {
                label: 'sensing.presence.currentStatus',
                value: 'sensing.presence.detecting',
                color: T.brandMist,
              },
            ] as Array<{ label: TranslationKey; value: TranslationKey; color: string }>
          ).map((item) => (
            <div
              key={item.label}
              style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}
            >
              <span style={{ fontSize: 11, color: T.textFaint }}>{t(item.label)}</span>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <div style={{ width: 6, height: 6, borderRadius: '50%', background: item.color }} />
                <span style={{ fontSize: 12, color: T.textSec }}>{t(item.value)}</span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </Card>
  )
}

// ─── 2. Body trends ────────────────────────────────────────────────────────────
function TrendBar({
  label,
  value,
  desc,
  warn,
}: {
  label: TranslationKey
  value: number
  desc: TranslationKey
  warn?: boolean
}) {
  const { t } = useTranslation('translation')
  return (
    <div style={{ marginBottom: 12 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
        <span style={{ fontSize: 13, color: warn === true ? T.warning : T.textSec }}>
          {t(label)}
        </span>
        <span style={{ fontSize: 11, color: T.textFaint }}>{t(desc)}</span>
      </div>
      <Progress
        value={value}
        className={warn === true ? '[&>div]:bg-[#c08820]' : '[&>div]:bg-[#5278e8]'}
      />
    </div>
  )
}

function BodyTrends() {
  return (
    <Card
      style={{
        borderRadius: 20,
        padding: '16px 18px 8px',
        background: T.cardBg,
        border: `1px solid ${T.border}`,
        backdropFilter: 'blur(20px)',
        WebkitBackdropFilter: 'blur(20px)',
        marginBottom: 14,
        boxShadow: T.shadowSm,
      }}
    >
      <TrendBar
        label="sensing.trends.workDuration"
        value={74}
        desc="sensing.trends.workDurationValue"
        warn
      />
      <TrendBar label="sensing.trends.movementReduction" value={62} desc="common.moderate" warn />
      <TrendBar label="sensing.trends.postureStability" value={82} desc="common.good" />
      <TrendBar
        label="sensing.trends.signalStability"
        value={90}
        desc="sensing.trends.excellentShort"
      />
    </Card>
  )
}

// ─── 3. Contact area ───────────────────────────────────────────────────────────
function ContactArea() {
  const { t } = useTranslation('translation')
  const zones: Array<{ zone: TranslationKey; status: TranslationKey; pct: number }> = [
    { zone: 'sensing.contact.leftForearm', status: 'sensing.contact.continuous', pct: 88 },
    { zone: 'sensing.contact.wrist', status: 'sensing.contact.lightPressure', pct: 72 },
    { zone: 'sensing.contact.rightForearm', status: 'sensing.contact.continuous', pct: 85 },
  ]
  return (
    <Card
      style={{
        borderRadius: 20,
        padding: '16px',
        background: T.cardBg,
        border: `1px solid ${T.border}`,
        backdropFilter: 'blur(20px)',
        WebkitBackdropFilter: 'blur(20px)',
        marginBottom: 14,
        boxShadow: T.shadowSm,
      }}
    >
      <div style={{ display: 'flex', gap: 8 }}>
        {zones.map((z, i) => (
          <div
            key={z.zone}
            style={{
              flex: 1,
              borderRadius: 16,
              padding: '12px 8px',
              background: T.cardBgEl,
              border: `1px solid rgba(70,100,160,0.10)`,
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              gap: 7,
            }}
          >
            <motion.div
              style={{
                width: 34,
                height: 34,
                borderRadius: '50%',
                background: `rgba(82,120,232,${String(z.pct / 250 + 0.05)})`,
                border: `1px solid rgba(82,120,232,0.20)`,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
              animate={{
                boxShadow: [
                  `0 0 6px rgba(82,120,232,0.08)`,
                  `0 0 16px rgba(82,120,232,0.18)`,
                  `0 0 6px rgba(82,120,232,0.08)`,
                ],
              }}
              transition={{ duration: 3.5 + i * 0.6, repeat: Infinity }}
            >
              <div style={{ width: 8, height: 8, borderRadius: '50%', background: T.brand }} />
            </motion.div>
            <span style={{ fontSize: 18, fontWeight: 300, color: T.textPri }}>
              {z.pct}
              <span style={{ fontSize: 10, color: T.textFaint }}>%</span>
            </span>
            <div style={{ textAlign: 'center' }}>
              <div style={{ fontSize: 11, color: T.textSec, marginBottom: 2 }}>{t(z.zone)}</div>
              <div style={{ fontSize: 10, color: T.textMut }}>{t(z.status)}</div>
            </div>
          </div>
        ))}
      </div>
      <div
        style={{
          marginTop: 12,
          paddingTop: 12,
          borderTop: `1px solid rgba(70,100,160,0.06)`,
          display: 'flex',
          justifyContent: 'center',
          gap: 14,
        }}
      >
        {(
          [
            { color: T.brand, label: 'sensing.contact.continuous' },
            { color: T.brandMid, label: 'sensing.contact.lightPressure' },
            { color: T.brandMist, label: 'sensing.contact.none' },
          ] as Array<{ color: string; label: TranslationKey }>
        ).map((l) => (
          <div key={l.label} style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
            <div style={{ width: 8, height: 8, borderRadius: 2, background: l.color }} />
            <span style={{ fontSize: 10, color: T.textFaint }}>{t(l.label)}</span>
          </div>
        ))}
      </div>
    </Card>
  )
}

// ─── 4. Physiological estimates ────────────────────────────────────────────────
function PhysioEstimates() {
  const { t } = useTranslation('translation')
  return (
    <div style={{ display: 'flex', gap: 8, marginBottom: 14 }}>
      {(
        [
          {
            icon: '〜',
            label: 'sensing.physio.breathingRhythm',
            value: 'sensing.physio.steady',
            sub: 'sensing.physio.estimatedRpm',
            iconColor: T.brandMid,
            period: 3.5,
            note: 'sensing.physio.breathingNote',
          },
          {
            icon: '◦',
            label: 'sensing.physio.movementTrend',
            value: 'sensing.physio.decreasing',
            sub: 'sensing.physio.lastFifteenMinutes',
            iconColor: T.warning,
            period: 2.2,
            note: 'sensing.physio.movementNote',
          },
        ] as Array<{
          icon: string
          label: TranslationKey
          value: TranslationKey
          sub: TranslationKey
          iconColor: string
          period: number
          note: TranslationKey
        }>
      ).map((item) => (
        <Card
          key={item.label}
          style={{
            flex: 1,
            borderRadius: 20,
            padding: '16px 14px 12px',
            background: T.cardBg,
            border: `1px solid ${T.border}`,
            backdropFilter: 'blur(20px)',
            WebkitBackdropFilter: 'blur(20px)',
            boxShadow: T.shadowSm,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 9 }}>
            <motion.span
              style={{ fontSize: 13, color: item.iconColor }}
              animate={{ opacity: [0.5, 1, 0.5] }}
              transition={{ duration: item.period, repeat: Infinity }}
            >
              {item.icon}
            </motion.span>
            <span style={{ fontSize: 11, color: T.textFaint, letterSpacing: '0.08em' }}>
              {t(item.label)}
            </span>
          </div>
          <div style={{ fontSize: 22, fontWeight: 300, color: T.textPri, marginBottom: 2 }}>
            {t(item.value)}
          </div>
          <div style={{ fontSize: 11, color: T.textFaint, marginBottom: 10 }}>{t(item.sub)}</div>
          <div
            style={{
              fontSize: 10,
              color: T.textFaint,
              lineHeight: 1.6,
              borderTop: `1px solid rgba(70,100,160,0.06)`,
              paddingTop: 10,
            }}
          >
            {t(item.note)}
          </div>
        </Card>
      ))}
    </div>
  )
}

// ─── Main page ─────────────────────────────────────────────────────────────────
export function SensingDetailPage({ telemetry, onClose }: SensingDetailPageProps) {
  const { t } = useTranslation('translation')
  return (
    <motion.div
      initial={{ opacity: 0, y: 28 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: 28 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      style={{
        position: 'absolute',
        inset: 0,
        zIndex: 50,
        background: 'linear-gradient(175deg, #F7FBFF 0%, #EEF5FF 45%, #E6EFFF 100%)',
        display: 'flex',
        flexDirection: 'column',
        borderRadius: 'inherit',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '16px 20px 12px',
          borderBottom: `1px solid rgba(70,100,160,0.08)`,
          flexShrink: 0,
          background: 'rgba(255,255,255,0.55)',
          backdropFilter: 'blur(20px)',
          WebkitBackdropFilter: 'blur(20px)',
        }}
      >
        <div>
          <div
            style={{ fontSize: 20, fontWeight: 500, color: T.textPri, letterSpacing: '-0.01em' }}
          >
            {t('sensing.title')}
          </div>
        </div>
        <Button
          type="button"
          data-testid="close-sensing-detail"
          aria-label={t('common.close')}
          variant="ghost"
          size="icon"
          onClick={onClose}
          style={{
            width: 36,
            height: 36,
            borderRadius: '50%',
            background: 'rgba(70,100,160,0.06)',
            border: `1px solid rgba(70,100,160,0.12)`,
            color: T.textMut,
            fontSize: 18,
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            lineHeight: 1,
            padding: 0,
          }}
        >
          ×
        </Button>
      </div>

      {/* Live indicator */}
      <div style={{ padding: '8px 20px 0', flexShrink: 0 }}>
        <div style={{ display: 'inline-flex', alignItems: 'center', gap: 7 }}>
          <motion.div
            style={{
              width: 6,
              height: 6,
              borderRadius: '50%',
              background: T.brand,
              boxShadow: `0 0 5px rgba(82,120,232,0.45)`,
            }}
            animate={{ opacity: [0.45, 1, 0.45] }}
            transition={{ duration: 2, repeat: Infinity }}
          />
          <span style={{ fontSize: 11, color: T.brand, letterSpacing: '0.06em', fontWeight: 500 }}>
            {t('sensing.live')}
          </span>
          <span style={{ fontSize: 11, color: T.textFaint }}>·</span>
          <span style={{ fontSize: 11, color: T.textFaint }}>{t('sensing.updateFrequency')}</span>
        </div>
      </div>

      {/* Scrollable */}
      <div
        style={{ flex: 1, overflowY: 'auto', padding: '16px 20px 72px', boxSizing: 'border-box' }}
      >
        <SLabel label={t('sensing.pointCloud.title')} />
        <Card
          style={{
            position: 'relative',
            height: 220,
            overflow: 'hidden',
            marginBottom: 16,
            borderRadius: 20,
            border: `1px solid ${T.border}`,
            background: 'linear-gradient(160deg, #EDF6FF, #D9E9FF)',
            boxShadow: T.shadowSm,
          }}
        >
          <PointCloudView
            points={telemetry.points}
            breathingPhase={telemetry.breathingPhase}
            paused={false}
            viewResetKey={0}
            hasSpatialData={telemetry.hasSpatialData}
          />
          {!telemetry.hasSpatialData && (
            <div
              style={{
                position: 'absolute',
                inset: 0,
                display: 'grid',
                placeItems: 'center',
                color: T.textMut,
                fontSize: 12,
                pointerEvents: 'none',
              }}
            >
              {t(
                telemetry.connected
                  ? 'sensing.pointCloud.waiting'
                  : 'sensing.pointCloud.connectPrompt',
              )}
            </div>
          )}
        </Card>

        <SLabel label={t('sensing.presence.title')} />
        <PresenceCard />

        <SLabel label={t('sensing.trends.title')} />
        <BodyTrends />

        <SLabel label={t('sensing.contact.title')} />
        <ContactArea />

        <SLabel label={t('sensing.physio.title')} />
        <PhysioEstimates />

        <Card
          style={{
            padding: '12px 16px',
            borderRadius: 14,
            background: 'rgba(70,100,160,0.04)',
            border: `1px solid rgba(70,100,160,0.08)`,
            fontSize: 11,
            color: T.textFaint,
            lineHeight: 1.7,
            letterSpacing: '0.02em',
          }}
        >
          {t('sensing.disclaimer')}
        </Card>
      </div>
    </motion.div>
  )
}
