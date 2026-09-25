import { deleteDB, openDB, type IDBPDatabase, type IDBPTransaction } from 'idb'
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

const STORES = [
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
type Store = (typeof STORES)[number]
type WriteTransaction = IDBPTransaction<unknown, ArrayLike<string>, 'readwrite'>
export async function registry() {
  return openDB('dmsg:registry:1', 1, {
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
export const prefixRange = (prefix: string) => IDBKeyRange.bound(prefix, `${prefix}￿`)
export const objectHead = (record: EncryptedObject, channelId = '') => ({
  id: `head:${record.id}`,
  revision: record.revision,
  kind: record.kind,
  channelId
})
/** Conflict markers let views read unresolved branches without scanning history. */
export async function putObject(tx: WriteTransaction, record: EncryptedObject) {
  await tx.objectStore('objects').put(record)
  if (record.conflict) await tx.objectStore('meta').put({ id: `conflict:${record.key}` })
  else await tx.objectStore('meta').delete(`conflict:${record.key}`)
}
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
      upgrade(db) {
        const store = (name: Store) =>
          db.createObjectStore(name, { keyPath: name === 'objects' ? 'key' : 'id' })
        for (const name of STORES) {
          const created = store(name)
          if (name === 'meta') {
            created.createIndex('head-kind', 'kind')
            created.createIndex('head-channel', ['kind', 'channelId'])
          } else if (name === 'objects') created.createIndex('kind', 'kind')
          else if (name === 'outbox') created.createIndex('state', 'state')
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
    this.assertLease(await this.db.get('meta', 'crypto-owner'), lease)
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
  /** Runs IDB-only work in one transaction owned by the current lease holder.
   * The callback must not await network or crypto work. */
  async guarded<T>(
    stores: Store[],
    lease: Lease,
    run: (tx: WriteTransaction) => Promise<T>
  ): Promise<T> {
    const tx = this.db.transaction(
      ['meta', ...stores.filter((store) => store !== 'meta')],
      'readwrite'
    )
    try {
      this.assertLease(await tx.objectStore('meta').get('crypto-owner'), lease)
      const result = await run(tx)
      await tx.done
      return result
    } catch (error) {
      try {
        tx.abort()
      } catch {}
      await tx.done.catch(() => {})
      throw error
    }
  }
  async renew(lease: Lease) {
    await this.guarded([], lease, async (tx) => {
      const current = (await tx.objectStore('meta').get('crypto-owner')) as Lease
      await tx.objectStore('meta').put({ ...current, expiresAt: Date.now() + 20000 })
    })
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
    store: 'meta' | 'key_envelopes' | 'local_private' | 'inbox_cursors',
    value: unknown,
    lease: Lease
  ) {
    await this.guarded([store], lease, (tx) => tx.objectStore(store).put(value))
  }
  async cacheChunks(chunks: Chunk[], lease: Lease) {
    ensure(
      chunks.length <= 128 && new Set(chunks.map((c) => c.id)).size === chunks.length,
      'INVALID_INPUT'
    )
    await this.guarded(['chunks'], lease, async (tx) => {
      for (const chunk of chunks) {
        const old = await tx.objectStore('chunks').get(chunk.id)
        ensure(
          !old || (old.digest === chunk.digest && old.ciphertext === chunk.ciphertext),
          'IDEMPOTENCY_CONFLICT'
        )
        await tx.objectStore('chunks').put(chunk)
      }
    })
  }
  async completeRecovery(value: WorkspaceMeta, lease: Lease) {
    await this.guarded(['local_private'], lease, async (tx) => {
      await tx.objectStore('meta').put({ id: 'workspace', value })
      await tx.objectStore('local_private').delete('pending-recovery')
    })
  }
  async replaceKeys(meta: WorkspaceMeta, envelope: LocalEnvelope, lease: Lease) {
    await this.guarded(['key_envelopes'], lease, async (tx) => {
      await tx.objectStore('meta').put({ id: 'workspace', value: meta })
      await tx.objectStore('key_envelopes').put(envelope)
    })
  }
  async legacyCheckpoint(
    header: Record<string, unknown>,
    privateRecord: { id: string; ciphertext: string },
    lease: Lease
  ) {
    await this.guarded(['local_private', 'migration_jobs'], lease, async (tx) => {
      await tx.objectStore('local_private').put(privateRecord)
      await tx.objectStore('migration_jobs').put(header)
    })
  }
  async authorizeFormal(
    requestId: string,
    digest: string,
    executionId: string,
    ciphertext: string,
    lease: Lease
  ) {
    await this.guarded(['requests', 'local_private'], lease, async (tx) => {
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
    })
  }
  async checkpointFile(
    version: string,
    ciphertext: string,
    job: Record<string, unknown>,
    chunk: Chunk | undefined,
    lease: Lease
  ) {
    await this.guarded(['local_private', 'migration_jobs', 'chunks'], lease, async (tx) => {
      if (chunk) await tx.objectStore('chunks').put(chunk)
      await tx.objectStore('local_private').put({ id: `file-job:${version}`, ciphertext })
      await tx.objectStore('migration_jobs').put(job)
    })
  }
  async completeFile(version: string, job: Record<string, unknown>, lease: Lease) {
    await this.guarded(['local_private', 'migration_jobs'], lease, async (tx) => {
      await tx.objectStore('migration_jobs').put(job)
      await tx.objectStore('local_private').delete(`file-job:${version}`)
    })
  }
  async commit(
    record: EncryptedObject,
    base: string | null,
    lease: Lease,
    job: OutboxJob,
    channelId = ''
  ) {
    return this.guarded(['objects', 'outbox'], lease, async (tx) => {
      const head = await tx.objectStore('meta').get(`head:${record.id}`)
      const conflict = (head?.revision ?? null) !== base
      const previous = await tx.objectStore('objects').get(record.key)
      if (previous) {
        ensure(previous.digest === record.digest, 'IDEMPOTENCY_CONFLICT')
        return previous as EncryptedObject
      }
      const saved = { ...record, conflict }
      await putObject(tx, saved)
      await tx
        .objectStore('outbox')
        .put({ ...job, ...(conflict ? { state: 'blocked', error: 'VERSION_CONFLICT' } : {}) })
      if (!conflict) await tx.objectStore('meta').put(objectHead(record, channelId))
      return saved
    })
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
        ? await store.index('head-channel').getAll([kind, channelId])
        : await store.index('head-kind').getAll(kind)
      : await store.getAll(prefixRange('head:'))
    const values = await Promise.all(
      heads.map((head) =>
        tx.objectStore('objects').get(`${head.id.slice(5)}:${head.revision}`)
      )
    )
    await tx.done
    ensure(values.every(Boolean), 'RECOVERY_INCOMPLETE', '本地历史有缺口，请恢复备份。')
    return values
  }
  /** Conflicting object versions, oldest markers first. */
  async conflicts(): Promise<EncryptedObject[]> {
    const tx = this.db.transaction(['meta', 'objects'])
    const markers = await tx.objectStore('meta').getAll(prefixRange('conflict:'))
    const values = await Promise.all(
      markers.map((marker) => tx.objectStore('objects').get(marker.id.slice(9)))
    )
    await tx.done
    return values.filter(Boolean)
  }

  async contentAcknowledge(
    key: string,
    result: { revision_id: string; head: string; conflict: boolean; tombstone: boolean },
    lease: Lease
  ) {
    await this.guarded(['objects', 'outbox'], lease, async (tx) => {
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
      await putObject(tx, { ...record, conflict: result.conflict })
      if (!result.conflict) {
        ensure(result.head === record.revision, 'INTEGRITY_FAILED')
        await tx
          .objectStore('meta')
          .put({ id: `cloud-head:${record.id}`, revision: record.revision })
        return
      }
      const local = await tx.objectStore('meta').get(`head:${record.id}`)
      if (local?.revision !== record.revision) return
      const head = await tx.objectStore('objects').get(`${record.id}:${result.head}`)
      if (head) await tx.objectStore('meta').put(objectHead(head, local.channelId))
      else await tx.objectStore('meta').delete(`head:${record.id}`)
    })
  }

  async contentReceive(
    records: EncryptedObject[],
    heads: [string, string][],
    through: number,
    evidence: string,
    lease: Lease,
    snapshot?: WorkspaceMeta['cloudSnapshot'],
    channels = new Map<string, string>()
  ) {
    await this.guarded(['objects', 'outbox'], lease, async (tx) => {
      const meta = tx.objectStore('meta')
      ensure(
        Number.isSafeInteger(through) &&
          through >= ((await meta.get('cloud-snapshot'))?.through ?? 0) &&
          evidence.length <= 8 * 1024 * 1024,
        'INTEGRITY_FAILED',
        '云端清单回退或证据超过限制。'
      )
      const incoming = new Set(records.map((r) => r.key))
      for (const head of await meta.getAll(prefixRange('cloud-head:')))
        ensure(
          incoming.has(`${head.id.slice(11)}:${head.revision}`),
          'INTEGRITY_FAILED',
          '云端清单遗漏本机已确认的版本。'
        )
      for (const record of records) {
        const prior = await tx.objectStore('objects').get(record.key)
        ensure(!prior || prior.digest === record.digest, 'IDEMPOTENCY_CONFLICT')
        await putObject(tx, record)
        const job = await tx.objectStore('outbox').get(record.revision)
        if (job) await tx.objectStore('outbox').put({ ...job, state: 'stored' })
      }
      for (const [id, revision] of heads) {
        const local = await meta.get(`head:${id}`)
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
              await putObject(tx, { ...branch, conflict: true })
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
          await meta.put(objectHead(head, channels.get(head.key)))
        }
        await meta.put({ id: `cloud-head:${id}`, revision })
      }
      await meta.put({ id: 'cloud-snapshot', through, evidence })
      if (snapshot) {
        const row = await meta.get('workspace')
        await meta.put({ ...row, value: { ...row.value, cloudSnapshot: snapshot } })
      }
    })
  }
}
