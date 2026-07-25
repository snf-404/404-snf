export type AnalysisSnapshot = {
  connected: boolean
  heartRate: number | null
  respirationRate: number | null
  heartConfidence: number
  respirationConfidence: number
  qualityLabel: string
  motionLabel: string
  hasSpatialData: boolean
  processorTemperature: number | null
}

export type AnalysisLocale = 'zh-CN' | 'en-US'

type StreamChatEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'error'; message: string }
  | { type: string }

type StreamChat = (options: {
  messages: Array<{ role: 'system' | 'user'; content: string }>
  responseFormat: { type: 'text' }
  onEvent: (event: StreamChatEvent) => void
}) => Promise<{ content: string }>

const SYSTEM_PROMPTS: Record<AnalysisLocale, string> = {
  'zh-CN':
    '你是带健康监测功能的智能气垫助手。请结合当前感知数据，给出坐姿、承托、休息或气垫调节方面的简短建议。必须只输出普通文本，禁止使用任何Markdown、标题、列表或符号格式，总共不超过50个汉字。不要诊断疾病。',
  'en-US':
    'You are an intelligent support-surface assistant. Use the current sensing data to give one concise suggestion about posture, support, rest, or surface adjustment. Output plain English text only, without Markdown, headings, lists, or diagnosis. Keep it under 35 words.',
}

function formatValue(value: number | null, suffix: string, digits = 0): string {
  return value === null ? '暂无' : `${value.toFixed(digits)}${suffix}`
}

export function buildAnalysisPrompt(
  snapshot: AnalysisSnapshot,
  locale: AnalysisLocale = 'zh-CN',
): string {
  if (locale === 'en-US') {
    return [
      `Device: ${snapshot.connected ? 'connected' : 'disconnected'}`,
      `Heart rate: ${snapshot.heartRate === null ? 'unavailable' : `${snapshot.heartRate.toFixed(0)} BPM`}`,
      `Respiration: ${snapshot.respirationRate === null ? 'unavailable' : `${snapshot.respirationRate.toFixed(1)} RPM`}`,
      `Heart confidence: ${String(snapshot.heartConfidence)}%`,
      `Respiration confidence: ${String(snapshot.respirationConfidence)}%`,
      `Signal quality: ${snapshot.qualityLabel}`,
      `Motion interference: ${snapshot.motionLabel}`,
      `Presence: ${snapshot.hasSpatialData ? 'detected' : 'not detected'}`,
      `Device temperature: ${snapshot.processorTemperature === null ? 'unavailable' : `${snapshot.processorTemperature.toFixed(1)}°C`}`,
      'Surface mode: flat work mode',
      'Generate one brief analysis and suggestion from the current data.',
    ].join('\n')
  }
  return [
    `设备：${snapshot.connected ? '已连接' : '未连接'}`,
    `心率：${formatValue(snapshot.heartRate, '次/分')}`,
    `呼吸：${formatValue(snapshot.respirationRate, '次/分', 1)}`,
    `心率置信度：${String(snapshot.heartConfidence)}%`,
    `呼吸置信度：${String(snapshot.respirationConfidence)}%`,
    `信号质量：${snapshot.qualityLabel}`,
    `运动干扰：${snapshot.motionLabel}`,
    `人体存在：${snapshot.hasSpatialData ? '检测到' : '未检测到'}`,
    `设备温度：${formatValue(snapshot.processorTemperature, '℃', 1)}`,
    '气垫模式：平摊工作态',
    '请结合以上当前数据，生成简短分析和建议。',
  ].join('\n')
}

