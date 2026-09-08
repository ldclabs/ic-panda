// Locale registry and detection. Pure module (no runes, no browser globals) so
// it can be imported from anywhere, including the pre-paint script's twin logic.

export const locales = ['en', 'zh', 'ru', 'ar', 'fr', 'es'] as const

export type Locale = (typeof locales)[number]

export const defaultLocale: Locale = 'en'

/** Native display name for each locale, shown in the switcher. Always in the
 * language itself — a reader looking for their language is looking for the word
 * they would write, not its English name. */
export const localeNames: Record<Locale, string> = {
  en: 'English',
  zh: '中文',
  ru: 'Русский',
  ar: 'العربية',
  fr: 'Français',
  es: 'Español'
}

const rtlLocales = new Set<Locale>(['ar'])

export function localeDir(locale: Locale): 'ltr' | 'rtl' {
  return rtlLocales.has(locale) ? 'rtl' : 'ltr'
}

export function isLocale(value: unknown): value is Locale {
  return (
    typeof value === 'string' && (locales as readonly string[]).includes(value)
  )
}

/** Shared with the pre-paint script in app.html — change one, change both. */
export const LOCALE_STORAGE_KEY = 'dmsg:locale'

/**
 * Best supported locale for a single tag, or `null`.
 *
 * Every Chinese variant — `zh`, `zh-CN`, `zh-Hans`, `zh-TW`, `zh-HK` — collapses
 * to Simplified `zh`. Traditional is not offered and falls back to Simplified
 * rather than to English, which is the less wrong of the two.
 */
export function matchLocale(value: string | null | undefined): Locale | null {
  if (!value) return null
  const tag = value.trim().toLowerCase()
  if (!tag) return null
  const primary = tag.split(/[-_]/)[0]
  if (primary === 'zh') return 'zh'
  const exact = locales.find((l) => l === tag)
  if (exact) return exact
  return locales.find((l) => l === primary) ?? null
}

/** Like {@link matchLocale} but always resolves. */
export function normalizeLocale(value: string | null | undefined): Locale {
  return matchLocale(value) ?? defaultLocale
}

/**
 * The locale to render: an explicit stored choice first, then the browser's
 * preference list in its own priority order, then English.
 *
 * A stored choice always wins. Someone who picked English on a Chinese-language
 * machine meant it.
 */
export function resolveLocale(
  stored: string | null | undefined,
  preferred: readonly string[]
): Locale {
  const explicit = matchLocale(stored)
  if (explicit) return explicit
  for (const tag of preferred) {
    const match = matchLocale(tag)
    if (match) return match
  }
  return defaultLocale
}
