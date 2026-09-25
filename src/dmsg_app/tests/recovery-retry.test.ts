import { beforeEach, afterEach, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import { startChannel } from '../src/lib/protocol/channel'
import { xidText } from '../src/lib/protocol/identity'
import { InboxClient } from '../src/lib/services/inbox'
import { Principal } from '@icp-sdk/core/principal'
import { readCloudCommand } from '../src/lib/protocol/cloud'
import { ed25519 } from '../src/lib/crypto/primitives'
import { id } from '../src/lib/protocol/codec'
const password = 'review-only-local-fixture'
const accountId = xidText(new Uint8Array(12).fill(1))
const note = (body = 'a') => ({
  type: 'note',
  title: 'Review',
  body,
  username: '',
  secret: '',
  tags: [],
  url: '',
  favorite: false,
  createdAt: 1,
  updatedAt: 1
})
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
afterEach(() => vi.restoreAllMocks())
async function fresh() {
  const engine = new CryptoEngine()
  const setup = await engine.initialize(password)
  await engine.verifyRecovery(setup.recoveryCode)
  return { engine, setup }
}
it('restores every unsynced ancestor into the outbox', async () => {
  const { engine, setup } = await fresh()
  const a = await engine.saveItem({ item: note() as any })
  const b = await engine.saveItem({ item: note('b') as any, id: a.id, base: a.revision })
  expect((await engine.view()).outbox).toHaveLength(2)
  const backup = await engine.exportBackup(password)
  await engine.lock()
  globalThis.indexedDB = new IDBFactory()
  const restored = new CryptoEngine()
  await restored.restore({
    file: new File([backup.blob], 'review.dmsg'),
    code: setup.recoveryCode,
    password
  })
  const view = await restored.view()
  expect(new Set(view.outbox.map((job) => job.id))).toEqual(new Set([a.revision, b.revision]))
  expect(view.entries[0].record.parent).toBe(a.revision)
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  expect(await db.db.count('objects')).toBe(2)
  db.db.close()
  await restored.lock()
})
it('preserves resolved conflicts through later edits and offline recovery', async () => {
  const { engine, setup } = await fresh()
  const a = await engine.saveItem({ item: note() as any })
  const b = await engine.saveItem({ item: note('b') as any, id: a.id, base: a.revision })
  const c = await engine.saveItem({ item: note('c') as any, id: a.id, base: a.revision })
  const resolved = await engine.resolveConflict({ key: c.key, base: b.revision })
  await engine.saveItem({ item: note('later edit') as any, id: a.id, base: resolved.revision })
  expect((await engine.view()).conflicts).toHaveLength(0)
  const backup = await engine.exportBackup(password)
  await engine.lock()
  globalThis.indexedDB = new IDBFactory()
  const restored = new CryptoEngine()
  await restored.restore({
    file: new File([backup.blob], 'review.dmsg'),
    code: setup.recoveryCode,
    password
  })
  expect((await restored.view()).conflicts).toHaveLength(0)
  await restored.lock()
})
it('does not enqueue unchanged channel refreshes or empty receives', async () => {
  const { engine } = await fresh()
  const channel = id(),
    genesis = id()
  await engine.channelRemember({ channel, name: 'review', genesis })
  const ledger = startChannel(
    { channel_id: channel, nonce: id(), type: 'collaboration' },
    accountId,
    genesis
  )
  await engine.channelAdvance(channel, ledger, 1, [])
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  const n = await db.db.count('objects'),
    jobs = await db.db.count('outbox')
  for (let i = 0; i < 20; i++) {
    await engine.channelAdvance(channel, ledger, 1, [])
    await engine.channelReceive(channel, [], 0)
  }
  expect(await db.db.count('objects')).toBe(n)
  expect(await db.db.count('outbox')).toBe(jobs)
  db.db.close()
  await engine.lock()
})
it('reads accepted large formal jobs and keeps the write size bound', async () => {
  const { engine } = await fresh()
  const channel = id()
  await engine.channelJob(channel, 'large', { data: 'x'.repeat(300000) })
  expect(await engine.channelJob(channel, 'large')).toEqual({ data: 'x'.repeat(300000) })
  await expect(
    engine.channelJob(channel, 'too-large', { data: 'x'.repeat(450000) })
  ).rejects.toThrow('QUOTA_EXCEEDED')
  await engine.lock()
})
it('submits a fresh action when inbox archive state toggles back', async () => {
  const data = new Map<string, string>(),
    seed = new Uint8Array(32).fill(1)
  const crypto = {
    call: async (method: string, key: any, value?: string) => {
      if (method === 'commerceJournal') {
        if (value !== undefined) data.set(key, value)
        return data.get(key) ?? null
      }
      if (method === 'deviceSign') return ed25519.sign(key, seed)
      throw Error(method)
    }
  }
  let archived = false,
    posts = 0
  const cloud = {
    post: async (_path: any, signed: any) => {
      posts++
      archived = readCloudCommand(signed).body.payload.archived as boolean
      return { archived }
    }
  }
  const client = new InboxClient(
    { crypto } as any,
    cloud as any,
    {} as any,
    'aaaaa-aa',
    accountId
  )
  ;(client as any).session.context = async (requestId = id()) => ({
    accountId,
    issuer: `https://dmsg.test/u/${accountId}`,
    deviceId: '01'.repeat(32),
    securityEpoch: 1,
    requestId,
    deadline: Date.now() + 45000
  })
  const order = id()
  await client.mark(order, true, false)
  await client.mark(order, true, true)
  const result = await client.mark(order, true, false)
  expect(posts).toBe(3)
  expect(result.archived).toBe(false)
  expect(archived).toBe(false)
})
it('reauthorizes a root request only when certified state proves its sequence was not consumed', async () => {
  const { AccountRootClient } = await import('../src/lib/services/account-root')
  const { canonical, b64, digest, hash, unhex } = await import('../src/lib/protocol/codec')
  const seed = new Uint8Array(32).fill(3),
    home = Principal.fromUint8Array(new Uint8Array([1])),
    saved = new Map<string, string>()
  const context = {
    account: accountId,
    homeCose: home.toText(),
    environment: 'local',
    generation: 1,
    opId: id(),
    recoveryGeneration: 1,
    recoveryPublic: b64(seed),
    recoverySigningPublic: b64(seed)
  }
  const payload = {
    format: 'dmsg-root-bundle/1',
    context,
    key: { publicKey: b64(seed), fingerprint: id(), keyId: id(), keyName: 'test_key_1' },
    online: b64(seed),
    recovery: { enc: b64(seed), ciphertext: b64(seed) },
    previous: null,
    device: id(),
    signingPublic: b64(ed25519.getPublicKey(seed))
  }
  const bytes = canonical({
      payload,
      signature: ed25519.sign(digest('dmsg/root-bundle/1', payload), seed)
    }),
    expected = hash(bytes)
  const now = Date.now(),
    clock = vi.spyOn(Date, 'now').mockReturnValue(now)
  const derive = vi.fn(async (request: any) => {
    if (request.approval.expires_at <= BigInt(Date.now())) return { Err: { Expired: null } }
    throw Error('network unavailable before dispatch')
  })
  const crypto = {
    call: async (method: string, ...args: any[]) => {
      if (method === 'controlGet') return saved.get(args[0]) ?? null
      if (method === 'controlPut') {
        saved.set(args[0], args[1])
        return
      }
      if (method === 'prepareAccountRoot') return new Uint8Array(48).fill(1)
      if (method === 'deviceSign') return ed25519.sign(args[0], seed)
      throw Error(method)
    }
  }
  const ref = { generation: 1n, bundle_digest: unhex(expected) }
  const state = {
    info: {
      current_root: [ref],
      home_cose: home,
      issuer: `https://dmsg.test/u/${accountId}`,
      security_epoch: 1n
    },
    verified: {
      evidence: {},
      securityEpoch: 1,
      get certifiedAt() {
        return Date.now()
      }
    },
    device: { revoked_at: [], next_sequence: 0n }
  }
  const account = {
    home,
    meta: { deviceId: id(), environment: 'local' },
    crypto,
    refresh: async () => state,
    user: {
      get_execution: async () => ({ Err: { ResultExpired: null } }),
      derive_root: derive
    }
  }
  const cloud = {
    publishSecurity: async () => {},
    get: async () => ({ root: { upload_id: id(), digest: expected, root_generation: 1 } }),
    getChunk: async () => bytes
  }
  const client = new AccountRootClient(account as any, cloud as any)
  await expect(client.openCurrent(accountId)).rejects.toThrow('network unavailable')
  const original = [...saved.entries()]
  state.device.next_sequence = 1n
  clock.mockReturnValue(now + 300001)
  await expect(client.openCurrent(accountId)).rejects.toMatchObject({ code: 'RESULT_EXPIRED' })
  expect(derive).toHaveBeenCalledTimes(1)
  state.device.next_sequence = 0n
  clock.mockReturnValue(now + 300001)
  await expect(client.openCurrent(accountId)).rejects.toThrow('network unavailable')
  expect([...saved.entries()]).not.toEqual(original)
  expect(derive.mock.calls[1][0].approval.expires_at).toBeGreaterThan(
    derive.mock.calls[0][0].approval.expires_at
  )
  expect(derive.mock.calls[0][0].approval.request_id).toEqual(
    derive.mock.calls[1][0].approval.request_id
  )
})
it('rejects an external signature ACK without a delivered digest', async () => {
  const { config } = await import('../src/lib/config')
  const { engine } = await fresh(),
    db = await WorkspaceDB.open((await currentWorkspace())!)
  const source = {
    origin: 'https://review.example',
    tabId: 7,
    frameId: 0,
    documentId: 'review-document'
  }
  ;(config.externalOrigins as string[]).push(source.origin)
  let connect: any, message: any
  const respond = vi.fn()
  const badge = vi.fn().mockResolvedValue(undefined)
  vi.stubGlobal('chrome', {
    runtime: {
      id: 'a'.repeat(32),
      getURL: (path: string) => `chrome-extension://${'a'.repeat(32)}/${path}`,
      onMessage: { addListener: () => {} },
      onInstalled: { addListener: () => {} },
      onStartup: { addListener: () => {} },
      onConnectExternal: {
        addListener: (fn: any) => {
          connect = fn
        }
      }
    },
    action: { setBadgeText: badge, setBadgeBackgroundColor: async () => {} },
    alarms: { create: async () => {}, onAlarm: { addListener: () => {} } }
  })
  try {
    await import('../src/service-worker')
    await vi.waitFor(() => expect(badge).toHaveBeenCalled())
    const requestId = id()
    await db.db.put('requests', {
      id: requestId,
      kind: 'document',
      source,
      state: 'awaiting_user',
      expiresAt: Date.now() + 300000,
      createdAt: Date.now(),
      digest: id(),
      payload: { enc: '', ciphertext: '' }
    })
    connect({
      name: 'dmsg-extension/4',
      sender: {
        origin: source.origin,
        url: source.origin + '/',
        frameId: 0,
        tab: { id: source.tabId },
        documentId: source.documentId,
        documentLifecycle: 'active'
      },
      onMessage: {
        addListener: (fn: any) => {
          message = fn
        }
      },
      onDisconnect: { addListener: () => {} },
      postMessage: respond,
      disconnect: () => {}
    })
    message({ protocol: 'dmsg-extension/4', method: 'signature.ack', requestId })
    await vi.waitFor(() =>
      expect(respond).toHaveBeenCalledWith(expect.objectContaining({ ok: false }))
    )
    expect((await db.db.get('requests', requestId)).state).toBe('awaiting_user')
    expect((await db.db.get('requests', requestId)).executionId).toBeUndefined()
  } finally {
    config.externalOrigins.pop()
    vi.unstubAllGlobals()
    db.db.close()
    await engine.lock()
  }
})
it('prepares each note without decrypting unrelated upload jobs', async () => {
  const { engine } = await fresh()
  ;(engine as any).meta.registered = true
  ;(engine as any).meta.account = {
    id: accountId,
    homeUser: 'aaaaa-aa',
    issuer: `https://dmsg.test/u/${accountId}`,
    rootDigest: id()
  }
  const records = []
  for (let i = 0; i < 5; i++)
    records.push(await engine.saveItem({ item: note(String(i)) as any }))
  const decrypt = vi.spyOn(globalThis.crypto.subtle, 'decrypt'),
    counts = []
  for (const record of records) {
    decrypt.mockClear()
    await engine.contentPrepare(record.key)
    counts.push(decrypt.mock.calls.length)
  }
  expect(counts).toEqual([2, 2, 2, 2, 2])
  await engine.lock()
})
it('indexes channel tasks locally without exposing channel routing in uploaded records', async () => {
  const { engine } = await fresh(),
    first = id(),
    second = id(),
    requestId = id()
  await engine.channelJob(first, 'send', {
    action: 'dmsg/channel/message/v1',
    context: { requestId }
  })
  await engine.channelJob(second, 'send', {
    action: 'dmsg/channel/message/v1',
    context: { requestId: id() }
  })
  const decrypt = vi.spyOn(globalThis.crypto.subtle, 'decrypt')
  expect(await engine.channelPending(first)).toEqual([
    { key: 'send', action: 'dmsg/channel/message/v1', requestId }
  ])
  expect(decrypt).toHaveBeenCalledTimes(1)
  // Retry journals stay on this device: no synchronized object or outbox entry.
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  expect(await db.db.count('objects')).toBe(0)
  expect(await db.db.count('outbox')).toBe(0)
  expect(JSON.stringify(await db.db.getAll('local_private'))).not.toContain(requestId)
  db.db.close()
  await engine.lock()
})
it('ACKs only the matching source and an already signed request', async () => {
  const { acknowledgeRequest } = await import('../src/lib/requests')
  const { engine } = await fresh(),
    db = await WorkspaceDB.open((await currentWorkspace())!)
  const source = {
      origin: 'https://example.test',
      tabId: 1,
      frameId: 0,
      documentId: 'document'
    },
    requestId = id(),
    resultDigest = id()
  const row = {
    id: requestId,
    source,
    state: 'awaiting_user',
    expiresAt: Date.now() + 60000,
    createdAt: Date.now(),
    digest: id(),
    payload: { enc: '', ciphertext: '' }
  }
  await db.db.put('requests', row)
  await expect(
    acknowledgeRequest(requestId, source, resultDigest, resultDigest)
  ).rejects.toThrow()
  await db.db.put('requests', { ...row, state: 'signed' })
  await expect(
    acknowledgeRequest(
      requestId,
      { ...source, documentId: 'other' },
      resultDigest,
      resultDigest
    )
  ).rejects.toThrow()
  await expect(acknowledgeRequest(requestId, source, id(), resultDigest)).rejects.toThrow()
  expect((await db.db.get('requests', requestId)).state).toBe('signed')
  await acknowledgeRequest(requestId, source, resultDigest, resultDigest)
  expect((await db.db.get('requests', requestId)).state).toBe('returned')
  db.db.close()
  await engine.lock()
})
