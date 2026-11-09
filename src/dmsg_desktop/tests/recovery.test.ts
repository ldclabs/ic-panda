import assert from 'node:assert/strict'
import { test } from 'node:test'
import { WebSocketServer } from 'ws'
import { Daemon } from '../src/main/daemon.ts'
import { ReadRecovery } from '../src/main/recovery.ts'
import { isRead } from '../src/shared/requests.ts'

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))
async function until(check: () => boolean) {
  for (let i = 0; i < 200; i++) { if (check()) return; await wait(5) }
  assert.fail('recovery did not finish')
}
const unavailable = () => Object.assign(new Error('private URL'), { code: 'NETWORK_UNAVAILABLE' })

test('initialization owns the connection attempt and business calls are refused until it finishes', async (t) => {
  const server = new WebSocketServer({ port: 0, host: '127.0.0.1' })
  await new Promise<void>(resolve => server.on('listening', resolve))
  let initialize: (() => void) | undefined
  const methods: string[] = []
  server.on('connection', socket => socket.on('message', bytes => {
    const { id, method } = JSON.parse(String(bytes))
    methods.push(method)
    const reply = () => socket.send(JSON.stringify({ id, result: {} }))
    if (method === 'app/initialize') initialize = reply
    else reply()
  }))
  const daemon = new Daemon({ binary: '', home: '', fixed: { url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, token: 'test' } })
  t.after(() => { daemon.close(); for (const socket of server.clients) socket.terminate(); server.close() })
  const opening = daemon.connect()
  assert.equal(daemon.connect(), opening)
  await until(() => !!initialize)
  assert.equal(daemon.connected, false)
  await assert.rejects(daemon.call('vault/list'), { code: 'DAEMON_DISCONNECTED' })
  assert.deepEqual(methods, ['app/initialize'])
  initialize!()
  await opening
  assert.equal(daemon.connected, true)
  await daemon.call('vault/list')
  assert.deepEqual(methods, ['app/initialize', 'vault/list'])
})

test('closing an initializing connection cannot revive it or close its replacement', async (t) => {
  const server = new WebSocketServer({ port: 0, host: '127.0.0.1' })
  await new Promise<void>(resolve => server.on('listening', resolve))
  let initialized = 0, resyncs = 0
  server.on('connection', socket => socket.on('message', bytes => {
    const { id, method } = JSON.parse(String(bytes))
    if (method === 'app/initialize' && ++initialized > 1) socket.send(JSON.stringify({ id, result: {} }))
  }))
  const daemon = new Daemon({ binary: '', home: '', fixed: { url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, token: 'test' } })
  t.after(() => { daemon.close(); for (const socket of server.clients) socket.terminate(); server.close() })
  daemon.on('event', event => { if (event.type === 'resync') resyncs++ })
  const old = daemon.connect()
  await until(() => initialized === 1)
  daemon.close()
  await Promise.all([old, daemon.connect()])
  assert.equal(initialized, 2)
  assert.equal(resyncs, 1)
  assert.equal(daemon.connected, true)
  const read = assert.rejects(daemon.call('vault/list'), { code: 'DAEMON_DISCONNECTED' })
  const write = assert.rejects(daemon.call('vault/add', { title: 'fixture' }), { code: 'EXECUTION_UNKNOWN' })
  daemon.close()
  await Promise.all([read, write])
  assert.equal(daemon.connected, false)
  await wait(30)
  assert.equal(resyncs, 1)
})

test('only reviewed reads recover; account changes discard an old retry result', async () => {
  for (const method of ['plan/quote','wallet/quote','approval/respond','approval/quote','wallet/execute','wallet/propose','plan/checkout','agent/grant','ap/call','channel/send','inbox/ack','new/method']) assert.equal(isRead(method), false, method)
  let finish!: () => void
  let reads = 0, refreshes = 0
  const recovery = new ReadRecovery(async () => { reads++; await new Promise<void>((resolve) => { finish = resolve }) }, () => {}, () => refreshes++, 1)
  recovery.add('agent/principal', {}, recovery.generation)
  await until(() => reads === 1)
  recovery.reset()
  finish()
  await wait(10)
  assert.equal(recovery.state, 'idle')
  assert.equal(refreshes, 0)
  recovery.add('approval/respond', {}, recovery.generation)
  await wait(10)
  assert.equal(reads, 1)
  recovery.reset()
})

test('simultaneous read failures share recovery and continue after transient errors', async () => {
  let calls = 0, refreshes = 0
  const recovery = new ReadRecovery(async () => { if (++calls === 1) throw unavailable() }, () => {}, () => refreshes++, 1)
  recovery.add('agent/principal', { a: 1, b: 2 }, 0)
  recovery.add('agent/principal', { b: 2, a: 1 }, 0)
  await until(() => recovery.state === 'idle')
  assert.equal(calls, 2)
  assert.equal(refreshes, 1)
  // A later outage starts again from the first delay, not as still offline.
  recovery.add('agent/principal', {}, 0)
  assert.equal(recovery.state, 'recovering')
  await until(() => recovery.state === 'idle')
  recovery.reset()
})

