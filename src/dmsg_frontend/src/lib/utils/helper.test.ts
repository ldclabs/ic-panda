import { afterEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import { errorText, locales, setLocale } from '$lib/i18n'
import { getCurrentTimeString, getShortNumber2 } from './helper'

afterEach(async () => {
  vi.useRealTimers()
  await setLocale('en', false)
})

describe('localized archive data', () => {
  it('formats the same historical timestamps on every language switch', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-09-08T12:00:00Z'))
    for (const language of locales) {
      await setLocale(language, false)
      for (const days of [0, 3, 30]) {
        const timestamp = Date.now() - days * 86400000
        const date = new Date(timestamp)
        const expected =
          days === 0
            ? date.toLocaleTimeString(language)
            : days < 7
              ? date.toLocaleDateString(language, { weekday: 'long' })
              : date.toLocaleDateString(language)
        expect(getCurrentTimeString(timestamp, language)).toBe(expected)
      }
      expect(getShortNumber2(1000000)).toBe(
        new Intl.NumberFormat(language, {
          notation: 'compact',
          compactDisplay: 'long',
          maximumFractionDigits: 2,
          roundingMode: 'expand'
        }).format(1000000)
      )
    }
  })

  it('translates decoder explanations while preserving error details', async () => {
    await setLocale('zh', false)
    expect(get(errorText)('Failed to decrypt message: tag mismatch')).toBe(
      '消息解密失败：tag mismatch'
    )
    expect(get(errorText)('unrecognized diagnostic')).toBe(
      'unrecognized diagnostic'
    )
  })
})
