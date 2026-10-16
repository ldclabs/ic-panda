// Electron smoke test: the built app against a mock daemon, in a temporary
// profile. Checks the main window, that only a person's input is activity
// for the idle lock, the approval window opened by a dmsg:// link, that
// only the approval window decides, and that it decides what it
// showed, that payments and device revocation take a hold to approve, and
// that the channel, inbox, plan and agent views reach the daemon with
// payments and grants only as approval requests; that another account's
// workspace is selected from Settings, that binding a login and a web
// checkout are approval requests, and that a web checkout is quoted before it
// can be approved; that a web login shows what it proves and pays nothing;
// that a kept legacy archive reads offline and transfer files never pass
// through a window; that a vault secret is shown or copied only after a
// second click and the copy never passes through a window; that an item
// with a repeated tag opens; that conflict copies resolve side by side; that
// a library file is added through the main process, which refuses a file a
// page made; that a device's capabilities change and a device is revoked
// only through the approval window; that a view the main process opened
// leaves the others a click away; that channel files, roles, history,
// archiving and transfers reach the daemon; that the Agents view runs only
// the Mail and Knowledge tools, grants services at a chosen origin (each
// capability once) and manages MCP Events endpoints; that
// a delivery notifies without its content; that application actions and
// attestation bundles enter through the main process's dialogs; and that a
// language and an appearance picked in the main window reach both windows.
// Run with `pnpm test:e2e`.
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import assert from 'node:assert/strict'
import { WebSocketServer } from 'ws'

