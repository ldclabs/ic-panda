import 'fake-indexeddb/auto'
import { beforeEach, expect, it, vi } from 'vitest'
import { openDB } from 'idb'
const state = vi.hoisted(() => ({
  name: '',
  meta: { registered: true, account: { id: 'account-a', homeUser: '' } }
}))
vi.mock('../src/lib/db', async () => {
  const { openDB } = await import('idb')
  return {
    currentWorkspace: async () => state.name,
    WorkspaceDB: {
      open: async (name: string) => ({
        db: await openDB(name, 1, {
          upgrade(db) {
            db.createObjectStore('requests', { keyPath: 'id' })
          }
        }),
        meta: async () => state.meta
      })
    }
  }
})
import {
  bindBrowserOperation,
  validateAuthenticationPayload
} from '../src/lib/bridge-requests'
import { acknowledgeRequest, rejectRequest, setRequestState } from '../src/lib/requests'
import { config } from '../src/lib/config'
import type { BrowserCommand } from '@dmsg/sdk/browser'

const source = { origin: 'https://product.test', tabId: 1, frameId: 0, documentId: 'a' }
const id = '01'.repeat(32),
  digest = '02'.repeat(32)
const command: BrowserCommand = {
  appId: 'product',
  operationId: id,
  method: 'getOperation',
  publicKey: 'original-key',
  payload: null,
  resultDigest: null
}
beforeEach(async () => {
  state.name = `bridge-${crypto.randomUUID()}`
  state.meta = {
    registered: true,
    account: { id: 'account-a', homeUser: config.canisters.user }
  }
  const db = await openDB(state.name, 1, {
    upgrade(db) {
      db.createObjectStore('requests', { keyPath: 'id' })
    }
  })
  await db.put('requests', {
    id,
    kind: 'authentication',
    source,
    digest,
    state: 'awaiting_user',
    expiresAt: Date.now() + 300000,
    bridge: {
      appId: 'product',
      publicKey: 'original-key',
      accountId: 'account-a',
      appVersion: '1',
      operationDigest: digest
    },
    payload: { enc: '', ciphertext: '' }
  })
  db.close()
})
it('rebinds a refreshed document only for the original app, origin, account and session key', async () => {
  const next = { ...source, documentId: 'b', tabId: 2 }
  expect((await bindBrowserOperation(command, next)).source).toEqual(next)
  await expect(
    bindBrowserOperation({ ...command, publicKey: 'another-key' }, next)
  ).rejects.toThrow()
  await expect(bindBrowserOperation({ ...command, appId: 'other' }, next)).rejects.toThrow()
  await expect(
    bindBrowserOperation(command, { ...next, origin: 'https://evil.test' })
  ).rejects.toThrow()
  state.meta.account.id = 'account-b'
  await expect(bindBrowserOperation(command, next)).rejects.toThrow()
})
it('cancellation wins before dispatch and an ACK requires a delivered digest', async () => {
  await rejectRequest(id, 'cancelled')
  await expect(setRequestState(id, 'authorized')).rejects.toThrow()
  await expect(setRequestState(id, 'execution_unknown')).rejects.toThrow()
  await expect(acknowledgeRequest(id, source, digest, undefined)).rejects.toThrow()
})
it('unknown operations cannot be cancelled and refreshed results keep their acknowledged state', async () => {
  await setRequestState(id, 'authorized')
  await setRequestState(id, 'execution_unknown')
  await rejectRequest(id, 'cancelled')
  expect((await bindBrowserOperation(command, source)).state).toBe('execution_unknown')
  await setRequestState(id, 'signed')
  await acknowledgeRequest(id, source, digest, digest)
  await setRequestState(id, 'signed')
  expect((await bindBrowserOperation(command, source)).state).toBe('returned')
})

it('rejects an authentication payload claiming another registered origin or session', async () => {
  const { canonical, sha256 } = await import('@dmsg/sdk')
  const { base64, hex } = await import('@dmsg/sdk/browser')
  const publicKey = new Uint8Array(91).fill(4),
    now = BigInt(Date.now())
  const app = {
    version: 1n,
    environment: 'Local' as const,
    app_id: 'product',
    config_version: 1n,
    origins: [source.origin, 'https://other.test'],
    user_homes: [new Uint8Array([1, 1])],
    cose_homes: [new Uint8Array([2, 1])],
    product_ids: [],
    capabilities: ['Authenticate' as const],
    profiles: [],
    authentication_receiver: new Uint8Array([3, 1]),
    paused: false
  }
  const request = {
    version: 1n,
    environment: 'Local' as const,
    app_id: 'product',
    app_config_version: 1n,
    origin: source.origin,
    receiver: app.authentication_receiver,
    challenge_hash: new Uint8Array(32).fill(5),
    session_key_hash: await sha256(publicKey),
    purpose: 'Login' as const,
    nonce: new Uint8Array(32).fill(6),
    operation_id: new Uint8Array(32).fill(7),
    issued_at_ms: now,
    expires_at_ms: now + 300000n
  }
  const command: BrowserCommand = {
    method: 'authenticate',
    appId: 'product',
    operationId: hex(request.operation_id),
    publicKey: base64(publicKey),
    payload: base64(canonical(request)),
    resultDigest: null
  }
  await validateAuthenticationPayload(command, source, app, now)
  await expect(
    validateAuthenticationPayload(
      { ...command, payload: base64(canonical({ ...request, origin: 'https://other.test' })) },
      source,
      app,
      now
    )
  ).rejects.toThrow()
  await expect(
    validateAuthenticationPayload(
      { ...command, publicKey: base64(new Uint8Array(91).fill(8)) },
      source,
      app,
      now
    )
  ).rejects.toThrow()
})
