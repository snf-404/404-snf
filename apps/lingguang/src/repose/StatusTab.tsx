import { motion, AnimatePresence } from 'motion/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Alert } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { useSnfTelemetry } from '@/hooks/useSnfTelemetry'
import type { TranslationKey } from '@/i18n/resources'

import { DeskMatSVG, type MatState } from './DeskMatSVG'
import { SensingDetailPage } from './SensingDetailPage'

type Telemetry = ReturnType<typeof useSnfTelemetry>

const QUALITY_LABEL_KEYS: Record<Telemetry['qualityLabel'], TranslationKey> = {
  calibrating: 'status.metric.calibrating',
  interference: 'status.metric.interference',
  excellent: 'common.excellent',
  good: 'common.good',
  low: 'common.low',
}

export type StatusState =
  | 'work'
  | 'fatigue'
  | 'nudge'
  | 'cradle-forming'
  | 'cradle-stable'
  | 'wrap-forming'
  | 'wrap-active'
  | 'resetting'
  | 'safe-flat'
  | 'error'

const STATE_ORDER: StatusState[] = [
  'work',
  'fatigue',
  'nudge',
  'cradle-forming',
  'cradle-stable',
  'wrap-forming',
  'wrap-active',
  'resetting',
  'safe-flat',
  'error',
]
const STATE_LABELS: Record<StatusState, TranslationKey> = {
  work: 'status.state.work',
  fatigue: 'status.state.fatigue',
  nudge: 'status.state.nudge',
  'cradle-forming': 'status.state.cradleForming',
  'cradle-stable': 'status.state.cradleStable',
  'wrap-forming': 'status.state.wrapForming',
  'wrap-active': 'status.state.wrapActive',
  resetting: 'status.state.resetting',
  'safe-flat': 'status.state.safeFlat',
  error: 'status.state.error',
}

// ─── Design tokens — Ice Blue Light style ──────────────────────────────────────
const T = {
  // Cards
  cardBg: 'rgba(255,255,255,0.76)',
  cardBgEl: 'rgba(255,255,255,0.90)',
  cardSub: 'rgba(240,246,255,0.72)',
  border: 'rgba(70,100,160,0.10)',
  borderSub: 'rgba(70,100,160,0.06)',
  // Text — all dark now
  textPri: '#17233D',
  textSec: '#43516A',
  textMut: '#7C8AA2',
  textFaint: '#9BA8BB',
  // Brand blue
  brand: '#5278E8',
  brandMid: '#6B8EF0',
  brandMist: '#A8BFEF',
  // Semantic
  error: '#D94F4F',
  errorText: '#C43E3E',
  warning: '#C08820',
  // Buttons
  btnBrand: '#5278E8',
  // Shadow
  shadow: '0 12px 36px rgba(45,70,125,0.08), inset 0 1px 0 rgba(255,255,255,0.80)',
  shadowSm: '0 6px 18px rgba(45,70,125,0.06), inset 0 1px 0 rgba(255,255,255,0.75)',
}

// ─── Micro wave helpers ────────────────────────────────────────────────────────
function HeartWave() {
  return (
    <svg viewBox="0 0 100 22" fill="none" style={{ width: '100%', height: 14, opacity: 0.55 }}>
      <path
        d="M 0 11 Q 10 11 14 11 L 19 4 L 25 18 L 30 7 L 36 14 L 42 11 Q 52 11 100 11"
        stroke="#5278E8"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
    </svg>
  )
}
function BreathWave() {
  return (
    <svg viewBox="0 0 100 22" fill="none" style={{ width: '100%', height: 14, opacity: 0.45 }}>
      <path
        d="M 0 11 C 12 11 18 3 25 3 C 32 3 38 19 50 19 C 62 19 68 3 75 3 C 82 3 88 11 100 11"
        stroke="#6B8EF0"
        strokeWidth="1.4"
        strokeLinecap="round"
        fill="none"
      />
    </svg>
  )
}

// ─── Auto-sensing label (on light bg) ─────────────────────────────────────────
function AutoLabel({ label, isError }: { label: string; isError?: boolean }) {
  return (
    <Badge
      variant={isError === true ? 'destructive' : 'secondary'}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 6,
        padding: '4px 12px',
        borderRadius: 999,
        border: `1px solid ${isError === true ? 'rgba(200,60,60,0.22)' : 'rgba(82,120,232,0.22)'}`,
        background: isError === true ? 'rgba(200,60,60,0.06)' : 'rgba(82,120,232,0.07)',
      }}
    >
      <motion.div
        style={{
          width: 6,
          height: 6,
          borderRadius: '50%',
          background: isError === true ? T.error : T.brand,
          boxShadow:
            isError === true ? `0 0 5px rgba(200,60,60,0.4)` : `0 0 5px rgba(82,120,232,0.4)`,
        }}
        animate={{ opacity: [0.45, 1, 0.45] }}
        transition={{ duration: 2.2, repeat: Infinity, ease: 'easeInOut' }}
      />
      <span
        style={{
          fontSize: 11,
          color: isError === true ? T.errorText : T.brand,
          letterSpacing: '0.07em',
          fontWeight: 500,
        }}
      >
        {label}
      </span>
    </Badge>
  )
}

