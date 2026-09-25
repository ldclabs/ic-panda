import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import { canonical, id } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'
import { signCloudCommand, signCloudHttp } from '../src/lib/protocol/cloud'
import { flushCipherDispatches, type CipherDispatch } from '../src/lib/services/background'
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
afterEach(() => vi.unstubAllGlobals())
it('dispatches exact authorized ciphertext while locked, reconciles a lost response, and cannot retarget or renew authority', async () => {
  const engine = new CryptoEngine(),
    setup = await engine.initialize('background-only-fixture')
  await engine.verifyRecovery(setup.recoveryCode)
  const db = await WorkspaceDB.open((await currentWorkspace())!),
    meta = (await db.meta())!
  const account = xidText(new Uint8Array(12).fill(4)),
    origin = 'https://relay.test'
  meta.account = { id: account, issuer: `https://dmsg.test/${account}`, homeUser: 'aaaaa-aa' }
  meta.registered = true
  await db.db.put('meta', { id: 'workspace', value: meta })
  await engine.lock()
  await engine.unlock('background-only-fixture')
  const context = {
    accountId: account,
    issuer: meta.account.issuer,
    deviceId: meta.deviceId,
    securityEpoch: 1,
    requestId: id(),
    deadline: Date.now() + 45000
  }
  const revision = id(),
    payload = {
      item_id: id(),
      revision_id: revision,
      base_revision: null,
      upload_id: id(),
      tombstone: false,
      restore: false
    }
  const sign = (bytes: Uint8Array) => engine.deviceSign(bytes)
  const signed = await signCloudCommand(context, 'dmsg/vault/revision/v1', payload, sign)
  const task: CipherDispatch = {
    id: context.requestId,
    format: 'dmsg-cipher-dispatch/1',
    account,
    origin,
    deadline: context.deadline,
    state: 'queued',
    attempts: 0,
    recordKey: `${payload.item_id}:${revision}`,
    requestId: context.requestId,
    revision,
    tombstone: false,
    command: signed.cose_sign1,
    postProof: await signCloudHttp(
      context,
      new URL(`/v1/accounts/${account}/vault`, origin),
      'POST',
      canonical(signed),
      sign
    ),
    statusProof: await signCloudHttp(
      context,
      new URL(`/v1/accounts/${account}/operations/${context.requestId}`, origin),
      'GET',
      new Uint8Array(),
      sign
    )
  }
  await db.db.put('meta', { id: `dispatch:${task.id}`, value: task })
  await engine.lock()
  let stored = false,
    publishes = 0
  const receipt = { revision_id: revision, tombstone: false, conflict: false, head: revision }
  vi.stubGlobal(
    'fetch',
    vi.fn(async (_url, options) => {
      if (options.method === 'GET')
        return new Response(
          JSON.stringify({
            ok: true,
            data: stored ? { found: true, result: receipt } : { found: false }
          })
        )
      publishes++
      stored = true
      throw new Error('reply lost after storing')
    })
  )
  await flushCipherDispatches(db, origin)
  expect((await db.db.get('meta', `dispatch:${task.id}`)).value.state).toBe('unknown')
  await flushCipherDispatches(db, origin)
  expect(publishes).toBe(1)
  expect((await db.db.get('meta', `dispatch:${task.id}`)).value.state).toBe('complete')
  await db.db.put('meta', {
    id: `dispatch:${task.id}`,
    value: { ...task, origin: 'https://attacker.test' }
  })
  await flushCipherDispatches(db, origin)
  expect((await db.db.get('meta', `dispatch:${task.id}`)).value.state).toBe('blocked')
  await db.db.put('meta', { id: `dispatch:${task.id}`, value: task })
  await flushCipherDispatches(db, origin, task.deadline + 1)
  expect((await db.db.get('meta', `dispatch:${task.id}`)).value.state).toBe('paused')
  expect(publishes).toBe(1)
  db.db.close()
})