const token = 'desktop-smoke-token'
const account = '0123456789abcdefghig'
const request = {
  id: 'ab'.repeat(32),
  source: 'connect:https://app.example',
  kind: 'connect',
  type: 'connect',
  state: 'awaiting_user',
  created_at: Date.now(),
  expires_at: Date.now() + 600_000,
  digest: 'cd'.repeat(32),
  execution: null,
  error: null,
  detail: {
    origin: 'https://app.example',
    app_id: 'token-list',
    app_version: 1,
    pairing_label: 'Chrome · Test',
    account,
    method: 'signDocument',
    kind: 'document',
    operation_id: 'ef'.repeat(32),
    requested_at: Date.now(),
    expires_at: Date.now() + 600_000,
    state: 'awaiting_user',
    channels: 1,
    requests: ['12'.repeat(32)],
    relayed_by: 'connect.dmsg.net',
    content: {
      statement: {
        profile: 'text',
        issuer: `https://dmsg.test/u/${account}`,
        content: { text: 'I approve release 1.0' }
      }
    }
  }
}
const peer = '0123456789abcdefghi0'
const channel = {
  id: '33'.repeat(32),
  name: 'Smoke channel',
  genesis: '44'.repeat(32),
  ledger: {
    channel_id: '33'.repeat(32),
    type: 'collaboration',
    owner: account,
    epoch: 1,
    status: 'active',
    members: {
      [account]: { account, role: 'owner', joined_epoch: 1, can_rotate: true, history: [] },
      [peer]: { account: peer, role: 'member', joined_epoch: 2, can_rotate: false, history: [] }
    }
  },
  message_cursor: 2,
  readable_epochs: [1, 2]
}
const messages = [
  { seq: 1, account: peer, device: '55'.repeat(32), message_id: '66'.repeat(32), epoch: 1, created_at: Date.now(), stored_at: Date.now(), text: 'Hello channel', file: null },
  { seq: 2, account: peer, device: '55'.repeat(32), message_id: '67'.repeat(32), epoch: 2, created_at: Date.now(), stored_at: Date.now(), text: 'The plan', file: { name: 'plan.pdf', size: 2048 } }
]
const item = (id, revision, fields) => ({
  id,
  revision,
  synced: true,
  conflict: false,
  deleted: false,
  item: { tags: [], body: '', username: '', secret: '', url: '', favorite: false, createdAt: Date.now(), updatedAt: Date.now(), ...fields }
})
const SECRET = 'smoke-secret-value'
const vault = [
  item('22'.repeat(32), 'r1', { title: 'Smoke note', type: 'note', body: 'hello' }),
  item('24'.repeat(32), 'r3', { title: 'Smoke login', type: 'login', tags: ['work'], username: 'panda', secret: '********', url: 'https://example.com' }),
  // Agents and the CLI may write a tag twice; the daemon keeps it as given.
  item('27'.repeat(32), 'r1', { title: 'Tagged twice', type: 'note', tags: ['team', 'team'], updatedAt: Date.now() - 60_000 })
]
let conflicts = [{ ...item('22'.repeat(32), 'r9', { title: 'Smoke note', type: 'note', body: 'hello\nfrom the laptop' }), conflict: true }]
const trash = []
/** The file library: files of the root folder, as `files/list` and `files/show` give them. */
const libraryFile = (id, name, size, status = 'local') => ({
  id,
  folder: 'root',
  name,
  mime: name.endsWith('.pdf') ? 'application/pdf' : 'application/octet-stream',
  size,
  sha256: 'ab'.repeat(32),
  tags: [],
  note: '',
  starred: false,
  created_at: Date.now() - 120_000,
  updated_at: Date.now() - 60_000,
  current: 'v1',
  versions: 1,
  trashed_at: null,
  status,
  backup: status === 'backed_up' ? { upload_id: '71'.repeat(32), generation: 1, backed_up_at: Date.now() - 30_000, verified: true } : null,
  path: []
})
const library = [libraryFile('23'.repeat(32), 'report.pdf', 1234, 'backed_up')]
const libraryTrash = []
const version = (file) => ({ id: file.current, size: file.size, sha256: file.sha256, created_at: file.created_at, device: '11'.repeat(32), origin: { type: 'imported' }, local: true, backup: file.backup, current: true, status: file.status })
const endpoints = []
const grants = []
const notes = []
const ledger = 'ryjl3-tyaaa-aaaaa-aaaba-cai'
const claude = 'did:agent:' + 'Q'.repeat(43)
const audit = [
  { seq: 2, at: Date.now(), actor: claude, via: 'mcp', action: 'dmsg_vault_read', target: 'ab'.repeat(16), detail: null, outcome: 'ok' },
  { seq: 1, at: Date.now() - 60_000, actor: 'owner', via: 'gui', action: 'grants/add', target: 'cd'.repeat(8), detail: `${claude} vault.read:work`, outcome: 'ok' }
]
const other = '0k0g2081040g2081040g'
const login = 'rdmx6-jaaaa-aaaaa-aaadq-cai'
const checkoutRequest = {
  ...request,
  id: 'c1'.repeat(32),
  digest: 'c2'.repeat(32),
  detail: {
    ...request.detail,
    requests: ['13'.repeat(32)],
    method: 'checkout',
    kind: 'checkout',
    content: { product_id: 'token-list', sku: 'pro', amount_usd_micros: '3000000', method: 'Cash' }
  }
}
/** A web login: the page's session proven for this account, nothing signed or paid. */
const authRequest = {
  ...request,
  id: 'a7'.repeat(32),
  digest: 'a8'.repeat(32),
  detail: {
    ...request.detail,
    requests: ['14'.repeat(32)],
    method: 'authenticate',
    kind: 'authenticate',
    content: { purpose: 'Login', receiver: 'aaaaa-aa', expires_at: Date.now() + 300_000 }
  }
}
/** A 1×1 PNG. */
const PIXEL = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
let published = {
  account,
  profile: { version: 1, prev_hash: null, display_name: 'Smoke', bio: '', avatar_upload: 'a0'.repeat(32), links: [] },
  hash: 'aa'.repeat(32),
  device_id: '11'.repeat(32),
  self_asserted: true
}
const checkouts = []
const calls = []
// The checkout comes first, so a link finds its request, not the first one.
const requests = new Map([
  [checkoutRequest.id, checkoutRequest],
  [request.id, request]
])
let sockets = []
function emit(type, data) {
  for (const s of sockets) s.send(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { seq: 1, type, data } }))
}
/** A request created from the main window, as the daemon would show it. */
function created(id, type, detail) {
  const r = { ...request, id, source: 'gui', state: 'awaiting_user', type, kind: type, digest: 'f0'.repeat(32), detail }
  requests.set(id, r)
  return r
}
function status() {
  return {
    state: 'unlocked',
    account,
    device_id: '11'.repeat(32),
    approvals_waiting: [...requests.values()].filter((r) => r.state === 'awaiting_user').length,
    pending_sync: 2,
    background_online: false,
    lock_at: Date.now() + 14 * 60_000,
    idle_lock_ms: 15 * 60_000
  }
}
const offer = {
  nonce: '77'.repeat(32),
  origin: 'https://dmsg.net',
  target: '11'.repeat(32),
  expires_at: Date.now() + 600_000,
  fingerprint: '88'.repeat(32),
  link: 'https://dmsg.net/legacy#pair=pmVub25jZQ',
  started: false
}
const archive = { id: '99'.repeat(32), principal: 'aaaaa-aa', mode: 'Local', objects: 7, gaps: 0, imported_at: Date.now(), parts: 1 }
const methods = {
  'legacy/pairs': () => [offer],
  'legacy/list': () => [archive],
  'legacy/report': (p) => ({
    ...archive,
    id: p.id,
    snapshot: 'pre_migration',
    stage: 'content_verified',
    gaps: [],
    checks: 5,
    profile: { name: 'Old Panda', bio: '', image: '' },
    files: [{ source: 'b/file/7', kind: 'file', name: 'notes.txt', verified: true, mime: null }],
    resources: [],
    entitlements: [],
    messages: [{ source: 'c/channel/1/message/2', text: 'Hello from the old app', deleted: false, system: false, author: 'aaaaa-aa', time: '1700000000002', file: null }],
    total_messages: 1,
    frozen: { checked_channels: 0, matched: false, total_changes: 1, changes: [] }
  }),
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'app/activity': () => null,
  'daemon/status': status,
  'vault/list': (p) => (p?.trash ? trash : vault),
  'vault/show': (p) => {
    const entry = vault.find((e) => e.id === p.id)
    return p.reveal ? { ...entry, item: { ...entry.item, secret: SECRET } } : entry
  },
  'vault/put': (p) => {
    const entry = item('25'.repeat(32), 'r5', { ...p.item, secret: p.item.secret ? '********' : '' })
    vault.push(entry)
    return entry
  },
  'vault/edit': (p) => {
    const entry = vault.find((e) => e.id === p.id)
    assert.equal(p.base, entry.revision)
    Object.assign(entry.item, p.changes, p.changes.secret ? { secret: '********' } : {})
    entry.revision = `${entry.revision}+`
    return entry
  },
  'vault/delete': (p) => {
    const at = vault.findIndex((e) => e.id === p.id)
    const [entry] = vault.splice(at, 1)
    trash.push({ ...entry, deleted: true })
    return entry
  },
  'files/tree': () => [],
  'files/list': (p) => (p?.trash ? { folder: null, folders: [], files: libraryTrash } : { folder: null, folders: [], files: library }),
  'files/search': () => library,
  'files/show': (p) => {
    const file = library.find((f) => f.id === p.id) ?? libraryTrash.find((f) => f.id === p.id)
    return { file, versions: [version(file)] }
  },
  'files/import': (p) => {
    assert.equal(p.folder, 'root')
    return p.paths.map((path) => {
      const file = libraryFile('26'.repeat(32), basename(path), 4096)
      library.push(file)
      return { file, version: 'v1', resumed: false, duplicate_of: [] }
    })
  },
  'files/preview': () => ({ kind: 'none', mime: 'application/pdf', size: 1234, content_base64url: null, thumbnail: false }),
  'files/update': (p) => {
    const file = library.find((f) => f.id === p.id)
    if (p.starred !== undefined) file.starred = p.starred
    if (p.tags) file.tags = p.tags
    if (p.note !== undefined) file.note = p.note
    return file
  },
  'files/trash': (p) => {
    for (const id of p.ids) {
      const at = library.findIndex((f) => f.id === id)
      const [file] = library.splice(at, 1)
      libraryTrash.push({ ...file, trashed_at: Date.now() })
    }
    return { trashed: p.ids.length }
  },
  'files/usage': () => ({ files: library.length, trashed: libraryTrash.length, versions: library.length, local_versions: library.length, blobs: 2, blob_bytes: 5330, open_copies: 0, max_file_bytes: 4294967296, trash_days: 30, last_check: null }),
  'files/backup_status': () => [],
  'files/mount_status': () => ({ mounted: false, enabled: false, os: 'macos', supported: true, note: '' }),
  'files/cloud_list': () => [],
  'files/cloud_usage': () => ({ reserved: 0, committed: 1300, storage_limit: 104857600, status: 'within_limit', backups: 1, backup_bytes: 1300 }),
  'files/save_shared': (p) => {
    const file = libraryFile('28'.repeat(32), 'plan.pdf', 2048)
    library.push(file)
    return { file, version: 'v1', resumed: false, duplicate_of: [] }
  },
  'sync/run': () => {
    for (const entry of vault) entry.synced = true
    return { pushed: 1 }
  },
  'vault/conflicts': () => conflicts,
  'vault/resolve': (p) => {
    conflicts = []
    return vault[0]
  },
  'approval/list': () => [...requests.values()],
  'approval/show': (p) => requests.get(p.id),
  'approval/respond': (p) => {
    const r = requests.get(p.id)
    assert.equal(p.digest, r.digest)
    r.state = p.approve ? 'done' : 'rejected'
    emit('approval/changed')
    return r
  },
  'attest/usage': () => ({ charged: 3, allowed: 100, remaining: 97, valid_until_ms: Date.now() + 86_400_000, history: [] }),
  'attest/history': () => [
    { execution: 'e1'.repeat(32), stage: 'complete', statement: { profile: 'text', content: { text: 'Signed before' } }, origin: 'https://dmsg.net', created_at: Date.now() }
  ],
  'attest/export': (p) => ({ format: 'dmsg-attestation/1', account, home: 'aaaaa-aa', request_id: p.execution, cose_sign1: 'x', cose_key: 'y', receipt: 'z' }),
  'attest/verify': (p) => {
    assert.equal(p.bundle.format, 'dmsg-attestation/1')
    return {
      statement: { profile: 'text', issuer: 'x', content: { text: 'Signed before' } },
      content: 'text',
      signature: true,
      key_fingerprint: 'ab'.repeat(32),
      account,
      device_id: '11'.repeat(32),
      security_epoch: 3,
      approved_at: Date.now(),
      origin: 'https://dmsg.net',
      certified_at: Date.now(),
      current: 'active'
    }
  },
  'attest/action': (p) => {
    assert.equal(p.source, 'gui')
    assert.equal(Buffer.from(p.action, 'base64url').toString(), 'prepared action')
    return created('a1'.repeat(32), 'attest', {
      origin: 'https://app.example',
      file: null,
      statement: {
        profile: 'app_action',
        issuer: 'x',
        content: { app_id: 'token-list', origin: 'https://app.example', command: 'review', args: [{ name: 'score', value: 'Nat(5)' }], operation_id: 'cd'.repeat(32), expires_at: Date.now() + 300_000 }
      }
    })
  },
  'device/revoke': (p) => {
    assert.equal(p.source, 'gui')
    return created('d1'.repeat(32), 'device_revoke', { device_id: p.device, this_device: false })
  },
  'device/capabilities': (p) => {
    assert.equal(p.source, 'gui')
    return created('d2'.repeat(32), 'device_capabilities', { device_id: p.device, capabilities: p.capabilities, this_device: false })
  },
  'attest/request': (p) => {
    assert.equal(p.source, 'gui')
    const created = {
      ...request,
      id: '99'.repeat(32),
      source: 'gui',
      type: 'attest',
      kind: 'attest',
      digest: '88'.repeat(32),
      detail: { origin: 'https://dmsg.net', statement: { profile: 'text', issuer: 'x', content: { text: p.statement.text } } }
    }
    requests.set(created.id, created)
    return created
  },
  'account/show': () => ({
    account,
    account_version: 7,
    security_epoch: 3,
    root_generation: 2,
    devices: [
      { device_id: '11'.repeat(32), role: 'Admin', capabilities: ['ContentSign', 'VaultUnlock', 'RootManage', 'FormalApprove'], added_at: Date.now(), added_by: null, revoked_at: null, current: true },
      { device_id: '12'.repeat(32), role: 'Admin', capabilities: ['ContentSign', 'VaultUnlock'], added_at: Date.now(), added_by: '11'.repeat(32), revoked_at: null, current: false }
    ]
  }),
  'session/accounts': () => [
    { account, device_id: '11'.repeat(32), bound: true, fast_unlock: false, login_unlocked_at: Date.now(), current: true },
    { account: other, device_id: '12'.repeat(32), bound: true, fast_unlock: false, login_unlocked_at: Date.now() - 1, current: false }
  ],
  'session/switch': (p) => ({ ...status(), state: 'locked', account: p.account }),
  'account/security': () => ({
    logins: [{ principal: login, current: true }],
    policy: { frozen: false, allowed_purposes: ['FileAttestation', 'Statement', 'AppAction'], daily_executions: 20 },
    recovery_delay_ms: 3 * 86_400_000,
    pending_recovery: null,
    recovered_device: null
  }),
  'account/bind_approve': (p) => {
    assert.equal(p.source, 'gui')
    assert.equal(p.token, 'binding-token')
    const created = {
      ...request,
      id: 'b1'.repeat(32),
      source: 'gui',
      state: 'awaiting_user',
      type: 'account',
      kind: 'account',
      digest: 'b2'.repeat(32),
      detail: { change: 'bind_login', principal: p.principal, nonce: 'b3'.repeat(32) }
    }
    requests.set(created.id, created)
    return created
  },
  'approval/quote': (p) => {
    assert.deepEqual(p, { id: checkoutRequest.id, selection: ledger })
    const quoted = {
      ...checkoutRequest,
      digest: 'c3'.repeat(32),
      detail: {
        ...checkoutRequest.detail,
        checkout: {
          id: 'c4'.repeat(32),
          actor: login,
          sku: 'pro',
          terms: { method: 'Cash', ledger, asset: 'Icp', amount: '300000000', fee_reserve: '20000', decimals: 8, funding_deadline_ms: Date.now() + 600_000 }
        }
      }
    }
    requests.set(quoted.id, quoted)
    return quoted
  },
  'handle/operations': () => [],
  'profile/show': () => published,
  'profile/avatar': (p) => {
    const upload = p.remove ? null : 'a1'.repeat(32)
    published = { ...published, profile: { ...published.profile, version: published.profile.version + 1, avatar_upload: upload } }
    return published
  },
  'profile/avatar_image': () => ({ mime: 'image/png', url: `data:image/png;base64,${PIXEL}` }),
  'connect/pairings': () => [],
  'connect/sync': () => ({ fetched: 0 }),
  'channel/list': () => [channel],
  'channel/show': () => channel,
  'channel/messages': () => messages,
  'channel/send': (p) => {
    messages.push({ ...messages[0], seq: messages.length + 1, account, text: p.text })
    return { seq: messages.length }
  },
  'channel/send_file': (p) => {
    const file = library.find((f) => f.id === p.file)
    messages.push({ ...messages[0], seq: messages.length + 1, account, text: p.text, file: { name: file.name, size: file.size } })
    return { seq: messages.length }
  },
  'channel/download': (p) => ({ name: 'plan.pdf', size: 2048, sha256: 'aa'.repeat(32), path: p.path }),
  'channel/control': (p) => {
    if (p.action.type === 'role') Object.assign(channel.ledger.members[p.action.account], { role: p.action.role, can_rotate: p.action.can_rotate })
    if (p.action.type === 'archive') channel.ledger.status = 'archived'
    return channel
  },
  'channel/history': (p) => {
    channel.ledger.members[p.account].history.push([p.from, p.to])
    return channel.ledger
  },
  'channel/reactivate': () => {
    channel.ledger.status = 'active'
    return channel
  },
  'channel/owner_propose': (p) => ({ format: 'dmsg-channel-owner-transfer/1', channel: channel.id, payload: { to: p.account }, acceptance: null }),
  'inbox/list': () => [
    { order_id: '77'.repeat(32), sender: peer, state: 'visible', text: 'A paid letter', created_at: Date.now(), read: false, archived: false }
  ],
  'inbox/outgoing': () => [],
  'inbox/policy': () => ({ mode: 'paid', version: 1 }),
  'inbox/configure': (p) => ({ ...p, version: 2 }),
  'plan/status': () => ({ plan: 'Free', storage_bytes: 1_000_000_000 }),
  'plan/catalog': () => ({
    version: 1,
    plans: [
      { plan_id: 'Free', price_cents: 0, limits: { storage_bytes: 1_000_000_000, active_channels: 3, monthly_execution_units: 10 } },
      { plan_id: 'Plus', price_cents: 500, limits: { storage_bytes: 50_000_000_000, active_channels: 20, monthly_execution_units: 100 } }
    ]
  }),
  'plan/assets': () => [{ ledger, asset: 'Icp', decimals: 8, network_fee: '10000' }],
  'plan/checkouts': () => checkouts,
  'plan/quote': (p) => {
    assert.deepEqual(p, { sku: 'plus', method: 'cash', selection: ledger })
    const job = {
      id: '88'.repeat(32),
      actor: 'aaaaa-aa',
      origin: 'https://dmsg.net',
      sku: 'plus',
      amount_usd_micros: '5000000',
      terms: { method: 'Cash', ledger, asset: 'Icp', amount: '500000000', fee_reserve: '20000', decimals: 8, funding_deadline_ms: Date.now() + 600_000 },
      stage: 'review'
    }
    checkouts.push(job)
    return job
  },
  'agent/ids': () => ({
    active: claude,
    agents: [{ label: 'claude', agent_id: claude, resident: false, credential: true, created_at: Date.now() }],
    hosts: [{ host: 'https://api.al.ink', label: 'alink', allowed: true }]
  }),
  'agent/principal': () => ({
    principal_id: `https://id.dmsg.net/${account}`,
    type: 'Person',
    published: true,
    version: 2,
    published_version: 2,
    next_generation: 2,
    controllers: [
      { generation: 1, agent_id: 'did:agent:' + 'C'.repeat(43), name: 'desktop', delegation: '*', valid_from: Date.now(), retired_at: null }
    ]
  }),
  'agent/credentials': () => [],
  'ap/call': (p) => {
    switch (p.tool) {
      case 'agent_protocols_mail_mailboxes_list':
        return { result: { mailboxes: [{ mailbox_id: 'mb1', address: 'mail:claude@relay.example', routes: [], status: 'open', unread_count: 1, expires_at: Date.now() + 86_400_000 }] } }
      case 'agent_protocols_mail_list':
        return {
          result: {
            unread_count: 1,
            messages: [
              { direction: 'received', message_id: 'm1', thread_id: 't1', from: 'did:agent:other', to: claude, mailbox_id: 'mb1', subject: 'Plan', created_at: Date.now(), expires_at: Date.now() + 86_400_000, received_at: Date.now(), reply_address: 'mail:other@relay.example', parts: [{ media_type: 'text/plain', size: 15, text: 'Mail for Claude' }], read: false }
            ]
          }
        }
      case 'agent_protocols_mail_send':
        assert.deepEqual(p.input, { text: 'Reply from the desktop', subject: 'Re: Plan', in_reply_to: 'm1' })
        return { result: { status: 'delivered', message: { message_id: 'm2' } } }
      default:
        return { error: { code: 'not_found', message: p.tool } }
    }
  },
  'mcp-events/status': () => ({ state: 'unlocked', fetching: true, last_fetch: null, rejected: 0 }),
  'mcp-events/list': () => ({ endpoints }),
  'mcp-events/create': (p) => {
    const endpoint = { endpoint_id: 'ee'.repeat(16), url: `https://api.dmsg.net/mcp-events/${'ee'.repeat(16)}`, state: 'active', agent: null, context: p.context, created_at: Date.now(), binding: {} }
    endpoints.push(endpoint)
    return { ...endpoint, secret: 'whsec_smoke' }
  },
  'grants/list': () => grants,
  'grants/add': (p) => {
    const grant = { id: 'ab'.repeat(8), grantee: p.agent, capabilities: p.capabilities, created_at: Date.now(), expires_at: null, revoked_at: null }
    grants.push(grant)
    return grant
  },
  'audit/list': (p) => ({
    entries: p.agent === 'owner' ? audit.filter((e) => e.actor === 'owner') : audit
  }),
  'agent/grant': (p) => {
    assert.equal(p.source, 'gui')
    assert.deepEqual(p.scopes, ['message.draft'])
    const created = {
      ...request,
      id: '96'.repeat(32),
      source: 'gui',
      state: 'awaiting_user',
      type: 'delegation',
      kind: 'agent',
      digest: '95'.repeat(32),
      detail: {
        id: '94'.repeat(32),
        generation: 1,
        stage: 'prepared',
        kind: { type: 'grant', id: `${account}.x`, subject: claude, scopes: p.scopes, audiences: p.audiences, expires_at: Date.now() + p.days * 86_400_000 }
      }
    }
    requests.set(created.id, created)
    return created
  },
  'plan/checkout': (p) => {
    assert.equal(p.source, 'gui')
    const created = {
      ...request,
      id: '98'.repeat(32),
      source: 'gui',
      state: 'awaiting_user',
      type: 'checkout',
      kind: 'payment',
      digest: '97'.repeat(32),
      detail: { ...checkouts.find((c) => c.id === p.id), fresh: false }
    }
    requests.set(created.id, created)
    return created
  }
}
const server = createServer((_req, res) => res.writeHead(404).end())
const wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (req, socket, head) => {
  if (req.url !== '/ws/app/v1' || req.headers.authorization !== `Bearer ${token}`) {
    socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n')
    return socket.destroy()
  }
  wss.handleUpgrade(req, socket, head, (ws) => {
    sockets.push(ws)
    ws.on('message', (data) => {
      const message = JSON.parse(String(data))
      calls.push(message)
      const handler = methods[message.method]
      const reply = handler
        ? { jsonrpc: '2.0', id: message.id, result: handler(message.params ?? {}) }
        : { jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'method not found', data: { code: 'NOT_FOUND' } } }
      ws.send(JSON.stringify(reply))
    })
    ws.on('close', () => (sockets = sockets.filter((s) => s !== ws)))
  })
})
async function waitFor(check) {
  for (let i = 0; i < 100 && !check(); i++) await new Promise((r) => setTimeout(r, 100))
  assert.ok(check())
}
/** Picks an option of a dropdown menu (`components/Select.svelte`, the language menu) by its value. */
async function choose(page, testid, value) {
  await page.click(`[data-testid="${testid}"]`)
  await page.click(`[data-testid="${testid}-menu"] [data-value="${value}"]`)
}
await new Promise((r) => server.listen(0, '127.0.0.1', r))
const port = server.address().port
const profile = await mkdtemp(join(tmpdir(), 'dmsg-desktop-smoke-'))
const shots = resolve('test-results')
await mkdir(shots, { recursive: true })
// The windows start in English and light, whatever the system's languages and appearance.
await writeFile(join(profile, 'locale'), 'en')
await writeFile(join(profile, 'appearance'), 'light')

