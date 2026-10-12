import assert from 'node:assert/strict'
import { mock, test } from 'node:test'
import { Bridge } from '../src/bridge.ts'
import { base64, hex, unhex, utf8 } from '../src/codec.ts'
import { p256Pair, p256Sign, spki } from '../src/crypto.ts'
import { ConnectError } from '../src/errors.ts'
import { BridgePage, type PageHooks, type Target } from '../src/page.ts'
import { PROTOCOL, proofMessage, type Command } from '../src/protocol.ts'

const APP = 'http://localhost:5173'

/** A bridge whose certified registration is fixed; no network. */
function bridge(app: Partial<Record<string, unknown>> = {}) {
  const b = new Bridge({
    environment: 'Local',
    relay: 'http://127.0.0.1:1',
    ic: { host: 'http://127.0.0.1:1', rootKey: new Uint8Array(), handle: 'aaaaa-aa', commerce: 'aaaaa-aa' }
  })
  const registration = {
    environment: 'Local',
    app_id: 'token-list',
    config_version: 1n,
    origins: [APP],
    product_ids: [],
    capabilities: ['SignDocument'],
    profiles: ['TextStatementV1'],
    paused: false,
    ...app
  }
  b.certified.application = async (id: string) => {
    if (id !== 'token-list') throw new ConnectError('APP_NOT_REGISTERED')
    return registration as never
  }
  return b
}

function opener(): Target & { sent: { message: any; target: string }[]; closed: boolean } {
  const w = {
    sent: [] as { message: any; target: string }[],
    closed: false,
    postMessage(message: unknown, target: string) {
      w.sent.push({ message, target })
    }
  }
  return w
}

function page(b: Bridge, w: ReturnType<typeof opener>) {
  const events: string[] = []
  let current: Target | null = w
  const hooks: PageHooks = {
    opener: () => current,
    connected: (c) => events.push(`connected ${c.origin}`),
    refused: (origin, code) => events.push(`refused ${origin} ${code}`),
    ended: () => events.push('ended'),
    done: (method, outcome) => events.push(`${method} ${outcome.error ?? outcome.value?.state}`),
    beforeCreate: async () => {
      events.push('gate')
    }
  }
  const p = new BridgePage(b, hooks)
  return { p, events, setOpener: (t: Target | null) => (current = t) }
}

const hello = { protocol: PROTOCOL, type: 'hello', appId: 'token-list' }

async function session() {
  const pair = await p256Pair()
  return { pair, publicKey: base64(await spki(pair.publicKey)) }
}

async function connected() {
  const w = opener()
  const b = bridge()
  const { p, events, setOpener } = page(b, w)
  await p.receive({ source: w, origin: APP, data: hello })
  const ready = w.sent.at(-1)!.message
  assert.equal(ready.type, 'ready')
  return { w, b, p, events, setOpener, connectionId: ready.connectionId as string, s: await session() }
}

function command(publicKey: string, method = 'getOperation', operationId = 'a1'.repeat(32), payload: string | null = null): Command {
  return { method, appId: 'token-list', operationId, publicKey, payload, resultDigest: null }
}

/** Send a request, answer its challenge with `sign`, return the result frame. */
async function roundTrip(
  ctx: Awaited<ReturnType<typeof connected>>,
  c: Command,
  sign: (nonce: Uint8Array, connectionId: Uint8Array) => Command | { command: Command; connectionId?: Uint8Array; nonce?: Uint8Array } = () => c
) {
  const requestId = hex(crypto.getRandomValues(new Uint8Array(32)))
  await ctx.p.receive({ source: ctx.w, origin: APP, data: { protocol: PROTOCOL, type: 'request', requestId, command: c } })
  const challenge = ctx.w.sent.at(-1)!.message
  assert.equal(challenge.type, 'challenge', JSON.stringify(challenge))
  assert.equal(challenge.origin, APP)
  assert.equal(challenge.connectionId, ctx.connectionId)
  const nonce = unhex(challenge.nonce, 32)
  const connectionId = unhex(ctx.connectionId, 32)
  const choice = sign(nonce, connectionId)
  const signed = 'command' in choice ? choice : { command: choice }
  const signature = await p256Sign(
    ctx.s.pair.privateKey,
    proofMessage(signed.nonce ?? nonce, APP, signed.connectionId ?? connectionId, signed.command)
  )
  await ctx.p.receive({ source: ctx.w, origin: APP, data: { protocol: PROTOCOL, type: 'proof', requestId, signature: base64(signature) } })
  return { result: ctx.w.sent.at(-1)!.message, requestId, signature }
}

