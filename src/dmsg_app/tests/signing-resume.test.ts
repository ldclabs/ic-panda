import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { Principal } from '@icp-sdk/core/principal'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import { ed25519 } from '../src/lib/crypto/primitives'
import { encodeControl } from '../src/lib/protocol/account'
import { unhex } from '../src/lib/protocol/codec'
import { xidBytes } from '../src/lib/protocol/identity'
import { prepareAttest } from '../src/lib/services/cose'
import { SigningClient } from '../src/lib/services/signing'
import { boundEngine } from './support/engine'

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
afterEach(() => vi.restoreAllMocks())

it('declares a lost attestation expired only from certified time and an unused sequence', async () => {
  const { meta } = await boundEngine()
  const account = meta.account,
    expiresAt = 1_800_000_000_000n,
    requestId = '0a'.repeat(32)
  const prepared = prepareAttest(
    {
      homeUser: Principal.fromUint8Array(Uint8Array.of(0, 1, 1)),
      accountId: xidBytes(account.id),
      issuer: account.issuer,
      deviceId: unhex(meta.deviceId),
      securityEpoch: 0n,
      sequence: 4n,
      expiresAt
    },
    {
      origin: 'https://example.com',
      devicePublicKey: ed25519.getPublicKey(new Uint8Array(32).fill(7)),
      statement: { issuer: account.issuer, content: { kind: 'text', text: 'Approve' } }
    },
    Number(expiresAt) - 60_000
  )
  const job = {
    format: 'dmsg-signing-journal/3',
    method: 'attest',
    externalId: requestId,
    digest: '0b'.repeat(32),
    account: account.id,
    origin: 'https://example.com',
    executionId: prepared.requestId,
    operation: encodeControl('attest', [prepared.review.request]),
    stage: 'unknown'
  }
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  await db.db.put('requests', {
    id: requestId,
    kind: 'document',
    source: { origin: 'https://example.com', tabId: 1, frameId: 0, documentId: 'doc' },
    digest: job.digest,
    state: 'execution_unknown',
    expiresAt: Number(expiresAt),
    createdAt: 1,
    payload: { enc: '', ciphertext: '' }
  })
  const journals = new Map([[`formal:${requestId}`, JSON.stringify(job)]])
  const observed = { certifiedAt: Number(expiresAt) - 1, sequence: 4n }
  const client = new SigningClient(
    {
      meta,
      crypto: {
        call: async (method: string, key: string, value?: string) =>
          method === 'controlGet'
            ? (journals.get(key) ?? null)
            : void journals.set(key, value!)
      },
      user: { get_attestation: async () => ({ Err: { ResultExpired: null } }) },
      refresh: async () => ({
        verified: { certifiedAt: observed.certifiedAt },
        device: { next_sequence: observed.sequence }
      })
    } as any,
    async () => {}
  )
  vi.spyOn(Date, 'now').mockReturnValue(Number(expiresAt) + 60_000)
  // The local clock alone proves nothing while certified time is before the deadline.
  await expect(client.resume(requestId)).rejects.toMatchObject({ code: 'EXECUTION_UNKNOWN' })
  // A consumed sequence without a stored artifact stays unknown.
  Object.assign(observed, { certifiedAt: Number(expiresAt), sequence: 5n })
  await expect(client.resume(requestId)).rejects.toMatchObject({ code: 'EXECUTION_UNKNOWN' })
  expect((await db.db.get('requests', requestId)).state).toBe('execution_unknown')
  observed.sequence = 4n
  expect((await client.resume(requestId)).stage).toBe('result_expired')
  expect((await db.db.get('requests', requestId)).state).toBe('result_expired')
  db.db.close()
})
