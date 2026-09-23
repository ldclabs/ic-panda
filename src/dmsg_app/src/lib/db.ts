import { deleteDB, openDB, type IDBPDatabase } from 'idb'
import type {
  Chunk,
  EncryptedObject,
  Lease,
  LocalEnvelope,
  OutboxJob,
  ObjectKind,
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
export async function registerWorkspace(name: string, expectedActive?: string) {
  const db = await registry(),
    tx = db.transaction('workspaces', 'readwrite')
  try {
    const records = await tx.store.getAll()
    ensure(
      !records.some(
        (record) => record.active && record.name !== name && record.name !== expectedActive
      ),
      'VERSION_CONFLICT',
      '此浏览器配置已经有工作台。请在空白浏览器配置中恢复。'
    )
    if (expectedActive) {
      ensure(
        records.some((record) => record.name === expectedActive && record.active),
        'VERSION_CONFLICT'
      )
      await tx.store.put({ name: expectedActive, active: false, retained: true })
    }
    await tx.store.put({ name, active: true })
    await tx.done
  } finally {
    db.close()
  }
}
export async function removeWorkspaceDatabase(name: string) {
  await deleteDB(name)
}
export const objectHead = (record: EncryptedObject, channelId = '') => ({
  id: `head:${record.id}`,
  revision: record.revision,
  kind: record.kind,
  channelId
})
export class WorkspaceDB {
  constructor(
    readonly db: IDBPDatabase,
    readonly name: string
  ) {}
  static async open(name: string) {
    ensure(
      /^dmsg:(local|staging|production):(?:[0-9a-f]{64}|[0-9a-v]{19}[0g]):[0-9a-f]{64}$/.test(
        name
      ),
      'INVALID_INPUT'
    )
    const db = await openDB(name, 2, {
      async upgrade(db, oldVersion, _newVersion, tx) {
        if (!oldVersion)
          for (const store of STORES)
            db.createObjectStore(store, { keyPath: store === 'objects' ? 'key' : 'id' })
        const meta = tx.objectStore('meta')
        meta.createIndex('head-kind', 'kind')
        meta.createIndex('head-channel', ['kind', 'channelId'])
        tx.objectStore('objects').createIndex('kind', 'kind')
        tx.objectStore('outbox').createIndex('state', 'state')
        // Index construction only; ciphertext and object formats stay untouched.
        for (const row of await meta.getAll(IDBKeyRange.bound('head:', 'head:\uffff'))) {
          const record = await tx
            .objectStore('objects')
            .get(`${row.id.slice(5)}:${row.revision}`)
          if (record) await meta.put(objectHead(record))
        }
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
  async cacheChunks(chunks: Chunk[], lease: Lease) {
    ensure(
      chunks.length <= 128 && new Set(chunks.map((c) => c.id)).size === chunks.length,
      'INVALID_INPUT'
    )
    const tx = this.db.transaction(['meta', 'chunks'], 'readwrite')
    try {
      this.assertLease(await tx.objectStore('meta').get('crypto-owner'), lease)
      for (const chunk of chunks) {
        const old = await tx.objectStore('chunks').get(chunk.id)
        ensure(
          !old || (old.digest === chunk.digest && old.ciphertext === chunk.ciphertext),
          'IDEMPOTENCY_CONFLICT'
        )
        await tx.objectStore('chunks').put(chunk)
      }
      await tx.done
    } catch (error) {
      try {
        tx.abort()
      } catch {}
      await tx.done.catch(() => {})
      throw error
    }
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

  async replaceKeys(meta: WorkspaceMeta, envelope: LocalEnvelope, lease: Lease) {
    const tx = this.db.transaction(['meta', 'key_envelopes'], 'readwrite')
    this.assertLease(
      (await tx.objectStore('meta').get('crypto-owner')) as Lease | undefined,
      lease
    )
    await tx.objectStore('meta').put({ id: 'workspace', value: meta })
    await tx.objectStore('key_envelopes').put(envelope)
    await tx.done
  }
  async legacyCheckpoint(
    header: Record<string, unknown>,
    privateRecord: { id: string; ciphertext: string },
    lease: Lease
  ) {
    const tx = this.db.transaction(['meta', 'local_private', 'migration_jobs'], 'readwrite')
    this.assertLease(await tx.objectStore('meta').get('crypto-owner'), lease)
    await tx.objectStore('local_private').put(privateRecord)
    await tx.objectStore('migration_jobs').put(header)
    await tx.done
  }
  async authorizeFormal(
    requestId: string,
    digest: string,
    executionId: string,
    ciphertext: string,
    lease: Lease
  ) {
    const tx = this.db.transaction(['meta', 'requests', 'local_private'], 'readwrite')
    this.assertLease(await tx.objectStore('meta').get('crypto-owner'), lease)
    const request = await tx.objectStore('requests').get(requestId)
    ensure(
      request?.state === 'awaiting_user' &&
        request.digest === digest &&
        request.expiresAt > Date.now(),
      'EXPIRED'
    )
    await tx
      .objectStore('local_private')
      .put({ id: `control:formal:${requestId}`, ciphertext })
    await tx.objectStore('requests').put({ ...request, state: 'authorized', executionId })
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
  async commit(
    record: EncryptedObject,
    base: string | null,
    lease: Lease,
    job: OutboxJob,
    channelId = ''
  ) {
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
    if (!conflict) await tx.objectStore('meta').put(objectHead(record, channelId))
    await tx.done
    return saved
  }
  async getHead(id: string, kind?: ObjectKind): Promise<EncryptedObject | undefined> {
    const tx = this.db.transaction(['meta', 'objects'])
    const head = await tx.objectStore('meta').get(`head:${id}`)
    const record = head
      ? await tx.objectStore('objects').get(`${id}:${head.revision}`)
      : undefined
    await tx.done
    ensure(!head || record, 'RECOVERY_INCOMPLETE')
    return record && (!kind || record.kind === kind) ? record : undefined
  }
  async outboxSummary(): Promise<Pick<OutboxJob, 'id' | 'state' | 'error'>[]> {
    const rows: Pick<OutboxJob, 'id' | 'state' | 'error'>[] = []
    const tx = this.db.transaction('outbox')
    for (let cursor = await tx.store.openCursor(); cursor; cursor = await cursor.continue()) {
      const { id, state, error } = cursor.value as OutboxJob
      rows.push({ id, state, ...(error ? { error } : {}) })
    }
    await tx.done
    return rows
  }
  async heads(kind?: ObjectKind, channelId?: string): Promise<EncryptedObject[]> {
    const tx = this.db.transaction(['meta', 'objects'])
    const store = tx.objectStore('meta')
    const heads = kind
      ? channelId
        ? [
            ...(await store.index('head-channel').getAll([kind, channelId])),
            ...(await store.index('head-channel').getAll([kind, '']))
          ]
        : await store.index('head-kind').getAll(kind)
      : await store.getAll(IDBKeyRange.bound('head:', 'head:\uffff'))
    const values = await Promise.all(
      heads.map((head) =>
        tx.objectStore('objects').get(`${head.id.slice(5)}:${head.revision}`)
      )
    )
    await tx.done
    ensure(values.every(Boolean), 'RECOVERY_INCOMPLETE', '本地历史有缺口，请恢复备份。')
    return values
  }

  async contentAcknowledge(
    key: string,
    result: { revision_id: string; head: string; conflict: boolean; tombstone: boolean },
    lease: Lease
  ) {
    const tx = this.db.transaction(['meta', 'objects', 'outbox'], 'readwrite')
    try {
      this.assertLease(await tx.objectStore('meta').get('crypto-owner'), lease)
      const record = (await tx.objectStore('objects').get(key)) as EncryptedObject
      ensure(
        record &&
          record.revision === result.revision_id &&
          record.tombstone === result.tombstone &&
          /^[0-9a-f]{64}$/.test(result.head),
        'INTEGRITY_FAILED'
      )
      const job = await tx.objectStore('outbox').get(record.revision)
      ensure(job, 'NOT_FOUND')
      await tx.objectStore('outbox').put({ ...job, state: 'stored', cloudReceipt: result })
      await tx.objectStore('objects').put({ ...record, conflict: result.conflict })
      if (!result.conflict) {
        ensure(result.head === record.revision, 'INTEGRITY_FAILED')
        await tx
          .objectStore('meta')
          .put({ id: `cloud-head:${record.id}`, revision: record.revision })
      } else if (
        (await tx.objectStore('meta').get(`head:${record.id}`))?.revision === record.revision
      ) {
        const head = await tx.objectStore('objects').get(`${record.id}:${result.head}`)
        if (head) await tx.objectStore('meta').put(objectHead(head))
        else await tx.objectStore('meta').delete(`head:${record.id}`)
      }
      await tx.done
    } catch (error) {
      tx.abort()
      await tx.done.catch(() => {})
      throw error
    }
  }

  async contentReceive(
    records: EncryptedObject[],
    chunks: Chunk[],
    heads: [string, string][],
    through: number,
    evidence: string,
    lease: Lease,
    snapshot?: WorkspaceMeta['cloudSnapshot'],
    channels = new Map<string, string>()
  ) {
    const tx = this.db.transaction(['meta', 'objects', 'chunks', 'outbox'], 'readwrite')
    try {
      this.assertLease(await tx.objectStore('meta').get('crypto-owner'), lease)
      const meta = await tx.objectStore('meta').getAll()
      ensure(
        Number.isSafeInteger(through) &&
          through >= (meta.find((m) => m.id === 'cloud-snapshot')?.through ?? 0) &&
          evidence.length <= 8 * 1024 * 1024,
        'INTEGRITY_FAILED',
        '云端清单回退或证据超过限制。'
      )
      const incoming = new Set(records.map((r) => r.key))
      for (const head of meta.filter((m) => m.id.startsWith('cloud-head:')))
        ensure(
          incoming.has(`${head.id.slice(11)}:${head.revision}`),
          'INTEGRITY_FAILED',
          '云端清单遗漏本机已确认的版本。'
        )
      for (const chunk of chunks) {
        const prior = await tx.objectStore('chunks').get(chunk.id)
        ensure(!prior || prior.digest === chunk.digest, 'IDEMPOTENCY_CONFLICT')
        await tx.objectStore('chunks').put(chunk)
      }
      for (const record of records) {
        const prior = await tx.objectStore('objects').get(record.key)
        ensure(!prior || prior.digest === record.digest, 'IDEMPOTENCY_CONFLICT')
        await tx.objectStore('objects').put(record)
        const job = await tx.objectStore('outbox').get(record.revision)
        if (job) await tx.objectStore('outbox').put({ ...job, state: 'stored' })
      }
      for (const [id, revision] of heads) {
        const local = await tx.objectStore('meta').get(`head:${id}`)
        let keepLocal = false
        if (local && local.revision !== revision && !incoming.has(`${id}:${local.revision}`)) {
          const branch = await tx.objectStore('objects').get(`${id}:${local.revision}`)
          if (branch) {
            let ancestor: EncryptedObject | undefined = branch
            const seen = new Set<string>()
            while (ancestor?.parent && !seen.has(ancestor.revision)) {
              seen.add(ancestor.revision)
              if (ancestor.parent === revision) {
                keepLocal = true
                break
              }
              ancestor = await tx.objectStore('objects').get(`${id}:${ancestor.parent}`)
            }
            if (!keepLocal) {
              await tx.objectStore('objects').put({ ...branch, conflict: true })
              const job = await tx.objectStore('outbox').get(branch.revision)
              if (job)
                await tx
                  .objectStore('outbox')
                  .put({ ...job, state: 'blocked', error: 'VERSION_CONFLICT' })
            }
          }
        }
        if (!keepLocal) {
          const head = await tx.objectStore('objects').get(`${id}:${revision}`)
          ensure(head, 'RECOVERY_INCOMPLETE')
          await tx.objectStore('meta').put(objectHead(head, channels.get(head.key)))
        }
        await tx.objectStore('meta').put({ id: `cloud-head:${id}`, revision })
      }
      await tx.objectStore('meta').put({ id: 'cloud-snapshot', through, evidence })
      if (snapshot) {
        const row = await tx.objectStore('meta').get('workspace')
        await tx
          .objectStore('meta')
          .put({ ...row, value: { ...row.value, cloudSnapshot: snapshot } })
      }
      await tx.done
    } catch (error) {
      tx.abort()
      await tx.done.catch(() => {})
      throw error
    }
  }
}
