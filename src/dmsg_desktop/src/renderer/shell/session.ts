import type { Glyph } from '../components/glyphs'
import { toast } from '../components/toast.svelte'
import { writable } from 'svelte/store'
import { t, type Label } from '../i18n/index.svelte'
import { call, code, errorText } from '../rpc'
import { closeLogin, setupStates, startLogin } from './login.svelte'

/**
 * The session actions the sidebar, the locked pages and the command palette
 * share. Their outcome reaches the windows as daemon events; a failure shows
 * as a notice, and a login follows its own dialog.
 */
async function attempt(work: () => Promise<unknown>) {
  try {
    await work()
  } catch (error) {
    toast(() => errorText(error))
  }
}

/** Drop the login and every key now, as the idle lock would. */
export const lock = () => attempt(() => call('session/lock'))

/** Forget the saved login too; opening this account again takes a browser login. */
export const logout = () =>
  attempt(async () => {
    await call('session/logout')
    closeLogin()
  })

/** Whether this lock has asked for its fast unlock; a new lock asks again. */
let offered = false

/** Shared by automatic unlock, the locked view and the account menu. */
export const unlocking = writable(false)
let opening: Promise<void> | null = null

function openOnce(work: () => Promise<unknown>): Promise<void> {
  if (opening) return opening
  unlocking.set(true)
  opening = Promise.resolve().then(work).then(() => {}).finally(() => {
    opening = null
    unlocking.set(false)
  })
  return opening
}

/**
 * Open the account again: fast unlock when it is on (the daemon asks for
 * Touch ID or the device password where the Secure Enclave keeps its key),
 * else a login in the system browser. Fast unlock that has run out (thirty
 * days after the last login) or lost its key leads to a login instead; a
 * declined Touch ID leaves the device locked.
 */
export function unlock(status: any) {
  if (status?.state === 'unlocked') return
  if (!status?.fast_unlock) return startLogin()
  // The system's question takes the focus; the window it returns to does not ask again.
  offered = true
  return openOnce(async () => {
    try {
      await call('session/unlock', { fast: true })
    } catch (error) {
      if (['AUTH_REQUIRED', 'NOT_FOUND'].includes(code(error))) return startLogin()
      if (code(error) !== 'FORBIDDEN') toast(() => errorText(error))
    }
  })
}

/**
 * A device locked with fast unlock that asks for the person offers it once,
 * when its window comes to the front; never right after the person locked
 * it in front of the window, and not again once it asked.
 */
export function offerUnlock(status: any) {
  if (opening || offered || status?.state !== 'locked' || !status.fast_presence) return
  void unlock(status)
}

/** A status that is not locked lets the next lock offer again. */
export function settleOffer(status: any) {
  // `offerUnlock` itself waits for `opening`, so an unlock under way must not
  // keep a later lock from offering.
  if (status && status.state !== 'locked') offered = false
}

/** The step that opens the device from its session state; null once unlocked. */
export function nextStep(status: any): { label: Label; icon: Glyph; run: () => unknown } | null {
  const state = status?.state
  if (state === 'logged_out') return { label: 'login', icon: 'login', run: () => startLogin() }
  if (state === 'locked') {
    const label: Label = !status.fast_unlock ? 'login' : status.fast_presence === 'biometrics' ? 'touchIdUnlock' : 'fastUnlock'
    return { label, icon: 'unlock', run: () => unlock(status) }
  }
  if (setupStates.includes(state)) return { label: 'continueSetup', icon: 'settings', run: () => (location.hash = '/settings/session') }
  return null
}

export const sync = () => attempt(() => call('sync/run'))

/** Another account's workspace opens locked; its fast unlock or a login opens it. */
export const switchAccount = (account: string, fast: boolean) =>
  openOnce(() => attempt(() => call(fast ? 'session/unlock' : 'session/switch', fast ? { fast: true, account } : { account })))

/** An account ID shortened for display; the full ID stays one copy away. */
export const shortId = (id: string) => (id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id)
