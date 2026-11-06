import { daemonBlocked } from './connection-gate'
import { call, CallError } from './rpc'

export type Mode = 'local' | 'swr' | 'fresh'
export interface Snapshot<T> {
  state: 'missing' | 'present' | 'absent' | 'invalid'
  data: T
  observed_at_ms: number | null
  stale: boolean
  refreshing: boolean
  error: string | null
}

/** Metadata only; values and form state remain owned by their visible view. */
export const displays = $state<Record<string, Omit<Snapshot<never>, 'data'>>>({})
export function displayKey(method: string, params: Record<string, unknown> = {}) {
  return Object.keys(params).length ? `${method}:${JSON.stringify(Object.entries(params).sort(([a], [b]) => a.localeCompare(b)))}` : method
}
let generation = 0
const requests = new Map<string, number>()
export function clearDisplays() { generation++; requests.clear(); for (const key of Object.keys(displays)) delete displays[key] }

export async function display<T = any>(method: string, params: Record<string, unknown> = {}, mode: Mode = 'swr'): Promise<T> {
  const started = generation
  const key = displayKey(method, params), request = (requests.get(key) ?? 0) + 1
  requests.set(key, request)
  const snapshot = await call<Snapshot<T>>(method, { ...params, mode })
  if (started !== generation) throw new CallError('LOCKED: display session changed')
  const { data, ...meta } = snapshot
  if (requests.get(key) === request) displays[key] = meta
  if (snapshot.state === 'missing' || snapshot.state === 'invalid') throw new CallError(`${snapshot.error ?? 'CACHE_MISSING'}: display not available yet`)
  return data
}

/** Cache events reread only local data. Timed refreshes run only while visible. */
export function watchDisplays(kinds: string[], reload: (mode: Mode) => unknown, interval = 60_000) {
  const off = window.dmsg.onEvent((event) => {
    const kind = (event.data as { kind?: string } | undefined)?.kind
    if (event.type === 'cache/changed' && (kind === 'all' || (kind && kinds.includes(kind)))) void reload('local')
  })
  const timer = setInterval(() => { if (document.visibilityState === 'visible' && !daemonBlocked()) void reload('swr') }, interval)
  return () => { off(); clearInterval(timer) }
}
