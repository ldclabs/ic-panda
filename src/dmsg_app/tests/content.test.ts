import { beforeEach, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { xidText } from '../src/lib/protocol/identity'
import { b64, canonical, hash, unb64 } from '../src/lib/protocol/codec'
import { MAX_CIPHER_CHUNK } from '../src/lib/config'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import type { StoredUploadPlan } from '../src/lib/protocol/content'

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
const account = xidText(new Uint8Array(12).fill(8)),
  password = 'content fixture password'
async function localFixture(root?: string) {
  const engine = new CryptoEngine(),
    initialized = await engine.initialize(password)
  await engine.verifyRecovery(initialized.recoveryCode)
  // Crypto-only fixture. Real account authorization is exercised by PocketIC;
  // there is no production API that can set this state.
  const internals = engine as any
  internals.meta = {
    ...internals.meta,
    subjectId: account,
    registered: true,
    account: {
      id: account,
      homeUser: 'aaaaa-aa',
      issuer: `https://example.test/${account}`,
      rootDigest: '1'.repeat(64)
    }
  }
  if (root) internals.bundle.root = root
  return engine
}
it('uses real encrypted byte sizes, persists immutable manifests and imports the complete file with its revision', async () => {
  const source = await localFixture(),
    bytes = new Uint8Array(1024 * 1024).fill(37)
  const record = await source.importFile(new File([bytes], 'private-one-mib.bin'))
  const job = await source.contentPrepare(record.key)
  const file = job.uploads.find((u) => u.plan.kind === 'file')!
  expect(file.plan.chunks[0].size).toBeGreaterThan(1024 * 1024)
  expect(file.plan.chunks[0].size).toBeLessThanOrEqual(MAX_CIPHER_CHUNK)
  expect(await source.contentPrepare(record.key)).toEqual(job)
  const downloads: { plan: StoredUploadPlan; manifest: string; chunks: string[] }[] = []
  for (const upload of job.uploads) {
    const chunks = []
    for (let i = 0; i < upload.plan.chunks.length; i++)
      chunks.push(b64(await source.contentChunk(upload, i)))
    downloads.push({ plan: upload.plan, manifest: upload.manifest, chunks })
  }
  const root = (source as any).bundle.root
  await source.lock()
  globalThis.indexedDB = new IDBFactory()
  const target = await localFixture(root)
  const revision = {
    item_id: record.id,
    revision_id: record.revision,
    base_revision: null,
    upload_id: job.uploads.find((u) => u.plan.kind === 'vault')!.plan.upload_id,
    tombstone: false,
    conflict: false
  }
  const input = {
    objects: downloads.map((object) => object.plan),
    revisions: [revision],
    through: 9,
    evidence: '{}'
  }
  await target.contentCache(downloads[0].plan, 'manifest', unb64(downloads[0].manifest))
  await expect(
    target.contentCache(downloads[0].plan, 0, new Uint8Array([1, 2]))
  ).rejects.toThrow('INTEGRITY_FAILED')
  await expect(target.contentReceive(input)).rejects.toThrow('RECOVERY_INCOMPLETE')
  for (const object of downloads) {
    await target.contentCache(object.plan, 'manifest', unb64(object.manifest))
    for (const [index, bytes] of object.chunks.entries())
      await target.contentCache(object.plan, index, unb64(bytes))
    expect(await target.contentMissing(object.plan)).toEqual({ manifest: true, chunks: [] })
  }
  expect((await target.view()).entries).toHaveLength(0)
  expect(await target.contentReceive(input)).toMatchObject({
    records: 1,
    files: 1,
    through: 9
  })
  const downloaded = await target.downloadFile(record.key)
  expect(new Uint8Array(await downloaded.blob.arrayBuffer())).toEqual(bytes)
  await expect(target.contentReceive({ ...input, through: 8 })).rejects.toThrow('回退')
  expect((await target.view()).entries).toHaveLength(1)
  await target.lock()
})
it('aborts the whole import transaction when a later chunk conflicts', async () => {
  const engine = await localFixture(),
    db = await WorkspaceDB.open((await currentWorkspace())!)
  const original = {
    id: 'already',
    ciphertext: b64(new Uint8Array([1])),
    digest: hash(new Uint8Array([1]))
  }
  await db.db.put('chunks', original)
  const fresh = {
    id: 'must-not-remain',
    ciphertext: b64(new Uint8Array([2])),
    digest: hash(new Uint8Array([2]))
  }
  await expect(
    db.contentReceive(
      [],
      [fresh, { ...fresh, id: 'already' }],
      [],
      1,
      '{}',
      (engine as any).lease
    )
  ).rejects.toThrow('IDEMPOTENCY_CONFLICT')
  expect(await db.db.get('chunks', fresh.id)).toBeUndefined()
  expect(await db.db.get('chunks', original.id)).toEqual(original)
  db.db.close()
  await engine.lock()
})
it('replans expired uploads and a pending vault revision after root rotation without changing content IDs', async () => {
  const engine = await localFixture()
  const record = await engine.importFile(new File([new Uint8Array(32).fill(17)], 'replan.bin'))
  const original = await engine.contentPrepare(record.key)
  const oldChunks = await Promise.all(
    original.uploads[0].plan.chunks.map((_, index) =>
      engine.contentChunk(original.uploads[0], index)
    )
  )
  const expired = await engine.contentReplan(
    record.key,
    1,
    original.uploads.map((upload) => upload.plan.upload_id)
  )
  expect(expired.recordKey).toBe(original.recordKey)
  expect(expired.recordDigest).toBe(original.recordDigest)
  expect(expired.revision.requestId).toBe(original.revision.requestId)
  expect(expired.uploads.map((upload) => upload.plan.upload_id)).not.toEqual(
    original.uploads.map((upload) => upload.plan.upload_id)
  )
  expect(await engine.contentChunk(expired.uploads[0], 0)).toEqual(oldChunks[0])
  const internals = engine as any
  const priorRoot = internals.bundle.root
  internals.bundle.roots = { '1': priorRoot }
  internals.bundle.root = b64(new Uint8Array(32).fill(18))
  internals.meta.rootGeneration = 2
  const rotated = await engine.contentReplan(record.key, 2, [
    expired.uploads[1].plan.upload_id
  ])
  expect(rotated.uploads[0].plan.upload_id).toBe(expired.uploads[0].plan.upload_id)
  expect(rotated.uploads[1].plan.root_generation).toBe(2)
  expect(rotated.uploads[1].plan.version_id).toBe(record.revision)
  expect(rotated.revision.requestId).toBe(original.revision.requestId)
  expect(await engine.contentChunk(rotated.uploads[1], 0)).toEqual(
    await engine.contentChunk(original.uploads[1], 0)
  )
  await engine.lock()
})
