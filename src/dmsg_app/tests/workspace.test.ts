import { beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { currentWorkspace, registerWorkspace, WorkspaceDB } from '../src/lib/db'
import { CHUNK_SIZE } from '../src/lib/config'
import type { Item, RecoveryArchive } from '../src/lib/models'

const password = 'horse-river-silent-paper',
  replacement = 'another-distinct-passphrase'
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
describe('real encrypted workspace lifecycle', () => {
  it('encrypts private metadata, preserves conflicts/tombstones, locks, and changes password', async () => {
    let engine = new CryptoEngine()
    const setup = await engine.initialize(password)
    await expect(engine.saveItem({ item: note() })).rejects.toThrow('恢复')
    await engine.lock()
    engine = new CryptoEngine()
    await engine.unlock(password)
    expect((await engine.pendingRecovery()).recoveryCode).toBe(setup.recoveryCode)
    const pendingDb = await WorkspaceDB.open((await currentWorkspace())!)
    expect(JSON.stringify(await pendingDb.db.getAll('local_private'))).not.toContain(
      setup.recoveryCode.replaceAll('-', '')
    )
    await engine.verifyRecovery(setup.recoveryCode)
    expect(await pendingDb.db.get('local_private', 'pending-recovery')).toBeUndefined()
    pendingDb.db.close()
    await expect(engine.pendingRecovery()).rejects.toThrow()
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
    for (const secret of [
      note().title,
      note().body,
      note().secret,
      'hidden-tag',
      password,
      setup.recoveryCode
    ])
      expect(dump).not.toContain(secret)
    db.db.close()
    await engine.changePassword({ current: password, next: replacement })
    await engine.lock()
    await expect(engine.view()).rejects.toThrow('解锁')
    await expect(engine.unlock(password)).rejects.toThrow()
    await engine.unlock(replacement)
    expect((await engine.view()).entries[0].item.secret).toBe(note().secret)
    await engine.lock()
  })
  it('detects damaged chunks; restores files, history and drafts on a fresh device without device private keys', async () => {
    const engine = new CryptoEngine(),
      setup = await engine.initialize(password)
    await engine.verifyRecovery(setup.recoveryCode)
    const empty = await engine.importFile(new File([], 'empty.bin'))
    expect((await engine.downloadFile(empty.key)).blob.size).toBe(0)
    const data = new Uint8Array(CHUNK_SIZE + 19).fill(42)
    const file = await engine.importFile(
      new File([data], 'private-file.bin', { type: 'application/octet-stream' })
    )
    expect(
      new Uint8Array(await (await engine.downloadFile(file.key)).blob.arrayBuffer())
    ).toEqual(data)
    const channel = await engine.createChannel({
      name: 'Private channel',
      type: 'direct',
      recipient: 'ab'.repeat(32)
    })
    await engine.saveDraft({ channelId: channel.id, text: 'Unsent text survives restore' })
    const name = (await currentWorkspace())!,
      db = await WorkspaceDB.open(name)
    const local = await db.envelope()
    const chunk = (await db.db.getAll('chunks'))[0]
    await db.db.put('chunks', { ...chunk, digest: '00'.repeat(32) })
    await expect(engine.downloadFile(file.key)).rejects.toThrow('校验')
    await expect(engine.exportBackup(password)).rejects.toThrow('校验')
    await db.db.put('chunks', chunk)
    const backup = await engine.exportBackup(password),
      archive = JSON.parse(await backup.blob.text()) as RecoveryArchive
    expect(JSON.stringify(archive)).not.toContain(local.privateBundle)
    expect(archive.scope).toBe('local-inclusive')
    await expect(
      new CryptoEngine().restore({
        file: new File([backup.blob], 'recovery.dmsg'),
        code: setup.recoveryCode,
        password: replacement
      })
    ).rejects.toThrow('空白浏览器配置')
    db.db.close()
    await engine.lock()
    globalThis.indexedDB = new IDBFactory()
    const restored = new CryptoEngine()
    const output = await restored.restore({
      file: new File([backup.blob], 'recovery.dmsg'),
      code: setup.recoveryCode,
      password: replacement
    })
    expect(output.meta.deviceId).not.toBe(setup.meta.deviceId)
    expect(output.meta.signingPublic).not.toBe(setup.meta.signingPublic)
    expect(output.meta.subjectId).toBe(setup.meta.subjectId)
    expect(output.meta.registered).toBe(false)
    expect((await restored.view()).entries).toHaveLength(2)
    expect(await restored.getDraft(channel.id)).toBe('Unsent text survives restore')
    expect((await restored.downloadFile(file.key)).blob.size).toBe(data.length)
    await restored.lock()
  })
  it('rejects a tampered backup even if an attacker recomputes its public digest', async () => {
    const engine = new CryptoEngine(),
      setup = await engine.initialize(password)
    await engine.verifyRecovery(setup.recoveryCode)
    await engine.saveItem({ item: note() })
    const backup = await engine.exportBackup(password),
      archive = JSON.parse(await backup.blob.text()) as RecoveryArchive
    await engine.lock()
    archive.objects = []
    const { hash, utf8 } = await import('../src/lib/protocol/codec')
    const { authentication, manifestDigest, ...content } = archive
    const bad = {
      ...content,
      authentication,
      manifestDigest: hash(utf8(JSON.stringify(content)))
    }
    globalThis.indexedDB = new IDBFactory()
    await expect(
      new CryptoEngine().restore({
        file: new File([JSON.stringify(bad)], 'tampered.dmsg'),
        code: setup.recoveryCode,
        password
      })
    ).rejects.toThrow('清单认证')
  })
  it('fences a previous owner and refuses lease takeover while it is active', async () => {
    const engine = new CryptoEngine(),
      setup = await engine.initialize(password)
    await engine.verifyRecovery(setup.recoveryCode)
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
    const engine = new CryptoEngine((progress) => {
      if (progress.stage === '正在分块加密' && progress.completed === 1)
        throw new Error('simulated page close')
    })
    const setup = await engine.initialize(password)
    await engine.verifyRecovery(setup.recoveryCode)
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
    await resumed.unlock(password)
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
