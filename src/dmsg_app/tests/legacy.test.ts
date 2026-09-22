import { beforeEach, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import {
  pack,
  observation,
  digest,
  encodeArchive,
  sealTransfer,
  type LegacyArchive
} from '@dmsg/legacy'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'

beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
const password = 'legacy test passphrase'
it('imports exact encrypted transfer once, resumes after restart and recovers history offline without the pairing key', async () => {
  let engine = new CryptoEngine()
  const initialized = await engine.initialize(password)
  await engine.verifyRecovery(initialized.recoveryCode)
  const pairing = await engine.legacyPair(
    'https://dmsg.net',
    `chrome-extension://${'a'.repeat(32)}`
  )
  const original = pack(
    observation({ kind: 1, payload: pack('original system history'), created_at: 1n })
  )
  const archive: LegacyArchive = {
    format: 'dmsg-legacy-archive/1',
    inventory: {
      format: 'dmsg-legacy-inventory/1',
      principal: 'aaaaa-aa',
      messageCanister: 'aaaaa-aa',
      mode: 'Local',
      snapshot: 'pre_migration',
      objects: [
        {
          key: 'aaaaa-aa/channel/1/message/1',
          kind: 'message',
          bytes: original,
          digest: digest(original),
          trust: 'query_observation',
          observedAt: 1
        }
      ],
      gaps: [{ source: 'missing-file', code: 'missing', detail: 'Synthetic missing file' }],
      calls: []
    },
    keys: [],
    checks: [],
    createdAt: 1
  }
  const encrypted = await sealTransfer(encodeArchive(archive), pairing.offer, {
    origin: pairing.offer.origin,
    target: pairing.offer.target,
    fingerprint: pairing.fingerprint
  })
  const request = {
    file: new File([encrypted], 'legacy.dmsg-migration'),
    nonce: pairing.offer.nonce,
    fingerprint: pairing.fingerprint,
    principal: 'aaaaa-aa'
  }
  await expect(engine.legacyImport({ ...request, principal: '2vxsx-fae' })).rejects.toThrow(
    '身份'
  )
  await engine.lock()
  engine = new CryptoEngine()
  await engine.unlock(password)
  expect((await engine.legacyPairs())[0]?.fingerprint).toBe(pairing.fingerprint)
  const report = await engine.legacyImport(request)
  expect(report.stage).toBe('partial')
  expect(report.messages[0]?.text).toBe('original system history')
  expect((await engine.view()).entries).toHaveLength(0)
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  const parts = (await db.heads()).filter((r) => r.kind === 'migration_part')
  expect(parts).toHaveLength(1)
  await expect(engine.downloadFile(parts[0].key)).rejects.toThrow('NOT_FOUND')
  expect((await engine.legacyImport(request)).key).toBe(report.key)
  const another = await engine.legacyPair(
    'https://dmsg.net',
    `chrome-extension://${'a'.repeat(32)}`
  )
  const reread = structuredClone(archive)
  reread.createdAt += 1000
  reread.inventory.objects.forEach((o) => (o.observedAt += 1000))
  const reexported = await sealTransfer(encodeArchive(reread), another.offer, {
    origin: another.offer.origin,
    target: another.offer.target,
    fingerprint: another.fingerprint
  })
  expect(
    (
      await engine.legacyImport({
        file: new File([reexported], 'again'),
        nonce: another.offer.nonce,
        fingerprint: another.fingerprint,
        principal: 'aaaaa-aa'
      })
    ).key
  ).toBe(report.key)
  expect(await engine.legacyList()).toHaveLength(1)
  expect(await engine.legacyPairs()).toHaveLength(0)
  const changed = new Uint8Array(encrypted)
  changed[changed.length - 1] ^= 1
  await expect(
    engine.legacyImport({ ...request, file: new File([changed], 'changed') })
  ).rejects.toThrow('另一个档案')
  const backup = await engine.exportBackup(password),
    text = await backup.blob.text()
  expect(text).not.toContain(pairing.offer.nonce)
  expect(text).not.toContain('original system history')
  db.db.close()
  await engine.lock()
  globalThis.indexedDB = new IDBFactory()
  engine = new CryptoEngine()
  await engine.restore({
    file: new File([backup.blob], 'backup.dmsg'),
    code: initialized.recoveryCode,
    password
  })
  expect((await engine.legacyReport(report.key)).messages[0]?.text).toBe(
    'original system history'
  )
  expect((await engine.legacyReport(report.key)).recovery).toBe('partial_verified')
  expect(await engine.legacyPairs()).toHaveLength(0)
  expect((await engine.status()).meta?.registered).toBe(false)
  await engine.lock()
}, 60000)

it('pauses and resumes a durable migration, while cancellation retains copied data and disables the old pairing', async () => {
  let pause = true
  const engine = new CryptoEngine((progress) => {
    if (pause && progress.stage === '正在分块加密') {
      pause = false
      engine.legacyPause()
    }
  })
  const initialized = await engine.initialize(password)
  await engine.verifyRecovery(initialized.recoveryCode)
  const bytes = pack(observation(new Uint8Array(1024 * 1024 + 32).fill(17)))
  const archive: LegacyArchive = {
    format: 'dmsg-legacy-archive/1',
    inventory: {
      format: 'dmsg-legacy-inventory/1',
      principal: 'aaaaa-aa',
      messageCanister: 'aaaaa-aa',
      mode: 'Local',
      snapshot: 'pre_migration',
      objects: [
        {
          key: 'local/cache',
          kind: 'local',
          bytes,
          digest: digest(bytes),
          trust: 'local_cache',
          observedAt: 1
        }
      ],
      gaps: [],
      calls: []
    },
    keys: [],
    checks: [],
    createdAt: 1
  }
  async function transfer() {
    const pairing = await engine.legacyPair(
      'https://dmsg.net',
      `chrome-extension://${'a'.repeat(32)}`
    )
    const bytes = await sealTransfer(encodeArchive(archive), pairing.offer, {
      origin: pairing.offer.origin,
      target: pairing.offer.target,
      fingerprint: pairing.fingerprint
    })
    return {
      file: new File([bytes], 'legacy'),
      nonce: pairing.offer.nonce,
      fingerprint: pairing.fingerprint,
      principal: 'aaaaa-aa'
    }
  }
  const first = await transfer()
  await expect(engine.legacyImport(first)).rejects.toThrow('迁移已暂停')
  const job = (await engine.legacyJobs())[0]
  expect(job.state).toBe('paused')
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  const chunk = (await db.db.getAll('chunks'))[0]
  await engine.legacyCancel(job.id)
  await expect(engine.legacyImport(first)).rejects.toThrow('此配对已取消')
  expect(await db.db.get('chunks', chunk.id)).toEqual(chunk)
  const next = await transfer()
  const result = await engine.legacyImport(next)
  expect(result.stage).toBe('content_verified')
  expect((await engine.legacyJobs())[0]).toMatchObject({
    id: job.id,
    state: 'complete',
    completed: 1
  })
  expect(await db.db.get('chunks', chunk.id)).toEqual(chunk)
  db.db.close()
  await engine.lock()
}, 60000)
