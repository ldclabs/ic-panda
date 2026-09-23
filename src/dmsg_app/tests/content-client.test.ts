import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { ContentEngine } from '../src/lib/crypto/content'
import { ContentClient } from '../src/lib/services/content'
import { RelayError } from '../src/lib/services/relay'
import { canonical, b64, hash, id, unb64, unhex } from '../src/lib/protocol/codec'
import { readCloudCommand } from '../src/lib/protocol/cloud'
import { xidText } from '../src/lib/protocol/identity'
import type { StoredUploadPlan } from '../src/lib/protocol/content'

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
afterEach(() => vi.restoreAllMocks())
const accountId = xidText(new Uint8Array(12).fill(9))
async function fixture() {
  const engine = new CryptoEngine()
  const setup = await engine.initialize('content-client-test-password')
  await engine.verifyRecovery(setup.recoveryCode)
  const meta = (engine as any).meta
  meta.subjectId = accountId
  meta.registered = true
  meta.account = {
    id: accountId,
    homeUser: 'aaaaa-aa',
    issuer: `https://dmsg.test/u/${accountId}`,
    rootDigest: id()
  }
  const state = {
    info: {
      issuer: meta.account.issuer,
      vault_write_state: { Ready: null },
      current_root: [{ generation: 1n }]
    },
    verified: {
      securityEpoch: 1,
      expiresAt: Date.now() + 60000,
      evidence: {},
      devices: [
        { input: { device_id: unhex(meta.deviceId), signing_pub: unb64(meta.signingPublic) } }
      ]
    }
  }
  const account = {
    meta,
    refresh: vi.fn(async () => state),
    crypto: {
      call: vi.fn(async (method: string, ...args: any[]) => (engine as any)[method](...args))
    }
  }
  const uploads = new Map<string, any>(),
    operations = new Map<string, any>(),
    revisions: any[] = []
  let loseReply = false,
    invalidReply = false,
    failInventory = false
  const missing = () => new RelayError('NOT_FOUND', 'not found', false)
  const cloud = {
    publishSecurity: vi.fn(async () => {}),
    get: vi.fn(async (path: string) => {
      if (path.includes('/operations/')) {
        const value = operations.get(path.split('/').at(-1)!)
        return value ? { found: true, result: value } : { found: false }
      }
      if (path.includes('/uploads/'))
        return uploads.get(path.split('/').at(-1)!) ?? Promise.reject(missing())
      if (path.includes('/vault/'))
        return (
          revisions.findLast((revision) => revision.item_id === path.split('/').at(-1)) ??
          Promise.reject(missing())
        )
      if (path.includes('/exports/')) {
        if (failInventory) throw new Error('inventory unavailable')
        let cursor = 0
        const entries = [...uploads.values()].map((upload) => ({
          cursor: ++cursor,
          kind: 'object',
          plan: upload.plan,
          signature: upload.signature,
          value: { upload_id: upload.plan.upload_id, digest: upload.plan.manifest_digest }
        }))
        return {
          entries: [
            ...entries,
            ...revisions.map((revision) => ({
              cursor: ++cursor,
              kind: 'revision',
              value: revision
            }))
          ],
          next_cursor: cursor,
          inventory_end: true
        }
      }
      throw missing()
    }),
    post: vi.fn(async (path: string, signed: any) => {
      const command = readCloudCommand(signed).body,
        payload = command.payload as any
      if (path.endsWith('/exports')) {
        return {
          account: accountId,
          through: uploads.size + revisions.length,
          scope: 'cloud_snapshot',
          export_id: id(),
          counts: [
            { kind: 'object', n: uploads.size },
            { kind: 'revision', n: revisions.length }
          ]
        }
      }
      if (path.endsWith('/uploads')) {
        uploads.set(payload.upload_id, {
          plan: payload,
          signature: signed,
          status: 'staging',
          chunks: new Map()
        })
        return { upload_id: payload.upload_id }
      }
      if (path.endsWith('/uploads/finalize')) {
        const upload = uploads.get(payload.upload_id)!
        upload.status = 'committed'
        return {
          upload_id: payload.upload_id,
          digest: upload.plan.manifest_digest,
          object_id: upload.plan.object_id,
          version_id: upload.plan.version_id
        }
      }
      if (path.endsWith('/vault')) {
        if (payload.base_revision)
          expect(revisions.some((r) => r.revision_id === payload.base_revision)).toBe(true)
        const receipt = {
          revision_id: payload.revision_id,
          head: payload.revision_id,
          conflict: false,
          tombstone: payload.tombstone
        }
        operations.set(command.request_id, receipt)
        revisions.push({ ...payload, conflict: false, signed })
        if (loseReply) {
          loseReply = false
          throw new Error('lost ACK')
        }
        return invalidReply ? { ...receipt, revision_id: id() } : receipt
      }
      throw new Error(path)
    }),
    putChunk: vi.fn(async (path: string, bytes: Uint8Array) => {
      const match = /uploads\/([^/]+)\/chunks\/(.+)$/.exec(path)!
      uploads.get(match[1])!.chunks.set(match[2], bytes)
    }),
    getChunk: vi.fn(async (path: string) => {
      const match = /objects\/([^/]+)\/chunks\/(.+)$/.exec(path)!
      return uploads.get(match[1])!.chunks.get(match[2]) as Uint8Array
    })
  }
  const client = new ContentClient(account as any, cloud as any, accountId)
  return {
    engine,
    account,
    cloud,
    client,
    uploads,
    revisions,
    loseReply: () => {
      loseReply = true
    },
    invalidReply: () => {
      invalidReply = true
    },
    failInventory: (value: boolean) => {
      failInventory = value
    }
  }
}
const note = (body: string) => ({
  type: 'note' as const,
  title: 'Note',
  body,
  tags: [],
  username: '',
  secret: '',
  url: '',
  favorite: false,
  createdAt: 1,
  updatedAt: 1
})

