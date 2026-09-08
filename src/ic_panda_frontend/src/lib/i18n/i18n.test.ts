import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import en from './messages/en'
import { mockStorage } from './test-storage'
import {
  initLocale,
  loadCatalog,
  locale,
  setLocale,
  t,
  translate
} from './index'
import {
  isLocale,
  localeDir,
  locales,
  LOCALE_STORAGE_KEY,
  matchLocale,
  resolveLocale
} from './locales'

describe('locale detection', () => {
  it('matches regional tags and skips unsupported browser preferences', () => {
    for (const tag of ['zh-CN', 'zh-Hant-TW', 'ZH_hk'])
      expect(matchLocale(tag)).toBe('zh')
    expect(matchLocale('fr-CA')).toBe('fr')
    expect(matchLocale(' ar-EG ')).toBe('ar')
    expect(matchLocale('de-DE')).toBeNull()
    expect(resolveLocale(null, ['de-DE', 'ru-RU', 'en'])).toBe('ru')
    expect(resolveLocale('en', ['zh-CN'])).toBe('en')
    expect(resolveLocale('invalid', ['de'])).toBe('en')
    expect(isLocale('__proto__')).toBe(false)
  })

  it('keeps the pre-paint script consistent with runtime detection', () => {
    const html = readFileSync(resolve('src/app.html'), 'utf8')
    const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1]
    expect(script).toBeTruthy()
    for (const stored of [null, 'en', 'zh-TW', 'invalid', 'AR_eg']) {
      for (const preferred of [
        ['de', 'fr-CA'],
        ['zh-CN'],
        ['unsupported'],
        []
      ]) {
        const document = { documentElement: { lang: '', dir: '' } }
        const storage = {
          getItem: (key: string) => (key === LOCALE_STORAGE_KEY ? stored : null)
        }
        new Function('document', 'localStorage', 'navigator', script!)(
          document,
          storage,
          { languages: preferred, language: preferred[0] || 'ar-EG' }
        )
        const expected = resolveLocale(
          stored,
          preferred.length ? preferred : ['ar-EG']
        )
        expect(document.documentElement).toEqual({
          lang: expected,
          dir: localeDir(expected)
        })
      }
    }
  })
})

describe('catalogs and switching', () => {
  beforeEach(async () => {
    mockStorage()
    await setLocale('en', false)
  })
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('ships complete catalogs with identical interpolation parameters', async () => {
    const params = (text: string) =>
      [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()
    for (const language of locales) {
      const catalog = await loadCatalog(language)
      expect(Object.keys(catalog).sort()).toEqual(Object.keys(en).sort())
      for (const [key, value] of Object.entries(catalog)) {
        expect(value.trim(), `${language}: ${key}`).not.toBe('')
        expect(params(value), `${language}: ${key}`).toEqual(params(key))
      }
      if (language !== 'en') {
        expect(
          Object.entries(catalog).filter(([key, value]) => key !== value).length
        ).toBeGreaterThan(Object.keys(en).length * 0.8)
      }
    }
  })

  it('updates subscribers, document direction, and the saved preference', async () => {
    const updates: string[] = []
    const unsubscribe = t.subscribe((fn) => updates.push(fn('Language')))
    for (const language of locales) {
      await setLocale(language)
      expect(get(locale)).toBe(language)
      expect(document.documentElement.lang).toBe(language)
      expect(document.documentElement.dir).toBe(localeDir(language))
      expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe(language)
    }
    unsubscribe()
    expect(new Set(updates).size).toBe(6)
  })

  it('honors a saved choice and survives blocked storage', async () => {
    vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['zh-TW', 'fr'])
    localStorage.setItem(LOCALE_STORAGE_KEY, 'en')
    await initLocale()
    expect(get(locale)).toBe('en')
    localStorage.clear()
    await initLocale()
    expect(get(locale)).toBe('zh')
    vi.spyOn(localStorage, 'getItem').mockImplementation(() => {
      throw new Error('blocked')
    })
    vi.spyOn(localStorage, 'setItem').mockImplementation(() => {
      throw new Error('blocked')
    })
    await expect(initLocale()).resolves.toBeUndefined()
    await expect(setLocale('ar')).resolves.toBeUndefined()
    expect(get(locale)).toBe('ar')
  })

  it('lets the newest switch win even when earlier loads are pending', async () => {
    const first = setLocale('zh')
    const second = setLocale('ru')
    const last = setLocale('en')
    await Promise.all([first, second, last])
    expect(get(locale)).toBe('en')
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('en')
  })

  it('formats existing token displays without changing transfer amounts', async () => {
    const { TokenDisplay, PANDAToken } = await import('../utils/token')
    const display = new TokenDisplay(PANDAToken, 123450000000n)
    const initial = {
      amount: display.amount,
      fee: display.fee,
      total: display.total
    }
    for (const language of locales) {
      await setLocale(language)
      const expected = new Intl.NumberFormat(language, {
        minimumFractionDigits: 1,
        maximumFractionDigits: PANDAToken.decimals,
        roundingMode: 'floor'
      }).format(display.num)
      expect(display.display()).toBe(expected)
      expect({
        amount: display.amount,
        fee: display.fee,
        total: display.total
      }).toEqual(initial)
    }
  })

  it('supports zero-decimal tokens and reuses the formatter within a locale', async () => {
    const { TokenDisplay, PANDAToken } = await import('../utils/token')
    const display = new TokenDisplay({ ...PANDAToken, decimals: 0 }, 42n)
    const englishFormatter = display.formater
    expect(display.display()).toBe('42')
    expect(display.formater).toBe(englishFormatter)
    await setLocale('ar', false)
    expect(display.display()).toBe(new Intl.NumberFormat('ar').format(42))
    expect(display.formater).not.toBe(englishFormatter)
    expect(display.amount).toBe(42n)
  })

  it('interpolates values once and preserves unknown errors and identifiers', async () => {
    await loadCatalog('zh')
    expect(
      translate('zh', 'Will start at {time}', { time: '<b>{other}</b>' })
    ).toContain('<b>{other}</b>')
    expect(translate('zh', 'Unknown remote error: 503')).toBe(
      'Unknown remote error: 503'
    )
    expect(translate('zh', '__proto__')).toBe('__proto__')
    expect(translate('zh', 'Language')).toBe('语言')
  })
})
