/** Interface language shared by both windows and the native desktop menus. */
export const locales = ['en', 'zh', 'zh-Hant', 'ru', 'ar', 'fr', 'es', 'hi', 'id', 'pt-BR', 'ja', 'de', 'ko', 'it', 'tr', 'vi', 'th', 'fil', 'ms', 'bn', 'ur', 'pl', 'nl'] as const

export type Locale = (typeof locales)[number]

export const defaultLocale: Locale = 'en'

/** Native names let people find their language regardless of the current UI. */
export const localeNames: Record<Locale, string> = {
  en: 'English',
  zh: '简体中文',
  'zh-Hant': '繁體中文',
  ru: 'Русский',
  ar: 'العربية',
  fr: 'Français',
  es: 'Español',
  hi: 'हिन्दी',
  id: 'Bahasa Indonesia',
  'pt-BR': 'Português (Brasil)',
  ja: '日本語',
  de: 'Deutsch',
  ko: '한국어',
  it: 'Italiano',
  tr: 'Türkçe',
  vi: 'Tiếng Việt',
  th: 'ไทย',
  fil: 'Filipino',
  ms: 'Bahasa Melayu',
  bn: 'বাংলা',
  ur: 'اردو',
  pl: 'Polski',
  nl: 'Nederlands'
}

export function localeDir(locale: Locale): 'ltr' | 'rtl' {
  return (locale === 'ar' || locale === 'ur') ? 'rtl' : 'ltr'
}

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (locales as readonly string[]).includes(value)
}

/**
 * Explicit Chinese script wins over region. TW/HK/MO share one Traditional
 * catalog; unqualified Chinese stays Simplified for existing saved choices.
 * All Portuguese preferences use the Brazilian catalog. `in` is the legacy
 * Indonesian language code still found in some system preferences.
 * Legacy Tagalog `tl` preferences use the Filipino catalog.
 */
export function matchLocale(value: string | null | undefined): Locale | null {
  const tag = value?.trim().toLowerCase().replaceAll('_', '-')
  if (!tag) return null
  const [primary, ...subtags] = tag.split('-')
  if (primary === 'zh') {
    if (subtags.includes('hans')) return 'zh'
    if (subtags.includes('hant')) return 'zh-Hant'
    return subtags.some((part) => ['tw', 'hk', 'mo'].includes(part)) ? 'zh-Hant' : 'zh'
  }
  if (primary === 'pt') return 'pt-BR'
  if (primary === 'in') return 'id'
  if (primary === 'tl') return 'fil'
  return isLocale(primary) ? primary : null
}

/** Saved choice, then the first supported system preference, then English. */
export function resolveLocale(stored: string | null | undefined, preferred: readonly string[]): Locale {
  for (const tag of [stored, ...preferred]) {
    const match = matchLocale(tag)
    if (match) return match
  }
  return defaultLocale
}