// ─── Joint visual card (light bg — person visible with dark-blue strokes) ──────
function JointVisualCard({ onShowSensing }: { onShowSensing: () => void }) {
  const { t } = useTranslation('translation')
  return (
    <Card
      style={{
        borderRadius: 24,
        overflow: 'hidden',
        background:
          'linear-gradient(162deg, rgba(240,247,255,0.90) 0%, rgba(225,238,255,0.85) 100%)',
        border: `1px solid rgba(70,100,160,0.10)`,
        marginBottom: 12,
        position: 'relative',
        boxShadow: T.shadow,
      }}
    >
      {/* Soft center glow */}
      <div
        style={{
          position: 'absolute',
          top: 0,
          left: 0,
          right: 0,
          height: 80,
          background:
            'radial-gradient(ellipse at 50% 0%, rgba(158,187,255,0.22) 0%, transparent 70%)',
          pointerEvents: 'none',
        }}
      />
      <div
        style={{
          position: 'absolute',
          top: 14,
          right: 16,
          textAlign: 'right',
          pointerEvents: 'none',
        }}
      >
        <div style={{ fontSize: 10, letterSpacing: '0.12em', color: T.textFaint, fontWeight: 500 }}>
          {t('status.work.mode')}
        </div>
        <div
          style={{
            fontSize: 9,
            letterSpacing: '0.08em',
            color: T.textFaint,
            marginTop: 3,
            opacity: 0.7,
          }}
        >
          {t('status.work.autoSensing')}
        </div>
      </div>

      <svg viewBox="0 0 320 260" fill="none" style={{ width: '100%', display: 'block' }}>
        <defs>
          {/* Mat — ice blue, stays light */}
          <linearGradient id="jvMat" x1="0.5" y1="0" x2="0.5" y2="1">
            <stop offset="0%" stopColor="rgba(230,243,255,0.96)" />
            <stop offset="100%" stopColor="rgba(190,220,255,0.88)" />
          </linearGradient>
          <linearGradient id="jvMatSheen" x1="0.08" y1="0" x2="0.92" y2="1">
            <stop offset="0%" stopColor="rgba(255,255,255,0.55)" />
            <stop offset="45%" stopColor="rgba(255,255,255,0.12)" />
            <stop offset="100%" stopColor="rgba(255,255,255,0)" />
          </linearGradient>
          {/* Person body — soft gray-blue on light bg */}
          <linearGradient id="jvBody" x1="0.5" y1="0" x2="0.5" y2="1">
            <stop offset="0%" stopColor="rgba(130,165,220,0.22)" />
            <stop offset="100%" stopColor="rgba(150,185,235,0.08)" />
          </linearGradient>
          <radialGradient id="jvHeadGlow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="rgba(82,120,232,0.12)" />
            <stop offset="100%" stopColor="rgba(82,120,232,0)" />
          </radialGradient>
          <radialGradient id="jvSenseField" cx="50%" cy="44%" r="55%">
            <stop offset="0%" stopColor="rgba(82,120,232,0.06)" />
            <stop offset="100%" stopColor="rgba(82,120,232,0)" />
          </radialGradient>
          <radialGradient id="jvCL" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="rgba(82,120,232,0.24)" />
            <stop offset="100%" stopColor="rgba(82,120,232,0)" />
          </radialGradient>
          <radialGradient id="jvCR" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="rgba(82,120,232,0.20)" />
            <stop offset="100%" stopColor="rgba(82,120,232,0)" />
          </radialGradient>
          <filter id="jvBlur">
            <feGaussianBlur stdDeviation="5" />
          </filter>
        </defs>

        {/* Sensing field */}
        <motion.ellipse
          cx="160"
          cy="150"
          rx="148"
          ry="96"
          fill="url(#jvSenseField)"
          animate={{ opacity: [0.5, 1, 0.5] }}
          transition={{ duration: 7.5, repeat: Infinity }}
        />
        <motion.ellipse
          cx="160"
          cy="150"
          rx="148"
          ry="92"
          stroke="rgba(82,120,232,0.08)"
          strokeWidth="1"
          fill="none"
          strokeDasharray="3 7"
          animate={{ opacity: [0.3, 0.7, 0.3] }}
          transition={{ duration: 6, repeat: Infinity }}
        />

        {/* Person — dark blue strokes visible on light bg */}
        <ellipse cx="160" cy="29" rx="34" ry="34" fill="url(#jvHeadGlow)" />
        <circle
          cx="160"
          cy="27"
          r="16"
          fill="rgba(150,185,230,0.18)"
          stroke="rgba(82,120,210,0.32)"
          strokeWidth="1.5"
        />
        <rect x="156" y="43" width="8" height="14" rx="4" fill="rgba(120,160,220,0.12)" />
        <path d="M 128 60 Q 160 56 192 60 L 196 120 Q 160 126 124 120 Z" fill="url(#jvBody)" />
        <path
          d="M 108 62 Q 160 54 212 62"
          stroke="rgba(82,120,210,0.30)"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
        {/* Arms — thick soft fill + thin outline */}
        <path
          d="M 108 62 Q 84 90 70 118 Q 60 152 55 185"
          stroke="rgba(82,120,210,0.10)"
          strokeWidth="8"
          fill="none"
          strokeLinecap="round"
        />
        <path
          d="M 108 62 Q 84 90 70 118"
          stroke="rgba(82,120,210,0.28)"
          strokeWidth="1.6"
          fill="none"
          strokeLinecap="round"
        />
        <path
          d="M 70 118 Q 60 152 55 185"
          stroke="rgba(82,120,210,0.20)"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
        <path
          d="M 212 62 Q 236 90 250 118 Q 260 152 265 180"
          stroke="rgba(82,120,210,0.10)"
          strokeWidth="8"
          fill="none"
          strokeLinecap="round"
        />
        <path
          d="M 212 62 Q 236 90 250 118"
          stroke="rgba(82,120,210,0.28)"
          strokeWidth="1.6"
          fill="none"
          strokeLinecap="round"
        />
        <path
          d="M 250 118 Q 260 152 265 180"
          stroke="rgba(82,120,210,0.20)"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />

        {/* Mat surface */}
        <ellipse
          cx="160"
          cy="238"
          rx="148"
          ry="10"
          fill="rgba(100,140,200,0.12)"
          filter="url(#jvBlur)"
        />
        <path d="M 20 222 L 300 222 L 285 155 L 35 155 Z" fill="url(#jvMat)" />
        <path d="M 20 222 L 300 222 L 285 155 L 35 155 Z" fill="url(#jvMatSheen)" />
        <path
          d="M 45 207 Q 160 202 275 192"
          stroke="rgba(255,255,255,0.45)"
          strokeWidth="1"
          fill="none"
        />
        <path
          d="M 58 183 Q 160 179 262 172"
          stroke="rgba(255,255,255,0.28)"
          strokeWidth="1"
          fill="none"
        />

        {/* Contact zones */}
        <motion.ellipse
          cx="76"
          cy="212"
          rx="42"
          ry="14"
          fill="url(#jvCL)"
          animate={{ opacity: [0.3, 0.6, 0.3] }}
          transition={{ duration: 3.8, repeat: Infinity }}
        />
        <ellipse
          cx="76"
          cy="212"
          rx="58"
          ry="19"
          stroke="rgba(82,120,232,0.12)"
          strokeWidth="1"
          fill="none"
        />
        <motion.ellipse
          cx="76"
          cy="212"
          rx="58"
          ry="19"
          stroke="rgba(82,120,232,0.08)"
          strokeWidth="1.2"
          fill="none"
          animate={{ rx: [58, 74, 58], ry: [19, 25, 19], opacity: [0.08, 0, 0.08] }}
          transition={{ duration: 3.5, repeat: Infinity }}
        />
        <path
          d="M 57 186 L 76 212"
          stroke="rgba(82,120,232,0.14)"
          strokeWidth="1"
          strokeDasharray="2 3"
          strokeLinecap="round"
        />

        <motion.ellipse
          cx="244"
          cy="206"
          rx="42"
          ry="14"
          fill="url(#jvCR)"
          animate={{ opacity: [0.25, 0.55, 0.25] }}
          transition={{ duration: 3.8, repeat: Infinity, delay: 0.7 }}
        />
        <ellipse
          cx="244"
          cy="206"
          rx="58"
          ry="19"
          stroke="rgba(82,120,232,0.12)"
          strokeWidth="1"
          fill="none"
        />
        <motion.ellipse
          cx="244"
          cy="206"
          rx="58"
          ry="19"
          stroke="rgba(82,120,232,0.08)"
          strokeWidth="1.2"
          fill="none"
          animate={{ rx: [58, 74, 58], ry: [19, 25, 19], opacity: [0.08, 0, 0.08] }}
          transition={{ duration: 3.5, repeat: Infinity, delay: 1.3 }}
        />
        <path
          d="M 263 181 L 244 206"
          stroke="rgba(82,120,232,0.14)"
          strokeWidth="1"
          strokeDasharray="2 3"
          strokeLinecap="round"
        />

        <path d="M 20 222 L 300 222 L 300 230 L 20 230 Z" fill="rgba(220,237,255,0.90)" />
        <path d="M 20 230 L 300 230 L 300 235 L 20 235 Z" fill="rgba(100,140,200,0.15)" />

        {/* Sensing particles */}
        {(
          [
            [112, 108, 0],
            [218, 95, 1.2],
            [148, 138, 0.8],
            [178, 126, 2.0],
          ] as [number, number, number][]
        ).map(([cx, cy, d]) => (
          <motion.circle
            key={`${String(cx)}-${String(cy)}`}
            cx={cx}
            cy={cy}
            r={1.5}
            fill="#A8BFEF"
            opacity={0}
            animate={{ opacity: [0, 0.55, 0], r: [1.5, 2.2, 1.5] }}
            transition={{ duration: 3 + d * 0.3, repeat: Infinity, delay: d, ease: 'easeInOut' }}
          />
        ))}
      </svg>

      <div style={{ textAlign: 'center', paddingBottom: 12, paddingTop: 2 }}>
        <Button
          type="button"
          data-testid="open-sensing-detail"
          variant="unstyled"
          size="unstyled"
          onClick={onShowSensing}
          style={{
            background: 'none',
            border: 'none',
            color: T.textFaint,
            fontSize: 12,
            cursor: 'pointer',
            letterSpacing: '0.05em',
          }}
        >
          {t('status.work.viewSensing')}
        </Button>
      </div>
    </Card>
  )
}

