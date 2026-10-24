import { failureLabel } from './failure'
import { locale, t } from './i18n/index.svelte'

export { single } from '../shared/single'

/** Calls through `window.dmsg`, with the daemon's error code split off. */
export class CallError extends Error {
  readonly code: string
  constructor(message: string) {
    const [code, ...rest] = message.split(': ')
    super(rest.join(': ') || code)
    this.code = /^[A-Z_]+$/.test(code) ? code : 'UNAVAILABLE'
  }
}

/** Bridge calls numbered as they start, and those under way: which work a click started, for its button. */
let started = 0
const running = new Set<number>()
const settled = new Set<() => void>()

export const startedCalls = () => started

/** Whether a call started after `mark`, a count from `startedCalls`, is still under way. */
export const runningSince = (mark: number) => [...running].some((id) => id > mark)

/** Hear each bridge call end. Returns the unsubscribe. */
export function onSettled(listener: () => void): () => void {
  settled.add(listener)
  return () => settled.delete(listener)
}

/** A bridge call other than `call`, such as a file dialog, with the daemon's error code split off. */
export async function bridge<T = any>(work: () => Promise<unknown>): Promise<T> {
  const id = ++started
  running.add(id)
  try {
    return (await work()) as T
  } catch (error) {
    throw new CallError(error instanceof Error ? error.message : String(error))
  } finally {
    running.delete(id)
    for (const listener of settled) listener()
  }
}

/**
 * A daemon method. The params cross the bridge as the JSON the daemon reads:
 * `$state` arrays and objects are proxies, which the bridge cannot clone.
 */
export const call = <T = any>(method: string, params?: unknown): Promise<T> =>
  bridge<T>(() => window.dmsg.call(method, params === undefined ? undefined : JSON.parse(JSON.stringify(params))))

export function code(error: unknown): string {
  return error instanceof CallError ? error.code : 'UNAVAILABLE'
}

/** The device cannot read yet: locked, or logged in without an opened account. */
export const closed = (error: unknown, status: any) =>
  code(error) === 'LOCKED' || (code(error) === 'RECOVERY_INCOMPLETE' && status?.state !== 'unlocked')

/**
 * A failure for people: locked, the payer's short balance, a service still
 * checking, a temporary connection problem, or an unconfirmed operation.
 * Raw transport and server messages stay out of the product interface.
 */
export const errorText = (error: unknown) => {
  const failure = code(error)
  const label = failureLabel(failure)
  return label ? t(label) : `${t('error')}: ${failure}`
}

/** A failure toast's options: one at the plan's limits offers the plan page. */
export const planAction = (error: unknown) => (code(error) === 'QUOTA_EXCEEDED' ? { action: { label: t('openPlan'), run: () => (location.hash = '/plan') } } : {})

/**
 * Reload on the daemon events that change what a view shows, and on
 * `resync`, which says events were missed. Returns the unsubscribe.
 */
export function onChange(types: string[], reload: () => unknown): () => void {
  return window.dmsg.onEvent((event) => {
    if (event.type === 'resync' || types.includes(event.type)) void reload()
  })
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

/** A size in bytes, as the interface language writes it, in decimal units (1 kB is 1000 bytes); the number and its unit stay on one line. */
export function size(bytes: number): string {
  const units = ['byte', 'kilobyte', 'megabyte', 'gigabyte', 'terabyte'] as const
  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit++
  }
  return new Intl.NumberFormat(locale(), { style: 'unit', unit: units[unit], unitDisplay: 'short', maximumFractionDigits: unit ? 1 : 0 }).format(value).replace(/ /g, '\u00a0')
}

/** An amount in base units as a decimal with `decimals` places. */
export function units(atomic: string, decimals: number): string {
  const digits = atomic.padStart(decimals + 1, '0')
  const whole = digits.slice(0, digits.length - decimals)
  const fraction = digits.slice(digits.length - decimals).replace(/0+$/, '')
  return fraction ? `${whole}.${fraction}` : whole
}
