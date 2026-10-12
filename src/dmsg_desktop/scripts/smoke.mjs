// Electron smoke test: the built app against a mock daemon, in a temporary
// profile. Checks the main window, the approval window opened by a dmsg://
// link, that only the approval window decides, and that it decides what it
// showed, and that the channel, inbox and plan views reach the daemon with
// payments only as approval requests. Run with `pnpm test:e2e`.
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir } from 'node:fs/promises'
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
      [peer]: { account: peer, role: 'member', joined_epoch: 1, can_rotate: false, history: [] }
    }
  },
  message_cursor: 1,
  readable_epochs: [1]
}
const messages = [{ seq: 1, account: peer, device: '55'.repeat(32), message_id: '66'.repeat(32), epoch: 1, created_at: Date.now(), stored_at: Date.now(), text: 'Hello channel', file: null }]
const ledger = 'ryjl3-tyaaa-aaaaa-aaaba-cai'
const checkouts = []
const calls = []
const requests = new Map([[request.id, request]])
let sockets = []
function emit(type) {
  for (const s of sockets) s.send(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { seq: 1, type } }))
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
const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'daemon/status': status,
  'vault/list': () => [
    { id: '22'.repeat(32), item: { title: 'Smoke note', type: 'note', body: 'hello', updatedAt: Date.now() }, synced: true, conflict: false }
  ],
  'approval/list': () => [...requests.values()],
  'approval/show': (p) => requests.get(p.id),
  'approval/respond': (p) => {
    const r = requests.get(p.id)
    assert.equal(p.digest, r.digest)
    r.state = p.approve ? 'done' : 'rejected'
    emit('approval/changed')
    return r
  },
  'attest/history': () => [],
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
  'account/show': () => ({ devices: [{ current: true, capabilities: ['ContentSign', 'FormalApprove'] }] }),
  'connect/pairings': () => [],
  'connect/sync': () => ({ fetched: 0 }),
  'channel/list': () => [channel],
  'channel/show': () => channel,
  'channel/messages': () => messages,
  'channel/send': (p) => {
    messages.push({ ...messages[0], seq: messages.length + 1, account, text: p.text })
    return { seq: messages.length }
  },
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
try {
  const main = await app.firstWindow()
  await main.waitForSelector('text=Smoke note', { timeout: 20_000 })
  assert.equal(await main.textContent('[data-testid="waiting-count"]'), '1')
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
  assert.deepEqual(responded[0].params, { id: request.id, digest: request.digest, approve: true, human: true })

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
  await main.screenshot({ path: join(shots, 'channels.png') })

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
  console.log('desktop smoke test passed')
} finally {
  await app.close()
  server.close()
}