// ─── State summary card ────────────────────────────────────────────────────────
function StateSummaryCard() {
  const { t } = useTranslation('translation')
  return (
    <Card
      style={{
        borderRadius: 20,
        padding: '16px 18px',
        marginBottom: 12,
        background: T.cardBgEl,
        border: `1px solid ${T.border}`,
        backdropFilter: 'blur(20px) saturate(115%)',
        WebkitBackdropFilter: 'blur(20px) saturate(115%)',
        boxShadow: T.shadow,
        position: 'relative',
        overflow: 'hidden',
      }}
    >
      <div
        style={{
          position: 'absolute',
          top: 0,
          right: 0,
          width: 120,
          height: 80,
          background:
            'radial-gradient(circle at 100% 0%, rgba(82,120,232,0.06) 0%, transparent 70%)',
          pointerEvents: 'none',
        }}
      />
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <div
            style={{
              fontSize: 17,
              fontWeight: 500,
              color: T.textPri,
              letterSpacing: '-0.01em',
              marginBottom: 5,
            }}
          >
            {t('status.work.summaryTitle')}
          </div>
          <div style={{ fontSize: 13, color: T.textSec, lineHeight: 1.6 }}>
            {t('status.work.summaryLine1')}
            <br />
            {t('status.work.summaryLine2')}
          </div>
        </div>
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'flex-end',
            gap: 5,
            paddingTop: 2,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
            <motion.div
              style={{ width: 6, height: 6, borderRadius: '50%', background: T.brand }}
              animate={{ opacity: [0.5, 1, 0.5] }}
              transition={{ duration: 2, repeat: Infinity }}
            />
            <span style={{ fontSize: 11, color: T.brand, letterSpacing: '0.04em' }}>
              {t('status.work.sensingNormal')}
            </span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
            <div style={{ width: 6, height: 6, borderRadius: '50%', background: T.brandMid }} />
            <span style={{ fontSize: 11, color: T.brandMid, letterSpacing: '0.04em' }}>
              {t('status.work.flatMode')}
            </span>
          </div>
        </div>
      </div>
    </Card>
  )
}

