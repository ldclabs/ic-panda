/**
 * What windows may ask of the daemon. The desktop holds the owner
 * credential; a window only names a method. Decisions (`approval/respond`)
 * and browser grants carry `human: true`, which the main process adds only
 * for the approval window and the main window's confirmation; no window
 * sends it itself. `dmsg://` links only move the view (connect-v1 §8).
 */

/**
 * Methods any window may call; parameters pass through without `human`.
 * Methods that name local files (`wallet/use`, `channel/send_file`,
 * `channel/download`) and refunds to named blocks stay with the CLI.
 */
export const WINDOW_METHODS = new Set([
  'app/initialize',
  'daemon/status',
  'session/status',
  'session/login_result',
  'session/unlock',
  'session/lock',
  'session/fast_unlock',
  'account/show',
  'vault/put',
  'vault/edit',
  'vault/delete',
  'vault/restore',
  'vault/list',
  'vault/show',
  'vault/conflicts',
  'vault/resolve',
  'file/verify',
  'sync/run',
  'sync/status',
  'profile/show',
  'approval/list',
  'approval/show',
  'approval/cancel',
  'attest/request',
  'attest/history',
  'attest/export',
  'device/formal_approve',
  'device/capabilities',
  'connect/candidates',
  'connect/pairings',
  'connect/revoke',
  'connect/sync',
  'connect/status',
  'channel/list',
  'channel/show',
  'channel/create',
  'channel/invite',
  'channel/join',
  'channel/control',
  'channel/rotate',
  'channel/send',
  'channel/sync',
  'channel/messages',
  'channel/pending',
  'channel/resume',
  'inbox/policy',
  'inbox/configure',
  'inbox/send',
  'inbox/order',
  'inbox/outgoing',
  'inbox/pay',
  'inbox/escrow',
  'inbox/refund',
  'inbox/list',
  'inbox/mark',
  'wallet/balance',
  'plan/catalog',
  'plan/status',
  'plan/assets',
  'plan/quote',
  'plan/checkout',
  'plan/advance',
  'plan/order',
  'plan/checkouts',
  'plan/cancel',
  'plan/reconcile',
  'handle/resolve',
  'handle/price',
  'handle/claim',
  'handle/register'
])

/** Methods whose requests the desktop marks as its own (`source: "gui"`). */
const GUI_SOURCED = new Set([
  'attest/request',
  'device/formal_approve',
  'device/capabilities',
  'inbox/pay',
  'plan/checkout',
  'plan/advance',
  'handle/claim',
  'handle/register'
])

/** The parameters a window's call reaches the daemon with. */
export function windowParams(method: string, params: unknown): Record<string, unknown> {
  if (!WINDOW_METHODS.has(method)) throw new Error(`FORBIDDEN: ${method}`)
  const value =
    params && typeof params === 'object' && !Array.isArray(params) ? { ...(params as Record<string, unknown>) } : {}
  delete value.human
  if (GUI_SOURCED.has(method)) value.source = 'gui'
  return value
}

const HEX32 = /^[0-9a-f]{64}$/
const XID = /^[0-9a-v]{19}[0g]$/

/** A `dmsg://` link as navigation: where to go, never what to do. */
export type Link = { route: 'browsers'; account: string | null } | { route: 'request'; account: string; request: string }

export function parseLink(url: string): Link | null {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return null
  }
  if (parsed.protocol !== 'dmsg:' || parsed.host !== 'connect') return null
  const account = parsed.searchParams.get('account') ?? ''
  // A bridge window that does not know the account links to pairing without one.
  if (parsed.pathname === '/pair' && !parsed.searchParams.has('account')) return { route: 'browsers', account: null }
  if (!XID.test(account)) return null
  if (parsed.pathname === '/pair') return { route: 'browsers', account }
  const request = parsed.searchParams.get('request') ?? ''
  if (parsed.pathname === '/request' && HEX32.test(request)) return { route: 'request', account, request }
  return null
}

/** Pages the desktop opens in the system browser: the login page and the bridge's pairing page. */
export function externalAllowed(url: string, login: string[], pairing: string[]): boolean {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return false
  }
  const page = `${parsed.origin}${parsed.pathname}`
  return login.includes(page) || pairing.some((p) => page === `${p}/v1/pair`)
}
