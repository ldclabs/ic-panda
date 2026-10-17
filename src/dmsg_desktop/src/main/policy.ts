/**
 * What windows may ask of the daemon. The desktop holds the owner
 * credential; a window only names a method. Decisions (`approval/respond`)
 * and browser grants carry `human: true`, which the main process adds only
 * for the approval window and the main window's confirmation; no window
 * sends it itself. `dmsg://` links only move the view (connect-v1 §8).
 */
import { DaemonError } from './error.ts'

/**
 * Methods any window may call, and only those its views use; parameters
 * pass through without `human`.
 * Methods that name local files (`payer/use`, `channel/download`,
 * `attest/verify` with a file) and refunds to named blocks stay with the CLI
 * (`wallet/refund` names only a quote, which the daemon returns to the payer);
 * files enter and leave the library only through the main process's file
 * dialogs or a drop the preload resolves to a path (`files/import`,
 * `files/export`, `files/open`), as do shared channel files, application
 * actions, attestation bundles, legacy transfer files, frozen-history files
 * and old attachments.
 * `ap/call` runs only the Mail and Knowledge tools (`DESKTOP_TOOLS`) as a
 * hosted Agent ID; the MCP Events methods only manage endpoints, never read
 * or acknowledge deliveries. The agent methods (`mcp/*`) answer only an
 * agent's own credential. The `app/*` methods are the main process's, which
 * declares at `app/initialize` that only a person's input is activity.
 */
export const WINDOW_METHODS = new Set([
  'daemon/status',
  'session/login_result',
  'session/accounts',
  'session/switch',
  'session/unlock',
  'session/lock',
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
  'device/revoke',
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
  'files/list',
  'files/tree',
  'files/show',
  'files/search',
  'files/mkdir',
  'files/rename',
  'files/move',
  'files/update',
  'files/preview',
  'files/open_cleanup',
  'files/version_use',
  'files/version_delete',
  'files/trash',
  'files/restore',
  'files/purge',
  'files/verify',
  'files/usage',
  'files/share_channel',
  'files/save_shared',
  'files/backup',
  'files/backup_status',
  'files/backup_cancel',
  'files/cloud_list',
  'files/cloud_refresh',
  'files/cloud_fetch',
  'files/cloud_delete',
  'files/cloud_verify',
  'files/cloud_usage',
  'files/evict',
  'files/mount',
  'files/unmount',
  'files/mount_status',
  'files/share_link',
  'files/share_links',
  'files/share_revoke',
  'sync/run',
  'profile/show',
  'profile/publish',
  'profile/resolve',
  'profile/avatar_image',
  'approval/list',
  'approval/show',
  'approval/quote',
  'attest/request',
  'attest/history',
  'attest/usage',
  'device/formal_approve',
  'device/capabilities',
  'connect/candidates',
  'connect/pairings',
  'connect/revoke',
  'channel/list',
  'channel/show',
  'channel/create',
  'channel/invite',
  'channel/join',
  'channel/control',
  'channel/leave',
  'channel/rotate',
  'channel/send',
  'channel/send_file',
  'channel/sync',
  'channel/messages',
  'channel/history',
  'channel/owner_propose',
  'channel/owner_accept',
  'channel/owner_commit',
  'channel/sponsor_propose',
  'channel/sponsor_accept',
  'channel/sponsor_commit',
  'channel/reactivate',
  'inbox/policy',
  'inbox/configure',
  'inbox/send',
  'inbox/order',
  'inbox/outgoing',
  'inbox/pay',
  'inbox/escrow',
  'inbox/list',
  'inbox/mark',
  'payment/revise',
  'payer/balance',
  'wallet/list',
  'wallet/show',
  'wallet/principal',
  'wallet/quote',
  'wallet/create',
  'wallet/deposits',
  'wallet/refund',
  'wallet/join',
  'wallet/proposals',
  'wallet/send',
  'wallet/approve',
  'wallet/reject',
  'wallet/veto',
  'wallet/cancel',
  'wallet/freeze',
  'wallet/execute',
  'wallet/broadcast',
  'wallet/settle',
  'wallet/balances',
  'wallet/audit',
  'wallet/cosigners',
  'wallet/cosigner_new',
  'plan/catalog',
  'plan/status',
  'plan/assets',
  'plan/quote',
  'plan/checkout',
  'plan/order',
  'plan/checkouts',
  'plan/cancel',
  'plan/reconcile',
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
  'agent/publish',
  'agent/grant',
  'agent/revoke',
  'agent/credentials',
  'agent/ids',
  'agent/create',
  'agent/use',
  'agent/resident',
  'agent/credential',
  'grants/list',
  'grants/add',
  'grants/revoke',
  'ap/call',
  'audit/list',
  'mcp-events/create',
  'mcp-events/list',
  'mcp-events/rotate',
  'mcp-events/delete',
  'mcp-events/status',
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
  'device/revoke',
  'inbox/pay',
  'plan/checkout',
  'handle/claim',
  'handle/register',
  'handle/transfer_commit',
  'handle/accept',
  'agent/enable',
  'agent/register',
  'agent/retire',
  'agent/grant',
  'agent/revoke',
  'agent/create',
  'agent/resident',
  'agent/credential',
  'grants/add',
  'grants/revoke',
  'ap/call',
  'wallet/create',
  'wallet/join',
  'wallet/approve',
  'wallet/reject',
  'wallet/veto',
  'wallet/execute'
])

/** The tools the Agents view runs as a hosted Agent ID: Agent Mail and Agent Knowledge. */
export const DESKTOP_TOOLS = new Set([
  'agent_protocols_mail_mailboxes_list',
  'agent_protocols_mail_mailbox_publish',
  'agent_protocols_mail_send',
  'agent_protocols_mail_sync',
  'agent_protocols_mail_list',
  'agent_protocols_knowledge_submit',
  'agent_protocols_knowledge_query',
  'agent_protocols_knowledge_get',
  'agent_protocols_knowledge_search',
  'agent_protocols_knowledge_status'
])

/** The parameters a window's call reaches the daemon with. */
export function windowParams(method: string, params: unknown): Record<string, unknown> {
  if (!WINDOW_METHODS.has(method)) throw new DaemonError('FORBIDDEN', method)
  const value =
    params && typeof params === 'object' && !Array.isArray(params) ? { ...(params as Record<string, unknown>) } : {}
  if (method === 'ap/call' && !DESKTOP_TOOLS.has(String(value.tool))) throw new DaemonError('FORBIDDEN', String(value.tool))
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