// ─── Core data dashboard ───────────────────────────────────────────────────────
function CoreDashboard({ telemetry }: { telemetry: Telemetry }) {
  const { t } = useTranslation('translation')
  return (
    <div style={{ marginBottom: 12 }}>
      <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
        <Card
          style={{
            flex: 1,
            borderRadius: 20,
            padding: '16px 16px 12px',
            background: T.cardBg,
            border: `1px solid ${T.border}`,
            backdropFilter: 'blur(20px)',
            WebkitBackdropFilter: 'blur(20px)',
            boxShadow: T.shadowSm,
          }}
        >
          <div
            style={{
              fontSize: 11,
              letterSpacing: '0.09em',
              color: T.textMut,
              marginBottom: 8,
              display: 'flex',
              alignItems: 'center',
              gap: 6,
            }}
          >
            <motion.span
              animate={{ opacity: [0.45, 1, 0.45] }}
              transition={{ duration: 1.2, repeat: Infinity }}
              style={{ fontSize: 11, color: 'rgba(82,120,232,0.65)' }}
            >
              ♡
            </motion.span>
            {t('status.metric.heartRate')}
          </div>
          <div style={{ display: 'flex', alignItems: 'flex-end', gap: 4, marginBottom: 8 }}>
            <span
              style={{
                fontSize: 38,
                fontWeight: 300,
                color: T.textPri,
                lineHeight: 1,
                letterSpacing: '-0.025em',
              }}
            >
              {telemetry.heartRate === null ? '--' : telemetry.heartRate.toFixed(0)}
            </span>
            <span style={{ fontSize: 13, color: T.textMut, paddingBottom: 5 }}>BPM</span>
          </div>
          <HeartWave />
        </Card>
        <Card
          style={{
            flex: 1,
            borderRadius: 20,
            padding: '16px 16px 12px',
            background: T.cardBg,
            border: `1px solid ${T.border}`,
            backdropFilter: 'blur(20px)',
            WebkitBackdropFilter: 'blur(20px)',
            boxShadow: T.shadowSm,
          }}
        >
          <div
            style={{
              fontSize: 11,
              letterSpacing: '0.09em',
              color: T.textMut,
              marginBottom: 8,
              display: 'flex',
              alignItems: 'center',
              gap: 6,
            }}
          >
            <motion.span
              animate={{ opacity: [0.38, 0.92, 0.38] }}
              transition={{ duration: 3.5, repeat: Infinity }}
              style={{ fontSize: 12, color: 'rgba(107,142,240,0.70)' }}
            >
              〜
            </motion.span>
            {t('status.metric.respiration')}
          </div>
          <div style={{ display: 'flex', alignItems: 'flex-end', gap: 4, marginBottom: 8 }}>
            <span
              style={{
                fontSize: 38,
                fontWeight: 300,
                color: T.textPri,
                lineHeight: 1,
                letterSpacing: '-0.025em',
              }}
            >
              {telemetry.respirationRate === null ? '--' : telemetry.respirationRate.toFixed(1)}
            </span>
            <span style={{ fontSize: 13, color: T.textMut, paddingBottom: 5 }}>RPM</span>
          </div>
          <BreathWave />
        </Card>
      </div>

      <div style={{ display: 'flex', gap: 6, marginBottom: 6 }}>
        {[
          {
            label: t('status.metric.sensingStatus'),
            value: t(telemetry.connected ? 'status.metric.running' : 'common.disconnected'),
            dot: telemetry.connected ? T.brand : null,
          },
          {
            label: t('status.metric.signalQuality'),
            value: t(QUALITY_LABEL_KEYS[telemetry.qualityLabel]),
            dot: T.brandMid,
          },
          {
            label: t('status.metric.motionInterference'),
            value: t(
              telemetry.motionLabel === 'high'
                ? 'status.metric.motionHigh'
                : 'status.metric.motionLow',
            ),
            dot: null,
          },
        ].map((item) => (
          <Card
            key={item.label}
            style={{
              flex: 1,
              padding: '10px 9px',
              borderRadius: 14,
              background: T.cardBg,
              border: `1px solid ${T.border}`,
              backdropFilter: 'blur(16px)',
              WebkitBackdropFilter: 'blur(16px)',
            }}
          >
            <div
              style={{ fontSize: 10, color: T.textFaint, marginBottom: 5, letterSpacing: '0.02em' }}
            >
              {item.label}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
              {item.dot !== null && (
                <div
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: '50%',
                    background: item.dot,
                    flexShrink: 0,
                  }}
                />
              )}
              <span style={{ fontSize: 13, color: T.textSec }}>{item.value}</span>
            </div>
          </Card>
        ))}
      </div>

      <div style={{ display: 'flex', gap: 6 }}>
        {[
          {
            label: t('status.metric.presence'),
            value: t(telemetry.hasSpatialData ? 'common.detected' : 'common.waitingForData'),
            dot: telemetry.hasSpatialData ? T.brand : null,
          },
          {
            label: t('status.metric.deviceTemperature'),
            value:
              telemetry.processorTemperature === null
                ? '--'
                : `${telemetry.processorTemperature.toFixed(1)}°C`,
            dot: null,
          },
        ].map((item) => (
          <Card
            key={item.label}
            style={{
              flex: 1,
              padding: '10px 12px',
              borderRadius: 14,
              background: T.cardSub,
              border: `1px solid ${T.borderSub}`,
              backdropFilter: 'blur(16px)',
              WebkitBackdropFilter: 'blur(16px)',
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
            }}
          >
            <span style={{ fontSize: 11, color: T.textFaint }}>{item.label}</span>
            <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
              {item.dot !== null && (
                <div style={{ width: 6, height: 6, borderRadius: '50%', background: item.dot }} />
              )}
              <span style={{ fontSize: 12, color: T.textSec }}>{item.value}</span>
            </div>
          </Card>
        ))}
      </div>
    </div>
  )
}

