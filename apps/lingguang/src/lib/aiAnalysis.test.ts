import { describe, expect, it } from 'vitest'

import { createChatCompletionStreamParser } from './aiAnalysis'

describe('Chat Completions stream parsing', () => {
  it('emits accumulated text when data lines use single newlines', () => {
    const updates: string[] = []
    const parse = createChatCompletionStreamParser((text) => updates.push(text))

    parse('data: {"choices":[{"delta":{"content":"状态"}}]}\n')
    parse('data: {"choices":[{"delta":{"content":"平稳"}}]}\n')

    expect(updates).toEqual(['状态', '状态平稳'])
  })

  it('keeps an incomplete JSON line until the next network chunk', () => {
    const updates: string[] = []
    const parse = createChatCompletionStreamParser((text) => updates.push(text))

    parse('data: {"choices":[{"delta":{"content":"建议')
    expect(updates).toEqual([])
    parse('休息"}}]}\n\ndata: [DONE]\n', true)

    expect(updates).toEqual(['建议休息'])
  })
})
