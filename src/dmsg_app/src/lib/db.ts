import { deleteDB, openDB, type IDBPDatabase } from 'idb'
import type {
  Chunk,
  EncryptedObject,
  Lease,
  LocalEnvelope,
  OutboxJob,
  WorkspaceMeta
} from './models'
import { ensure } from './errors'

export const STORES = [
  'meta',
  'key_envelopes',
  'objects',
  'chunks',
  'local_private',
  'outbox',
  'inbox_cursors',
  'requests',
  'migration_jobs'
] as const
export const registryName = 'dmsg:registry:1'
export async function registry() {
  return openDB(registryName, 1, {
    upgrade(db) {
      db.createObjectStore('workspaces', { keyPath: 'name' })
    }
  })
}
export async function currentWorkspace(): Promise<string | null> {
  const db = await registry()
  try {
    return (await db.getAll('workspaces')).find((x) => x.active)?.name ?? null
  } finally {
    db.close()
  }
}
export async function registerWorkspace(name: string) {
  const db = await registry(),
    tx = db.transaction('workspaces', 'readwrite')
  try {
    const records = await tx.store.getAll()
    ensure(
      !records.some((record) => record.active && record.name !== name),
      'VERSION_CONFLICT',
      '此浏览器配置已经有工作台。请在空白浏览器配置中恢复。'
    )
    await tx.store.put({ name, active: true })
    await tx.done
  } finally {
    db.close()
  }
}
export async function removeWorkspaceDatabase(name: string) {
  await deleteDB(name)
}
export class WorkspaceDB {
  constructor(
    readonly db: IDBPDatabase,
    readonly name: string
  ) {}
  static async open(name: string) {
    ensure(
      /^dmsg:(local|staging|production):[0-9a-f]{64}:[0-9a-f]{64}$/.test(name),
      'INVALID_INPUT'
    )
    const db = await openDB(name, 1, {
      upgrade(db) {
        for (const store of STORES)
          db.createObjectStore(store, { keyPath: store === 'objects' ? 'key' : 'id' })
      },
      blocking() {
        db.close()
      }
    })
    return new WorkspaceDB(db, name)
  }
  async meta(): Promise<WorkspaceMeta | null> {
    return (await this.db.get('meta', 'workspace'))?.value ?? null
  }
  async envelope(): Promise<LocalEnvelope> {
    const value = await this.db.get('key_envelopes', 'local')
    ensure(value, 'RECOVERY_INCOMPLETE')
    return value
  }
  async acquire(owner: string, now = Date.now(), documentId?: string): Promise<Lease> {
    const tx = this.db.transaction('meta', 'readwrite'),
      old = (await tx.store.get('crypto-owner')) as Lease | undefined
    if (old && old.owner !== owner && old.expiresAt > now) {
      await tx.done
      throw new Error('WORKSPACE_BUSY：工作台正在另一窗口解锁。请先锁定该窗口。')
    }
    const lease: Lease = {
      id: 'crypto-owner',
      owner,
      fence: (old?.fence ?? 0) + 1,
      expiresAt: now + 20000,
      ...(documentId ? { documentId } : {})
    }
    await tx.store.put(lease)
    await tx.done
    return lease
  }
  async check(lease: Lease) {
    const current = (await this.db.get('meta', 'crypto-owner')) as Lease | undefined
    this.assertLease(current, lease)
  }
  private assertLease(current: Lease | undefined, lease: Lease) {
    ensure(
      current &&
        current.owner === lease.owner &&
        current.fence === lease.fence &&
        current.expiresAt > Date.now(),
      'LOCKED',
      '会话已结束，请重新解锁。'
    )
  }
  async renew(lease: Lease) {
    const tx = this.db.transaction('meta', 'readwrite'),
      current = (await tx.store.get('crypto-owner')) as Lease | undefined
    if (
      !current ||
      current.owner !== lease.owner ||
      current.fence !== lease.fence ||
      current.expiresAt <= Date.now()
    ) {
      await tx.done
      throw new Error('LOCKED')
    }
    current.expiresAt = Date.now() + 20000
    await tx.store.put(current)
    await tx.done
  }
  async release(lease: Lease) {
    const tx = this.db.transaction('meta', 'readwrite'),
      current = (await tx.store.get('crypto-owner')) as Lease | undefined
    if (current?.owner === lease.owner && current.fence === lease.fence)
      await tx.store.put({ ...current, expiresAt: 0 })
    await tx.done
  }
  async invalidate() {
    const tx = this.db.transaction('meta', 'readwrite'),
      current = (await tx.store.get('crypto-owner')) as Lease | undefined
    if (current) await tx.store.put({ ...current, fence: current.fence + 1, expiresAt: 0 })
    await tx.done
  }
  async guardedPut(
    store: 'meta' | 'key_envelopes' | 'local_private',
    value: unknown,
    lease: Lease
  ) {
    const tx = this.db.transaction(store === 'meta' ? 'meta' : ['meta', store], 'readwrite')
    this.assertLease(
      (await tx.objectStore('meta').get('crypto-owner')) as Lease | undefined,
      lease
    )
    await tx.objectStore(store).put(value)
    await tx.done
  }
  async completeRecovery(value: WorkspaceMeta, lease: Lease) {
    const tx = this.db.transaction(['meta', 'local_private'], 'readwrite')
    this.assertLease(
      (await tx.objectStore('meta').get('crypto-owner')) as Lease | undefined,
      lease
    )
    await tx.objectStore('meta').put({ id: 'workspace', value })
    await tx.objectStore('local_private').delete('pending-recovery')
    await tx.done
  }
  async checkpointFile(
    version: string,
    ciphertext: string,
    job: Record<string, unknown>,
    chunk: Chunk | undefined,
    lease: Lease
  ) {
    const tx = this.db.transaction(
      ['meta', 'local_private', 'migration_jobs', 'chunks'],
      'readwrite'
    )
    this.assertLease(
      (await tx.objectStore('meta').get('crypto-owner')) as Lease | undefined,
      lease
    )
    if (chunk) await tx.objectStore('chunks').put(chunk)
    await tx.objectStore('local_private').put({ id: `file-job:${version}`, ciphertext })
    await tx.objectStore('migration_jobs').put(job)
    await tx.done
  }
  async completeFile(version: string, job: Record<string, unknown>, lease: Lease) {
    const tx = this.db.transaction(['meta', 'local_private', 'migration_jobs'], 'readwrite')
    this.assertLease(
      (await tx.objectStore('meta').get('crypto-owner')) as Lease | undefined,
      lease
    )
    await tx.objectStore('migration_jobs').put(job)
    await tx.objectStore('local_private').delete(`file-job:${version}`)
    await tx.done
  }
  async commit(record: EncryptedObject, base: string | null, lease: Lease, job: OutboxJob) {
    const tx = this.db.transaction(['meta', 'objects', 'outbox'], 'readwrite')
    const current = (await tx.objectStore('meta').get('crypto-owner')) as Lease | undefined
    this.assertLease(current, lease)
    const head = await tx.objectStore('meta').get(`head:${record.id}`)
    const conflict = (head?.revision ?? null) !== base
    const previous = await tx.objectStore('objects').get(record.key)
    if (previous) {
      ensure(previous.digest === record.digest, 'IDEMPOTENCY_CONFLICT')
      await tx.done
      return previous as EncryptedObject
    }
    const saved = { ...record, conflict }
    await tx.objectStore('objects').put(saved)
    await tx
      .objectStore('outbox')
      .put({ ...job, ...(conflict ? { state: 'blocked', error: 'VERSION_CONFLICT' } : {}) })
    if (!conflict)
      await tx.objectStore('meta').put({ id: `head:${record.id}`, revision: record.revision })
    await tx.done
    return saved
  }
  async heads(): Promise<EncryptedObject[]> {
    const tx = this.db.transaction(['meta', 'objects'])
    const heads = (await tx.objectStore('meta').getAll()).filter((m) =>
      m.id.startsWith('head:')
    )
    const values = await Promise.all(
      heads.map((head) =>
        tx.objectStore('objects').get(`${head.id.slice(5)}:${head.revision}`)
      )
    )
    await tx.done
    ensure(values.every(Boolean), 'RECOVERY_INCOMPLETE', '本地历史有缺口，请恢复备份。')
    return values
  }
}