// ─── Mat status card ───────────────────────────────────────────────────────────
function MatStatusCard() {
  const { t } = useTranslation('translation')
  return (
    <Card
      style={{
        borderRadius: 20,
        padding: '16px 18px',
        marginBottom: 16,
        background: T.cardBg,
        border: `1px solid ${T.border}`,
        backdropFilter: 'blur(20px)',
        WebkitBackdropFilter: 'blur(20px)',
        boxShadow: T.shadowSm,
      }}
    >
      <div style={{ fontSize: 11, letterSpacing: '0.12em', color: T.textFaint, marginBottom: 12 }}>
        {t('status.mat.title')}
      </div>
      {(
        [
          { label: 'status.mat.currentMode', value: 'status.work.title', hi: true },
          { label: 'status.mat.title', value: 'status.work.flatMode' },
          { label: 'status.mat.contactArea', value: 'status.mat.forearmsTouching' },
          { label: 'status.mat.autoAdjustment', value: 'common.enabled' },
        ] as Array<{ label: TranslationKey; value: TranslationKey; hi?: boolean }>
      ).map((r, i) => (
        <div
          key={r.label}
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            padding: '10px 0',
            borderBottom: i < 3 ? `1px solid rgba(70,100,160,0.06)` : 'none',
          }}
        >
          <span style={{ fontSize: 13, color: T.textMut }}>{t(r.label)}</span>
          <span
            style={{
              fontSize: 13,
              fontWeight: r.hi === true ? 500 : 400,
              color: r.hi === true ? T.textPri : T.textSec,
            }}
          >
            {t(r.value)}
          </span>
        </div>
      ))}
      <div
        style={{
          textAlign: 'center',
          paddingTop: 12,
          borderTop: `1px solid rgba(70,100,160,0.05)`,
          marginTop: 2,
        }}
      >
        <Button
          type="button"
          data-testid="pause-auto-adjustment"
          variant="unstyled"
          size="unstyled"
          style={{
            background: 'none',
            border: 'none',
            color: T.textFaint,
            fontSize: 12,
            cursor: 'pointer',
            letterSpacing: '0.04em',
          }}
        >
          {t('status.mat.pauseAdjustment')}
        </Button>
      </div>
    </Card>
  )
}

// ─── WORK page ─────────────────────────────────────────────────────────────────
function WorkPage({
  onShowSensing,
  telemetry,
}: {
  onShowSensing: () => void
  telemetry: Telemetry
}) {
  const { t } = useTranslation('translation')
  return (
    <motion.div
      key="work"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.38, ease: 'easeOut' }}
      style={{ padding: '0 20px' }}
    >
      <div style={{ marginBottom: 14 }}>
        <AutoLabel label={t('status.work.autoSensing')} />
        <h1
          style={{
            fontSize: 30,
            fontWeight: 500,
            color: T.textPri,
            margin: '10px 0 4px',
            letterSpacing: '-0.02em',
            lineHeight: 1.2,
          }}
        >
          {t('status.work.title')}
        </h1>
        <p style={{ fontSize: 14, color: T.textSec, margin: 0, lineHeight: 1.6 }}>
          {t('status.work.subtitle')}
        </p>
      </div>
      <JointVisualCard onShowSensing={onShowSensing} />
      <StateSummaryCard />
      <CoreDashboard telemetry={telemetry} />
      <MatStatusCard />
    </motion.div>
  )
}