const app = await electron.launch({
  executablePath: electronPath,
  // The appearance comes from the profile through nativeTheme, not from emulation.
  colorScheme: null,
  args: [resolve('out/main/index.js')],
  env: {
    ...process.env,
    DMSG_DESKTOP_TEST: '1',
    DMSG_DESKTOP_USER_DATA: profile,
    DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${port}`,
    DMSG_DESKTOP_TOKEN: token,
    DMSG_HOME: join(profile, 'home')
  }
})
// Dialogs, the clipboard and notifications are the main process's; the test
// answers the dialogs and watches the other two instead of the system's.
const picks = join(profile, 'picks')
await mkdir(picks, { recursive: true })
await writeFile(join(picks, 'action.cbor'), 'prepared action')
await writeFile(join(picks, 'bundle.json'), JSON.stringify({ format: 'dmsg-attestation/1' }))
await writeFile(join(picks, 'release.tar'), 'release bytes')
await app.evaluate(({ dialog, clipboard, Notification }) => {
  Object.assign(globalThis, { __open: [], __save: [], __clip: '', __notes: [] })
  dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [globalThis.__open.shift()] })
  dialog.showSaveDialog = async () => ({ canceled: false, filePath: globalThis.__save.shift() })
  clipboard.writeText = async (text) => void (globalThis.__clip = text)
  clipboard.readText = async () => globalThis.__clip
  clipboard.clear = () => void (globalThis.__clip = '')
  Notification.prototype.show = function () {
    globalThis.__notes.push(this.body)
  }
})
const answer = (kind, path) => app.evaluate((_electron, [kind, path]) => globalThis[kind].push(path), [kind, path])
const latest = (method) => calls.filter((c) => c.method === method).at(-1)
/** Wait for an element's animations, such as a dialog fading in, to end before a screenshot. */
const settled = (page, selector) => page.locator(selector).evaluate((el) => Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished)))
try {
  const main = await app.firstWindow()
  await main.waitForSelector('text=Smoke note', { timeout: 20_000 })
  assert.equal(await main.textContent('[data-testid="waiting-count"]'), '2')
  // The gate rail keeps the lock, its countdown and what waits to sync in view.
  assert.equal(await main.textContent('[data-testid="gate-state"]'), 'Unlocked')
  assert.match(await main.textContent('[data-testid="gate-rail"]'), /1[34]:\d\d[\s\S]*2 versions to sync/)
  await main.screenshot({ path: join(shots, 'main.png') })
  // The palette goes to a view by name; ⌘1 comes back to the vault.
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
  // The desktop's reads are not activity for the idle lock; a person's input is.
  assert.equal(latest('app/initialize').params.activity, 'reported')
  assert.equal(latest('app/activity'), undefined)
  await main.keyboard.press(`${mod}+k`)
  await waitFor(() => latest('app/activity'))
  await main.waitForSelector('[data-testid="palette"]')
  await main.fill('[data-testid="palette-input"]', 'Inbox')
  await main.screenshot({ path: join(shots, 'palette.png') })
  await main.keyboard.press('Enter')
  await main.waitForSelector('[data-testid="palette"]', { state: 'detached' })
  assert.equal(await main.textContent('[data-testid="view-title"]'), 'Inbox')
  await main.keyboard.press(`${mod}+1`)
  await main.waitForSelector('[data-testid="view-title"] >> text=Vault')
  // A view the main process opens (the tray, a notification) leaves the one
  // the window showed before a click away.
  await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].webContents.send('dmsg:navigate', '/approvals'))
  await main.waitForSelector('[data-testid="view-title"] >> text=Signatures & approvals')
  await main.keyboard.press(`${mod}+1`)
  await main.waitForSelector('[data-testid="view-title"] >> text=Vault')
  await main.click('[data-testid="sidebar-toggle"]')
  await main.screenshot({ path: join(shots, 'sidebar-collapsed.png') })
  await main.click('[data-testid="sidebar-toggle"]')

  // The main window cannot decide, even through the bridge's own methods.
  await assert.rejects(main.evaluate((id) => window.dmsg.decide(id, 'cd'.repeat(32), true), request.id), /FORBIDDEN/)
  await assert.rejects(
    main.evaluate((id) => window.dmsg.call('approval/respond', { id, digest: 'cd'.repeat(32), approve: true, human: true }), request.id),
    /FORBIDDEN/
  )
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 0)

  // A dmsg:// link only opens the approval window on the daemon's request:
  // the bridge links the relay request, which the request's detail lists.
  const opened = app.waitForEvent('window')
  await app.evaluate(({ app }, url) => app.emit('open-url', { preventDefault() {} }, url), `dmsg://connect/request?account=${account}&request=${request.detail.requests[0]}`)
  const approval = await opened
  await approval.waitForSelector('[data-testid="approval-origin"]', { timeout: 20_000 })
  assert.equal(await approval.textContent('[data-testid="approval-origin"]'), 'https://app.example')
  assert.ok((await approval.textContent('[data-testid="approval-relayed"]')).includes('connect.dmsg.net'))
  assert.equal(await approval.textContent('[data-testid="approval-text"]'), 'I approve release 1.0')
  await approval.screenshot({ path: join(shots, 'approval.png') })
  await approval.click('[data-testid="approve"]')
  await approval.waitForSelector('[data-testid="approval-outcome"]')
  const responded = calls.filter((c) => c.method === 'approval/respond')
  assert.equal(responded.length, 1)
  assert.deepEqual(responded[0].params, { id: request.id, digest: request.digest, approve: true, human: true, source: 'gui' })

  // A statement started in the main window is marked as the desktop's and opens for approval.
  await main.click('text=/Signatures|签名与授权/')
  await main.click('[data-testid="approvals-tab-sign"]')
  await main.fill('[data-testid="statement-text"]', 'Signed from the desktop')
  await main.click('[data-testid="statement-submit"]')
  await approval.waitForSelector('text=Signed from the desktop', { timeout: 20_000 })
  const created = calls.find((c) => c.method === 'attest/request')
  assert.equal(created.params.source, 'gui')

  // Channels read and write through the daemon; a message shows as sending
  // until the relay stores it.
  await main.click('nav >> text=/Channels|频道/')
  await main.click('[data-testid="channel-list"] >> text=Smoke channel')
  await main.waitForSelector('text=Hello channel')
  await main.fill('[data-testid="channel-text"]', 'Reply from the desktop')
  await main.keyboard.press('Enter')
  await waitFor(() => latest('channel/send'))
  await main.waitForSelector('[data-testid="channel-messages"] article:not([data-testid]) >> text=Reply from the desktop')
  assert.equal(await main.locator('[data-testid="message-outgoing"]').count(), 0)

  // A library file goes with a message as a shared edition; a shared file
  // is saved where the main process's dialog says, or into the library;
  // roles, history, a transfer proposal and archiving reach the daemon from
  // the owner's view.
  await choose(main, 'channel-attachment', '23'.repeat(32))
  await main.waitForSelector('[data-testid="channel-attached"] >> text=report.pdf')
  await main.fill('[data-testid="channel-text"]', 'With a file')
  await main.click('[data-testid="channel-send"]')
  await waitFor(() => latest('channel/send_file'))
  assert.deepEqual(latest('channel/send_file').params, { id: channel.id, file: '23'.repeat(32), text: 'With a file' })
  const saved = join(profile, 'plan.pdf')
  await answer('__save', saved)
  await main.locator('[data-testid="message-download"]').first().click()
  await waitFor(() => latest('channel/download'))
  assert.deepEqual(latest('channel/download').params, { id: channel.id, seq: 2, path: saved })
  await main.waitForSelector('[data-testid="channel-notice"]')
  await main.locator('[data-testid="message-save"]').first().click()
  await waitFor(() => latest('files/save_shared'))
  assert.deepEqual(latest('files/save_shared').params, { channel: channel.id, seq: 2 })
  await choose(main, 'member-actions', 'admin')
  await waitFor(() => latest('channel/control'))
  assert.deepEqual(latest('channel/control').params, { id: channel.id, action: { type: 'role', account: peer, role: 'admin', can_rotate: false } })
  await main.click('[data-testid="member-actions"]')
  await main.click('[data-testid="member-history"]')
  await main.click('[data-testid="history-submit"]')
  await waitFor(() => latest('channel/history'))
  assert.deepEqual(latest('channel/history').params, { id: channel.id, account: peer, from: 1, to: 1 })
  await main.waitForSelector('[data-testid="history-dialog"]', { state: 'detached' })
  await main.screenshot({ path: join(shots, 'channels.png') })
  await main.click('[data-testid="channel-manage"]')
  await main.click('[data-testid="owner-transfer"]')
  await main.fill('[data-testid="transfer-target"]', peer)
  await main.click('[data-testid="owner-propose"]')
  await main.waitForSelector('[data-testid="channel-handed"]')
  assert.ok((await main.inputValue('[data-testid="channel-handed"]')).includes('dmsg-channel-owner-transfer/1'))
  await settled(main, '[data-testid="transfer-dialog"]')
  await main.screenshot({ path: join(shots, 'channel-transfer.png') })
  await main.keyboard.press('Escape')
  await main.waitForSelector('[data-testid="transfer-dialog"]', { state: 'detached' })
  await main.click('[data-testid="channel-manage"]')
  await main.click('[data-testid="channel-archive"]')
  await main.click('[data-testid="channel-reactivate"]')
  await waitFor(() => latest('channel/reactivate'))
  assert.equal(channel.ledger.status, 'active')

  // Letters in two columns; the contact rules are radio cards.
  await main.click('nav >> text=/Inbox|收件箱/')
  await main.waitForSelector('[data-testid="letter"] >> text=A paid letter')
  await main.screenshot({ path: join(shots, 'inbox.png') })
  await main.click('[data-testid="contact-rules"]')
  await main.click('[data-testid="rules-free"]')
  await main.click('[data-testid="rules-save"]')
  await waitFor(() => latest('inbox/configure'))
  assert.deepEqual(latest('inbox/configure').params, { mode: 'free' })

  // A subscription is quoted, then paid only once the approval window agrees.
  await main.click('nav >> text=/Plan & names|会员与名称/')
  await main.waitForSelector('[data-testid="plan-name"]')
  assert.equal(await main.textContent('[data-testid="plan-name"]'), 'Free')
  await main.click('button:has-text("Subscribe"), button:has-text("订阅")')
  await approval.waitForSelector('[data-testid="approval-sku"]', { timeout: 20_000 })
  assert.ok((await approval.textContent('[data-testid="approval-amount"]')).includes('5 Icp'))
  await approval.waitForSelector('[data-testid="approve"]')
  await approval.screenshot({ path: join(shots, 'checkout.png') })
  // A payment is approved by holding the button: a click alone decides nothing.
  await approval.click('[data-testid="approve"]')
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 1)
  await main.screenshot({ path: join(shots, 'account.png') })

  // A grant to a hosted Agent ID is signed only after the approval window agrees.
  await main.click('nav >> text=/Agents|Agent/')
  await main.waitForSelector('[data-testid="principal-id"]')
  await main.fill('[data-testid="grant-subject"]', 'claude')
  await main.fill('[data-testid="grant-scopes"]', 'message.draft')
  await main.fill('[data-testid="grant-audiences"]', 'https://dmsg.net')
  await main.click('[data-testid="grant-submit"]')
  await approval.waitForSelector('[data-testid="approval-subject"]', { timeout: 20_000 })
  assert.equal(await approval.textContent('[data-testid="approval-subject"]'), claude)
  await approval.screenshot({ path: join(shots, 'delegation.png') })
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 1)

  // Mail as a hosted Agent ID; a window runs only the Mail and Knowledge tools.
  await main.click('[data-testid="agents-tab-mail"]')
  await main.click('[data-testid="mail-open"]')
  await main.waitForSelector('text=Mail for Claude')
  await main.click('[data-testid="mail-messages"] >> text=/^(Reply|回复)$/')
  await main.fill('[data-testid="mail-text"]', 'Reply from the desktop')
  await main.click('[data-testid="mail-send"]')
  await waitFor(() => calls.some((c) => c.method === 'ap/call' && c.params.tool === 'agent_protocols_mail_send'))
  await assert.rejects(
    main.evaluate(() => window.dmsg.call('ap/call', { tool: 'agent_protocols_room_send_message', input: {} })),
    /FORBIDDEN/
  )
  assert.ok(calls.filter((c) => c.method === 'ap/call').every((c) => /^agent_protocols_(mail|knowledge)_/.test(c.params.tool)))
  // An MCP Events endpoint shows its secret once; a delivery notifies how
  // many arrived, never what or from where.
  await main.click('[data-testid="agents-tab-events"]')
  await main.fill('[data-testid="endpoint-label"]', 'Upstream')
  await main.click('[data-testid="endpoint-create"]')
  await main.waitForSelector('[data-testid="endpoint-secret"]')
  assert.equal(await main.textContent('[data-testid="endpoint-secret"]'), 'whsec_smoke')
  assert.deepEqual(latest('mcp-events/create').params, { context: { label: 'Upstream' } })
  emit('mcp-events/changed', { stored: 2, refresh_due: 0 })
  for (let i = 0; i < 50 && !notes.length; i++) {
    notes.push(...(await app.evaluate(() => globalThis.__notes.filter((n) => /2/.test(n)))))
    if (!notes.length) await new Promise((r) => setTimeout(r, 100))
  }
  assert.equal(notes.length, 1)
  assert.ok(!notes[0].includes('Upstream'))
  // The audit record names who did what and through which way in.
  await main.click('[data-testid="agents-tab-audit"]')
  await main.waitForSelector('[data-testid="audit-entries"] >> text=dmsg_vault_read')
  await choose(main, 'audit-actor', 'owner')
  await waitFor(() => latest('audit/list').params.agent === 'owner')
  await main.waitForSelector('[data-testid="audit-entries"] >> text=grants/add')
  assert.equal(await main.locator('[data-testid="audit-entries"] >> text=dmsg_vault_read').count(), 0)
  await choose(main, 'audit-actor', '')
  await main.waitForSelector('[data-testid="audit-entries"] >> text=dmsg_vault_read')
  await main.locator('[data-testid="agent-audit"]').screenshot({ path: join(shots, 'audit.png') })
  // A grant joins the services ticked at the chosen origin with those
  // typed, and the matrix shows what each Agent ID reaches.
  await main.click('[data-testid="agents-tab-capabilities"]')
  await choose(main, 'grant-agent', claude)
  await choose(main, 'grant-origin', 'https://api.al.ink')
  await main.check('[data-testid="service-discourse.read"]')
  await main.check('[data-testid="service-mail.write"]')
  // A capability typed twice, or typed as well as ticked, is granted once.
  await main.fill('[data-testid="grant-other"]', 'vault.list vault.list mail.write:https://api.al.ink')
  await main.locator('[data-testid="grants-card"]').screenshot({ path: join(shots, 'grants.png') })
  await main.click('[data-testid="grant-capabilities"]')
  await main.waitForSelector('[data-testid="capability-grants"] >> text=discourse.read:https://api.al.ink')
  assert.deepEqual(latest('grants/add').params, {
    agent: claude,
    capabilities: ['discourse.read:https://api.al.ink', 'mail.write:https://api.al.ink', 'vault.list'],
    source: 'gui'
  })
  assert.equal(await main.isChecked('[data-testid="service-discourse.read"]'), false)
  await main.screenshot({ path: join(shots, 'agents.png') })

  // A web checkout cannot be approved before it is quoted, and the quote is what is approved.
  await app.evaluate(({ app }, url) => app.emit('open-url', { preventDefault() {} }, url), `dmsg://connect/request?account=${account}&request=${checkoutRequest.id}`)
  await approval.waitForSelector('[data-testid="checkout-quote"]', { timeout: 20_000 })
  assert.equal(await approval.isDisabled('[data-testid="approve"]'), true)
  await approval.click('button:has-text("Get a quote"), button:has-text("获取报价")')
  await approval.waitForSelector('[data-testid="approval-amount"]', { timeout: 20_000 })
  assert.ok((await approval.textContent('[data-testid="approval-amount"]')).includes('3 Icp'))
  // A quoted request changed, so Approve opens again only after its delay.
  await approval.waitForSelector('[data-testid="approve"]:not([disabled])')
  await approval.screenshot({ path: join(shots, 'connect-checkout.png') })

  // A web login shows what the page gets proven, and approving it pays nothing.
  requests.set(authRequest.id, authRequest)
  await app.evaluate(({ app }, url) => app.emit('open-url', { preventDefault() {} }, url), `dmsg://connect/request?account=${account}&request=${authRequest.detail.requests[0]}`)
  await approval.waitForSelector('[data-testid="approval-receiver"]', { timeout: 20_000 })
  assert.equal(await approval.textContent('[data-testid="approval-title"]'), 'Confirm your account to a website')
  assert.equal(await approval.textContent('[data-testid="approval-purpose"]'), 'Login')
  assert.equal(await approval.textContent('[data-testid="approval-receiver"]'), 'aaaaa-aa')
  assert.ok(!(await approval.textContent('body')).includes('paying account'))
  await approval.screenshot({ path: join(shots, 'connect-authenticate.png') })

  // Legacy migration: the offer shows its fingerprint and target device, a
  // kept archive reads offline, and transfer files reach the daemon only
  // through the main process's dialog.
  // A started migration that is not imported yet shows a notice; Legacy itself lives in Settings.
  await main.click('[data-testid="legacy-notice"] >> text=Open')
  await main.waitForSelector('[data-testid="legacy-archives"]')
  await main.check(`[data-testid="legacy-pick-${offer.nonce}"]`)
  await main.waitForSelector('[data-testid="legacy-offer"]')
  assert.ok((await main.textContent('[data-testid="legacy-offer"]')).includes(offer.fingerprint))
  assert.ok((await main.textContent('[data-testid="legacy-offer"]')).includes(offer.target))
  await main.click('button:has-text("Check offline and read"), button:has-text("离线核对与阅读")')
  await main.waitForSelector('text=Hello from the old app')
  await main.screenshot({ path: join(shots, 'legacy.png') })
  await assert.rejects(
    main.evaluate(() => window.dmsg.call('legacy/import', { path: '/tmp/x', nonce: '', fingerprint: '', principal: '' })),
    /FORBIDDEN/
  )
  await assert.rejects(main.evaluate(() => window.dmsg.openLegacy('https://evil.example/legacy#pair=x')), /INVALID_INPUT/)

  // Vault: a secret is shown or copied only on a second click, and the copy
  // goes from the daemon to the clipboard through the main process; an edit
  // names the version it replaces; a new item comes from its dialog; a
  // conflict copy is adopted from the side-by-side dialog.
  await main.click('nav >> text=/Vault|秘密库/')
  // An item that carries a tag twice opens like any other.
  await main.click('[data-testid="vault-list"] >> text=Tagged twice')
  await main.waitForSelector('[data-testid="vault-item-title"] >> text=Tagged twice')
  assert.equal(await main.locator('[data-testid="vault-inspector"] header span', { hasText: /^#team$/ }).count(), 2)
  await main.click('[data-testid="vault-list"] >> text=Smoke login')
  await main.waitForSelector('[data-testid="vault-secret"][data-shown="false"]')
  const revealed = () => calls.filter((c) => c.method === 'vault/show' && c.params.reveal).length
  await main.click('[data-testid="reveal-secret"]')
  await main.waitForSelector('[data-testid="reveal-secret"][data-armed="true"]')
  assert.equal(revealed(), 0)
  await main.click('[data-testid="reveal-secret"]')
  await main.waitForSelector(`[data-testid="vault-secret"] >> text=${SECRET}`)
  await main.click('[data-testid="copy-secret"]')
  assert.equal(await app.evaluate(() => globalThis.__clip), '')
  await main.click('[data-testid="copy-secret"]')
  await main.waitForSelector('[data-testid="secret-copied"]')
  assert.equal(await app.evaluate(() => globalThis.__clip), SECRET)
  assert.equal(revealed(), 2)
  await main.screenshot({ path: join(shots, 'vault.png') })
  await main.click('[data-testid="vault-edit-open"]')
  await main.fill('[data-testid="vault-edit"] input >> nth=0', 'Smoke login 2')
  await main.click('[data-testid="vault-save"]')
  await waitFor(() => latest('vault/edit'))
  assert.deepEqual(latest('vault/edit').params, {
    id: '24'.repeat(32),
    base: 'r3',
    changes: { title: 'Smoke login 2', tags: ['work'], body: '', username: 'panda', url: 'https://example.com' }
  })
  await main.keyboard.press(`${mod}+n`)
  await main.waitForSelector('[data-testid="vault-new-dialog"]')
  await main.click('[data-testid="vault-type-login"]')
  await main.fill('[data-testid="vault-title"]', 'New login')
  await main.fill('[data-testid="vault-new-secret"]', 'new-secret')
  await settled(main, '[data-testid="vault-new-dialog"]')
  await main.screenshot({ path: join(shots, 'vault-new.png') })
  await main.click('[data-testid="vault-add"]')
  await waitFor(() => latest('vault/put'))
  assert.equal(latest('vault/put').params.item.type, 'login')
  assert.equal(latest('vault/put').params.item.secret, 'new-secret')
  await main.waitForSelector('[data-testid="vault-item-title"] >> text=New login')
  await main.click('[data-testid="vault-conflicts-review"]')
  await main.waitForSelector('[data-testid="conflict-dialog"] >> text=from the laptop')
  await settled(main, '[data-testid="conflict-dialog"]')
  await main.screenshot({ path: join(shots, 'vault-conflict.png') })
  await main.click('[data-testid="conflict-use"]')
  await waitFor(() => latest('vault/resolve'))
  assert.deepEqual(latest('vault/resolve').params, { key: `${'22'.repeat(32)}:r9`, keep: 'theirs' })
  await main.waitForSelector('[data-testid="conflict-dialog"]', { state: 'detached' })

  // The palette finds an item by its title.
  await main.keyboard.press(`${mod}+k`)
  await main.fill('[data-testid="palette-input"]', 'Smoke login 2')
  await main.keyboard.press('Enter')
  await main.waitForSelector('[data-testid="vault-item-title"] >> text=Smoke login 2')

  // Files: a file enters the library through the main process's dialog into
  // the folder shown and shows its way in; a file a page made has no path,
  // so it never reaches the daemon. The open file shows its status, and a
  // row's menu moves it to the trash, which lists it.
  await main.click('nav >> text=/^(Files|文件)$/')
  await main.waitForSelector('[data-testid="files-list"] >> text=report.pdf')
  await main.click('[data-testid="files-list"] >> text=report.pdf')
  await main.waitForSelector('[data-testid="files-status"] >> text=/Backed up/')
  await main.screenshot({ path: join(shots, 'files.png') })
  await answer('__open', join(picks, 'release.tar'))
  await main.click('[data-testid="files-add"]')
  await main.waitForSelector('[data-testid="files-job"][data-stage="done"] >> text=release.tar')
  assert.deepEqual(latest('files/import').params, { paths: [join(picks, 'release.tar')], folder: 'root' })
  await main.waitForSelector('[data-testid="files-name"] >> text=release.tar')
  const imports = calls.filter((c) => c.method === 'files/import').length
  await main.evaluate(() => {
    const transfer = new DataTransfer()
    transfer.items.add(new File(['made up'], 'made-up.txt'))
    const list = document.querySelector('[data-testid="files-list"]')
    for (const type of ['dragenter', 'dragover', 'drop']) list.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: transfer }))
  })
  await main.waitForSelector('[data-testid="files-job"][data-stage="error"] >> text=made-up.txt')
  assert.equal(calls.filter((c) => c.method === 'files/import').length, imports)
  await main.click('[data-testid="files-list"] >> text=release.tar', { button: 'right' })
  await main.click('[data-testid="files-row-menu"] >> text=/Move to trash/')
  await waitFor(() => latest('files/trash'))
  assert.deepEqual(latest('files/trash').params, { ids: ['26'.repeat(32)] })
  await main.click('[data-testid="files-scope-trash"]')
  await main.waitForSelector('[data-testid="files-trash"] >> text=release.tar')
  await main.click('[data-testid="files-scope-root"]')
  await main.click('nav >> text=/^(Vault|秘密库)$/')

  // Signatures: an application action and attestation bundles enter and
  // leave only through the main process's dialogs.
  await main.click('text=/Signatures|签名与授权/')
  await main.click('[data-testid="approvals-tab-sign"]')
  await answer('__open', join(picks, 'action.cbor'))
  await main.click('[data-testid="sign-action"]')
  await approval.waitForSelector('[data-testid="approval-command"]', { timeout: 20_000 })
  assert.equal(await approval.textContent('[data-testid="approval-command"]'), 'review')
  assert.equal(await approval.textContent('[data-testid="approval-app"]'), 'token-list')
  await approval.screenshot({ path: join(shots, 'app-action.png') })
  const bundle = join(profile, 'bundle-out.json')
  await answer('__save', bundle)
  await main.click('[data-testid="approvals-tab-bundles"]')
  await main.click('text=/^(Export bundle|导出证据包)$/')
  await main.waitForSelector('[data-testid="approvals-notice"]')
  assert.equal(JSON.parse(await readFile(bundle, 'utf8')).request_id, 'e1'.repeat(32))
  await answer('__open', join(picks, 'bundle.json'))
  await main.click('[data-testid="verify-bundle"] button >> nth=0')
  await main.waitForSelector('[data-testid="verify-report"]')
  assert.ok((await main.textContent('[data-testid="verify-report"]')).includes('https://dmsg.net'))
  await main.screenshot({ path: join(shots, 'signatures.png') })
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 1)

  // Settings lists this device's accounts, switches between them, and asks
  // the approval window to bind another login.
  await main.click('nav >> text=/Settings|设置/')
  await main.waitForSelector('[data-testid="local-accounts"]')
  await main.waitForSelector('[data-testid="settings-legacy"]')
  assert.ok((await main.textContent('[data-testid="local-accounts"]')).includes(other))
  await main.click('[data-testid="settings-security"]')
  await main.waitForSelector('[data-testid="logins"]')
  await main.fill('[data-testid="approve-request"]', 'binding-token')
  await main.fill('[data-testid="approve-shown"]', 'aaaaa-aa')
  await main.click('button:has-text("Approve the login"), button:has-text("批准绑定登录")')
  await approval.waitForSelector('[data-testid="approval-principal"]', { timeout: 20_000 })
  assert.equal(await approval.textContent('[data-testid="approval-principal"]'), 'aaaaa-aa')
  await approval.screenshot({ path: join(shots, 'bind-login.png') })
  // Another device is revoked only as an approval request.
  await main.click('[data-testid="settings-devices"]')
  await main.waitForSelector('[data-testid="security-snapshot"]')
  // The capabilities picked go to the daemon as plain values, though the
  // window keeps them as state, and the change is an approval request.
  await main.locator('[data-testid="device-capabilities"]').nth(1).click()
  await main.click('[data-testid="devices"] label:has-text("FormalApprove")')
  await main.click('[data-testid="capabilities-submit"]')
  await waitFor(() => latest('device/capabilities'))
  assert.deepEqual(latest('device/capabilities').params, { device: '12'.repeat(32), capabilities: ['ContentSign', 'VaultUnlock', 'FormalApprove'], source: 'gui' })
  await approval.waitForSelector('[data-testid="approval-title"] >> text=Capabilities', { timeout: 20_000 })
  await main.click('[data-testid="revoke-device"]')
  await approval.waitForSelector(`text=${'12'.repeat(32)}`, { timeout: 20_000 })
  assert.equal(latest('device/revoke').params.device, '12'.repeat(32))
  await approval.screenshot({ path: join(shots, 'approval-revoke.png') })
  await main.screenshot({ path: join(shots, 'settings.png') })
  // The public avatar: shown from a data URL, set from a picked image,
  // removed; the window never names a path itself.
  await main.click('[data-testid="settings-profile"]')
  await main.waitForSelector('[data-testid="profile-avatar"]')
  await answer('__open', join(picks, 'me.png'))
  await main.click('[data-testid="avatar-set"]')
  await waitFor(() => latest('profile/avatar')?.params.path === join(picks, 'me.png'))
  await main.locator('[data-testid="profile"]').screenshot({ path: join(shots, 'profile.png') })
  await main.click('[data-testid="avatar-remove"]')
  await waitFor(() => latest('profile/avatar')?.params.remove === true)
  await main.waitForSelector('[data-testid="avatar-remove"]', { state: 'detached' })
  await assert.rejects(main.evaluate(() => window.dmsg.call('profile/avatar', { path: '/etc/hosts' })), /FORBIDDEN/)
  // A locked daemon's hints notify that something arrived, never from
  // whom, and each kind at most once a minute.
  const arrived = async () =>
    (await app.evaluate(() => globalThis.__notes)).filter((n) => /letter|来信|channel messages|频道消息/.test(n))
  emit('hint', { kind: 'inbox' })
  emit('hint', { kind: 'inbox' })
  emit('hint', { kind: 'channel' })
  for (let i = 0; i < 50 && (await arrived()).length < 2; i++) await new Promise((r) => setTimeout(r, 100))
  assert.equal((await arrived()).length, 2)
  await main.click('[data-testid="settings-session"]')
  await main.click('[data-testid="local-accounts"] >> text=/^(Switch|切换)$/')
  await waitFor(() => calls.some((c) => c.method === 'session/switch'))
  assert.deepEqual(calls.find((c) => c.method === 'session/switch').params, { account: other })
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 1)

  // The language is picked in the main window only, kept in the profile, and
  // both windows follow it; Arabic reads right to left.
  await assert.rejects(approval.evaluate(() => window.dmsg.setLocale('fr')), /FORBIDDEN/)
  await main.click('[data-testid="locale"]')
  await main.waitForSelector('[data-testid="locale-menu"]')
  await main.screenshot({ path: join(shots, 'locale-menu.png') })
  await main.click('[data-testid="locale-menu"] [data-value="ar"]')
  await main.waitForSelector('html[lang="ar"][dir="rtl"]')
  await approval.waitForSelector('html[lang="ar"][dir="rtl"]')
  await main.waitForSelector('nav >> text=الإعدادات')
  assert.equal(await readFile(join(profile, 'locale'), 'utf8'), 'ar')
  await main.screenshot({ path: join(shots, 'locale-ar.png') })
  // The panes mirror right to left; IDs and code keep their own order.
  await main.keyboard.press(`${mod}+1`)
  await main.waitForSelector('[data-testid="vault-inspector"]')
  await main.screenshot({ path: join(shots, 'vault-ar.png') })
  await main.keyboard.press(`${mod}+3`)
  await main.click('[data-testid="channel-list"] button >> nth=0')
  await main.waitForSelector('[data-testid="channel-members"]')
  await main.screenshot({ path: join(shots, 'channels-ar.png') })
  await main.keyboard.press(`${mod}+,`)
  await choose(main, 'locale', 'fr')
  await approval.waitForSelector('html[lang="fr"][dir="ltr"]')
  await main.waitForSelector('nav >> text=Paramètres')
  await main.screenshot({ path: join(shots, 'locale-fr.png') })

  // The appearance is picked in the main window only and kept in the
  // profile; both windows follow it through nativeTheme.
  await assert.rejects(approval.evaluate(() => window.dmsg.setAppearance('dark')), /FORBIDDEN/)
  assert.ok(!(await main.evaluate(() => matchMedia('(prefers-color-scheme: dark)').matches)))
  await choose(main, 'appearance', 'dark')
  await main.waitForFunction(() => matchMedia('(prefers-color-scheme: dark)').matches)
  await approval.waitForFunction(() => matchMedia('(prefers-color-scheme: dark)').matches)
  assert.equal(await readFile(join(profile, 'appearance'), 'utf8'), 'dark')
  await main.waitForSelector('[data-testid="appearance-menu"]', { state: 'detached' })
  await main.screenshot({ path: join(shots, 'appearance-dark.png') })

  // Revoking a device takes a hold: a short press decides nothing, a second's hold approves.
  const decisions = () => calls.filter((c) => c.method === 'approval/respond').length
  const before = decisions()
  await approval.click('[data-testid="approve"]')
  await new Promise((r) => setTimeout(r, 300))
  assert.equal(decisions(), before)
  const box = await approval.locator('[data-testid="approve"]').boundingBox()
  await approval.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await approval.mouse.down()
  await waitFor(() => decisions() === before + 1)
  await approval.mouse.up()
  await approval.waitForSelector('[data-testid="approval-outcome"]')
  await approval.screenshot({ path: join(shots, 'approval-held.png') })
  console.log('desktop smoke test passed')
} finally {
  await app.close()
  server.close()
}
