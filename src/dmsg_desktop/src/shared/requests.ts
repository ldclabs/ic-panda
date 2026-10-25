/** Only reviewed, side-effect-free reads may be repeated after a lost reply.
 * Names such as sync, mark, refresh and ap/call are intentionally not inferred.
 * Unknown/new methods default to mutations until reviewed here. */
export const READ_METHODS = new Set([
  'app/initialize', 'daemon/status', 'session/status', 'session/accounts', 'session/login_result',
  'account/show', 'account/security', 'recovery/status',
  'vault/templates', 'vault/list', 'vault/show', 'vault/tags', 'vault/history', 'vault/conflicts', 'vault/health', 'vault/reveal', 'vault/otp',
  'files/list', 'files/tree', 'files/show', 'files/search', 'files/tags', 'files/preview', 'files/open_list',
  'files/usage', 'files/backup_status', 'files/cloud_list', 'files/storage', 'files/cloud_usage',
  'files/mount_status', 'files/preferences', 'files/share_links', 'files/share_quote',
  'sync/status', 'profile/show', 'profile/display', 'cache/status', 'profile/pending', 'profile/resolve', 'profile/avatar_image',
  'approval/list', 'approval/show', 'attest/readiness', 'attest/history', 'attest/usage',
  'connect/candidates', 'connect/pairings', 'connect/status',
  'channel/list', 'channel/show', 'channel/messages', 'channel/outbox', 'channel/search', 'channel/preview', 'channel/names', 'channel/receipts',
  'handle/resolve', 'handle/mine', 'handle/operations',
  'plan/catalog', 'plan/status', 'plan/assets', 'plan/order', 'plan/checkouts', 'plan/panda',
  'payer/status', 'payer/balance', 'inbox/policy', 'inbox/order', 'inbox/outgoing', 'inbox/escrow',
  'handle/price', 'ops/orders', 'ops/transfers', 'ops/claims', 'audit/list', 'agent/profile', 'agent/mail_push', 'mcp-events/list',
  'wallet/queue', 'wallet/policy_usage', 'wallet/readiness', 'wallet/receive', 'wallet/versions', 'wallet/health',
  'wallet/audit_page', 'wallet/cosign_status', 'wallet/principal', 'wallet/quote_preview', 'wallet/deposits', 'wallet/audit', 'wallet/cosigners',
  'inbox/list',
  'agent/principal', 'agent/credentials', 'agent/delegations', 'agent/ids', 'grants/list', 'ap/read',
  'mcp-events/status',
  'secrets/references', 'secrets/rotation/dependencies',
  'wallet/list', 'wallet/show', 'wallet/balances', 'wallet/assets', 'wallet/proposals', 'wallet/proposal',
  'wallet/history', 'wallet/book',
  'legacy/pairs', 'legacy/channels', 'legacy/list',
])

export const isRead = (method: string, params?: unknown) => READ_METHODS.has(method)
  && (method !== 'plan/status' || ((params as any)?.refresh !== true && ((params as any)?.refresh === false || ['local', 'swr', 'fresh'].includes((params as any)?.mode))))

/** Canonical only in memory; never log request keys or their parameters. */
export function requestKey(method: string, params: unknown): string {
  const sorted = (value: any): any => Array.isArray(value) ? value.map(sorted)
    : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, sorted(value[key])])) : value
  return JSON.stringify([method, sorted(params)])
}
