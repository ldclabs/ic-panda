import { b64, canonical, decodeCanonical, equal, hash, id, unb64 } from '../protocol/codec'
import {
  emptyFile,
  objectBytes,
  openContentManifest,
  readObject,
  sealContentManifest,
  uploadPlanSchema,
  storedUploadPlanSchema,
  type ContentJob,
  type StoredUploadPlan,
  type ContentUpload
} from '../protocol/content'
import { ensure } from '../errors'
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
  verifyFile(record: EncryptedObject, chunks?: Map<string, Chunk>): Promise<unknown>
  tick(): Promise<unknown>
}
export interface DownloadedContent {
  plan: StoredUploadPlan
  manifest: string
  chunks: string[]
}
export interface CloudRevision {
  item_id: string
  revision_id: string
  base_revision: string | null
  upload_id: string | null
  tombstone: boolean
  conflict: boolean
}
/** Handles keys and plaintext manifests exclusively in the unlocked worker. */
export class ContentEngine {
  constructor(private readonly port: Port) {}
  async jobs() {
    const { db, localKey } = await this.port.ready(),
      jobs: ContentJob[] = []
    for (const row of await db.db.getAll('local_private'))
      if (row.id.startsWith('content-job:'))
        jobs.push(
          decodeCanonical(
            await open(localKey, row.ciphertext, ['dmsg/content-job/1', db.name, row.id])
          )
        )
    return jobs
  }
  async save(job: ContentJob) {
    const { db, localKey, lease, meta } = await this.port.ready()
    ensure(meta.account?.id === job.account, 'AUTH_REQUIRED')
    const key = `content-job:${job.recordKey}`
    await db.guardedPut(
      'local_private',
      {
        id: key,
        ciphertext: await seal(localKey, canonical(job), ['dmsg/content-job/1', db.name, key])
      },
      lease
    )
  }
  async prepare(recordKey: string): Promise<ContentJob> {
    const { db, meta, bundle } = await this.port.ready()
    ensure(
      meta.account?.id && meta.registered && meta.account.rootDigest,
      'AUTH_REQUIRED',
      '请先绑定正式工作区并批准设备。'
    )
    const record = (await db.db.get('objects', recordKey)) as EncryptedObject
    ensure(
      record &&
        [
          'vault',
          'migration',
          'migration_part',
          'profile',
          'request',
          'formal_channel',
          'formal_control',
          'formal_operation',
          'formal_message',
          'formal_file',
          'commerce',
          'inbox'
        ].includes(record.kind),
      'INVALID_INPUT'
    )
    const existing = (await this.jobs()).find((j) => j.recordKey === recordKey)
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
        const previous = (await this.jobs())
          .flatMap((j) => j.uploads)
          .find(
            (u) =>
              u.plan.kind === 'file' &&
              u.plan.object_id === file.id &&
              u.plan.version_id === file.version &&
              u.plan.root_generation === meta.rootGeneration
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
  async receive(input: {
    objects: DownloadedContent[]
    revisions: CloudRevision[]
    through: number
    evidence: string
  }) {
    const { db, lease, meta, bundle } = await this.port.ready()
    ensure(
      meta.account?.id && input.objects.length <= 10000 && input.revisions.length <= 100000,
      'AUTH_REQUIRED'
    )
    const files = new Map<string, { file: FileManifest; chunks: Chunk[] }>(),
      records = new Map<string, EncryptedObject>()
    const preservedChunks = new Map<string, Chunk>(),
      uploads: NonNullable<WorkspaceMeta['cloudSnapshot']>['uploads'] = []
    let total = 0
    for (const object of input.objects) {
      await this.port.tick()
      const plan = storedUploadPlanSchema.parse(object.plan)
      const root =
        plan.root_generation === meta.rootGeneration
          ? bundle.root
          : bundle.roots?.[String(plan.root_generation)]
      if (plan.kind === 'vault' || plan.kind === 'file')
        ensure(root, 'RECOVERY_INCOMPLETE', '云端内容需要尚未取得的历史根。')
      ensure(object.chunks.length === plan.chunks.length, 'RECOVERY_INCOMPLETE')
      const bytes = object.chunks.map((c, i) => {
        const data = unb64(c)
        total += data.length
        ensure(
          total <= 256 * 1024 * 1024 &&
            data.length === plan.chunks[i].size &&
            hash(data) === plan.chunks[i].digest,
          'INTEGRITY_FAILED'
        )
        return data
      })
      const manifest = unb64(object.manifest)
      ensure(
        manifest.length === plan.manifest_size && hash(manifest) === plan.manifest_digest,
        'INTEGRITY_FAILED'
      )
      const content =
        plan.kind === 'vault' || plan.kind === 'file'
          ? await openContentManifest(
              unb64(root!),
              meta.account.id,
              uploadPlanSchema.parse(plan),
              manifest
            )
          : null
      const chunkIds = bytes.map((data, index) => {
        const key = content?.chunks[index]?.id ?? `cloud:${plan.upload_id}:${index}`
        const chunk = { id: key, ciphertext: b64(data), digest: hash(data) },
          old = preservedChunks.get(key)
        ensure(!old || old.digest === chunk.digest, 'IDEMPOTENCY_CONFLICT')
        preservedChunks.set(key, chunk)
        return key
      })
      uploads.push({ plan, manifest: object.manifest, chunkIds })
      if (plan.kind === 'vault') {
        ensure(bytes.length === 1, 'INTEGRITY_FAILED')
        const record = readObject(bytes[0])
        ensure(
          record.subjectId === meta.account.id &&
            record.id === plan.object_id &&
            record.revision === plan.version_id &&
            record.key === `${record.id}:${record.revision}` &&
            [
              'vault',
              'migration',
              'migration_part',
              'profile',
              'request',
              'formal_channel',
              'formal_control',
              'formal_operation',
              'formal_message',
              'formal_file',
              'commerce',
              'inbox'
            ].includes(record.kind),
          'INTEGRITY_FAILED'
        )
        await this.port.decode(record)
        ensure(
          !records.has(record.key) || records.get(record.key)!.digest === record.digest,
          'IDEMPOTENCY_CONFLICT'
        )
        records.set(record.key, record)
      } else if (plan.kind === 'file') {
        ensure(content, 'INTEGRITY_FAILED')
        if (!content.size)
          ensure(
            content.chunks.length === 0 && bytes.length === 1 && equal(bytes[0], emptyFile()),
            'INTEGRITY_FAILED'
          )
        else ensure(content.chunks.length === bytes.length, 'INTEGRITY_FAILED')
        const chunks = content.chunks.map((ref, i) => {
          ensure(
            ref.digest === plan.chunks[i].digest &&
              ref.id === `${content.id}:${content.version}:${i}`,
            'INTEGRITY_FAILED'
          )
          return { id: ref.id, ciphertext: b64(bytes[i]), digest: ref.digest }
        })
        const key = `${content.id}:${content.version}`,
          old = files.get(key)
        ensure(!old || equal(canonical(old.file), canonical(content)), 'IDEMPOTENCY_CONFLICT')
        files.set(key, { file: content, chunks })
      }
    }
    const chunks = new Map<string, Chunk>(preservedChunks)
    for (const file of files.values())
      for (const chunk of file.chunks) chunks.set(chunk.id, chunk)
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
        ensure(
          file && equal(canonical(file.file), canonical(payload.file)),
          'RECOVERY_INCOMPLETE'
        )
        await this.port.verifyFile(record, chunks)
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
      [...chunks.values()],
      [...heads],
      input.through,
      input.evidence,
      lease,
      snapshot
    )
    meta.cloudSnapshot = snapshot
    return { records: incoming.length, files: files.size, through: input.through }
  }
}