test('DT20: only the opener’s hello from a registered origin connects', async () => {
  const w = opener()
  const { p, events } = page(bridge(), w)
  // Another window, an opaque origin, a malformed hello: ignored.
  await p.receive({ source: opener(), origin: APP, data: hello })
  await p.receive({ source: w, origin: 'null', data: hello })
  await p.receive({ source: w, origin: APP, data: { ...hello, extra: 1 } })
  await p.receive({ source: w, origin: APP, data: { ...hello, appId: 'Bad App' } })
  assert.equal(w.sent.length, 0)
  // A registered app on another origin is refused there.
  await p.receive({ source: w, origin: 'http://localhost:6000', data: hello })
  assert.deepEqual(w.sent.at(-1), {
    message: { protocol: PROTOCOL, type: 'rejected', error: 'APP_NOT_REGISTERED' },
    target: 'http://localhost:6000'
  })
  await p.receive({ source: w, origin: APP, data: hello })
  const ready = w.sent.at(-1)!
  assert.equal(ready.target, APP)
  assert.equal(ready.message.type, 'ready')
  assert.equal(ready.message.origin, APP)
  assert.match(ready.message.connectionId, /^[0-9a-f]{64}$/)
  assert.deepEqual(events, ['refused http://localhost:6000 APP_NOT_REGISTERED', `connected ${APP}`])

  const paused = page(bridge({ paused: true }), opener())
  const pw = paused.p as BridgePage
  const target = opener()
  paused.setOpener(target)
  await pw.receive({ source: target, origin: APP, data: hello })
  assert.equal(target.sent.at(-1)!.message.error, 'APP_PAUSED')

  const old = page(bridge(), opener())
  const browser = opener()
  old.setOpener(browser)
  old.p.supported = false
  await old.p.receive({ source: browser, origin: APP, data: hello })
  assert.equal(browser.sent.at(-1)!.message.error, 'UNSUPPORTED_BROWSER')
})

test('DT20: frames from another window or origin are ignored; a gone or replaced opener ends the connection', async () => {
  const ctx = await connected()
  const before = ctx.w.sent.length
  const frame = { protocol: PROTOCOL, type: 'capabilities', requestId: '11'.repeat(32) }
  await ctx.p.receive({ source: opener(), origin: APP, data: frame })
  await ctx.p.receive({ source: ctx.w, origin: 'http://localhost:6000', data: frame })
  assert.equal(ctx.w.sent.length, before)
  await ctx.p.receive({ source: ctx.w, origin: APP, data: frame })
  assert.deepEqual(ctx.w.sent.at(-1)!.message.value.methods.slice(0, 2), ['authenticate', 'signDocument'])

  ctx.setOpener(opener())
  ctx.p.tick()
  assert.deepEqual(ctx.events.slice(-1), ['ended'])
  await ctx.p.receive({ source: ctx.w, origin: APP, data: frame })
  assert.equal(ctx.w.sent.at(-1)!.message.type, 'result')
  assert.equal(ctx.w.sent.length, before + 1, 'nothing after the end')

  const again = await connected()
  again.w.closed = true
  again.p.tick()
  assert.equal(again.p.current, null)
})

