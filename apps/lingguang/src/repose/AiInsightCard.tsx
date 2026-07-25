import { RefreshCw, Sparkles } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { useTranslation } from '@/i18n'
import { normalizeAnalysisText, streamAnalysis, type AnalysisSnapshot } from '@/lib/aiAnalysis'

export function AiInsightCard({ snapshot }: { snapshot: AnalysisSnapshot }) {
  const { t, i18n } = useTranslation('translation')
  const [text, setText] = useState('')
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading')
  const controllerRef = useRef<AbortController | null>(null)
  const displayTimerRef = useRef<number | null>(null)
  const snapshotRef = useRef(snapshot)
  const analysisTrigger = [
    snapshot.connected ? 'connected' : 'disconnected',
    snapshot.hasSpatialData ? 'present' : 'absent',
    snapshot.heartRate === null ? 'heart-unavailable' : 'heart-available',
    snapshot.respirationRate === null ? 'respiration-unavailable' : 'respiration-available',
    snapshot.qualityLabel,
    snapshot.motionLabel,
  ].join('|')

  useEffect(() => {
    snapshotRef.current = snapshot
  }, [snapshot])

  const generate = useCallback(() => {
    controllerRef.current?.abort()
    if (displayTimerRef.current !== null) window.clearTimeout(displayTimerRef.current)
    displayTimerRef.current = null
    const controller = new AbortController()
    controllerRef.current = controller
    const displayQueue: string[] = []
    let streamFinished = false

    const drainDisplayQueue = () => {
      const nextText = displayQueue.shift()
      if (nextText !== undefined && !controller.signal.aborted) setText(nextText)
      displayTimerRef.current = window.setTimeout(() => {
        displayTimerRef.current = null
        if (displayQueue.length > 0) {
          drainDisplayQueue()
        } else if (streamFinished && !controller.signal.aborted) {
          setStatus('ready')
        }
      }, 55)
    }

    setText('')
    setStatus('loading')

    void streamAnalysis(
      snapshotRef.current,
      (nextText) => {
        if (controller.signal.aborted) return
        const normalized = normalizeAnalysisText(nextText, i18n.resolvedLanguage)
        const latestQueued = displayQueue[displayQueue.length - 1]
        if (normalized === latestQueued) return
        displayQueue.push(normalized)
        if (displayTimerRef.current === null) drainDisplayQueue()
      },
      controller.signal,
      i18n.resolvedLanguage,
    )
      .then(() => {
        streamFinished = true
        if (
          displayQueue.length === 0 &&
          displayTimerRef.current === null &&
          !controller.signal.aborted
        ) {
          setStatus('ready')
        }
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return
        if (displayTimerRef.current !== null) window.clearTimeout(displayTimerRef.current)
        displayTimerRef.current = null
        displayQueue.length = 0
        console.log('[AI analysis]', error)
        setText(t('aiInsight.error'))
        setStatus('error')
      })
  }, [i18n.resolvedLanguage, t])

  useEffect(() => {
    const timer = window.setTimeout(generate, 0)
    return () => {
      window.clearTimeout(timer)
      controllerRef.current?.abort()
      if (displayTimerRef.current !== null) window.clearTimeout(displayTimerRef.current)
    }
  }, [analysisTrigger, generate])

  return (
    <Card
      style={{
        position: 'relative',
        marginBottom: 12,
        padding: '16px 16px 15px',
        overflow: 'hidden',
        border: '1px solid rgba(82,120,232,0.16)',
        borderRadius: 20,
        background: 'rgba(255,255,255,0.88)',
        boxShadow: '0 10px 32px rgba(45,70,125,0.08)',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
        <div
          aria-hidden="true"
          style={{
            display: 'grid',
            width: 30,
            height: 30,
            flex: '0 0 30px',
            placeItems: 'center',
            borderRadius: 8,
            background: '#5278E8',
            color: '#FFFFFF',
          }}
        >
          <Sparkles size={16} strokeWidth={2} />
        </div>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ color: '#17233D', fontSize: 14, fontWeight: 600 }}>
            {t('aiInsight.title')}
          </div>
          <div style={{ marginTop: 1, color: '#8A97AA', fontSize: 10 }}>
            {t('aiInsight.subtitle')}
          </div>
        </div>
        <Button
          type="button"
          data-testid="refresh-ai-analysis"
          variant="unstyled"
          size="unstyled"
          aria-label={t('aiInsight.refresh')}
          title={t('aiInsight.refresh')}
          disabled={status === 'loading'}
          onClick={generate}
          style={{
            display: 'grid',
            width: 32,
            height: 32,
            flex: '0 0 32px',
            placeItems: 'center',
            border: 0,
            borderRadius: 8,
            background: 'rgba(82,120,232,0.08)',
            color: '#5278E8',
            cursor: status === 'loading' ? 'default' : 'pointer',
            opacity: status === 'loading' ? 0.55 : 1,
          }}
        >
          <RefreshCw size={15} className={status === 'loading' ? 'ai-insight-spin' : undefined} />
        </Button>
      </div>

      <div
        aria-live="polite"
        style={{
          minHeight: 44,
          marginTop: 13,
          color: status === 'error' ? '#B24A4A' : '#43516A',
          fontSize: 14,
          lineHeight: 1.65,
        }}
      >
        {text !== '' ? text : t('aiInsight.loading')}
        {status === 'loading' && text !== '' && <span className="ai-insight-caret" />}
      </div>
    </Card>
  )
}
