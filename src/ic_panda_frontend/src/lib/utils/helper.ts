import { currentLocale } from '$lib/i18n'
export async function sleep(ms: number): Promise<void> {
  return new Promise((res) => setTimeout(res, ms))
}

export function getShortNumber(v: number | BigInt): string {
  return new Intl.NumberFormat(currentLocale(), {
    notation: 'compact',
    maximumFractionDigits: 2
  }).format(Number(v))
}

export function shortId(id: string, long: boolean = false): string {
  if (long) {
    return id.length > 28 ? id.slice(0, 14) + '...' + id.slice(-14) : id
  }
  return id.length > 14 ? id.slice(0, 7) + '...' + id.slice(-7) : id
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

export function pruneAddress(id: string, long?: boolean): string {
  if (long ?? window.innerWidth >= 640) {
    return id.length > 27 ? id.slice(0, 13) + '...' + id.slice(-11) : id
  }
  return id.length > 15 ? id.slice(0, 7) + '...' + id.slice(-5) : id
}