test('desktop RPC recovers reads but never replays timed-out, uncertain or disconnected writes', async (t) => {
  const server = new WebSocketServer({ port: 0, host: '127.0.0.1' })
  await new Promise<void>((resolve) => server.on('listening', resolve))
  const address = server.address() as { port: number }
  const counts: Record<string, number> = {}
  server.on('connection', (socket) => socket.on('message', (raw) => {
    const { id, method } = JSON.parse(String(raw))
    counts[method] = (counts[method] ?? 0) + 1
    if (method === 'wallet/execute') return // Submitted, but reply lost.
    if (method === 'channel/send') { socket.close(); return }
    if (method === 'session/login_result' || method === 'profile/show') {
      socket.send(JSON.stringify({ method: 'event', params: { type: 'session/changed', data: { state: 'unlocked' } } }))
    }
    const code = method === 'approval/respond' ? 'EXECUTION_UNKNOWN' : method === 'agent/grant' || (method === 'agent/principal' && counts[method] === 1) ? 'NETWORK_UNAVAILABLE' : null
    socket.send(JSON.stringify(code ? { id, error: { message: 'private URL', data: { code } } } : { id, result: {} }))
  }))
  const daemon = new Daemon({ binary: '', home: '', fixed: { url: `http://127.0.0.1:${address.port}`, token: 'test' }, timing: { read: 100, write: 20, recovery: 1 } })
  t.after(() => { daemon.close(); for (const socket of server.clients) socket.terminate(); server.close() })
  await daemon.connect()
  await assert.rejects(daemon.call('agent/principal'), { code: 'NETWORK_UNAVAILABLE' })
  await until(() => counts['agent/principal'] === 2 && daemon.recovery.state === 'idle')
  // Account data is fenced, while the login's own completion survives unlock.
  await assert.rejects(daemon.call('profile/show'), { code: 'LOCKED' })
  assert.deepEqual(await daemon.call('session/login_result'), {})
  const first = daemon.call('wallet/execute', { id: 'same-operation' })
  const second = daemon.call('wallet/execute', { id: 'same-operation' })
  assert.equal(first, second)
  await assert.rejects(first, { code: 'EXECUTION_UNKNOWN' })
  await assert.rejects(daemon.call('approval/respond', { id: 'approved' }), { code: 'EXECUTION_UNKNOWN' })
  // A write whose own reads failed is not retried, so it does not say it will be.
  await assert.rejects(daemon.call('agent/grant', { id: 'grant' }), { code: 'UNAVAILABLE' })
  assert.equal(daemon.recovery.state, 'idle')
  await assert.rejects(daemon.call('channel/send', { id: 'channel' }), { code: 'EXECUTION_UNKNOWN' })
  await assert.rejects(daemon.call('agent/grant', { id: 'offline' }), { code: 'DAEMON_DISCONNECTED' })
  await wait(50)
  assert.equal(counts['agent/grant'], 1)
  assert.equal(counts['wallet/execute'], 1)
  assert.equal(counts['approval/respond'], 1)
  assert.equal(counts['channel/send'], 1)
})

test('entitlement updates are never queued as ordinary read recovery', () => {
  assert.equal(isRead('plan/status'), false)
  assert.equal(isRead('plan/status', { refresh: true }), false)
  assert.equal(isRead('plan/status', { refresh: false }), true)
  assert.equal(isRead('plan/status', { mode: 'local' }), true)
  const recovery = new ReadRecovery(async () => assert.fail('must not replay an update'), () => {}, () => {}, 1)
  recovery.add('plan/status', { refresh: true }, recovery.generation)
  assert.equal(recovery.state, 'idle')
  recovery.reset()
})

test('cloud downloads outlive the normal write deadline without replay', async (t) => {
  const server = new WebSocketServer({ port: 0, host: '127.0.0.1' })
  await new Promise<void>(resolve => server.on('listening', resolve))
  const counts: Record<string, number> = {}
  server.on('connection', socket => socket.on('message', raw => {
    const { id, method } = JSON.parse(String(raw))
    counts[method] = (counts[method] ?? 0) + 1
    setTimeout(() => socket.send(JSON.stringify({ id, result: { verified: true } })), method === 'app/initialize' ? 0 : 80)
  }))
  const port = (server.address() as { port: number }).port
  const daemon = new Daemon({ binary: '', home: '', fixed: { url: `http://127.0.0.1:${port}`, token: 'test' }, timing: { write: 20, read: 100 } })
  t.after(() => { daemon.close(); for (const socket of server.clients) socket.terminate(); server.close() })
  await daemon.connect()
  for (const method of ['files/cloud_fetch', 'files/cloud_verify']) {
    assert.deepEqual(await daemon.call(method, { upload_id: 'same-backup' }), { verified: true })
    assert.equal(counts[method], 1)
  }
})