// ─── FATIGUE page ──────────────────────────────────────────────────────────────
function FatiguePage({ onExit }: { onExit: () => void }) {
  const { t } = useTranslation('translation')
  return (
    <motion.div
      key="fatigue"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.38, ease: 'easeOut' }}
      style={{ padding: '0 20px' }}
    >
      <div style={{ marginBottom: 14 }}>
        <AutoLabel label={t('status.fatigue.autoLabel')} />
        <h1
          style={{
            fontSize: 26,
            fontWeight: 500,
            color: T.textPri,
            margin: '10px 0 5px',
            letterSpacing: '-0.02em',
          }}
        >
          {t('status.fatigue.title')}
        </h1>
        <p
          style={{
            fontSize: 14,
            color: T.textSec,
            lineHeight: 1.6,
            margin: 0,
            whiteSpace: 'pre-line',
          }}
        >
          {t('status.fatigue.subtitle')}
        </p>
      </div>

      <Card
        style={{
          borderRadius: 24,
          background: T.cardBg,
          border: `1px solid ${T.border}`,
          backdropFilter: 'blur(20px)',
          WebkitBackdropFilter: 'blur(20px)',
          padding: '20px 14px 12px',
          marginBottom: 14,
          boxShadow: T.shadow,
        }}
      >
        <div style={{ height: 168 }}>
          <DeskMatSVG state="fatigue" />
        </div>
      </Card>

      <Card
        style={{
          borderRadius: 16,
          padding: '14px 16px',
          marginBottom: 12,
          background: T.cardBg,
          border: `1px solid ${T.border}`,
          backdropFilter: 'blur(20px)',
          WebkitBackdropFilter: 'blur(20px)',
        }}
      >
        <div style={{ fontSize: 16, fontWeight: 500, color: T.textPri, marginBottom: 5 }}>
          {t('status.fatigue.focusDuration')}
        </div>
        <div style={{ fontSize: 13, color: T.textSec, lineHeight: 1.6 }}>
          {t('status.fatigue.description')}
        </div>
      </Card>

      <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
        {(
          [
            { label: 'status.fatigue.movementTrend', value: 'status.fatigue.decreasing', hi: true },
            { label: 'status.fatigue.duration', value: 'status.fatigue.durationValue' },
            { label: 'status.fatigue.postureHold', value: 'status.fatigue.extended' },
          ] as Array<{ label: TranslationKey; value: TranslationKey; hi?: boolean }>
        ).map((r) => (
          <Card
            key={r.label}
            style={{
              flex: 1,
              padding: '12px 8px',
              borderRadius: 16,
              textAlign: 'center',
              background: r.hi === true ? 'rgba(82,120,232,0.08)' : T.cardBg,
              border: r.hi === true ? `1px solid rgba(82,120,232,0.20)` : `1px solid ${T.border}`,
              backdropFilter: 'blur(16px)',
              WebkitBackdropFilter: 'blur(16px)',
            }}
          >
            <div style={{ fontSize: 18, fontWeight: 300, color: T.textPri, marginBottom: 4 }}>
              {t(r.value)}
            </div>
            <div style={{ fontSize: 11, color: T.textMut }}>{t(r.label)}</div>
          </Card>
        ))}
      </div>

      <Button
        type="button"
        data-testid="fatigue-exit"
        onClick={onExit}
        style={{
          width: '100%',
          height: 52,
          borderRadius: 16,
          background: T.btnBrand,
          border: 'none',
          color: '#FFFFFF',
          fontSize: 16,
          fontWeight: 500,
          cursor: 'pointer',
          letterSpacing: '0.02em',
          boxShadow: '0 8px 22px rgba(82,120,232,0.28)',
        }}
      >
        {t('status.action.exitKeepFlat')}
      </Button>
    </motion.div>
  )
}

// ─── Generic rest state page ───────────────────────────────────────────────────
interface RestPageCfg {
  matState: MatState
  autoLabel: TranslationKey
  title: TranslationKey
  subtitle: TranslationKey
  rows: Array<{ label: TranslationKey; value?: string; valueKey?: TranslationKey; hi?: boolean }>
  primary?: TranslationKey
  danger?: TranslationKey
  secondary?: TranslationKey
  isError?: boolean
  errorDetail?: TranslationKey
}

