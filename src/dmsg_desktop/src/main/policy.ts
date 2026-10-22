/**
 * What windows may ask of the daemon. The desktop holds the owner
 * credential; a window only names a method. Decisions (`approval/respond`)
 * and browser grants carry `human: true`, which the main process adds only
 * for the approval window and the main window's confirmation; no window
 * sends it itself. `dmsg://` links only move the view (connect-v1 §8).
 */
import { validPrincipal } from '../shared/principal.ts'
import { WALLET_CREATION_PAUSED } from '../shared/wallet-availability.ts'
import { DaemonError } from './error.ts'

/**
 * Methods any window may call, and only those its views use; parameters
 * pass through without `human`.
 * Methods that name local files (`payer/use`, `channel/download`,
 * `attest/verify`, `approval/check_file`, the `file` of `attest/request`,
 * which a window's call loses; it names a library version instead) and
 * refunds to named blocks stay with the CLI
 * (`wallet/refund` names only a quote, which the daemon returns to the payer);
 * files enter and leave the library only through the main process's file
 * dialogs or a drop the preload resolves to a path (`files/import`,
 * `files/export`, `files/open`), as do shared channel files, application
 * actions, attestation bundles, legacy transfer files, frozen-history files
 * and old attachments. Vault imports (`vault/import_preview`, `vault/import`)
 * read only the export the person picked in the main process's dialog, and
 * QR codes for one-time passwords (`vault/otp_scan`) come only from a picked
 * image or the clipboard. The file library's settings change only through the
 * main process, which writes `preferences.json` (allowing the disk mount
 * after a system dialog) and has the daemon read it
 * (`files/preferences_reload`).
 * `ap/call` runs only the Mail and Knowledge tools (`DESKTOP_TOOLS`) as a
 * hosted Agent ID; `ap/read` shows what an Agent ID's connector holds
 * (rooms, timelines, held drafts) without fetching or saving anything. The
 * MCP Events methods only manage endpoints, never read or acknowledge
 * deliveries. The agent methods (`mcp/*`) answer only an
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
  'session/logout',
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
  'device/revoke',
  'recovery/status',
  'recovery/request',
  'recovery/complete',
  'recovery/dispute',
  'vault/templates',
  'vault/create',
  'vault/edit',
  'vault/duplicate',
  'vault/delete',
  'vault/restore',
  'vault/tags',
  'vault/list',
  'vault/show',
  'vault/reveal',
  'vault/history',
  'vault/revert',
  'vault/conflicts',
  'vault/resolve',
  'vault/otp',
  'vault/ssh_keygen',
  'vault/health',
  'files/list',
  'files/tree',
  'files/show',
  'files/search',
  'files/tags',
  'files/mkdir',
  'files/rename',
  'files/move',
  'files/update',
  'files/preview',
  'files/open_list',
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
  'files/preferences',
  'files/import_cancel',
  'files/share_link',
  'files/share_links',
  'files/share_revoke',
  'sync/run',
  'profile/show',
  'profile/publish',
  'profile/retry',
  'profile/resolve',
  'profile/avatar_image',
  'profile/avatar_legacy',
  'approval/list',
  'approval/show',
  'approval/quote',
  'approval/cancel',
  'approval/reconcile',
  'attest/request',
  'attest/readiness',
  'attest/history',
  'attest/usage',
  'device/formal_approve',
  'device/capabilities',
  'connect/candidates',
  'connect/pairings',
  'connect/revoke',
  'channel/list',
  'channel/show',
  'channel/delete_legacy_draft',
  'channel/create',
  'channel/invite',
  'channel/join',
  'channel/control',
  'channel/leave',
  'channel/rotate',
  'channel/rekey',
  'channel/outbox',
  'channel/discard',
  'channel/mark',
  'channel/draft',
  'channel/settings',
  'channel/search',
  'channel/hide',
  'channel/preview',
  'channel/sponsor_cancel',
  'channel/names',
  'channel/receipts',
  'handle/resolve',
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
  'wallet/activation_continue',
  'wallet/activation',
  'wallet/assets',
  'wallet/book',
  'wallet/readiness',
  'wallet/preview',
  'wallet/transfer',
  'wallet/change',
  'wallet/receive',
  'wallet/versions',
  'wallet/health',
  'wallet/upgrade',
  'wallet/lock_code',
  'wallet/rpc',
  'wallet/rpc_set',
  'wallet/rpc_check',
  'wallet/audit_page',
  'wallet/cosign',
  'wallet/cosign_status',
  'wallet/cosign_import',
  'wallet/cosign_execute',
  'wallet/channel',
  'wallet/notify',
  'wallet/proposal',
  'wallet/refuel',
  'wallet/reconcile',
  'wallet/list',
  'wallet/show',
  'wallet/principal',
  ...(!WALLET_CREATION_PAUSED ? ['wallet/quote', 'wallet/quote_preview', 'wallet/create'] : []),
  'wallet/deposits',
  'wallet/refund',
  'wallet/join',
  'wallet/proposals',
  'wallet/member_agents',
  'wallet/send',
  'wallet/approve',
  'wallet/reject',
  'wallet/veto',
  'wallet/cancel',
  'wallet/freeze',
  'wallet/execute',
  'wallet/prepare',
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
  'plan/advance',
  'plan/order',
  'plan/checkouts',
  'plan/discard',
  'plan/cancel',
  'plan/reconcile',
  'plan/refund_quote',
  'plan/panda',
  'plan/notice_ack',
  'payer/status',
  'handle/price',
  'handle/mine',
  'handle/claim',
  'handle/register',
  'handle/transfer',
  'handle/transfer_commit',
  'handle/accept',
  'handle/operations',
  'handle/token',
  'handle/forget',
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
  'agent/credential_revoke',
  'agent/pause',
  'agent/remove',
  'agent/profile',
  'grants/list',
  'grants/add',
  'grants/revoke',
  'ap/call',
  'ap/read',
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
  'legacy/channels',
  'legacy/delete_channel',
  'legacy/report',
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
  'agent/revoke',
  'agent/create',
  'agent/resident',
  'agent/credential',
  'agent/credential_revoke',
  'agent/pause',
  'agent/remove',
  'grants/add',
  'grants/revoke',
  'ap/call',
  'ap/read',
  'wallet/create',
  'wallet/join',
  'wallet/approve',
  'wallet/reject',
  'wallet/veto',
  'wallet/execute',
  'wallet/cosign',
  'wallet/activation_continue'
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
  // A local file reaches a statement only through the main process's dialog.
  if (method === 'attest/request') delete value.file
  if (GUI_SOURCED.has(method)) value.source = 'gui'
  return value
}

const HEX32 = /^[0-9a-f]{64}$/
const XID = /^[0-9a-v]{19}[0g]$/

/** A `dmsg://` link as navigation: where to go, never what to do. */
export type Link =
  | { route: 'browsers'; account: string | null }
  | { route: 'request'; account: string; request: string }
  | { route: 'join'; invitation: string }
  | { route: 'accept'; token: string }
  | { route: 'wallet'; wallet: string; proposal?: number }

