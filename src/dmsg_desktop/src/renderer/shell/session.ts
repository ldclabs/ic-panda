import type { Glyph } from '../components/glyphs'
import { toast } from '../components/toast.svelte'
import { t, type Label } from '../i18n/index.svelte'
import { call, code } from '../rpc'
import { setupStates, startLogin } from './login.svelte'

/**
 * The session actions the sidebar, the locked pages and the command palette
 * share. Their outcome reaches the windows as daemon events; a failure shows
 * as a notice, and a login follows its own dialog.
 */
async function attempt(work: () => Promise<unknown>) {
  try {
    await work()
  } catch (error) {
    toast(() => `${t('error')}: ${code(error)}`)
  }
}

/** Drop the login and every key now, as the idle lock would. */
export const lock = () => attempt(() => call('session/lock'))

/**
 * Open the account again: the OS keychain when fast unlock is on, else a
 * login in the system browser. Fast unlock that has run out (seven days
 * after the last login) or lost its key leads to a login instead.
 */
export async function unlock(status: any) {
  if (!status?.fast_unlock) return startLogin()
  try {
    await call('session/unlock', { fast: true })
  } catch (error) {
    if (['AUTH_REQUIRED', 'NOT_FOUND'].includes(code(error))) return startLogin()
    toast(() => `${t('error')}: ${code(error)}`)
  }
}

/** The step that opens the device from its session state; null once unlocked. */
export function nextStep(status: any): { label: Label; icon: Glyph; run: () => unknown } | null {
  const state = status?.state
  if (state === 'logged_out') return { label: 'login', icon: 'login', run: () => startLogin() }
  if (state === 'locked') return { label: status.fast_unlock ? 'fastUnlock' : 'login', icon: 'unlock', run: () => unlock(status) }
  if (setupStates.includes(state)) return { label: 'continueSetup', icon: 'settings', run: () => (location.hash = '/settings/session') }
  return null
}

export const sync = () => attempt(() => call('sync/run'))

/** Another account's workspace opens locked; its fast unlock or a login opens it. */
export const switchAccount = (account: string, fast: boolean) =>
  attempt(() => call(fast ? 'session/unlock' : 'session/switch', fast ? { fast: true, account } : { account }))

/** An account ID shortened for display; the full ID stays one copy away. */
export const shortId = (id: string) => (id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id)