function RestPage({ cfg, onExit }: { cfg: RestPageCfg; onExit: () => void }) {
  const { t } = useTranslation('translation')
  return (
    <motion.div
      key={cfg.title}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.38, ease: 'easeOut' }}
      style={{ padding: '0 20px' }}
    >
      <div style={{ marginBottom: 14 }}>
        <AutoLabel label={t(cfg.autoLabel)} isError={cfg.isError === true} />
        <h1
          style={{
            fontSize: 26,
            fontWeight: 500,
            letterSpacing: '-0.02em',
            lineHeight: 1.25,
            color: cfg.isError === true ? T.errorText : T.textPri,
            margin: '10px 0 5px',
          }}
        >
          {t(cfg.title)}
        </h1>
        <p
          style={{
            fontSize: 14,
            color: T.textSec,
            lineHeight: 1.6,
            margin: 0,
            whiteSpace: 'pre-line',
          }}
        >
          {t(cfg.subtitle)}
        </p>
      </div>

      <Card
        style={{
          borderRadius: 24,
          overflow: 'hidden',
          background: T.cardBg,
          border: cfg.isError === true ? `1px solid rgba(200,60,60,0.22)` : `1px solid ${T.border}`,
          backdropFilter: 'blur(20px)',
          WebkitBackdropFilter: 'blur(20px)',
          padding: '20px 14px 12px',
          marginBottom: 14,
          position: 'relative',
          boxShadow: T.shadow,
        }}
      >
        {cfg.isError === true && (
          <motion.div
            style={{
              position: 'absolute',
              inset: 0,
              borderRadius: 24,
              background: 'rgba(200,60,60,0.04)',
              pointerEvents: 'none',
            }}
            animate={{ opacity: [0.4, 0.9, 0.4] }}
            transition={{ duration: 2.5, repeat: Infinity }}
          />
        )}
        <div style={{ height: 168 }}>
          <DeskMatSVG state={cfg.matState} />
        </div>
      </Card>

      <Card
        style={{
          borderRadius: 20,
          overflow: 'hidden',
          background: T.cardBg,
          border: `1px solid ${T.border}`,
          backdropFilter: 'blur(20px)',
          WebkitBackdropFilter: 'blur(20px)',
          marginBottom: 16,
          boxShadow: T.shadowSm,
        }}
      >
        {cfg.rows.map((r, i) => (
          <div
            key={r.label}
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              padding: '13px 16px',
              borderBottom: i < cfg.rows.length - 1 ? `1px solid rgba(70,100,160,0.06)` : 'none',
              background: r.hi === true ? 'rgba(82,120,232,0.06)' : 'transparent',
            }}
          >
            <span style={{ fontSize: 13, color: T.textMut }}>{t(r.label)}</span>
            <span
              style={{
                fontSize: 13,
                fontWeight: r.hi === true ? 500 : 400,
                color: r.hi === true ? T.textPri : T.textSec,
              }}
            >
              {r.valueKey !== undefined ? t(r.valueKey) : r.value}
            </span>
          </div>
        ))}
      </Card>

      {cfg.errorDetail !== undefined && (
        <Alert variant="destructive" className="mb-4 text-[13px] leading-relaxed">
          {t(cfg.errorDetail)}
        </Alert>
      )}

      <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
        {cfg.primary !== undefined && (
          <Button
            type="button"
            data-testid="rest-primary-action"
            onClick={onExit}
            style={{
              width: '100%',
              height: 52,
              borderRadius: 16,
              background: T.btnBrand,
              border: 'none',
              color: '#FFFFFF',
              fontSize: 16,
              fontWeight: 500,
              cursor: 'pointer',
              boxShadow: '0 8px 22px rgba(82,120,232,0.28)',
            }}
          >
            {t(cfg.primary)}
          </Button>
        )}
        {cfg.danger !== undefined && (
          <Button
            type="button"
            data-testid="rest-danger-action"
            variant="destructive"
            style={{
              width: '100%',
              height: 48,
              borderRadius: 14,
              background: 'rgba(200,60,60,0.06)',
              border: `1px solid rgba(200,60,60,0.28)`,
              color: T.errorText,
              fontSize: 14,
              fontWeight: 500,
              cursor: 'pointer',
            }}
          >
            {t(cfg.danger)}
          </Button>
        )}
        {cfg.secondary !== undefined && (
          <div style={{ textAlign: 'center', paddingTop: 4 }}>
            <Button
              type="button"
              data-testid="rest-secondary-action"
              variant="unstyled"
              size="unstyled"
              style={{
                background: 'none',
                border: 'none',
                color: T.textMut,
                fontSize: 13,
                cursor: 'pointer',
                letterSpacing: '0.04em',
              }}
            >
              {t(cfg.secondary)}
            </Button>
          </div>
        )}
      </div>
    </motion.div>
  )
}

