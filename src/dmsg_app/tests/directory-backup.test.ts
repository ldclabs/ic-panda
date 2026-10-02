import { beforeEach, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import { id, unb64 } from '../src/lib/protocol/codec'
import { readObject } from '../src/lib/protocol/content'
import { open } from '../src/lib/crypto/primitives'

const password = 'directory-backup-test-password'
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
function directory() {
  const files = new Map<string, File>()
  const dir = {
    name: 'test',
    getDirectoryHandle: async () => dir,
    getFileHandle: async (name: string) => ({
      createWritable: async () => ({
        write: async (text: string) => {
          files.set(name, new File([text], name))
        },
        close: async () => {},
        abort: async () => {}
      })
    })
  }
  return { dir: dir as unknown as FileSystemDirectoryHandle, files }
}
it('round-trips bounded parts and archived signatures into a fresh offline workspace', async () => {
  const engine = new CryptoEngine(),
    setup = await engine.initialize(password)
  await engine.verifyRecovery(setup.recoveryCode)
  const bytes = new Uint8Array(2 * 1024 * 1024).fill(71)
  const record = await engine.importFile(new File([bytes], 'two-mib.bin'))
  const externalId = id(),
    value = JSON.stringify({
      stage: 'complete',
      externalId,
      artifact: {},
      receipt: 'retained',
      executionId: id()
    })
  await engine.formalHistory({ requestId: externalId, value })
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  await db.db.put('requests', { id: externalId, state: 'returned' })
  db.db.close()
  const output = directory()
  const result = await engine.exportDirectory(password, output.dir, 1024)
  expect(result.parts).toBeGreaterThan(2)
  expect(result.missing).not.toContain(`request:${externalId}`)
  expect((await engine.exportBackup(password)).missing).not.toContain(`request:${externalId}`)
  await engine.lock()
  globalThis.indexedDB = new IDBFactory()
  const restored = new CryptoEngine()
  await restored.restoreDirectory({
    files: [...output.files.values()],
    code: setup.recoveryCode,
    password
  })
  expect((await restored.status()).meta?.registered).toBe(false)
  expect((await restored.formalHistories())[0].value).toBe(value)
  expect(
    new Uint8Array(await (await restored.downloadFile(record.key)).blob.arrayBuffer())
  ).toEqual(bytes)
  await restored.lock()
  for (const broken of ['missing', 'tampered']) {
    globalThis.indexedDB = new IDBFactory()
    const files = new Map(output.files)
    const part = [...files.keys()].find((k) => k.startsWith('part-'))!
    if (broken === 'missing') files.delete(part)
    else files.set(part, new File(['corrupt'], part))
    const rejected = new CryptoEngine()
    await expect(
      rejected.restoreDirectory({
        files: [...files.values()],
        code: setup.recoveryCode,
        password
      })
    ).rejects.toThrow()
    expect(await currentWorkspace()).toBeNull()
  }
})

it('rotated pending objects cannot be opened by the prior root', async () => {
  const engine = new CryptoEngine(),
    setup = await engine.initialize(password)
  await engine.verifyRecovery(setup.recoveryCode)
  const internal = engine as any
  internal.meta.account = { id: '040g2081040g2081040g', rootDigest: id() }
  internal.meta.subjectId = internal.meta.account.id
  internal.meta.registered = true
  const record = await engine.importFile(new File([new Uint8Array(32)], 'rekey.bin'))
  const oldRoot = unb64(internal.bundle.root),
    old = await engine.contentPrepare(record.key)
  internal.bundle.roots = { '1': internal.bundle.root }
  internal.bundle.root = setup.meta.signingPublic
  internal.meta.rootGeneration = 2
  const next = await engine.contentReplan(
    record.key,
    2,
    old.uploads.map((u) => u.plan.upload_id)
  )
  const value = readObject(
    await engine.contentChunk(
      next.uploads.find((u) => u.plan.kind === 'vault')!,
      0
    )
  )
  expect(value.generation).toBe(2)
  await expect(open(oldRoot, value.keyEnvelope, internal.wrapContext(value))).rejects.toThrow()
  expect(await engine.contentPrepare(record.key)).toEqual(next)
  await engine.lock()
})
