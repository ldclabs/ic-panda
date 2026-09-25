import { b64, canonical, decodeCanonical, equal, hash, id, unb64 } from '../protocol/codec'
import {
  CLOUD_KINDS,
  emptyFile,
  objectBytes,
  openContentManifest,
  objectChannel,
  readObject,
  sealContentManifest,
  uploadPlanSchema,
  storedUploadPlanSchema,
  type ContentJob,
  type StoredUploadPlan,
  type ContentUpload
} from '../protocol/content'
import { ensure } from '../errors'
import { MAX_CIPHER_CHUNK } from '../config'
import { open, seal } from './primitives'
import type { WorkspaceDB } from '../db'
import type {
  Chunk,
  EncryptedObject,
  Item,
  Lease,
  WorkspaceMeta,
  FileManifest
} from '../models'

interface Port {
  ready(): Promise<{
    db: WorkspaceDB
    localKey: Uint8Array
    lease: Lease
    meta: WorkspaceMeta
    bundle: { root: string; roots?: Record<string, string> }
  }>
  decode<T>(record: EncryptedObject): Promise<T>
  verifyFile(record: EncryptedObject): Promise<unknown>
  tick(): Promise<unknown>
}
export interface CloudRevision {
  item_id: string
  revision_id: string
  base_revision: string | null
  upload_id: string | null
  tombstone: boolean
  conflict: boolean
}
type Download = {
  plan: StoredUploadPlan
  manifest: string
  content: FileManifest | null
  chunkIds: string[]
}
/** Handles keys and plaintext manifests exclusively in the unlocked worker. */
export class ContentEngine {
  // Opened manifests are immutable per upload and digest; the cache lives until lock.
  private downloads = new Map<string, Download>()
  constructor(private readonly port: Port) {}
  private async read<T>(key: string): Promise<T | null> {
    const { db, localKey } = await this.port.ready()
    const row = await db.db.get('local_private', key)
    return row
      ? decodeCanonical<T>(
          await open(localKey, row.ciphertext, ['dmsg/content-job/1', db.name, key]),
          MAX_CIPHER_CHUNK
        )
      : null
  }
  private async put(key: string, value: unknown) {
    const { db, localKey, lease } = await this.port.ready()
    const encoded = canonical(value)
    ensure(encoded.length <= MAX_CIPHER_CHUNK, 'QUOTA_EXCEEDED')
    await db.guardedPut(
      'local_private',
      {
        id: key,
        ciphertext: await seal(localKey, encoded, ['dmsg/content-job/1', db.name, key])
      },
      lease
    )
  }
  private fileKey(file: { object_id: string; version_id: string; root_generation: number }) {
    return `content-file:${file.object_id}:${file.version_id}:${file.root_generation}`
  }
  async save(job: ContentJob) {
    const { meta } = await this.port.ready()
    ensure(meta.account?.id === job.account, 'AUTH_REQUIRED')
    await this.put(`content-job:${job.recordKey}`, job)
    for (const upload of job.uploads)
      if (upload.plan.kind === 'file') await this.put(this.fileKey(upload.plan), upload)
  }
  async prepare(recordKey: string): Promise<ContentJob> {
    const { db, meta, bundle } = await this.port.ready()
    ensure(
      meta.account?.id && meta.registered && meta.account.rootDigest,
      'AUTH_REQUIRED',
      '请先绑定正式工作区并批准设备。'
    )
    const record = (await db.db.get('objects', recordKey)) as EncryptedObject
    ensure(record && CLOUD_KINDS.includes(record.kind), 'INVALID_INPUT')
    const existing = await this.read<ContentJob>(`content-job:${recordKey}`)
    if (existing) {
      ensure(existing.recordDigest === record.digest, 'IDEMPOTENCY_CONFLICT')
      return existing
    }
    const job: ContentJob = {
      format: 'dmsg-content-job/1',
      account: meta.account.id,
      recordKey,
      recordDigest: record.digest,
      uploads: [],
      revision: { requestId: id() }
    }
    const payload = await this.port.decode<Item>(record)
    const root = unb64(bundle.root)
    try {
      if ((record.kind === 'vault' || record.kind === 'migration_part') && payload.file) {
        await this.port.verifyFile(record)
        const file = payload.file,
          chunkIds: string[] = [],
          chunks = []
        for (const part of file.chunks) {
          const chunk = (await db.db.get('chunks', part.id)) as Chunk
          ensure(chunk && hash(unb64(chunk.ciphertext)) === part.digest, 'INTEGRITY_FAILED')
          chunks.push({ digest: part.digest, size: unb64(chunk.ciphertext).length })
          chunkIds.push(part.id)
        }
        if (!chunks.length)
          chunks.push({ digest: hash(emptyFile()), size: emptyFile().length })
        const previous = await this.read<ContentUpload>(
          this.fileKey({
            object_id: file.id,
            version_id: file.version,
            root_generation: meta.rootGeneration
          })
        )
        job.uploads.push(
          previous ?? {
            ...(await sealContentManifest(
              root,
              meta.account.id,
              {
                upload_id: id(),
                object_id: file.id,
                version_id: file.version,
                root_generation: meta.rootGeneration,
                epoch: 0,
                control_head: null,
                kind: 'file',
                chunks,
                expires_at: Date.now() + 86400000
              },
              file
            )),
            chunkIds
          }
        )
      }
      const object = objectBytes(record)
      job.uploads.push({
        ...(await sealContentManifest(
          root,
          meta.account.id,
          {
            upload_id: id(),
            object_id: record.id,
            version_id: record.revision,
            root_generation: meta.rootGeneration,
            epoch: 0,
            control_head: null,
            kind: 'vault',
            chunks: [{ digest: hash(object), size: object.length }],
            expires_at: Date.now() + 86400000
          },
          null
        )),
        chunkIds: [],
        object: b64(object)
      })
      await this.save(job)
      return job
    } finally {
      root.fill(0)
    }
  }
  /** Rewrap only uploads which can no longer be committed. The encrypted
   * object, revision ID, file chunks and operation request ID remain fixed. */
  async replan(recordKey: string, generation: number, uploadIds: string[]) {
    const { db, meta, bundle } = await this.port.ready()
    ensure(
      meta.account?.id && meta.rootGeneration === generation && uploadIds.length > 0,
      'REKEY_REQUIRED'
    )
    const job = await this.read<ContentJob>(`content-job:${recordKey}`)
    const record = (await db.db.get('objects', recordKey)) as EncryptedObject | undefined
    ensure(
      job && record && job.recordDigest === record.digest && !job.revision.result,
      'INTEGRITY_FAILED'
    )
    const replacement = new Set(uploadIds)
    ensure(
      replacement.size === uploadIds.length &&
        uploadIds.every((uploadId) => job.uploads.some((u) => u.plan.upload_id === uploadId)),
      'INVALID_INPUT'
    )
    const file = job.uploads.some((u) => u.plan.kind === 'file')
      ? (await this.port.decode<Item>(record)).file
      : null
    const root = unb64(bundle.root)
    try {
      const replaced: ContentUpload[] = []
      let vaultChanged = false
      for (const upload of job.uploads) {
        if (!replacement.has(upload.plan.upload_id)) {
          replaced.push(upload)
          continue
        }
        if (upload.plan.kind === 'file')
          ensure(
            file &&
              file.id === upload.plan.object_id &&
              file.version === upload.plan.version_id,
            'INTEGRITY_FAILED'
          )
        else {
          ensure(
            upload.plan.kind === 'vault' &&
              upload.object &&
              readObject(unb64(upload.object)).digest === record.digest,
            'INTEGRITY_FAILED'
          )
          vaultChanged = true
        }
        const { manifest_digest: _digest, manifest_size: _size, ...plan } = upload.plan
        replaced.push({
          ...(await sealContentManifest(
            root,
            meta.account.id,
            {
              ...plan,
              upload_id: id(),
              root_generation: generation,
              expires_at: Date.now() + 86400000
            },
            upload.plan.kind === 'file' ? file! : null
          )),
          chunkIds: upload.chunkIds,
          ...(upload.object ? { object: upload.object } : {})
        })
      }
      job.uploads = replaced
      if (vaultChanged) {
        delete job.revision.signed
        delete job.revision.deadline
      }
      await this.save(job)
      return job
    } finally {
      root.fill(0)
    }
  }
  async chunk(upload: ContentUpload, index: number) {
    const { db } = await this.port.ready()
    const plan = uploadPlanSchema.parse(upload.plan)
    ensure(
      Number.isInteger(index) && index >= 0 && index < plan.chunks.length,
      'INVALID_INPUT'
    )
    const bytes = upload.object
      ? unb64(upload.object)
      : upload.chunkIds.length
        ? unb64((await db.db.get('chunks', upload.chunkIds[index]))?.ciphertext ?? '')
        : emptyFile()
    ensure(
      bytes.length === plan.chunks[index].size && hash(bytes) === plan.chunks[index].digest,
      'INTEGRITY_FAILED'
    )
    return bytes
  }
  async acknowledge(job: ContentJob) {
    const { db, lease } = await this.port.ready()
    const result = job.revision.result
    ensure(result && result.revision_id === job.recordKey.split(':')[1], 'INTEGRITY_FAILED')
    await this.save(job)
    await db.contentAcknowledge(job.recordKey, result, lease)
  }
  private async download(plan: StoredUploadPlan): Promise<Download | null> {
    plan = storedUploadPlanSchema.parse(plan)
    const memo = `${plan.upload_id}:${plan.manifest_digest}`,
      cached = this.downloads.get(memo)
    if (cached && equal(canonical(cached.plan), canonical(plan))) return cached
    const { db, meta, bundle } = await this.port.ready()
    ensure(meta.account?.id, 'AUTH_REQUIRED')
    const manifest = (await db.db.get('chunks', `cloud-manifest:${plan.upload_id}`)) as
      Chunk | undefined
    if (!manifest) return null
    const bytes = unb64(manifest.ciphertext)
    ensure(
      bytes.length === plan.manifest_size && hash(bytes) === plan.manifest_digest,
      'INTEGRITY_FAILED'
    )
    let content: FileManifest | null = null
    if (plan.kind === 'vault' || plan.kind === 'file') {
      const root =
        plan.root_generation === meta.rootGeneration
          ? bundle.root
          : bundle.roots?.[String(plan.root_generation)]
      ensure(root, 'RECOVERY_INCOMPLETE', '云端内容需要尚未取得的历史根。')
      content = await openContentManifest(
        unb64(root),
        meta.account.id,
        uploadPlanSchema.parse(plan),
        bytes
      )
    }
    if (plan.kind === 'file') {
      ensure(
        content && plan.chunks.length === Math.max(1, content.chunks.length),
        'INTEGRITY_FAILED'
      )
      content.chunks.forEach((ref, index) =>
        ensure(
          ref.id === `${content!.id}:${content!.version}:${index}` &&
            ref.digest === plan.chunks[index].digest,
          'INTEGRITY_FAILED'
        )
      )
    }
    const result = {
      plan,
      manifest: manifest.ciphertext,
      content,
      chunkIds: plan.chunks.map(
        (_, index) => content?.chunks[index]?.id ?? `cloud:${plan.upload_id}:${index}`
      )
    }
    if (this.downloads.size >= 256) this.downloads.clear()
    this.downloads.set(memo, result)
    return result
  }
  async missing(plan: StoredUploadPlan) {
    const download = await this.download(plan)
    if (!download) return { manifest: false, chunks: [] as number[] }
    const { db } = await this.port.ready(),
      chunks: number[] = []
    // Stored chunks were hashed when written; their recorded digest identifies them.
    for (const [index, key] of download.chunkIds.entries()) {
      const chunk = (await db.db.get('chunks', key)) as Chunk | undefined
      if (!chunk) chunks.push(index)
      else ensure(chunk.digest === plan.chunks[index].digest, 'INTEGRITY_FAILED')
    }
    return { manifest: true, chunks }
  }
  async cache(plan: StoredUploadPlan, index: number | 'manifest', bytes: Uint8Array) {
    plan = storedUploadPlanSchema.parse(plan)
    const expected =
      index === 'manifest'
        ? { digest: plan.manifest_digest, size: plan.manifest_size }
        : Number.isInteger(index) && index >= 0
          ? plan.chunks[index]
          : undefined
    ensure(
      expected && bytes.length === expected.size && hash(bytes) === expected.digest,
      'INTEGRITY_FAILED'
    )
    const download = index === 'manifest' ? null : await this.download(plan)
    ensure(index === 'manifest' || download, 'RECOVERY_INCOMPLETE')
    const key =
      index === 'manifest' ? `cloud-manifest:${plan.upload_id}` : download!.chunkIds[index]
    const { db, lease } = await this.port.ready()
    await db.cacheChunks([{ id: key, ciphertext: b64(bytes), digest: expected.digest }], lease)
  }
  async receive(input: {
    objects: StoredUploadPlan[]
    revisions: CloudRevision[]
    through: number
    evidence: string
  }) {
    const { db, lease, meta } = await this.port.ready()
    ensure(
      meta.account?.id &&
        input.objects.length <= 10000 &&
        input.revisions.length <= 100000 &&
        input.evidence.length <= 8 * 1024 * 1024,
      'QUOTA_EXCEEDED'
    )
    const files = new Map<string, FileManifest>(),
      records = new Map<string, EncryptedObject>(),
      channels = new Map<string, string>()
    const uploads: NonNullable<WorkspaceMeta['cloudSnapshot']>['uploads'] = []
    // Ciphertext is already staged in bounded writes. Only authenticated object
    // records and inventory metadata are held until the atomic head commit.
    for (const plan of input.objects) {
      const download = await this.download(plan)
      ensure(download, 'RECOVERY_INCOMPLETE')
      const { content, chunkIds } = download
      for (const [index, key] of chunkIds.entries()) {
        await this.port.tick()
        const chunk = (await db.db.get('chunks', key)) as Chunk | undefined
        ensure(chunk, 'RECOVERY_INCOMPLETE')
        ensure(chunk.digest === plan.chunks[index].digest, 'INTEGRITY_FAILED')
        if (plan.kind === 'vault') {
          ensure(chunkIds.length === 1, 'INTEGRITY_FAILED')
          const bytes = unb64(chunk.ciphertext)
          ensure(
            bytes.length === plan.chunks[index].size &&
              hash(bytes) === plan.chunks[index].digest,
            'INTEGRITY_FAILED'
          )
          const record = readObject(bytes)
          ensure(
            record.subjectId === meta.account.id &&
              record.id === plan.object_id &&
              record.revision === plan.version_id &&
              record.key === `${record.id}:${record.revision}` &&
              CLOUD_KINDS.includes(record.kind),
            'INTEGRITY_FAILED'
          )
          channels.set(record.key, objectChannel(record, await this.port.decode(record)))
          ensure(
            !records.has(record.key) || records.get(record.key)!.digest === record.digest,
            'IDEMPOTENCY_CONFLICT'
          )
          records.set(record.key, record)
        } else if (plan.kind === 'file' && content?.size === 0) {
          ensure(
            content.chunks.length === 0 && equal(unb64(chunk.ciphertext), emptyFile()),
            'INTEGRITY_FAILED'
          )
        }
      }
      uploads.push({ plan, manifest: download.manifest, chunkIds })
      if (plan.kind === 'file') {
        ensure(content, 'INTEGRITY_FAILED')
        const key = `${content.id}:${content.version}`,
          old = files.get(key)
        ensure(!old || equal(canonical(old), canonical(content)), 'IDEMPOTENCY_CONFLICT')
        files.set(key, content)
      }
    }
    const incoming: EncryptedObject[] = [],
      heads = new Map<string, string>()
    for (const revision of input.revisions) {
      const key = `${revision.item_id}:${revision.revision_id}`,
        record = records.get(key)
      ensure(
        record &&
          record.tombstone === revision.tombstone &&
          record.parent === revision.base_revision,
        'RECOVERY_INCOMPLETE',
        '已提交版本的密文容器缺失或不一致。'
      )
      const payload = await this.port.decode<Item>(record)
      if ((record.kind === 'vault' || record.kind === 'migration_part') && payload.file) {
        const file = files.get(`${payload.file.id}:${payload.file.version}`)
        ensure(file && equal(canonical(file), canonical(payload.file)), 'RECOVERY_INCOMPLETE')
        // A version already stored here was verified when it was imported or received.
        const known = (await db.db.get('objects', record.key)) as EncryptedObject | undefined
        if (known?.digest !== record.digest) await this.port.verifyFile(record)
      }
      incoming.push({ ...record, conflict: revision.conflict })
      if (!revision.conflict) heads.set(record.id, record.revision)
    }
    await this.port.tick()
    const snapshot = {
      through: input.through,
      evidence: input.evidence,
      uploads,
      heads: [...heads]
    }
    await db.contentReceive(
      incoming,
      [...heads],
      input.through,
      input.evidence,
      lease,
      snapshot,
      channels
    )
    meta.cloudSnapshot = snapshot
    return { records: incoming.length, files: files.size, through: input.through }
  }
}