it('reconciles a lost vault ACK after recreating the client without submitting again', async () => {
  const f = await fixture()
  const record = await f.engine.saveItem({ item: note('first') })
  f.loseReply()
  await expect(f.client.push(record.key)).rejects.toThrow('lost ACK')
  const job = await f.engine.contentPrepare(record.key)
  const posts = f.cloud.post.mock.calls.length
  const resumed = new ContentClient(f.account as any, f.cloud as any, accountId)
  await resumed.pushPending()
  expect(f.cloud.post.mock.calls).toHaveLength(posts)
  expect((await f.engine.contentPrepare(record.key)).revision.requestId).toBe(
    job.revision.requestId
  )
  expect((await f.engine.view()).outbox[0].state).toBe('stored')
  await f.engine.lock()
})

it('pushes ancestors first, reuses current evidence, and refuses a mismatched ACK', async () => {
  const f = await fixture()
  const first = await f.engine.saveItem({ item: note('first') })
  await f.engine.saveItem({ item: note('second'), id: first.id, base: first.revision })
  expect(await f.client.pushPending()).toBe(2)
  expect(f.revisions[0].revision_id).toBe(first.revision)
  expect(f.account.refresh).toHaveBeenCalledTimes(1)
  const other = await f.engine.saveItem({ item: note('third') })
  f.invalidReply()
  await expect(f.client.push(other.key)).rejects.toThrow('INTEGRITY_FAILED')
  expect((await f.engine.view()).outbox.find((job) => job.id === other.revision)?.state).toBe(
    'local'
  )
  await f.engine.lock()
})

it('stages encrypted chunks and reuses verified blocks on repeated snapshots', async () => {
  const f = await fixture()
  const record = await f.engine.importFile(
    new File([new Uint8Array(1024 * 1024).fill(7)], 'cached.bin')
  )
  await f.client.push(record.key)
  await f.client.pull()
  const calls = f.cloud.getChunk.mock.calls.length
  expect(calls).toBeGreaterThan(0)
  await f.client.pull()
  expect(f.cloud.getChunk).toHaveBeenCalledTimes(calls)
  expect(
    f.account.crypto.call.mock.calls
      .filter(([method]) => method === 'contentReceive')
      .every(([, input]) =>
        input.objects.every(
          (object: any) =>
            !('chunks' in object) ||
            object.chunks.every((chunk: any) => typeof chunk === 'object')
        )
      )
  ).toBe(true)
  f.failInventory(true)
  await expect(f.client.pull()).rejects.toThrow('inventory unavailable')
  expect((await f.engine.downloadFile(record.key)).blob.size).toBe(1024 * 1024)
  await f.engine.lock()
})

it('verifies an inventory above 256 MiB one cached chunk at a time', async () => {
  // The backing store returns one reusable block. This exercises 270 MiB of
  // verified input without retaining a whole account in the test process.
  const bytes = new Uint8Array(1024 * 1024),
    cipher = b64(bytes),
    blockDigest = hash(bytes)
  const manifest = canonical({ format: 'review-avatar' }),
    encodedManifest = b64(manifest)
  const objects: StoredUploadPlan[] = Array.from({ length: 3 }, () => ({
    upload_id: id(),
    object_id: id(),
    version_id: id(),
    root_generation: 1,
    epoch: 0,
    control_head: null,
    kind: 'avatar',
    chunks: Array.from({ length: 90 }, () => ({ digest: blockDigest, size: bytes.length })),
    manifest_digest: hash(manifest),
    manifest_size: manifest.length,
    expires_at: Date.now() + 86400000
  }))
  const commit = vi.fn(async (..._args: any[]) => {})
  const db = {
    db: {
      get: async (_store: string, key: string) => ({
        id: key,
        ciphertext: key.startsWith('cloud-manifest:') ? encodedManifest : cipher,
        digest: key.startsWith('cloud-manifest:') ? hash(manifest) : blockDigest
      })
    },
    contentReceive: commit
  }
  const engine = new ContentEngine({
    ready: async () =>
      ({ db, meta: { account: { id: accountId } }, bundle: {}, lease: {} }) as any,
    decode: vi.fn(),
    verifyFile: vi.fn(),
    tick: async () => {}
  })
  await expect(
    engine.receive({ objects, revisions: [], through: 3, evidence: '[]' })
  ).resolves.toMatchObject({ records: 0, through: 3 })
  expect(commit).toHaveBeenCalledTimes(1)
  expect(commit.mock.calls[0][1]).toEqual([])
}, 60000)
