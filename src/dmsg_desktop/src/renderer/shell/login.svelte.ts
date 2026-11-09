import { toast } from '../components/toast.svelte'
import { t, type Label } from '../i18n/index.svelte'
import { bridge, call, CallError, code, errorText } from '../rpc'
import { failureLabel } from '../failure'

/**
 * The login, wherever it starts: the daemon opens the login page in the
 * system browser and `LoginDialog.svelte` follows it until the daemon has
 * checked what the browser returned. It reads `session/login_result` on
 * `login/changed` and `session/changed`, never on a timer, except once when
 * the page's time runs out. A failure stays in the dialog with its reason
 * and a way to start again; a binding login opens its approval request.
 */
type Phase = 'idle' | 'starting' | 'choose' | 'browser' | 'checking' | 'failed'

export const login = $state({
  phase: 'idle' as Phase,
  bind: false,
  /** The derivation origin: https://dmsg.net, or https://panda.fans for an identity of the old site. */
  origin: 'https://dmsg.net',
  provider: undefined as 'ii' | 'passkey' | undefined,
  action: 'login' as 'login' | 'register' | 'logout_all',
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

/** Bind a saved credential or explicitly create another identity for this account. */
export function choosePasskeyBinding() {
  run++
  clearTimeout(expiry)
  Object.assign(login, { phase: 'choose', bind: true, origin: 'https://dmsg.net', provider: 'passkey', action: 'login', url: '', expiresAt: 0, reason: 'loginFailedHint', detail: '' })
}

/**
 * Start a login, or with `bind` a second login to bind to the unlocked
 * account. `origin` https://panda.fans logs in with an identity of the old
 * site, such as the holder of a frozen name.
 */
export async function startLogin(bind = false, origin = 'https://dmsg.net', provider?: 'ii' | 'passkey', action: 'login' | 'register' | 'logout_all' = bind ? 'register' : 'login') {
  const id = ++run
  clearTimeout(expiry)
  Object.assign(login, { phase: 'starting', bind, origin, provider, action, url: '', expiresAt: 0, reason: 'loginFailedHint', detail: '' })
  try {
    // Choosing a passkey action never creates a hardware key or opens a browser.
    // Explicit providers, including account binding, keep their existing flow.
    if (!provider && !bind && origin === 'https://dmsg.net' && action === 'login') {
      const status: any = await call('daemon/status')
      if (id !== run) return
      if (status.passkey_available) {
        Object.assign(login, { phase: 'choose', provider: 'passkey' })
        return
      }
    }
    const started = await bridge(() => window.dmsg.login(origin, bind, { provider, action }))
    if (id !== run) return
    Object.assign(login, { phase: 'browser', provider: started.provider ?? 'ii', url: started.url, expiresAt: started.expires_at })
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
    toast(() => errorText(error))
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
  const action = login.action
  closeLogin()
  if (bind) {
    if (result.approval) await bridge(() => window.dmsg.openApproval(result.approval.id)).catch(() => {})
    return
  }
  toast(() => t(action === 'logout_all' ? 'loggedOut' : 'loggedIn'))
  if (setupStates.includes(result.status?.state)) location.hash = '/settings/session'
}

const reasons: Record<string, Label> = { UNAVAILABLE: 'loginUnavailable', EXPIRED: 'loginExpired', FORBIDDEN: 'loginRefused' }

function fail(failure: string, message = '') {
  clearTimeout(expiry)
  // The login page's own errors reach the daemon as `login: <code>`.
  const page = /(?:^|: )login: (\w+)$/.exec(message)?.[1]
  let reason: Label = page ? (page === 'user_cancelled' ? 'loginCancelled' : login.provider === 'passkey' ? 'loginFailedHint' : 'loginPageFailed') : (failureLabel(failure) ?? reasons[failure] ?? 'loginFailedHint')
  if (!page && login.provider === 'passkey') {
    if (login.action === 'login' && failure === 'NOT_FOUND') reason = 'passkeyNotRegistered'
    else if (login.action === 'register' && login.url) reason = 'passkeyRegistrationUnconfirmed'
    else if (login.action === 'register' && failure === 'EXECUTION_UNKNOWN') reason = 'loginUnavailable'
  }
  Object.assign(login, {
    phase: 'failed',
    reason,
    detail: /^[A-Z_]+$/.test(failure) ? failure : ''
  })
}
