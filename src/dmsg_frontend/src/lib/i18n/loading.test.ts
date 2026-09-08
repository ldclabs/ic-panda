import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import { mockStorage } from './test-storage'
beforeEach(() => mockStorage())

afterEach(() => {
  vi.doUnmock('./messages/fr')
  vi.restoreAllMocks()
  vi.resetModules()
  vi.unstubAllGlobals()
})

describe('catalog download failures', () => {
  it('keeps the last language on failure, then permits retry', async () => {
    vi.resetModules()
    vi.doMock('./messages/fr', () => {
      throw new Error('offline')
    })
    const { setLocale, locale, t } = await import('./index')
    await setLocale('en')
    await expect(setLocale('fr')).rejects.toThrow()
    expect(get(locale)).toBe('en')
    expect(get(t)('Language')).toBe('Language')
    vi.doUnmock('./messages/fr')
    await setLocale('fr')
    expect(get(locale)).toBe('fr')
    expect(get(t)('Language')).toBe('Langue')
  })

  it('renders English when the preferred catalog cannot load without erasing the preference', async () => {
    vi.resetModules()
    vi.doMock('./messages/fr', () => {
      throw new Error('offline')
    })
    const { initLocale, locale, LOCALE_STORAGE_KEY } = await import('./index')
    localStorage.setItem(LOCALE_STORAGE_KEY, 'fr')
    await expect(initLocale()).resolves.toBeUndefined()
    expect(get(locale)).toBe('en')
    expect(document.documentElement.lang).toBe('en')
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('fr')
  })
})

// Control when the import settles; a warm catalog does not exercise this race.
it('ignores a late startup failure after a newer language was selected', async () => {
  vi.resetModules()
  let rejectDownload!: (reason: Error) => void
  let started!: () => void
  const loading = new Promise<void>((resolve) => {
    started = resolve
  })
  vi.doMock('./messages/fr', () => {
    started()
    return new Promise((_, reject) => {
      rejectDownload = reject
    })
  })
  const { initLocale, setLocale, locale, LOCALE_STORAGE_KEY } =
    await import('./index')
  localStorage.setItem(LOCALE_STORAGE_KEY, 'fr')
  const initial = initLocale()
  await loading
  await setLocale('zh')
  rejectDownload(new Error('late network failure'))
  await initial
  expect(get(locale)).toBe('zh')
  expect(document.documentElement.dir).toBe('ltr')
  expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('zh')
})

it('ignores a slow successful import after a newer language was selected', async () => {
  vi.resetModules()
  type CatalogModule = typeof import('./messages/fr')
  const catalog = await vi.importActual<CatalogModule>('./messages/fr')
  let finish!: (value: CatalogModule) => void
  let started!: () => void
  const loading = new Promise<void>((resolve) => {
    started = resolve
  })
  vi.doMock('./messages/fr', () => {
    started()
    return new Promise<CatalogModule>((resolve) => {
      finish = resolve
    })
  })
  const { setLocale, locale, LOCALE_STORAGE_KEY } = await import('./index')
  const old = setLocale('fr')
  await loading
  await setLocale('ar')
  finish(catalog)
  await old
  expect(get(locale)).toBe('ar')
  expect(document.documentElement.dir).toBe('rtl')
  expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('ar')
})
