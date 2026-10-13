/**
 * The desktop's interface languages, those of the website
 * (`src/dmsg_frontend/src/lib/i18n/locales.ts`): English, Simplified
 * Chinese, Russian, Arabic, French and Spanish. The main process resolves
 * one for the tray, notifications, file dialogs and both windows: the
 * person's choice first, then the system's preferred languages, then English.
 */

export const locales = ['en', 'zh', 'ru', 'ar', 'fr', 'es'] as const

export type Locale = (typeof locales)[number]

export const defaultLocale: Locale = 'en'

/** Each language's name in itself, as the language menu shows it. */
export const localeNames: Record<Locale, string> = {
  en: 'English',
  zh: '中文',
  ru: 'Русский',
  ar: 'العربية',
  fr: 'Français',
  es: 'Español'
}

export function localeDir(locale: Locale): 'ltr' | 'rtl' {
  return locale === 'ar' ? 'rtl' : 'ltr'
}

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (locales as readonly string[]).includes(value)
}

/** The language for a tag such as `fr-CA` or `zh-Hant-TW`; every Chinese variant is Simplified Chinese. */
export function matchLocale(value: string | null | undefined): Locale | null {
  const tag = value?.trim().toLowerCase()
  if (!tag) return null
  const primary = tag.split(/[-_]/)[0]
  return isLocale(primary) ? primary : null
}

/** The person's stored choice, else the first preferred language offered here, else English. */
export function resolveLocale(stored: string | null | undefined, preferred: readonly string[]): Locale {
  for (const tag of [stored, ...preferred]) {
    const match = matchLocale(tag)
    if (match) return match
  }
  return defaultLocale
}
