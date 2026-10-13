// Electron smoke test: the built app against a mock daemon, in a temporary
// profile. Checks the main window, the approval window opened by a dmsg://
// link, that only the approval window decides, and that it decides what it
// showed, and that the channel, inbox, plan and agent views reach the daemon
// with payments and grants only as approval requests; that another account's
// workspace is selected from Settings, that binding a login and a web
// checkout are approval requests, and that a web checkout is quoted before it
// can be approved; that a kept legacy archive reads offline and transfer
// files never pass through a window; that a vault secret is shown or copied
// only after a second click and the copy never passes through a window; that
// conflict copies resolve; that a device is revoked only through the approval
// window; that channel files, roles, history, archiving and transfers reach
// the daemon; that the Agents view runs only the Mail and Knowledge tools and
// manages MCP Events endpoints; that a delivery notifies without its content;
// and that application actions and attestation bundles enter through the main
// process's dialogs. Run with `pnpm test:e2e`.
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
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
  item('23'.repeat(32), 'r2', { title: 'report.pdf', type: 'file', file: { name: 'report.pdf', size: 1234 } })
]
let conflicts = [{ ...item('22'.repeat(32), 'r9', { title: 'Smoke note', type: 'note', body: 'hello from the laptop' }), conflict: true }]
const endpoints = []
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
    method: 'checkout',
    kind: 'checkout',
    content: { product_id: 'token-list', sku: 'pro', amount_usd_micros: '3000000', method: 'Cash' }
  }
}
const checkouts = []
const calls = []
const requests = new Map([
  [request.id, request],
  [checkoutRequest.id, checkoutRequest]
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
    pending_sync: 0,
    background_online: false
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
  'daemon/status': status,
  'vault/list': () => vault,
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
  'profile/show': () => null,
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
    const file = vault.find((e) => e.id === p.item).item.file
    messages.push({ ...messages[0], seq: messages.length + 1, account, text: p.text, file })
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
  'plan/status': () => ({ plan: 'Free', storage_bytes: 1_000_000_000 }),
  'plan/catalog': () => ({
    version: 1,
    plans: [
      { plan: 'Free', price_cents: 0, storage_bytes: 1_000_000_000 },
      { plan: 'Plus', price_cents: 500, storage_bytes: 50_000_000_000 }
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
    const endpoint = { endpoint_id: 'ee'.repeat(16), url: `https://hooks.dmsg.net/mcp-events/${'ee'.repeat(16)}`, state: 'active', agent: null, context: p.context, created_at: Date.now(), binding: {} }
    endpoints.push(endpoint)
    return { ...endpoint, secret: 'whsec_smoke' }
  },
  'grants/list': () => [],
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
await new Promise((r) => server.listen(0, '127.0.0.1', r))
const port = server.address().port
const profile = await mkdtemp(join(tmpdir(), 'dmsg-desktop-smoke-'))
const shots = resolve('test-results')
await mkdir(shots, { recursive: true })

const app = await electron.launch({
  executablePath: electronPath,
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
try {
  const main = await app.firstWindow()
  await main.waitForSelector('text=Smoke note', { timeout: 20_000 })
  assert.equal(await main.textContent('[data-testid="waiting-count"]'), '2')
  await main.screenshot({ path: join(shots, 'main.png') })

  // The main window cannot decide, even through the bridge's own methods.
  await assert.rejects(main.evaluate((id) => window.dmsg.decide(id, 'cd'.repeat(32), true), request.id), /FORBIDDEN/)
  await assert.rejects(
    main.evaluate((id) => window.dmsg.call('approval/respond', { id, digest: 'cd'.repeat(32), approve: true, human: true }), request.id),
    /FORBIDDEN/
  )
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 0)

  // A dmsg:// link only opens the approval window on the daemon's request.
  const opened = app.waitForEvent('window')
  await app.evaluate(({ app }, url) => app.emit('open-url', { preventDefault() {} }, url), `dmsg://connect/request?account=${account}&request=${request.id}`)
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
  await main.fill('[data-testid="statement-text"]', 'Signed from the desktop')
  await main.click('[data-testid="statement-submit"]')
  await approval.waitForSelector('text=Signed from the desktop', { timeout: 20_000 })
  const created = calls.find((c) => c.method === 'attest/request')
  assert.equal(created.params.source, 'gui')

  // Channels read and write through the daemon.
  await main.click('nav >> text=/Channels|频道/')
  await main.click('text=Smoke channel')
  await main.waitForSelector('text=Hello channel')
  await main.fill('[data-testid="channel-text"]', 'Reply from the desktop')
  await main.click('button:has-text("Send"), button:has-text("发送")')
  await main.waitForSelector('text=Reply from the desktop')

  // A vault file goes with a message; a shared file is saved where the main
  // process's dialog says; roles, history, a transfer proposal and archiving
  // reach the daemon from the owner's view.
  await main.selectOption('[data-testid="channel-attachment"]', '23'.repeat(32))
  await main.fill('[data-testid="channel-text"]', 'With a file')
  await main.click('button:has-text("Send"), button:has-text("发送")')
  await waitFor(() => latest('channel/send_file'))
  assert.deepEqual(latest('channel/send_file').params, { id: channel.id, item: '23'.repeat(32), text: 'With a file' })
  const saved = join(profile, 'plan.pdf')
  await answer('__save', saved)
  await main.locator('[data-testid="channel-messages"] button', { hasText: /Download|下载/ }).first().click()
  await waitFor(() => latest('channel/download'))
  assert.deepEqual(latest('channel/download').params, { id: channel.id, seq: 2, path: saved })
  await main.waitForSelector('[data-testid="channel-notice"]')
  await main.selectOption('[data-testid="member-role"]', 'admin')
  await waitFor(() => latest('channel/control'))
  assert.deepEqual(latest('channel/control').params, { id: channel.id, action: { type: 'role', account: peer, role: 'admin', can_rotate: false } })
  await main.click('[data-testid="channel-members"] >> text=/Grant history|授权历史/')
  await main.click('[data-testid="history-submit"]')
  await waitFor(() => latest('channel/history'))
  assert.deepEqual(latest('channel/history').params, { id: channel.id, account: peer, from: 1, to: 1 })
  await main.fill('[data-testid="transfer-target"]', peer)
  await main.click('[data-testid="owner-propose"]')
  await main.waitForSelector('[data-testid="channel-handed"]')
  assert.ok((await main.inputValue('[data-testid="channel-handed"]')).includes('dmsg-channel-owner-transfer/1'))
  await main.screenshot({ path: join(shots, 'channels.png') })
  await main.click('[data-testid="channel-archive"]')
  await main.click('[data-testid="channel-reactivate"]')
  await waitFor(() => latest('channel/reactivate'))
  assert.equal(channel.ledger.status, 'active')

  await main.click('nav >> text=/Inbox|收件箱/')
  await main.waitForSelector('text=A paid letter')
  await main.screenshot({ path: join(shots, 'inbox.png') })

  // A subscription is quoted, then paid only once the approval window agrees.
  await main.click('nav >> text=/Plan & names|会员与名称/')
  await main.waitForSelector('[data-testid="plan-name"]')
  assert.equal(await main.textContent('[data-testid="plan-name"]'), 'Free')
  await main.click('button:has-text("Subscribe"), button:has-text("订阅")')
  await approval.waitForSelector('[data-testid="approval-sku"]', { timeout: 20_000 })
  assert.ok((await approval.textContent('[data-testid="approval-amount"]')).includes('5 Icp'))
  await approval.waitForSelector('[data-testid="approve"]')
  await approval.screenshot({ path: join(shots, 'checkout.png') })
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
  await main.waitForSelector('[data-testid="audit-entries"] >> text=dmsg_vault_read')
  await main.selectOption('[data-testid="audit-actor"]', 'owner')
  await waitFor(() => latest('audit/list').params.agent === 'owner')
  await main.waitForSelector('[data-testid="audit-entries"] >> text=grants/add')
  assert.equal(await main.locator('[data-testid="audit-entries"] >> text=dmsg_vault_read').count(), 0)
  await main.selectOption('[data-testid="audit-actor"]', '')
  await main.waitForSelector('[data-testid="audit-entries"] >> text=dmsg_vault_read')
  await main.locator('[data-testid="agent-audit"]').screenshot({ path: join(shots, 'audit.png') })
  await main.screenshot({ path: join(shots, 'agents.png') })

  // A web checkout cannot be approved before it is quoted, and the quote is what is approved.
  await app.evaluate(({ app }, url) => app.emit('open-url', { preventDefault() {} }, url), `dmsg://connect/request?account=${account}&request=${checkoutRequest.id}`)
  await approval.waitForSelector('[data-testid="checkout-quote"]', { timeout: 20_000 })
  assert.equal(await approval.isDisabled('[data-testid="approve"]'), true)
  await approval.click('button:has-text("Get a quote"), button:has-text("获取报价")')
  await approval.waitForSelector('[data-testid="approval-amount"]', { timeout: 20_000 })
  assert.ok((await approval.textContent('[data-testid="approval-amount"]')).includes('3 Icp'))
  assert.equal(await approval.isDisabled('[data-testid="approve"]'), false)
  await approval.screenshot({ path: join(shots, 'connect-checkout.png') })

  // Legacy migration: the offer shows its fingerprint and target device, a
  // kept archive reads offline, and transfer files reach the daemon only
  // through the main process's dialog.
  await main.click('nav >> text=/Legacy|旧版迁移/')
  await main.waitForSelector('[data-testid="legacy-archives"]')
  await main.check(`input[value="${offer.nonce}"]`)
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

  // Vault: a secret is shown or copied only after a second click, and the
  // copy goes from the daemon to the clipboard through the main process; an
  // edit names the version it replaces; a conflict copy is adopted.
  await main.click('nav >> text=/Vault|秘密库/')
  await main.click('text=Smoke login')
  await main.waitForSelector('[data-testid="vault-secret"]')
  assert.equal(await main.textContent('[data-testid="vault-secret"]'), '********')
  const revealed = () => calls.filter((c) => c.method === 'vault/show' && c.params.reveal).length
  await main.click('[data-testid="reveal-secret"]')
  await main.waitForSelector('[data-testid="secret-confirm"]')
  assert.equal(revealed(), 0)
  await main.click('[data-testid="secret-confirm-yes"]')
  await main.waitForSelector(`text=${SECRET}`)
  await main.click('[data-testid="copy-secret"]')
  await main.click('[data-testid="secret-confirm-yes"]')
  await main.waitForSelector('[data-testid="secret-copied"]')
  assert.equal(await app.evaluate(() => globalThis.__clip), SECRET)
  assert.equal(revealed(), 2)
  await main.click('[data-testid="vault-detail"] ~ div >> text=/^(Edit|编辑)$/')
  await main.fill('[data-testid="vault-edit"] input >> nth=0', 'Smoke login 2')
  await main.click('[data-testid="vault-save"]')
  await waitFor(() => latest('vault/edit'))
  assert.deepEqual(latest('vault/edit').params, {
    id: '24'.repeat(32),
    base: 'r3',
    changes: { title: 'Smoke login 2', tags: ['work'], body: '', username: 'panda', url: 'https://example.com' }
  })
  await main.selectOption('[data-testid="vault-type"]', 'login')
  await main.fill('[data-testid="vault-title"]', 'New login')
  await main.fill('[data-testid="vault-new-secret"]', 'new-secret')
  await main.click('[data-testid="vault-add"]')
  await waitFor(() => latest('vault/put'))
  assert.equal(latest('vault/put').params.item.type, 'login')
  assert.equal(latest('vault/put').params.item.secret, 'new-secret')
  await main.waitForSelector('[data-testid="vault-conflicts"]')
  await main.screenshot({ path: join(shots, 'vault.png') })
  await main.click('[data-testid="vault-conflicts"] >> text=/^(Use this copy|采用这个副本)$/')
  await waitFor(() => latest('vault/resolve'))
  assert.deepEqual(latest('vault/resolve').params, { key: `${'22'.repeat(32)}:r9`, keep: 'theirs' })

  // Signatures: an application action and attestation bundles enter and
  // leave only through the main process's dialogs.
  await main.click('text=/Signatures|签名与授权/')
  await answer('__open', join(picks, 'action.cbor'))
  await main.click('[data-testid="sign-action"]')
  await approval.waitForSelector('[data-testid="approval-command"]', { timeout: 20_000 })
  assert.equal(await approval.textContent('[data-testid="approval-command"]'), 'review')
  assert.equal(await approval.textContent('[data-testid="approval-app"]'), 'token-list')
  await approval.screenshot({ path: join(shots, 'app-action.png') })
  const bundle = join(profile, 'bundle-out.json')
  await answer('__save', bundle)
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
  assert.ok((await main.textContent('[data-testid="local-accounts"]')).includes(other))
  await main.waitForSelector('[data-testid="logins"]')
  await main.fill('[data-testid="approve-request"]', 'binding-token')
  await main.fill('[data-testid="approve-shown"]', 'aaaaa-aa')
  await main.click('button:has-text("Approve the login"), button:has-text("批准绑定登录")')
  await approval.waitForSelector('[data-testid="approval-principal"]', { timeout: 20_000 })
  assert.equal(await approval.textContent('[data-testid="approval-principal"]'), 'aaaaa-aa')
  await approval.screenshot({ path: join(shots, 'bind-login.png') })
  // Another device is revoked only as an approval request.
  await main.waitForSelector('[data-testid="security-snapshot"]')
  await main.click('[data-testid="revoke-device"]')
  await approval.waitForSelector(`text=${'12'.repeat(32)}`, { timeout: 20_000 })
  assert.equal(latest('device/revoke').params.device, '12'.repeat(32))
  await main.screenshot({ path: join(shots, 'settings.png') })
  await main.click('[data-testid="local-accounts"] >> text=/^(Switch|切换)$/')
  await waitFor(() => calls.some((c) => c.method === 'session/switch'))
  assert.deepEqual(calls.find((c) => c.method === 'session/switch').params, { account: other })
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 1)
  console.log('desktop smoke test passed')
} finally {
  await app.close()
  server.close()
}
