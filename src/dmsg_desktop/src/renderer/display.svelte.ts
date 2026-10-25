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
let generation = 0
export function clearDisplays() { generation++; for (const key of Object.keys(displays)) delete displays[key] }

export async function display<T = any>(method: string, params: Record<string, unknown> = {}, mode: Mode = 'swr'): Promise<T> {
  const started = generation
  const snapshot = await call<Snapshot<T>>(method, { ...params, mode })
  if (started !== generation) throw new CallError('LOCKED: display session changed')
  const { data, ...meta } = snapshot
  displays[method] = meta
  if (snapshot.state === 'missing' || snapshot.state === 'invalid') throw new CallError(`${snapshot.error ?? 'CACHE_MISSING'}: display not available yet`)
  return data
}

/** Cache events reread only local data. Timed refreshes run only while visible. */
export function watchDisplays(kinds: string[], reload: (mode: Mode) => unknown, interval = 60_000) {
  const off = window.dmsg.onEvent((event) => {
    const kind = (event.data as { kind?: string } | undefined)?.kind
    if (event.type === 'cache/changed' && (kind === 'all' || (kind && kinds.includes(kind)))) void reload('local')
  })
  const timer = setInterval(() => { if (document.visibilityState === 'visible' && !document.body.inert) void reload('swr') }, interval)
  return () => { off(); clearInterval(timer) }
}
