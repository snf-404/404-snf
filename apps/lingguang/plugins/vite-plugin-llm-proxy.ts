import type { IncomingMessage, ServerResponse } from 'node:http'

import type { Plugin } from 'vite'

type LlmProxyConfig = {
  apiKey: string
  baseUrl: string
  model: string
}

function completionUrl(baseUrl: string): string {
  const normalized = baseUrl.replace(/\/+$/, '')
  return normalized.endsWith('/chat/completions') ? normalized : `${normalized}/chat/completions`
}

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = []
  let size = 0
  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)
    size += buffer.length
    if (size > 32_000) throw new Error('Request body is too large')
    chunks.push(buffer)
  }
  return JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

async function relayOpenAiStream(
  request: IncomingMessage,
  response: ServerResponse,
  config: LlmProxyConfig,
) {
  if (request.method !== 'POST') {
    response.statusCode = 405
    response.end('Method not allowed')
    return
  }
  if (config.apiKey === '' || config.baseUrl === '' || config.model === '') {
    response.statusCode = 503
    response.end('请在 .env 中配置 LLM_MODEL、LLM_BASE_URL 和 LLM_API_KEY')
    return
  }

  try {
    const payload = await readJsonBody(request)
    if (payload === null || typeof payload !== 'object' || Array.isArray(payload)) {
      throw new Error('Invalid Chat Completions request')
    }

    const controller = new AbortController()
    response.on('close', () => {
      if (!response.writableEnded) controller.abort()
    })
    const upstream = await fetch(completionUrl(config.baseUrl), {
      method: 'POST',
      headers: {
        authorization: `Bearer ${config.apiKey}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify({ ...payload, model: config.model }),
      signal: controller.signal,
    })
    response.statusCode = upstream.status
    response.statusMessage = upstream.statusText
    const contentType = upstream.headers.get('content-type')
    if (contentType !== null) response.setHeader('content-type', contentType)
    const cacheControl = upstream.headers.get('cache-control')
    if (cacheControl !== null) response.setHeader('cache-control', cacheControl)
    response.setHeader('x-content-type-options', 'nosniff')
    response.flushHeaders()

    if (upstream.body === null) {
      response.end()
      return
    }

    const reader = upstream.body.getReader()
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      response.write(value)
    }
    response.end()
  } catch (error: unknown) {
    if (response.headersSent) {
      response.end()
      return
    }
    response.statusCode = 502
    response.end(error instanceof Error ? error.message : 'LLM proxy failed')
  }
}

export function llmProxyPlugin(config: LlmProxyConfig): Plugin {
  const middleware = (request: IncomingMessage, response: ServerResponse, next: () => void) => {
    if (request.url?.split('?')[0] !== '/api/chat/completions') {
      next()
      return
    }
    void relayOpenAiStream(request, response, config)
  }

  return {
    name: 'llm-proxy',
    configureServer(server) {
      server.middlewares.use(middleware)
    },
    configurePreviewServer(server) {
      server.middlewares.use(middleware)
    },
  }
}