const PAGE_CFGS: Partial<Record<StatusState, RestPageCfg>> = {
  nudge: {
    matState: 'nudge',
    autoLabel: 'status.rest.forming',
    title: 'status.rest.nudgeTitle',
    subtitle: 'status.rest.nudgeSubtitle',
    rows: [
      { label: 'status.rest.targetArea', valueKey: 'status.rest.wristsForearms', hi: true },
      { label: 'status.rest.currentStage', valueKey: 'status.state.nudge' },
      { label: 'status.metric.sensingStatus', valueKey: 'common.stable' },
    ],
    primary: 'status.action.exitReturnFlat',
  },
  'cradle-forming': {
    matState: 'cradle-forming',
    autoLabel: 'status.fatigue.autoLabel',
    title: 'status.rest.cradleTitle',
    subtitle: 'status.rest.cradleSubtitle',
    rows: [
      { label: 'status.rest.wristArea', valueKey: 'status.rest.lifted', hi: true },
      { label: 'status.rest.forearmArea', valueKey: 'status.rest.expanding' },
      { label: 'status.rest.sideEdges', valueKey: 'status.rest.movingInward' },
    ],
    primary: 'status.action.exitReturnFlat',
  },
  'cradle-stable': {
    matState: 'cradle-stable',
    autoLabel: 'status.rest.cradleStable',
    title: 'status.rest.supportTitle',
    subtitle: 'status.rest.supportSubtitle',
    rows: [
      { label: 'status.rest.heldFor', valueKey: 'status.rest.heldDuration', hi: true },
      { label: 'status.rest.contactStatus', valueKey: 'status.rest.continuouslyStable' },
      { label: 'status.rest.surfaceState', valueKey: 'common.stable' },
    ],
    primary: 'status.action.endRest',
  },
  'wrap-forming': {
    matState: 'wrap-forming',
    autoLabel: 'status.rest.forming',
    title: 'status.rest.wrapTitle',
    subtitle: 'status.rest.wrapSubtitle',
    rows: [
      { label: 'status.rest.headPosition', valueKey: 'status.rest.inRange', hi: true },
      { label: 'status.rest.wrapProgress', valueKey: 'status.rest.closing' },
      { label: 'status.rest.openingState', valueKey: 'status.rest.keptOpen' },
    ],
    primary: 'status.action.exitReturnFlat',
  },
  'wrap-active': {
    matState: 'wrap-active',
    autoLabel: 'status.rest.deepRest',
    title: 'status.rest.deepRest',
    subtitle: 'status.rest.deepRestSubtitle',
    rows: [
      { label: 'status.rest.rested', value: '06:32', hi: true },
      { label: 'status.rest.breathingRhythm', valueKey: 'sensing.physio.steady' },
      { label: 'status.rest.currentSensing', valueKey: 'common.stable' },
    ],
    primary: 'status.action.endRest',
    danger: 'status.action.emergencyDeflate',
  },
  resetting: {
    matState: 'resetting',
    autoLabel: 'status.reset.autoLabel',
    title: 'status.reset.title',
    subtitle: 'status.reset.subtitle',
    rows: [
      { label: 'status.reset.expansionProgress', valueKey: 'common.inProgress', hi: true },
      { label: 'status.reset.autoDeflation', valueKey: 'common.inProgress' },
    ],
    danger: 'status.action.deflateNow',
  },
  'safe-flat': {
    matState: 'safe-flat',
    autoLabel: 'status.safe.autoLabel',
    title: 'status.safe.title',
    subtitle: 'status.safe.subtitle',
    rows: [
      { label: 'status.mat.title', valueKey: 'status.safe.completelyFlat', hi: true },
      { label: 'status.safe.safetyCheck', valueKey: 'common.passed' },
    ],
    secondary: 'status.action.viewRestRecord',
  },
  error: {
    matState: 'error',
    autoLabel: 'status.error.autoLabel',
    title: 'status.error.title',
    subtitle: 'status.error.subtitle',
    rows: [
      { label: 'status.error.inflationStatus', valueKey: 'status.error.stopped', hi: true },
      { label: 'status.error.deflationStatus', valueKey: 'common.inProgress' },
      { label: 'status.error.resetProgress', valueKey: 'status.error.autoResetting' },
    ],
    danger: 'status.action.deflateNow',
    isError: true,
    errorDetail: 'status.error.sensingModule',
    secondary: 'status.action.viewDeviceIssue',
  },
}

// ─── Main StatusTab ────────────────────────────────────────────────────────────
interface StatusTabProps {
  telemetry: Telemetry
  currentState: StatusState
  onStateChange: (s: StatusState) => void
}

export function StatusTab({ telemetry, currentState, onStateChange }: StatusTabProps) {
  const [showSensing, setShowSensing] = useState(false)
  const { t } = useTranslation('translation')

  const handleExit = () => {
    onStateChange('resetting')
    setTimeout(() => {
      onStateChange('safe-flat')
    }, 3000)
    setTimeout(() => {
      onStateChange('work')
    }, 4500)
  }

  return (
    <div style={{ position: 'relative', height: '100%', overflow: 'hidden' }}>
      <AnimatePresence>
        {showSensing && (
          <SensingDetailPage
            telemetry={telemetry}
            onClose={() => {
              setShowSensing(false)
            }}
          />
        )}
      </AnimatePresence>

      <div
        style={{
          position: 'absolute',
          inset: 0,
          pointerEvents: 'none',
          background:
            currentState === 'error'
              ? 'radial-gradient(ellipse at 50% 22%, rgba(200,60,60,0.05) 0%, transparent 60%)'
              : 'radial-gradient(ellipse at 50% 18%, rgba(158,187,255,0.10) 0%, transparent 55%)',
        }}
      />

      <div
        style={{
          height: '100%',
          overflowY: showSensing ? 'hidden' : 'auto',
          paddingBottom: 72,
          boxSizing: 'border-box',
        }}
      >
        {/* State navigator */}
        <Tabs
          value={currentState}
          onValueChange={(value) => {
            if (STATE_ORDER.includes(value as StatusState)) onStateChange(value as StatusState)
          }}
        >
          <TabsList
            style={{
              display: 'flex',
              gap: 5,
              padding: '10px 20px 12px',
              overflowX: 'auto',
              scrollbarWidth: 'none',
            }}
          >
            {STATE_ORDER.map((s) => (
              <TabsTrigger
                key={s}
                value={s}
                data-testid={`state-${s}`}
                style={{
                  flexShrink: 0,
                  padding: '4px 10px',
                  borderRadius: 999,
                  background:
                    currentState === s ? 'rgba(82,120,232,0.12)' : 'rgba(70,100,160,0.06)',
                  border:
                    currentState === s
                      ? `1px solid rgba(82,120,232,0.30)`
                      : `1px solid rgba(70,100,160,0.08)`,
                  color: currentState === s ? T.brand : T.textMut,
                  fontSize: 10,
                  cursor: 'pointer',
                  letterSpacing: '0.06em',
                  fontWeight: 500,
                }}
              >
                {t(STATE_LABELS[s])}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>

        <AnimatePresence mode="wait">
          {currentState === 'work' && (
            <WorkPage
              telemetry={telemetry}
              onShowSensing={() => {
                setShowSensing(true)
              }}
            />
          )}
          {currentState === 'fatigue' && <FatiguePage onExit={handleExit} />}
          {currentState !== 'work' && currentState !== 'fatigue' && PAGE_CFGS[currentState] && (
            <RestPage key={currentState} cfg={PAGE_CFGS[currentState]} onExit={handleExit} />
          )}
        </AnimatePresence>
      </div>
    </div>
  )
}
