import { onMount } from 'svelte'
import { call, errorText, code, onChange, single } from '../../rpc'

import { ux } from './experience-text'
export class InputError extends Error {}
const message = (e: unknown) =>
  e instanceof InputError
    ? e.message
    : code(e) === 'INTEGRITY_FAILED'
      ? ux('integrity')
      : code(e) === 'VERSION_CONFLICT'
        ? ux('sourceChanged')
        : errorText(e)

/** One visible workflow owns its reads, errors and lifetime. Independent reads fail independently. */
export function panel(
  load: () => Promise<unknown>,
  poll: () => boolean = () => false
) {
  const state = $state({
    alive: true,
    loading: false,
    busy: false,
    loaded: false,
    errors: {} as Record<string, string>
  })
  const retries = new Map<string, () => Promise<void>>()
  const refresh = single(async () => {
    if (!state.alive) return
    state.loading = true
    delete state.errors.action
    try {
      await load()
    } finally {
      if (state.alive) {
        state.loading = false
        state.loaded = true
      }
    }
  })
  async function read<T>(
    key: string,
    task: () => Promise<T>,
    apply: (value: T) => void
  ) {
    retries.set(key, () => read(key, task, apply))
    try {
      const value = await task()
      if (state.alive) {
        delete state.errors[key]
        apply(value)
      }
    } catch (e) {
      if (state.alive) state.errors[key] = message(e)
    }
  }
  async function act<T>(
    task: () => Promise<T>,
    apply?: (value: T) => unknown | Promise<unknown>,
    reload = true
  ) {
    if (state.busy || !state.alive) return
    state.busy = true
    delete state.errors.action
    try {
      const value = await task()
      if (!state.alive) return
      await apply?.(value)
      if (reload && state.alive) await refresh()
      return value
    } catch (e) {
      if (state.alive) state.errors.action = message(e)
    } finally {
      if (state.alive) state.busy = false
    }
  }
  onMount(() => {
    void refresh()
    const off = onChange(
      ['approval/changed', 'vault/changed', 'files/changed', 'agent/changed'],
      refresh
    )
    const timer = setInterval(() => {
      if (!state.busy && poll()) void refresh()
    }, 2000)
    return () => {
      state.alive = false
      off()
      clearInterval(timer)
      retries.clear()
    }
  })
  return {
    state,
    refresh,
    read,
    act,
    retry: (key: string) =>
      act(async () => retries.get(key)?.(), undefined, false),
    request: <T = any>(
      key: string,
      method: string,
      params: any,
      apply: (value: T) => void
    ) => read(key, () => call<T>(method, params), apply)
  }
}
export type Panel = ReturnType<typeof panel>
