import { currentLocale } from '$lib/i18n'
export async function sleep(ms: number): Promise<void> {
  return new Promise((res) => setTimeout(res, ms))
}

export function getCurrentTimeString(
  ts: bigint | number,
  language = currentLocale()
): string {
  const now = Date.now()
  const t = Number(ts)
  if (t >= now - 24 * 3600 * 1000) {
    return new Date(t).toLocaleTimeString(language)
  } else if (t >= now - 7 * 24 * 3600 * 1000) {
    return new Date(t).toLocaleDateString(language, { weekday: 'long' })
  }
  return new Date(t).toLocaleDateString(language)
}

export function getBytesString(
  bytes: number | BigInt,
  language = currentLocale()
): string {
  const n = Number(bytes)
  const index = n < 1024 ? 0 : n < 1024 ** 2 ? 1 : n < 1024 ** 3 ? 2 : 3
  const value = new Intl.NumberFormat(language, {
    minimumFractionDigits: index ? 2 : 0,
    maximumFractionDigits: index ? 2 : 0,
    useGrouping: false
  }).format(n / 1024 ** index)
  return value + ['', 'KB', 'MB', 'GB'][index]
}

export function getPriceNumber(v: number, language = currentLocale()): string {
  const digits =
    v < 0.001 ? 6 : v < 0.01 ? 5 : v < 0.1 ? 4 : v < 1 ? 3 : v < 10 ? 2 : 1
  return new Intl.NumberFormat(language, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
    useGrouping: false
  }).format(v)
}

export function getShortNumber(v: number | BigInt): string {
  return new Intl.NumberFormat(currentLocale(), {
    notation: 'compact',
    maximumFractionDigits: 2
  }).format(Number(v))
}

export function getShortNumber2(v: number | BigInt): string {
  return new Intl.NumberFormat(currentLocale(), {
    notation: 'compact',
    compactDisplay: 'long',
    maximumFractionDigits: 2,
    roundingMode: 'expand'
  }).format(Number(v))
}