export function parseLink(url: string): Link | null {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return null
  }
  if(parsed.protocol==='dmsg:'&&parsed.host==='wallet'&&!parsed.search&&!parsed.hash){
    const parts=parsed.pathname.split('/').filter(Boolean)
    if(parts.length<1||parts.length>2||!validPrincipal(parts[0]))return null
    const proposal=parts[1]===undefined?undefined:Number(parts[1])
    return proposal===undefined || (/^\d+$/.test(parts[1])&&Number.isSafeInteger(proposal)) ? {route:'wallet',wallet:parts[0],proposal}:null
  }
  // A channel invitation opens the join dialog with it; joining is still the person's click.
  if (parsed.protocol === 'dmsg:' && parsed.host === 'channel' && parsed.pathname === '/join') {
    const invitation = parsed.hash.slice(1)
    return /^[A-Za-z0-9_-]{16,8192}$/.test(invitation) ? { route: 'join', invitation } : null
  }
  // A name transfer's token opens the accept dialog with it; accepting is still an approval.
  if (parsed.protocol === 'dmsg:' && parsed.host === 'handle' && parsed.pathname === '/accept') {
    const token = parsed.hash.slice(1)
    return /^[A-Za-z0-9_-]{16,8192}$/.test(token) ? { route: 'accept', token } : null
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

/** The old site's migration pages; their links carry an offer in the fragment. */
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
    /^#pair=[A-Za-z0-9_-]+$/.test(parsed.hash)
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

/** The days before a term ends when the app reminds, once each. */
export const REMIND_DAYS = [30, 7, 1]

/** Exact destinations an approval window may open in the main window. */
export const APPROVAL_MAIN_ROUTES = new Set([
  '/login', '/approvals', '/settings/session', '/settings/security', '/settings/devices', '/plan', '/plan?tab=payments'
])

/**
 * The reminder due when `left` milliseconds of a term remain: the nearest
 * threshold reached, unless one as near was shown already (`shown`, in
 * days); null otherwise.
 */
export function reminderDue(left: number, shown?: number): number | null {
  if (left <= 0) return null
  const due = REMIND_DAYS.filter((days) => left <= days * 86_400_000).pop()
  return due !== undefined && (shown === undefined || due < shown) ? due : null
}
