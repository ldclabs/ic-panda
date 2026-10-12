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
 * `channel/download`), refunds to named blocks and running a tool as an
 * agent (`ap/call`) stay with the CLI; files enter and leave the vault only
 * through the main process's file dialogs, as do legacy transfer files,
 * frozen-history files and old attachments. The agent methods (`mcp/*`)
 * answer only an agent's own credential.
 */
export const WINDOW_METHODS = new Set([
  'app/initialize',
  'daemon/status',
  'session/status',
  'session/login_result',
  'session/accounts',
  'session/switch',
  'session/unlock',
  'session/lock',
  'session/fast_unlock',
  'account/create',
  'account/show',
  'account/security',
  'account/bind_request',
  'account/bind_accept',
  'account/bind_approve',
  'account/unbind',
  'account/policy',
  'account/recovery_delay',
  'device/pair_request',
  'device/approve',
  'recovery/status',
  'recovery/request',
  'recovery/complete',
  'recovery/dispute',
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
  'profile/publish',
  'profile/resolve',
  'approval/list',
  'approval/show',
  'approval/cancel',
  'approval/quote',
  'attest/request',
  'attest/history',
  'attest/export',
  'attest/usage',
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
  'payment/transfers',
  'payment/revise',
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
  'handle/register',
  'handle/transfer',
  'handle/transfer_commit',
  'handle/accept',
  'handle/operations',
  'handle/reconcile',
  'ops/orders',
  'ops/transfers',
  'ops/claims',
  'ops/reconcile_order',
  'ops/process_transfer',
  'ops/revise_transfer',
  'ops/claim',
  'ops/sweep',
  'agent/principal',
  'agent/enable',
  'agent/register',
  'agent/retire',
  'agent/compromise',
  'agent/rename',
  'agent/publish',
  'agent/refresh',
  'agent/grant',
  'agent/revoke',
  'agent/credentials',
  'agent/delegations',
  'agent/ids',
  'agent/create',
  'agent/use',
  'agent/resident',
  'agent/credential',
  'grants/list',
  'grants/add',
  'grants/revoke',
  'legacy/pair',
  'legacy/pairs',
  'legacy/cancel',
  'legacy/list',
  'legacy/report',
  'legacy/shared_source',
  'legacy/shared_prepare',
  'legacy/shared_check',
  'legacy/shared_request',
  'legacy/shared_propose',
  'legacy/shared_consent',
  'legacy/shared_view',
  'legacy/shared_commit',
  'legacy/shared_activate',
  'legacy/shared_claim_request',
  'legacy/shared_claim',
  'legacy/shared_joined',
  'legacy/shared_grant',
  'legacy/shared_receive'
])

/** Methods whose requests the desktop marks as its own (`source: "gui"`). */
const GUI_SOURCED = new Set([
  'account/bind_approve',
  'account/unbind',
  'account/policy',
  'account/recovery_delay',
  'recovery/dispute',
  'attest/request',
  'device/formal_approve',
  'device/capabilities',
  'inbox/pay',
  'plan/checkout',
  'plan/advance',
  'handle/claim',
  'handle/register',
  'handle/transfer_commit',
  'handle/accept',
  'agent/enable',
  'agent/register',
  'agent/retire',
  'agent/compromise',
  'agent/rename',
  'agent/grant',
  'agent/revoke'
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
/** The old site's migration pages; their links carry an offer or a request in the fragment. */
const LEGACY_PAGES = ['https://dmsg.net/legacy', 'https://panda.fans/legacy']

export function legacyAllowed(url: string): boolean {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return false
  }
  return (
    LEGACY_PAGES.includes(`${parsed.origin}${parsed.pathname}`) &&
    !parsed.search &&
    /^#(pair|approve)=[A-Za-z0-9_-]+$/.test(parsed.hash)
  )
}

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
