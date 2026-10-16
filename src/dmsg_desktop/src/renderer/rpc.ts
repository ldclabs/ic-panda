import { locale } from './i18n/index.svelte'

/** Calls through `window.dmsg`, with the daemon's error code split off. */
export class CallError extends Error {
  readonly code: string
  constructor(message: string) {
    const [code, ...rest] = message.split(': ')
    super(rest.join(': ') || code)
    this.code = /^[A-Z_]+$/.test(code) ? code : 'UNAVAILABLE'
  }
}

/** A bridge call other than `call`, such as a file dialog, with the daemon's error code split off. */
export async function bridge<T = any>(work: () => Promise<unknown>): Promise<T> {
  try {
    return (await work()) as T
  } catch (error) {
    throw new CallError(error instanceof Error ? error.message : String(error))
  }
}

export const call = <T = any>(method: string, params?: unknown): Promise<T> => bridge<T>(() => window.dmsg.call(method, params))

export function code(error: unknown): string {
  return error instanceof CallError ? error.code : 'UNAVAILABLE'
}

/** A time in the interface language. */
export const when = (ms?: number | null) => (ms ? new Date(ms).toLocaleString(locale()) : '-')

/** How long ago, in the interface language: minutes and hours within a day, the date after a week. */
export function ago(ms?: number | null, now = Date.now()): string {
  if (!ms) return '-'
  const seconds = Math.round((ms - now) / 1000)
  const relative = new Intl.RelativeTimeFormat(locale(), { numeric: 'auto', style: 'short' })
  if (Math.abs(seconds) < 60) return relative.format(0, 'second')
  if (Math.abs(seconds) < 3600) return relative.format(Math.round(seconds / 60), 'minute')
  if (Math.abs(seconds) < 86_400) return relative.format(Math.round(seconds / 3600), 'hour')
  if (Math.abs(seconds) < 7 * 86_400) return relative.format(Math.round(seconds / 86_400), 'day')
  return new Date(ms).toLocaleDateString(locale(), { month: 'short', day: 'numeric', year: new Date(ms).getFullYear() === new Date(now).getFullYear() ? undefined : 'numeric' })
}

/** A size in bytes, as the interface language writes it, in decimal units (1 kB is 1000 bytes). */
export function size(bytes: number): string {
  const units = ['byte', 'kilobyte', 'megabyte', 'gigabyte', 'terabyte'] as const
  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit++
  }
  return new Intl.NumberFormat(locale(), { style: 'unit', unit: units[unit], unitDisplay: 'short', maximumFractionDigits: unit ? 1 : 0 }).format(value)
}

/** An amount in base units as a decimal with `decimals` places. */
export function units(atomic: string, decimals: number): string {
  const digits = atomic.padStart(decimals + 1, '0')
  const whole = digits.slice(0, digits.length - decimals)
  const fraction = digits.slice(digits.length - decimals).replace(/0+$/, '')
  return fraction ? `${whole}.${fraction}` : whole
}
