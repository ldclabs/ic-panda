import { derived, get, writable } from 'svelte/store'
import {
  defaultLocale,
  LOCALE_STORAGE_KEY,
  localeDir,
  normalizeLocale,
  resolveLocale,
  type Locale
} from './locales'
import en from './messages/en'
import type { Messages, MessageKey } from './messages/en'
export * from './locales'
export type { Messages, MessageKey } from './messages/en'

const loaders = {
  zh: () => import('./messages/zh'),
  ru: () => import('./messages/ru'),
  ar: () => import('./messages/ar'),
  fr: () => import('./messages/fr'),
  es: () => import('./messages/es')
}
const catalogs = new Map<Locale, Messages>([['en', en]])
const pending = new Map<Locale, Promise<Messages>>()
export async function loadCatalog(locale: Locale): Promise<Messages> {
  const cached = catalogs.get(locale)
  if (cached) return cached
  let request = pending.get(locale)
  if (!request) {
    request = loaders[locale as keyof typeof loaders]()
      .then(({ default: messages }) => {
        catalogs.set(locale, messages)
        return messages
      })
      .finally(() => pending.delete(locale))
    pending.set(locale, request)
  }
  return request
}

// Both frontends are client-only SPAs (root +layout.ts). No browser globals are
// accessed on import, so build tools and tests can safely import this module.
const selected = writable<Locale>(defaultLocale)
export const locale = { subscribe: selected.subscribe }
export type Params = Record<string, string | number | bigint>
export function translate(
  locale: Locale,
  key: string,
  params: Params = {}
): string {
  const catalog = catalogs.get(locale) ?? en
  const template = Object.hasOwn(catalog, key)
    ? catalog[key as MessageKey]
    : key
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.hasOwn(params, name) ? String(params[name]) : match
  )
}
export const t = derived(
  locale,
  ($locale) => (key: string, params?: Params) => translate($locale, key, params)
)
/** For event-time messages outside components; UI uses the reactive $t store. */
export function tr(key: string, params?: Params): string {
  return get(t)(key, params)
}
export function currentLocale(): Locale {
  return get(locale)
}

let revision = 0
function applyDocument(locale: Locale) {
  if (typeof document === 'undefined') return
  document.documentElement.lang = locale
  document.documentElement.dir = localeDir(locale)
}
export async function setLocale(value: string, persist = true): Promise<void> {
  const next = normalizeLocale(value)
  const request = ++revision
  try {
    await loadCatalog(next)
  } catch (error) {
    // A superseded failure must not trigger initLocale's English fallback.
    if (request !== revision) return
    throw error
  }
  // A slow earlier download must not overwrite the most recent choice.
  if (request !== revision) return
  selected.set(next)
  applyDocument(next)
  if (persist) {
    try {
      localStorage.setItem(LOCALE_STORAGE_KEY, next)
    } catch {
      /* Session only when storage is blocked. */
    }
  }
}
export async function initLocale(): Promise<void> {
  if (typeof navigator === 'undefined') return
  let stored: string | null = null
  try {
    stored = localStorage.getItem(LOCALE_STORAGE_KEY)
  } catch {
    /* Private browsing. */
  }
  const next = resolveLocale(
    stored,
    navigator.languages?.length ? navigator.languages : [navigator.language]
  )
  const initialization = revision + 1
  try {
    await setLocale(next, false)
  } catch {
    if (revision === initialization) await setLocale(defaultLocale, false)
  }
}