export function normalizeAnalysisText(value: string, locale: AnalysisLocale = 'zh-CN'): string {
  const plainText = value
    .replace(/```[\s\S]*?```/g, '')
    .replace(/[*_`#>[\]]/g, '')
    .replace(/^\s*[-+•]\s*/gm, '')
    .replace(/^\s*\d+[.)、]\s*/gm, '')
    .replace(/\s+/g, locale === 'en-US' ? ' ' : '')
    .trim()
  return Array.from(plainText).slice(0, 100).join('')
}

function extractChatCompletionDelta(payload: unknown): string {
  if (payload === null || typeof payload !== 'object') return ''
  const choices = (payload as { choices?: unknown }).choices
  if (!Array.isArray(choices)) return ''
  const first: unknown = choices[0]
  if (first === null || typeof first !== 'object') return ''
  const delta = (first as { delta?: unknown }).delta
  if (delta === null || typeof delta !== 'object') return ''
  const content = (delta as { content?: unknown }).content
  return typeof content === 'string' ? content : ''
}

export function createChatCompletionStreamParser(onText: (text: string) => void) {
  let lineBuffer = ''
  let accumulatedText = ''

  return (chunk: string, flush = false) => {
    lineBuffer += chunk
    const lines = lineBuffer.split(/\r?\n/)
    lineBuffer = flush ? '' : (lines.pop() ?? '')

    for (const line of lines) {
      if (!line.startsWith('data:')) continue
      const data = line.slice(5).trim()
      if (data === '' || data === '[DONE]') continue

      try {
        const delta = extractChatCompletionDelta(JSON.parse(data) as unknown)
        if (delta === '') continue
        accumulatedText += delta
        onText(accumulatedText)
      } catch {
        // Ignore malformed non-data lines from loosely compatible providers.
      }
    }
  }
}

function localStream(
  messages: Array<{ role: 'system' | 'user'; content: string }>,
  onText: (text: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest()
    let receivedLength = 0
    let hasAnswerText = false
    const parseStream = createChatCompletionStreamParser((text) => {
      hasAnswerText = true
      onText(text)
    })

    const consumeStream = (flush: boolean) => {
      const chunk = request.responseText.slice(receivedLength)
      receivedLength = request.responseText.length
      parseStream(chunk, flush)
    }

    request.open('POST', '/api/chat/completions')
    request.setRequestHeader('content-type', 'application/json')
    request.onprogress = () => {
      consumeStream(false)
    }
    request.onreadystatechange = () => {
      if (request.readyState === XMLHttpRequest.LOADING) consumeStream(false)
    }
    request.onload = () => {
      if (request.status >= 200 && request.status < 300) {
        consumeStream(true)
        if (!hasAnswerText) {
          reject(new Error('模型未返回可展示的答案文本'))
          return
        }
        resolve()
        return
      }
      reject(
        new Error(
          request.responseText !== ''
            ? request.responseText
            : `LLM request failed (${String(request.status)})`,
        ),
      )
    }
    request.onerror = () => {
      reject(new Error('LLM request failed'))
    }
    request.onabort = () => {
      reject(new DOMException('Aborted', 'AbortError'))
    }

    const abort = () => {
      request.abort()
    }
    signal?.addEventListener('abort', abort, { once: true })
    request.onloadend = () => signal?.removeEventListener('abort', abort)
    request.send(
      JSON.stringify({
        messages,
        stream: true,
        max_tokens: 1000,
        reasoning_effort: 'none',
      }),
    )
  })
}

export async function streamAnalysis(
  snapshot: AnalysisSnapshot,
  onText: (text: string) => void,
  signal?: AbortSignal,
  locale: AnalysisLocale = 'zh-CN',
): Promise<void> {
  const messages: Array<{ role: 'system' | 'user'; content: string }> = [
    { role: 'system', content: SYSTEM_PROMPTS[locale] },
    { role: 'user', content: buildAnalysisPrompt(snapshot, locale) },
  ]
  const streamChat = (window.lingguang.ai as { streamChat?: StreamChat }).streamChat

  if (typeof streamChat === 'function') {
    let accumulatedText = ''
    const result = await streamChat({
      messages,
      responseFormat: { type: 'text' },
      onEvent: (event) => {
        if (event.type === 'text_delta' && 'text' in event) {
          accumulatedText = event.text.startsWith(accumulatedText)
            ? event.text
            : accumulatedText + event.text
          onText(accumulatedText)
        }
        if (event.type === 'error' && 'message' in event) throw new Error(event.message)
      },
    })
    onText(result.content)
    return
  }

  await localStream(messages, onText, signal)
}