test('DT21: tampered commands, connections and nonces fail the proof', async () => {
  const ctx = await connected()
  const c = command(ctx.s.publicKey)
  // A valid proof reaches the method: no pairing here, so NOT_PAIRED.
  assert.equal((await roundTrip(ctx, c)).result.error, 'NOT_PAIRED')
  for (const tamper of [
    () => ({ ...c, operationId: 'b2'.repeat(32) }),
    () => ({ ...c, method: 'cancelOperation' }),
    () => ({ command: c, connectionId: unhex('55'.repeat(32)) }),
    () => ({ command: c, nonce: unhex('44'.repeat(32)) })
  ]) {
    const { result } = await roundTrip(ctx, c, tamper)
    assert.equal(result.ok, false)
    assert.equal(result.error, 'FORBIDDEN')
  }
  // Another session key cannot speak for this one.
  const stranger = await session()
  const { result } = await roundTrip(ctx, command(stranger.publicKey))
  assert.equal(result.error, 'FORBIDDEN')

  // A nonce answers one proof only.
  const first = await roundTrip(ctx, c)
  await ctx.p.receive({
    source: ctx.w,
    origin: APP,
    data: { protocol: PROTOCOL, type: 'proof', requestId: first.requestId, signature: base64(first.signature) }
  })
  assert.equal(ctx.w.sent.at(-1)!.message.error, 'FORBIDDEN')

  // A command for another app is refused before any challenge.
  await ctx.p.receive({
    source: ctx.w,
    origin: APP,
    data: { protocol: PROTOCOL, type: 'request', requestId: '22'.repeat(32), command: { ...c, appId: 'other-app' } }
  })
  assert.equal(ctx.w.sent.at(-1)!.message.error, 'FORBIDDEN')
})

test('DT21: a payload that does not bind the operation is refused after the proof', async () => {
  const ctx = await connected()
  const operationId = 'c3'.repeat(32)
  const document = { protocol: PROTOCOL, method: 'signature.request', requestId: 'd4'.repeat(32), accountId: 'x', expiresAt: String(Date.now() + 60_000), statement: { content: { kind: 'text', text: 'hi' } } }
  const c = command(ctx.s.publicKey, 'signDocument', operationId, base64(utf8(JSON.stringify(document))))
  const { result } = await roundTrip(ctx, c)
  assert.equal(result.error, 'INTEGRITY_FAILED')
  assert.ok(ctx.events.includes('gate'), 'create methods pass the click gate first')
  const other = { ...document, requestId: operationId, protocol: 'dmsg-extension/4' }
  const c2 = command(ctx.s.publicKey, 'signDocument', operationId, base64(utf8(JSON.stringify(other))))
  assert.equal((await roundTrip(ctx, c2)).result.error, 'INTEGRITY_FAILED')
  const digest = { ...document, requestId: operationId, statement: { content: { kind: 'digest' } } }
  const c3 = command(ctx.s.publicKey, 'signDocument', operationId, base64(utf8(JSON.stringify(digest))))
  assert.equal((await roundTrip(ctx, c3)).result.error, 'FORBIDDEN', 'profile not registered')
})

test('DT21: nonces expire after 15 seconds; at most 16 wait for proofs', async () => {
  const ctx = await connected()
  const c = command(ctx.s.publicKey)
  mock.timers.enable({ apis: ['Date'], now: Date.now() })
  try {
    const requestId = '33'.repeat(32)
    await ctx.p.receive({ source: ctx.w, origin: APP, data: { protocol: PROTOCOL, type: 'request', requestId, command: c } })
    const challenge = ctx.w.sent.at(-1)!.message
    const signature = await p256Sign(
      ctx.s.pair.privateKey,
      proofMessage(unhex(challenge.nonce), APP, unhex(ctx.connectionId), c)
    )
    mock.timers.tick(15_001)
    await ctx.p.receive({ source: ctx.w, origin: APP, data: { protocol: PROTOCOL, type: 'proof', requestId, signature: base64(signature) } })
    assert.equal(ctx.w.sent.at(-1)!.message.error, 'EXPIRED')
    for (let i = 0; i < 16; i++)
      await ctx.p.receive({
        source: ctx.w,
        origin: APP,
        data: { protocol: PROTOCOL, type: 'request', requestId: hex(Uint8Array.of(i)).padStart(64, 'e'), command: c }
      })
    await ctx.p.receive({ source: ctx.w, origin: APP, data: { protocol: PROTOCOL, type: 'request', requestId: 'ff'.repeat(32), command: c } })
    assert.equal(ctx.w.sent.at(-1)!.message.error, 'QUOTA_EXCEEDED')
  } finally {
    mock.timers.reset()
  }
})
