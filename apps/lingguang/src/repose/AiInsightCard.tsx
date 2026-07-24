import { RefreshCw, Sparkles } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { normalizeAnalysisText, streamAnalysis, type AnalysisSnapshot } from '@/lib/aiAnalysis'

export function AiInsightCard({ snapshot }: { snapshot: AnalysisSnapshot }) {
  const [text, setText] = useState('')
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading')
  const controllerRef = useRef<AbortController | null>(null)
  const displayTimerRef = useRef<number | null>(null)
  const snapshotRef = useRef(snapshot)

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
        const normalized = normalizeAnalysisText(nextText)
        const latestQueued = displayQueue[displayQueue.length - 1]
        if (normalized === latestQueued) return
        displayQueue.push(normalized)
        if (displayTimerRef.current === null) drainDisplayQueue()
      },
      controller.signal,
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
        setText('暂时无法生成分析，请稍后重试')
        setStatus('error')
      })
  }, [])

  useEffect(() => {
    const timer = window.setTimeout(generate, 0)
    return () => {
      window.clearTimeout(timer)
      controllerRef.current?.abort()
      if (displayTimerRef.current !== null) window.clearTimeout(displayTimerRef.current)
    }
  }, [generate])

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
          <div style={{ color: '#17233D', fontSize: 14, fontWeight: 600 }}>智能承托建议</div>
          <div style={{ marginTop: 1, color: '#8A97AA', fontSize: 10 }}>基于健康监测与气垫状态</div>
        </div>
        <Button
          type="button"
          data-testid="refresh-ai-analysis"
          variant="unstyled"
          size="unstyled"
          aria-label="重新生成分析"
          title="重新生成分析"
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
        {text !== '' ? text : '正在结合心率、呼吸与体动数据生成建议…'}
        {status === 'loading' && text !== '' && <span className="ai-insight-caret" />}
      </div>
    </Card>
  )
}
