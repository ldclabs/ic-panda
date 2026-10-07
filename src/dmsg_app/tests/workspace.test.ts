import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { currentWorkspace, registerWorkspace, WorkspaceDB } from '../src/lib/db'
import { CHUNK_SIZE, LOGIN_UNLOCK_INTERVAL_MS } from '../src/lib/config'
import type { Item } from '../src/lib/models'
import { random } from '../src/lib/protocol/codec'
import { boundEngine } from './support/engine'

const note = (title = 'Sensitive project title'): Item => ({
  type: 'api',
  title,
  body: 'private deployment instructions',
  username: 'private-account',
  secret: 'secret-test-token-no-plaintext',
  tags: ['hidden-tag'],
  url: '',
  favorite: false,
  createdAt: 1,
  updatedAt: 1
})
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
afterEach(() => vi.restoreAllMocks())
describe('real encrypted workspace lifecycle', () => {
  it('encrypts private metadata, preserves conflicts/tombstones and locks', async () => {
    let { engine } = await boundEngine()
    const first = await engine.saveItem({ item: note() })
    const updated = await engine.saveItem({
      item: { ...note(), body: 'new revision' },
      id: first.id,
      base: first.revision
    })
    const conflict = await engine.saveItem({
      item: { ...note(), body: 'offline conflicting edit' },
      id: first.id,
      base: first.revision
    })
    expect(conflict.conflict).toBe(true)
    expect((await engine.view()).entries[0].item.body).toBe('new revision')
    expect((await engine.view()).conflicts).toHaveLength(1)
    const deleted = await engine.deleteItem({ id: first.id, base: updated.revision })
    await engine.saveItem({
      item: note('stale resurrection'),
      id: first.id,
      base: updated.revision
    })
    expect((await engine.view()).entries[0].record.tombstone).toBe(true)
    await engine.deleteItem({ id: first.id, base: deleted.revision, restore: true })
    expect((await engine.view()).entries[0].record.tombstone).toBe(false)
    const db = await WorkspaceDB.open((await currentWorkspace())!)
    const dump = JSON.stringify(
      await Promise.all(
        ['objects', 'local_private', 'key_envelopes', 'outbox'].map((store) =>
          db.db.getAll(store)
        )
      )
    )
    for (const secret of [note().title, note().body, note().secret, 'hidden-tag'])
      expect(dump).not.toContain(secret)
    db.db.close()
    await engine.lock()
    await expect(engine.view()).rejects.toThrow('解锁')
    engine = new CryptoEngine()
    await engine.unlock()
    expect((await engine.view()).entries[0].item.secret).toBe(note().secret)
    await engine.lock()
  })
  it('refuses writes until the workspace is bound to an account', async () => {
    const engine = new CryptoEngine()
    await engine.initialize()
    await expect(engine.saveItem({ item: note() })).rejects.toThrow('绑定账户')
    await expect(engine.importFile(new File([new Uint8Array(4)], 'f.bin'))).rejects.toThrow(
      '绑定账户'
    )
    await engine.lock()
  })
  it('replaces the provisional key with the login-gated secret and keeps PRF unlock bounded', async () => {
    const clock = vi.spyOn(Date, 'now').mockReturnValue(1_800_000_000_000)
    let { engine } = await boundEngine()
    const secret = random(),
      prf = random()
    expect((await engine.status()).meta?.unlock).toBe('provisional')
    await engine.bindUnlockSecret(secret)
    expect((await engine.status()).meta?.unlock).toBe('login')
    const db = await WorkspaceDB.open((await currentWorkspace())!)
    const envelope = await db.envelope()
    expect(envelope.provisional).toBeUndefined()
    expect(JSON.stringify(await db.db.getAll('key_envelopes'))).not.toContain(
      Buffer.from(secret).toString('base64')
    )
    db.db.close()
    await engine.saveItem({ item: note() })
    await engine.lock()
    engine = new CryptoEngine()
    // The provisional path is gone; only the secret from the user home opens the store.
    await expect(engine.unlock()).rejects.toThrow('登录')
    await expect(engine.unlock(random())).rejects.toThrow('不匹配')
    await expect(engine.unlockWithPrf(prf)).rejects.toThrow('生物识别')
    await engine.unlock(secret)
    expect((await engine.view()).entries[0].item.secret).toBe(note().secret)
    await engine.enablePrf({ credentialId: 'Y3JlZA', output: prf })
    expect((await engine.status()).meta?.prf?.credentialId).toBe('Y3JlZA')
    await engine.lock()
    engine = new CryptoEngine()
    await expect(engine.unlockWithPrf(random())).rejects.toThrow('不匹配')
    await engine.unlockWithPrf(prf)
    expect((await engine.view()).entries).toHaveLength(1)
    await engine.lock()
    // A week without a login unlock sends the device back through the login path.
    clock.mockReturnValue(1_800_000_000_000 + LOGIN_UNLOCK_INTERVAL_MS)
    engine = new CryptoEngine()
    await expect(engine.unlockWithPrf(prf)).rejects.toThrow('7 天')
    await engine.unlock(secret)
    await engine.lock()
    engine = new CryptoEngine()
    await engine.unlockWithPrf(prf)
    await engine.disablePrf()
    expect((await engine.status()).meta?.prf).toBeUndefined()
    await engine.lock()
    engine = new CryptoEngine()
    await expect(engine.unlockWithPrf(prf)).rejects.toThrow('生物识别')
    await engine.unlock(secret)
    await engine.lock()
  })
  it('detects damaged chunks and imports files with resumable encryption', async () => {
    const { engine } = await boundEngine()
    const empty = await engine.importFile(new File([], 'empty.bin'))
    expect((await engine.downloadFile(empty.key)).blob.size).toBe(0)
    const data = new Uint8Array(CHUNK_SIZE + 19).fill(42)
    const file = await engine.importFile(
      new File([data], 'private-file.bin', { type: 'application/octet-stream' })
    )
    expect(
      new Uint8Array(await (await engine.downloadFile(file.key)).blob.arrayBuffer())
    ).toEqual(data)
    const db = await WorkspaceDB.open((await currentWorkspace())!)
    const chunk = (await db.db.getAll('chunks'))[0]
    await db.db.put('chunks', { ...chunk, digest: '00'.repeat(32) })
    await expect(engine.downloadFile(file.key)).rejects.toThrow('校验')
    await db.db.put('chunks', chunk)
    db.db.close()
    expect((await engine.view()).entries).toHaveLength(2)
    await engine.lock()
  })
  it('fences a previous owner and refuses lease takeover while it is active', async () => {
    const { engine } = await boundEngine()
    const db = await WorkspaceDB.open((await currentWorkspace())!)
    await expect(db.acquire('other')).rejects.toThrow('WORKSPACE_BUSY')
    await db.invalidate()
    await expect(engine.saveItem({ item: note() })).rejects.toThrow('会话已结束')
    const stale = await db.acquire('stale-owner')
    await db.invalidate()
    await expect(
      db.guardedPut('local_private', { id: 'stale-write', ciphertext: 'bad' }, stale)
    ).rejects.toThrow('会话已结束')
    db.db.close()
    await engine.lock()
  })
  it('atomically selects only one active workspace', async () => {
    const first = `dmsg:local:${'11'.repeat(32)}:${'22'.repeat(32)}`,
      second = `dmsg:local:${'33'.repeat(32)}:${'44'.repeat(32)}`
    const results = await Promise.allSettled([
      registerWorkspace(first),
      registerWorkspace(second)
    ])
    expect(results.filter((result) => result.status === 'fulfilled')).toHaveLength(1)
    expect([first, second]).toContain(await currentWorkspace())
  })
  it('resumes an interrupted file with identical ciphertext and rejects changed source bytes', async () => {
    const { engine } = await boundEngine(undefined, undefined, (progress) => {
      if (progress.stage === '正在分块加密' && progress.completed === 1)
        throw new Error('simulated page close')
    })
    const content = new Uint8Array(CHUNK_SIZE + 12).fill(55),
      file = new File([content], 'resume.bin')
    await expect(engine.importFile(file)).rejects.toThrow('simulated page close')
    const pending = (await engine.view()).imports[0]
    expect(pending.completed).toBe(1)
    const db = await WorkspaceDB.open((await currentWorkspace())!),
      firstChunk = (await db.db.getAll('chunks'))[0]
    db.db.close()
    await engine.lock()
    const resumed = new CryptoEngine()
    await resumed.unlock()
    const changed = new Uint8Array(content)
    changed[0] ^= 1
    await expect(
      resumed.importFile(new File([changed], 'resume.bin'), pending.id)
    ).rejects.toThrow('原文件已变化')
    const record = await resumed.importFile(file, pending.id)
    const reopened = await WorkspaceDB.open((await currentWorkspace())!)
    expect(await reopened.db.get('chunks', firstChunk.id)).toEqual(firstChunk)
    expect((await resumed.view()).imports).toHaveLength(0)
    expect(
      new Uint8Array(await (await resumed.downloadFile(record.key)).blob.arrayBuffer())
    ).toEqual(content)
    reopened.db.close()
    await resumed.lock()
  })
})
