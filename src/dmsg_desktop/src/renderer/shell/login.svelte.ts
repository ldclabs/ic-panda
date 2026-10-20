import { toast } from '../components/toast.svelte'
import { t, type Label } from '../i18n/index.svelte'
import { bridge, call, CallError, code } from '../rpc'

/**
 * The login, wherever it starts: the daemon opens the login page in the
 * system browser and `LoginDialog.svelte` follows it until the daemon has
 * checked what the browser returned. It reads `session/login_result` on
 * `login/changed` and `session/changed`, never on a timer, except once when
 * the page's time runs out. A failure stays in the dialog with its reason
 * and a way to start again; a binding login opens its approval request.
 */
type Phase = 'idle' | 'starting' | 'browser' | 'checking' | 'failed'

export const login = $state({
  phase: 'idle' as Phase,
  bind: false,
  /** The derivation origin: https://dmsg.net, or https://panda.fans for an identity of the old site. */
  origin: 'https://dmsg.net',
  url: '',
  expiresAt: 0,
  /** Why it failed, as a label so it follows the language. */
  reason: 'loginFailedHint' as Label,
  /** The daemon's code and message, for someone looking into it. */
  detail: ''
})

/** States in which this device still has to create, join or open an account. */
export const setupStates = ['no_account', 'pairing_required', 'root_required']

let run = 0
let expiry: ReturnType<typeof setTimeout> | undefined

/**
 * Start a login, or with `bind` a second login to bind to the unlocked
 * account. `origin` https://panda.fans logs in with an identity of the old
 * site, such as the holder of a frozen name.
 */
export async function startLogin(bind = false, origin = 'https://dmsg.net') {
  const id = ++run
  clearTimeout(expiry)
  Object.assign(login, { phase: 'starting', bind, origin, url: '', expiresAt: 0, reason: 'loginFailedHint', detail: '' })
  try {
    const started = await bridge<{ url: string; expires_at: number }>(() => window.dmsg.login(origin, bind))
    if (id !== run) return
    Object.assign(login, { phase: 'browser', url: started.url, expiresAt: started.expires_at })
    expiry = setTimeout(() => void checkLogin(), Math.max(0, started.expires_at - Date.now()) + 1000)
  } catch (error) {
    if (id === run) fail(code(error), error instanceof CallError ? error.message : String(error))
  }
}

/** Close the dialog. A login finished in the browser later still opens the device. */
export function closeLogin() {
  run++
  clearTimeout(expiry)
  login.phase = 'idle'
}

/** The login page again, for a browser tab closed before the login finished. */
export async function reopenLogin() {
  try {
    await bridge(() => window.dmsg.openLogin(login.url))
  } catch (error) {
    toast(() => `${t('error')}: ${code(error)}`)
  }
}

/** Where the login stands, after a daemon event or once the page expired. */
export async function checkLogin() {
  if (login.phase !== 'browser' && login.phase !== 'checking') return
  const id = run
  let result: any
  try {
    result = await call('session/login_result')
  } catch (error) {
    if (id === run) fail(code(error), error instanceof CallError ? error.message : String(error))
    return
  }
  if (id !== run) return
  if (result.checking) {
    login.phase = 'checking'
    return
  }
  if (result.pending) return
  if (result.error) return fail(result.error.code, result.error.message)
  if (!result.done) return fail('EXPIRED')
  const bind = login.bind
  closeLogin()
  if (bind) {
    if (result.approval) await bridge(() => window.dmsg.openApproval(result.approval.id)).catch(() => {})
    return
  }
  toast(() => t('loggedIn'))
  if (setupStates.includes(result.status?.state)) location.hash = '/settings/session'
}

const reasons: Record<string, Label> = { UNAVAILABLE: 'loginUnavailable', EXPIRED: 'loginExpired', FORBIDDEN: 'loginRefused' }

function fail(failure: string, message = '') {
  clearTimeout(expiry)
  // The login page's own errors reach the daemon as `login: <code>`.
  const page = /(?:^|: )login: (\w+)$/.exec(message)?.[1]
  Object.assign(login, {
    phase: 'failed',
    reason: page ? (page === 'user_cancelled' ? 'loginCancelled' : 'loginPageFailed') : (reasons[failure] ?? 'loginFailedHint'),
    detail: message
  })
}
