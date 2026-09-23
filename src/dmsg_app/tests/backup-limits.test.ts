import { beforeEach, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { CryptoEngine } from '../src/lib/crypto/engine'
vi.mock('../src/lib/config', async (original) => ({
  ...(await original<typeof import('../src/lib/config')>()),
  MAX_BACKUP_RECORDS: 2,
  MAX_BACKUP_BYTES: 8000
}))
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
const password = 'backup-contract-fixture'
const item = {
  type: 'note' as const,
  title: 'note',
  tags: [],
  body: 'body',
  username: '',
  secret: '',
  url: '',
  favorite: false,
  createdAt: 1,
  updatedAt: 1
}
it('uses the same record bound for export and restore, including synced history', async () => {
  const engine = new CryptoEngine(),
    setup = await engine.initialize(password)
  await engine.verifyRecovery(setup.recoveryCode)
  const first = await engine.saveItem({ item }),
    second = await engine.saveItem({ item, id: first.id, base: first.revision })
  const { currentWorkspace, WorkspaceDB } = await import('../src/lib/db')
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  const prior = await db.db.get('outbox', first.revision)
  await db.db.put('outbox', { ...prior, state: 'stored' })
  db.db.close()
  const backup = await engine.exportBackup(password)
  const last = (await engine.status()).meta!.lastBackupAt
  await engine.saveItem({ item })
  await expect(engine.exportBackup(password)).rejects.toMatchObject({ code: 'QUOTA_EXCEEDED' })
  expect((await engine.status()).meta!.lastBackupAt).toBe(last)
  await engine.lock()
  globalThis.indexedDB = new IDBFactory()
  const restored = new CryptoEngine()
  await restored.restore({
    file: new File([backup.blob], 'backup.dmsg'),
    code: setup.recoveryCode,
    password
  })
  const rows = (await restored.view()).outbox
  expect(rows.find((row) => row.id === first.revision)?.state).toBe('stored')
  expect(rows.find((row) => row.id === second.revision)?.state).toBe('local')
  await restored.lock()
})
it('does not advance the backup marker when encoded file data exceeds the package limit', async () => {
  const engine = new CryptoEngine(),
    setup = await engine.initialize(password)
  await engine.verifyRecovery(setup.recoveryCode)
  await engine.importFile(new File([new Uint8Array(8000)], 'file.bin'))
  await expect(engine.exportBackup(password)).rejects.toMatchObject({ code: 'QUOTA_EXCEEDED' })
  expect((await engine.status()).meta!.lastBackupAt).toBeNull()
  await engine.lock()
})
