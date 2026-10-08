import { ChannelVault } from './channel'
import { sha256 } from '@noble/hashes/sha2.js'
import { LegacyVault } from './legacy'
import { ContentEngine, type CloudRevision } from './content'
import type { ContentJob, ContentUpload, StoredUploadPlan } from '../protocol/content'
import {
  openContentManifest,
  objectChannel,
  storedUploadPlanSchema,
  uploadPlanSchema
} from '../protocol/content'
import { z } from 'zod'
import {
  config,
  CHUNK_SIZE,
  LOGIN_UNLOCK_INTERVAL_MS,
  MAX_FILE,
  MAX_OBJECT_BYTES,
  MAX_FORMAL_OBJECT_BYTES
} from '../config'
import {
  currentWorkspace,
  registerWorkspace,
  removeWorkspaceDatabase,
  WorkspaceDB,
  objectHead,
  prefixRange,
  putObject
} from '../db'
import { ensure } from '../errors'
import { xidBytes } from '../protocol/identity'
import {
  openRoot,
  recoverRoot,
  rootMaterial,
  rootTransport,
  wrapRoot,
  type RecoveryKey,
  type RootContext,
  type RootMaterial
} from './root'
import type {
  Chunk,
  EncryptedObject,
  FileManifest,
  Item,
  Lease,
  LocalEnvelope,
  ObjectKind,
  Profile,
  ViewData,
  WorkspaceMeta
} from '../models'
import {
  b64,
  canonical,
  decodeCanonical,
  digest,
  equal,
  hash,
  hex,
  id,
  random,
  unb64,
  unhex,
  utf8
} from '../protocol/codec'
import {
  ed25519,
  hpkeOpen,
  hpkePublic,
  hpkeSeal,
  open,
  prfKey,
  seal,
  unlockKey
} from './primitives'

const itemSchema = z
  .object({
    type: z.enum(['note', 'login', 'api', 'key', 'file']),
    title: z.string().trim().min(1).max(200),
    tags: z.array(z.string().trim().max(40)).max(20),
    body: z.string().max(20000),
    username: z.string().max(512),
    secret: z.string().max(10000),
    url: z.string().max(2048),
    favorite: z.boolean(),
    createdAt: z.number().int().nonnegative(),
    updatedAt: z.number().int().nonnegative()
  })
  .strict()
interface Bundle {
  root: string
  signing: string
  hpke: string
  roots?: Record<string, string>
}
interface FilePlan {
  manifest: FileManifest
  plainDigests: string[]
}
export type Progress = { stage: string; completed: number; total: number }
export class CryptoEngine {
  private db: WorkspaceDB | null = null
  private lease: Lease | null = null
  private localKey: Uint8Array | null = null
  private bundle: Bundle | null = null
  private meta: WorkspaceMeta | null = null
  private documentId: string | undefined
  private legacyPaused = false
  private content: ContentEngine | null = null
  // Session key for Internet Identity delegations: memory only, per worker.
  private authSeed = random()
  constructor(
    private progress: (value: Progress) => void = () => {},
    private owner = id()
  ) {}

  setDocumentId(documentId?: string) {
    if (this.lease) ensure(this.documentId === documentId, 'LOCKED')
    this.documentId = documentId
  }

  async status() {
    const name = await currentWorkspace()
    if (!name) return { exists: false, meta: null }
    const db = await WorkspaceDB.open(name)
    try {
      return { exists: true, meta: await db.meta() }
    } finally {
      db.db.close()
    }
  }

  private legacyVault() {
    return new LegacyVault({
      paused: () => this.legacyPaused,
      ready: () => this.ready(),
      tick: () => this.tick(),
      decode: <T>(record: EncryptedObject) => this.decode<T>(record),
      write: (payload, id, base) => this.write('migration', payload, id, base),
      importFile: (file, resume) => this.importFileStorage(file, resume, 'migration_part'),
      downloadFile: (key) => this.downloadStoredFile(key, true)
    })
  }
  async legacyPair(origin: 'https://dmsg.net' | 'https://panda.fans', target: string) {
    return this.legacyVault().pair(origin, target)
  }
  async legacyImport(input: {
    file: File
    nonce: string
    fingerprint: string
    principal: string
  }) {
    this.legacyPaused = false
    return this.legacyVault().import(input)
  }
  legacyPause() {
    this.legacyPaused = true
  }
  async legacyJobs() {
    return this.legacyVault().jobs()
  }
  async legacyCancel(job: string) {
    return this.legacyVault().cancel(job)
  }
  async legacyList() {
    return this.legacyVault().list()
  }
  async legacyPairs() {
    return this.legacyVault().pairs()
  }
  async legacyReport(key: string) {
    return this.legacyVault().report(key)
  }
  async legacyFile(key: string, source: string) {
    return this.legacyVault().file(key, source)
  }
  private channelVault() {
    return new ChannelVault({
      ready: () => this.ready(),
      decode: <T>(record: EncryptedObject) => this.decode<T>(record),
      write: (kind, value, id, base) => this.write(kind, value, id, base),
      tick: () => this.tick(),
      cacheFile: (manifest, chunks, hidden) => this.cacheChannelFile(manifest, chunks, hidden)
    })
  }
  async channelRemember(...args: Parameters<ChannelVault['remember']>) {
    return this.channelVault().remember(...args)
  }
  async channelList() {
    return this.channelVault().list()
  }
  async channelGet(channel: string) {
    return this.channelVault().get(channel)
  }
  async channelAdvance(...args: Parameters<ChannelVault['advance']>) {
    return this.channelVault().advance(...args)
  }
  async channelJob(...args: Parameters<ChannelVault['job']>) {
    return this.channelVault().job(...args)
  }
  async channelRotation(...args: Parameters<ChannelVault['rotation']>) {
    return this.channelVault().rotation(...args)
  }
  async channelInstall(...args: Parameters<ChannelVault['install']>) {
    return this.channelVault().install(...args)
  }
  async channelMessage(...args: Parameters<ChannelVault['message']>) {
    return this.channelVault().message(...args)
  }
  async channelReceive(...args: Parameters<ChannelVault['receive']>) {
    return this.channelVault().receive(...args)
  }
  async channelPending(channel: string) {
    return this.channelVault().pending(channel)
  }
  private async cacheChannelFile(
    manifest: FileManifest,
    chunks: Map<string, Chunk>,
    hidden = false
  ) {
    const verified = await this.verifyFileManifest(manifest, chunks),
      { db, lease } = await this.ready()
    await db.cacheChunks([...chunks.values()], lease)
    const kind = hidden ? 'migration_part' : 'vault'
    const old = await db.getHead(manifest.id, kind)
    let savedKey = old?.key
    if (old)
      ensure(
        equal(canonical((await this.decode<Item>(old)).file), canonical(manifest)),
        'IDEMPOTENCY_CONFLICT'
      )
    else
      savedKey = (
        await this.write(
          kind,
          {
            type: 'file',
            title: manifest.name,
            body: '',
            username: '',
            secret: '',
            url: '',
            tags: [],
            favorite: false,
            createdAt: Date.now(),
            updatedAt: Date.now(),
            file: manifest
          } satisfies Item,
          manifest.id
        )
      ).key
    const blob = new Blob(verified.parts, { type: manifest.mime })
    verified.parts.forEach((part) => part.fill(0))
    return { blob, name: manifest.name, key: savedKey! }
  }
  async channelFilePrepare(...args: Parameters<ChannelVault['filePrepare']>) {
    return this.channelVault().filePrepare(...args)
  }
  async channelAttachmentSource(channel: string, messageId: string) {
    return this.channelVault().attachmentSource(channel, messageId)
  }
  async channelFileChunk(...args: Parameters<ChannelVault['fileChunk']>) {
    return this.channelVault().fileChunk(...args)
  }
  async channelFileManifest(...args: Parameters<ChannelVault['fileManifest']>) {
    return this.channelVault().fileManifest(...args)
  }
  async channelFileDownload(...args: Parameters<ChannelVault['fileDownload']>) {
    return this.channelVault().fileDownload(...args)
  }
  async legacyCompareFrozen(key: string, file: File) {
    ensure(
      file.size <= 32 * 1024 * 1024 && /^[0-9a-f]{64}$/.test(config.legacy.cutover),
      'INVALID_INPUT',
      '需先配置已审核的冻结批次；证明文件上限 32 MiB。'
    )
    const { IC_ROOT_KEY } = await import('@icp-sdk/core/agent')
    return this.legacyVault().compareFrozen(key, new Uint8Array(await file.arrayBuffer()), {
      rootKey: unhex(IC_ROOT_KEY),
      channels: config.legacy.channels,
      buckets: config.legacy.buckets,
      cutover: unhex(config.legacy.cutover)
    })
  }
  async legacyPrepareGrant(input: {
    archiveKey: string
    source: import('@dmsg/legacy').SharedSource
    scope: Omit<
      import('../protocol/shared-history').LegacyHistoryScope,
      'archive_digest' | 'principal'
    >
    recipients: import('../protocol/channel').EpochRecipient[]
  }) {
    const scoped = await this.legacyVault().scoped(input.archiveKey, input.source)
    const scope = {
      ...input.scope,
      archive_digest: scoped.archiveDigest,
      principal: scoped.principal
    }
    const uploads = []
    for (const [index, key] of scoped.parts.entries())
      uploads.push(
        await this.channelVault().filePrepare(
          scope.channel_id,
          hash(canonical(['dmsg/legacy-grant-part/1', scope.grant_id, index])),
          key
        )
      )
    const grant = await this.channelVault().legacyGrant(
      scope,
      uploads.map((u) => u.fileRecord!),
      input.recipients
    )
    return { grant, uploads }
  }
  async legacyOpenGrant(grant: import('../protocol/shared-history').LegacyHistoryGrant) {
    return this.channelVault().openLegacyGrant(grant)
  }
  async legacyGrantManifest(...args: Parameters<ChannelVault['legacyGrantManifest']>) {
    return this.channelVault().legacyGrantManifest(...args)
  }
  async legacyGrantPart(...args: Parameters<ChannelVault['legacyGrantPart']>) {
    return this.channelVault().legacyGrantPart(...args)
  }
  async legacyReceiveScoped(
    parts: string[],
    source: import('@dmsg/legacy').SharedSource,
    digest: string
  ) {
    const blobs: Blob[] = []
    for (const key of parts) blobs.push((await this.downloadStoredFile(key, true)).blob)
    const file = new Blob(blobs)
    ensure(file.size <= 256 * 1024 * 1024, 'QUOTA_EXCEEDED')
    return this.legacyVault().receiveScoped(
      new Uint8Array(await file.arrayBuffer()),
      source,
      digest
    )
  }
  async channelHistory(...args: Parameters<ChannelVault['history']>) {
    return this.channelVault().history(...args)
  }
  async channelHistoryInstall(...args: Parameters<ChannelVault['historyInstall']>) {
    return this.channelVault().historyInstall(...args)
  }
  async channelControls(channel: string) {
    return this.channelVault().controls(channel)
  }
  async channelMessages(channel: string) {
    return this.channelVault().messages(channel)
  }
  private contentEngine() {
    return (this.content ??= new ContentEngine({
      ready: () => this.ready(),
      decode: <T>(record: EncryptedObject) => this.decode<T>(record),
      verifyFile: (record) => this.verifyFile(record, undefined, undefined, false),
      rekey: (record) => this.rekeyPending(record),
      tick: () => this.tick()
    }))
  }
  async contentPrepare(key: string) {
    return this.contentEngine().prepare(key)
  }
  async contentNeedsRekey(key: string, generation: number) {
    return this.contentEngine().needsRekey(key, generation)
  }
  private async rekeyPending(record: EncryptedObject) {
    const { db, meta, bundle, lease } = await this.ready()
    const payload = await this.decode<any>(record)
    const parent = record.parent
      ? await db.db.get('meta', `content-replacement:${record.id}:${record.parent}`)
      : null
    if ((record.kind === 'vault' || record.kind === 'migration_part') && payload.file) {
      const old: FileManifest = payload.file
      await this.verifyFile(record, undefined, undefined, false)
      const next = {
        ...old,
        version: id(),
        key: b64(random()),
        chunks: [] as FileManifest['chunks']
      }
      for (const [index, ref] of old.chunks.entries()) {
        await this.tick()
        const stored = await db.db.get('chunks', ref.id)
        const plaintext = await open(unb64(old.key), stored.ciphertext, [
          'dmsg/file-chunk/1',
          old.id,
          old.version,
          index,
          old.chunks.length,
          ref.size
        ])
        try {
          const iv = new Uint8Array(12)
          iv.set([0x64, 0x4d, 0x73, 0x67])
          new DataView(iv.buffer).setBigUint64(4, BigInt(index))
          const ciphertext = await seal(
            unb64(next.key),
            plaintext,
            ['dmsg/file-chunk/1', next.id, next.version, index, old.chunks.length, ref.size],
            iv
          )
          const chunk = {
            id: `${next.id}:${next.version}:${index}`,
            ciphertext,
            digest: hash(unb64(ciphertext))
          }
          await db.cacheChunks([chunk], lease)
          next.chunks.push({ id: chunk.id, digest: chunk.digest, size: ref.size })
        } finally {
          plaintext.fill(0)
        }
      }
      payload.file = next
    }
    if (record.kind === 'vault')
      payload.resolvedConflicts = [
        ...new Set([...(payload.resolvedConflicts ?? []), record.key])
      ]
    const next = {
      ...record,
      revision: id(),
      parent: parent ? String(parent.key).split(':')[1] : record.parent,
      generation: meta.rootGeneration,
      deviceId: meta.deviceId,
      conflict: false,
      createdAt: Date.now()
    }
    next.key = `${next.id}:${next.revision}`
    const key = random()
    try {
      next.ciphertext = await seal(key, canonical(payload), this.aad(next))
      next.keyEnvelope = await seal(unb64(bundle.root), key, this.wrapContext(next))
      next.digest = hash(canonical([this.aad(next), next.ciphertext, next.keyEnvelope]))
    } finally {
      key.fill(0)
    }
    return db.guarded(['objects', 'outbox'], lease, async (tx) => {
      const previous = await tx.objectStore('outbox').get(record.revision)
      ensure(previous && previous.state !== 'stored', 'VERSION_CONFLICT')
      await putObject(tx, { ...record, conflict: true })
      await tx
        .objectStore('outbox')
        .put({ ...previous, state: 'blocked', error: 'REPLACED_AFTER_REKEY' })
      await putObject(tx, next)
      await tx.objectStore('outbox').put({
        id: next.revision,
        objectKey: next.key,
        frame: b64(canonical(next)),
        digest: next.digest,
        state: 'local'
      })
      await tx
        .objectStore('meta')
        .put({ id: `content-replacement:${record.key}`, key: next.key })
      const head = await tx.objectStore('meta').get(`head:${record.id}`)
      if (head?.revision === record.revision)
        await tx.objectStore('meta').put(objectHead(next, objectChannel(next, payload)))
      return next.key
    })
  }
  async contentReplan(key: string, generation: number, uploadIds: string[]) {
    return this.contentEngine().replan(key, generation, uploadIds)
  }
  async contentPending() {
    const { db } = await this.ready()
    return (
      await Promise.all(
        ['local', 'queued', 'sending', 'unknown'].map((state) =>
          db.db.getAllFromIndex('outbox', 'state', state)
        )
      )
    ).flat() as import('../models').OutboxJob[]
  }
  async contentSave(job: ContentJob) {
    return this.contentEngine().save(job)
  }
  async contentChunk(upload: ContentUpload, index: number) {
    return this.contentEngine().chunk(upload, index)
  }
  async contentAcknowledge(job: ContentJob) {
    return this.contentEngine().acknowledge(job)
  }
  async contentMissing(plan: StoredUploadPlan) {
    return this.contentEngine().missing(plan)
  }
  async contentCache(plan: StoredUploadPlan, index: number | 'manifest', bytes: Uint8Array) {
    return this.contentEngine().cache(plan, index, bytes)
  }
  async contentReceive(input: {
    objects: StoredUploadPlan[]
    revisions: CloudRevision[]
    through: number
    evidence: string
  }) {
    return this.contentEngine().receive(input)
  }
  async inboxKey(version: number) {
    const { db, meta } = await this.ready()
    ensure(meta.account && version > 0 && Number.isSafeInteger(version), 'AUTH_REQUIRED')
    const objectId = hash(canonical(['dmsg/inbox-key/1', meta.subjectId, version]))
    let record = await db.getHead(objectId, 'inbox')
    if (!record)
      record = await this.write(
        'inbox',
        {
          format: 'dmsg-inbox-key/1',
          version,
          rootGeneration: meta.rootGeneration,
          seed: b64(random())
        },
        objectId
      )
    const value = await this.decode<{ seed: string; rootGeneration: number }>(record)
    ensure(value.rootGeneration === meta.rootGeneration, 'REKEY_REQUIRED')
    return {
      version,
      publicKey: hex(unb64(await hpkePublic(unb64(value.seed)))),
      rootGeneration: value.rootGeneration
    }
  }
  async inboxSeal(
    recipient: string,
    version: number,
    publicKey: string,
    order: string,
    text: string
  ) {
    const { meta } = await this.ready()
    ensure(meta.account && text.length > 0 && utf8(text).length <= 6000, 'INVALID_INPUT')
    return b64(
      canonical(
        await hpkeSeal(b64(unhex(publicKey)), canonical({ text }), [
          'dmsg/inbox-envelope/1',
          meta.account.id,
          recipient,
          order,
          version
        ])
      )
    )
  }
  async inboxOpen(input: {
    order_id: string
    sender: string
    recipient: string
    inbox_key_version: number
    ciphertext: string
  }) {
    const { db, meta } = await this.ready()
    ensure(input.recipient === meta.account?.id, 'FORBIDDEN')
    const objectId = hash(
        canonical(['dmsg/inbox-key/1', meta.subjectId, input.inbox_key_version])
      ),
      record = await db.getHead(objectId, 'inbox')
    ensure(record, 'RECOVERY_INCOMPLETE')
    const key = await this.decode<{ seed: string }>(record)
    const plaintext = await hpkeOpen(
      unb64(key.seed),
      decodeCanonical(unb64(input.ciphertext)),
      [
        'dmsg/inbox-envelope/1',
        input.sender,
        input.recipient,
        input.order_id,
        input.inbox_key_version
      ]
    )
    try {
      const value = decodeCanonical<{ text: string }>(plaintext)
      ensure(typeof value.text === 'string', 'INTEGRITY_FAILED')
      return value.text
    } finally {
      plaintext.fill(0)
    }
  }
  async commerceJournal(key: string, value?: string) {
    ensure(
      key.length <= 160 && (value === undefined || utf8(value).length <= 180000),
      'INVALID_INPUT'
    )
    const { db, meta } = await this.ready(),
      objectId = hash(canonical(['dmsg/commerce-journal/1', meta.subjectId, key]))
    const record = await db.getHead(objectId, 'commerce'),
      saved = record ? (await this.decode<{ key: string; value: string }>(record)).value : null
    if (value === undefined) return saved
    // Every revision is synchronized; an unchanged journal is not a new version.
    if (saved !== value)
      await this.write('commerce', { key, value }, objectId, record?.revision ?? null)
    return value
  }
  async commerceJournals() {
    const { db } = await this.ready(),
      result: { key: string; value: string }[] = []
    for (const record of await db.heads('commerce')) result.push(await this.decode(record))
    return result
  }
  async formalAuthorize(input: {
    requestId: string
    digest: string
    executionId: string
    journal: string
  }) {
    const { db, localKey, lease } = await this.ready()
    ensure(
      /^[0-9a-f]{64}$/.test(input.requestId) &&
        /^[0-9a-f]{64}$/.test(input.executionId) &&
        utf8(input.journal).length <= 180000,
      'INVALID_INPUT'
    )
    const ciphertext = await seal(localKey, utf8(input.journal), [
      'dmsg/control-journal/1',
      db.name,
      `formal:${input.requestId}`
    ])
    await db.authorizeFormal(
      input.requestId,
      input.digest,
      input.executionId,
      ciphertext,
      lease
    )
  }
  async formalHistory(input: { requestId: string; value: string }) {
    ensure(
      /^[0-9a-f]{64}$/.test(input.requestId) && utf8(input.value).length <= 180000,
      'INVALID_INPUT'
    )
    const { db, meta } = await this.ready(),
      objectId = hash(canonical(['dmsg/formal-history/1', meta.subjectId, input.requestId]))
    const previous = await db.getHead(objectId)
    if (previous) {
      const stored = await this.decode<{ format: string; value: string }>(previous)
      ensure(
        stored.format === 'dmsg-formal-history/1' && stored.value === input.value,
        'IDEMPOTENCY_CONFLICT'
      )
      return previous.key
    }
    return (
      await this.write(
        'request',
        { format: 'dmsg-formal-history/1', value: input.value },
        objectId
      )
    ).key
  }
  async formalHistories() {
    const { db } = await this.ready()
    const result: { key: string; value: string }[] = []
    for (const record of await db.heads('request')) {
      const stored = await this.decode<{ format?: string; value?: string }>(record)
      if (stored.format === 'dmsg-formal-history/1' && typeof stored.value === 'string')
        result.push({ key: record.key, value: stored.value })
      await this.tick()
    }
    return result
  }
  private async ready() {
    ensure(
      this.db && this.lease && this.bundle && this.localKey && this.meta,
      'LOCKED',
      '请先解锁工作台。'
    )
    await this.db.check(this.lease)
    return {
      db: this.db,
      lease: this.lease,
      bundle: this.bundle,
      localKey: this.localKey,
      meta: this.meta
    }
  }
  /** A workspace is bound to an account before it holds any content: the
   * provisional key only protects fresh device keys until the user home's
   * login-gated secret replaces it. */
  async initialize() {
    ensure(
      (await currentWorkspace()) === null,
      'VERSION_CONFLICT',
      '此浏览器已有工作台，请解锁或清除。'
    )
    const subjectId = id(),
      deviceId = id()
    const bundle: Bundle = { root: b64(random()), signing: b64(random()), hpke: b64(random()) }
    const meta: WorkspaceMeta = {
      subjectId,
      deviceId,
      environment: config.environment,
      createdAt: Date.now(),
      signingPublic: b64(ed25519.getPublicKey(unb64(bundle.signing))),
      hpkePublic: await hpkePublic(unb64(bundle.hpke)),
      rootGeneration: 1,
      registered: false,
      unlock: 'provisional'
    }
    await this.install(meta, bundle)
    return meta
  }
  private async install(meta: WorkspaceMeta, bundle: Bundle) {
    const name = `dmsg:${meta.environment}:${meta.subjectId}:${meta.deviceId}`
    const db = await WorkspaceDB.open(name),
      lease = await db.acquire(this.owner, Date.now(), this.documentId)
    const localKey = random(),
      provisional = random()
    let registered = false
    try {
      await db.renew(lease)
      const envelope: LocalEnvelope = {
        id: 'local',
        provisional: b64(provisional),
        wrappedKey: await seal(provisional, localKey, ['dmsg/local-key/2', name]),
        privateBundle: await seal(localKey, canonical(bundle), ['dmsg/device-bundle/1', name])
      }
      await db.guarded(['key_envelopes'], lease, async (tx) => {
        ensure(!(await tx.objectStore('meta').get('workspace')), 'VERSION_CONFLICT')
        await tx.objectStore('key_envelopes').put(envelope)
        await tx.objectStore('meta').put({ id: 'workspace', value: meta })
      })
      await registerWorkspace(name)
      registered = true
      this.db = db
      this.lease = lease
      this.bundle = bundle
      this.localKey = localKey
      this.meta = meta
    } catch (error) {
      await db.release(lease)
      db.db.close()
      if (!registered) await removeWorkspaceDatabase(name)
      localKey.fill(0)
      throw error
    } finally {
      provisional.fill(0)
    }
  }
  /** Public key of this worker's Internet Identity session key; usable before unlock. */
  async authPublicKey() {
    return b64(ed25519.getPublicKey(this.authSeed))
  }
  /** Unlock with the user home's `unlock_secret`, or with nothing while the
   * workspace is still provisional. */
  async unlock(secret: Uint8Array | null = null) {
    return this.open(async (envelope, name) => {
      if (envelope.provisional) {
        ensure(!secret, 'INVALID_INPUT')
        return {
          key: unb64(envelope.provisional),
          wrapped: envelope.wrappedKey,
          aad: ['dmsg/local-key/2', name],
          login: false
        }
      }
      ensure(secret, 'AUTH_REQUIRED', '请先登录，取得本机解锁秘密。')
      return {
        key: unlockKey(secret, name),
        wrapped: envelope.wrappedKey,
        aad: ['dmsg/local-key/2', name],
        login: true
      }
    })
  }
  /** Unlock with a platform authenticator's PRF output. Refused once the last
   * login unlock is older than a week, so bindings and revocations take effect. */
  async unlockWithPrf(output: Uint8Array) {
    return this.open(async (envelope, name, meta) => {
      ensure(envelope.prfWrappedKey && meta.prf, 'NOT_FOUND', '此设备未启用生物识别解锁。')
      ensure(
        meta.loginUnlockedAt !== undefined &&
          Date.now() - meta.loginUnlockedAt < LOGIN_UNLOCK_INTERVAL_MS,
        'AUTH_REQUIRED',
        '距上次登录解锁已超过 7 天，请登录解锁一次。'
      )
      return {
        key: prfKey(output, name),
        wrapped: envelope.prfWrappedKey,
        aad: ['dmsg/local-key-prf/1', name],
        login: false
      }
    })
  }
  private async open(
    select: (
      envelope: LocalEnvelope,
      name: string,
      meta: WorkspaceMeta
    ) => Promise<{ key: Uint8Array; wrapped: string; aad: unknown; login: boolean }>
  ) {
    const name = await currentWorkspace()
    ensure(name, 'RECOVERY_INCOMPLETE', '此浏览器还没有工作台。')
    const db = await WorkspaceDB.open(name),
      lease = await db.acquire(this.owner, Date.now(), this.documentId)
    let luk: Uint8Array | null = null
    try {
      const envelope = await db.envelope(),
        meta = await db.meta()
      ensure(meta, 'RECOVERY_INCOMPLETE')
      const selected = await select(envelope, name, meta)
      luk = selected.key
      await db.renew(lease)
      let localKey: Uint8Array
      try {
        localKey = await open(luk, selected.wrapped, selected.aad)
      } catch {
        throw new Error('解锁材料不匹配，或本机密钥封装已损坏。')
      }
      const bundle = decodeCanonical<Bundle>(
        await open(localKey, envelope.privateBundle, ['dmsg/device-bundle/1', name])
      )
      ensure(
        b64(ed25519.getPublicKey(unb64(bundle.signing))) === meta.signingPublic &&
          (await hpkePublic(unb64(bundle.hpke))) === meta.hpkePublic,
        'INTEGRITY_FAILED'
      )
      this.db = db
      this.lease = lease
      this.localKey = localKey
      this.bundle = bundle
      this.meta = meta
      if (selected.login) {
        const updated = { ...meta, loginUnlockedAt: Date.now() }
        await db.guardedPut('meta', { id: 'workspace', value: updated }, lease)
        this.meta = updated
      }
      return this.meta
    } catch (error) {
      this.localKey?.fill(0)
      this.localKey = null
      this.bundle = null
      this.meta = null
      this.db = null
      this.lease = null
      await db.release(lease)
      db.db.close()
      throw error
    } finally {
      luk?.fill(0)
    }
  }
  /** Replace the provisional key with the login-gated unlock secret once the
   * account exists and this device is registered. `loginOrigin` is the
   * Internet Identity derivation origin whose Principal owns the secret. */
  async bindUnlockSecret(secret: Uint8Array, loginOrigin: string) {
    ensure(config.derivationOrigins.includes(loginOrigin), 'INVALID_INPUT')
    const { db, lease, localKey, meta } = await this.ready()
    const luk = unlockKey(secret, db.name)
    try {
      const envelope = await db.envelope()
      const next: LocalEnvelope = {
        id: 'local',
        wrappedKey: await seal(luk, localKey, ['dmsg/local-key/2', db.name]),
        privateBundle: envelope.privateBundle
      }
      const updated: WorkspaceMeta = {
        ...meta,
        unlock: 'login',
        loginOrigin,
        loginUnlockedAt: Date.now()
      }
      await db.replaceKeys(updated, next, lease)
      this.meta = updated
    } finally {
      luk.fill(0)
    }
  }
  /** Keep a copy of the local data key under the PRF-derived key. */
  async enablePrf(input: { credentialId: string; output: Uint8Array }) {
    const { db, lease, localKey, meta } = await this.ready()
    ensure(meta.unlock === 'login', 'AUTH_REQUIRED', '请先完成账户绑定。')
    ensure(unb64(input.credentialId).length > 0, 'INVALID_INPUT')
    const key = prfKey(input.output, db.name)
    try {
      const envelope = await db.envelope()
      const next: LocalEnvelope = {
        ...envelope,
        prfWrappedKey: await seal(key, localKey, ['dmsg/local-key-prf/1', db.name])
      }
      const updated: WorkspaceMeta = {
        ...meta,
        prf: { credentialId: input.credentialId, enabledAt: Date.now() }
      }
      await db.replaceKeys(updated, next, lease)
      this.meta = updated
    } finally {
      key.fill(0)
    }
  }
  async disablePrf() {
    const { db, lease, meta } = await this.ready()
    const { prfWrappedKey: _dropped, ...envelope } = await db.envelope()
    const { prf: _prf, ...updated } = meta
    await db.replaceKeys(updated, envelope, lease)
    this.meta = updated
  }
  async tick() {
    const { db, lease } = await this.ready()
    await db.renew(lease)
  }
  async lock() {
    this.localKey?.fill(0)
    this.localKey = null
    this.bundle = null
    this.meta = null
    this.content = null
    const db = this.db,
      lease = this.lease
    this.db = null
    this.lease = null
    if (db && lease) {
      await db.release(lease)
      db.db.close()
    }
  }
  private aad(
    record: Pick<
      EncryptedObject,
      | 'subjectId'
      | 'id'
      | 'revision'
      | 'parent'
      | 'deviceId'
      | 'generation'
      | 'kind'
      | 'tombstone'
    >
  ) {
    return [
      'dmsg/content/1',
      this.meta!.environment,
      record.subjectId,
      record.generation,
      record.id,
      record.revision,
      record.deviceId,
      record.kind,
      record.parent,
      record.tombstone
    ]
  }
  private wrapContext(
    record:
      EncryptedObject | Pick<EncryptedObject, 'subjectId' | 'id' | 'revision' | 'generation'>
  ) {
    return [
      'dmsg/wrap/1',
      record.subjectId,
      record.id,
      record.generation,
      record.revision,
      'item-version'
    ]
  }
  private async decode<T>(record: EncryptedObject, root?: Uint8Array): Promise<T> {
    ensure(
      hash(canonical([this.aad(record), record.ciphertext, record.keyEnvelope])) ===
        record.digest,
      'INTEGRITY_FAILED',
      '对象摘要不一致。'
    )
    let roots = this.bundle?.roots
    if (root && this.meta?.rootHistory)
      roots = decodeCanonical<Record<string, string>>(
        await open(root, this.meta.rootHistory, [
          'dmsg/root-history/1',
          this.meta.subjectId,
          this.meta.rootGeneration
        ])
      )
    const selectedRoot =
      record.generation === this.meta!.rootGeneration
        ? (root ?? unb64(this.bundle!.root))
        : roots?.[String(record.generation)]
          ? unb64(roots[String(record.generation)])
          : undefined
    ensure(selectedRoot, 'RECOVERY_INCOMPLETE', '缺少历史根封装。')
    const key = await open(selectedRoot, record.keyEnvelope, this.wrapContext(record))
    try {
      return decodeCanonical<T>(
        await open(key, record.ciphertext, this.aad(record)),
        record.kind.startsWith('formal_') ? MAX_FORMAL_OBJECT_BYTES : MAX_OBJECT_BYTES
      )
    } finally {
      key.fill(0)
    }
  }
  private async write(
    kind: ObjectKind,
    payload: unknown,
    objectId = id(),
    base: string | null = null,
    tombstone = false
  ) {
    const { db, lease, meta, bundle } = await this.ready()
    ensure(meta.account, 'RECOVERY_INCOMPLETE', '请先绑定账户并启用内容根。')
    ensure(
      canonical(payload).length <=
        (kind.startsWith('formal_') ? MAX_FORMAL_OBJECT_BYTES : MAX_OBJECT_BYTES),
      'QUOTA_EXCEEDED',
      '条目超过大小限制。'
    )
    const key = random(),
      revision = id()
    const record: EncryptedObject = {
      key: `${objectId}:${revision}`,
      id: objectId,
      revision,
      parent: base,
      subjectId: meta.subjectId,
      deviceId: meta.deviceId,
      generation: meta.rootGeneration,
      kind,
      ciphertext: '',
      keyEnvelope: '',
      digest: '',
      tombstone,
      conflict: false,
      createdAt: Date.now()
    }
    try {
      record.ciphertext = await seal(key, canonical(payload), this.aad(record))
      record.keyEnvelope = await seal(unb64(bundle.root), key, this.wrapContext(record))
      record.digest = hash(
        canonical([this.aad(record), record.ciphertext, record.keyEnvelope])
      )
      return await db.commit(
        record,
        base,
        lease,
        {
          id: revision,
          objectKey: record.key,
          frame: b64(canonical(record)),
          digest: record.digest,
          state: 'local'
        },
        objectChannel(record, payload)
      )
    } finally {
      key.fill(0)
    }
  }
  async saveItem(input: { item: Item; id?: string; base?: string | null }) {
    ensure(input.item.type !== 'file', 'INVALID_INPUT')
    const { resolvedConflicts: _resolutions, ...editable } = input.item
    const item = itemSchema.parse(editable)
    ensure(
      utf8(item.body).length + utf8(item.secret).length <= 32768,
      'QUOTA_EXCEEDED',
      '条目正文应小于 32 KiB。'
    )
    if (input.id && input.base) {
      const { db } = await this.ready()
      const previous = await db.db.get('objects', `${input.id}:${input.base}`)
      ensure(previous?.kind === 'vault', 'NOT_FOUND')
      const old = await this.decode<Item>(previous)
      return this.write(
        'vault',
        {
          ...item,
          ...(old.resolvedConflicts ? { resolvedConflicts: old.resolvedConflicts } : {})
        },
        input.id,
        input.base
      )
    }
    return this.write('vault', item, input.id, input.base)
  }
  async deleteItem(input: { id: string; base: string; restore?: boolean }) {
    const { db } = await this.ready(),
      record = (await db.db.get('objects', `${input.id}:${input.base}`)) as
        EncryptedObject | undefined
    ensure(record?.kind === 'vault', 'NOT_FOUND')
    return this.write('vault', await this.decode(record), input.id, input.base, !input.restore)
  }
  async resolveConflict(input: { key: string; base: string }) {
    const { db } = await this.ready(),
      record = (await db.db.get('objects', input.key)) as EncryptedObject | undefined
    ensure(record?.conflict && record.kind === 'vault', 'NOT_FOUND')
    const head = await db.db.get('objects', `${record.id}:${input.base}`)
    ensure(head?.kind === 'vault', 'NOT_FOUND')
    const item = await this.decode<Item>(record),
      current = await this.decode<Item>(head)
    return this.write(
      'vault',
      {
        ...item,
        resolvedConflicts: [
          ...new Set([
            ...(current.resolvedConflicts ?? []),
            ...(item.resolvedConflicts ?? []),
            record.key
          ])
        ]
      },
      record.id,
      input.base,
      record.tombstone
    )
  }
  async view(): Promise<ViewData> {
    const { db, meta, localKey } = await this.ready(),
      records = [...(await db.heads('vault')), ...(await db.heads('profile'))]
    const data: ViewData = {
      meta,
      entries: [],
      profile: null,
      outbox: await db.outboxSummary(),
      conflicts: [],
      imports: []
    }
    const decodeItem = async (record: EncryptedObject) => {
      const item = await this.decode<Item>(record)
      if (item.file) item.file.key = ''
      return { record, item }
    }
    for (let index = 0; index < records.length; index++) {
      const record = records[index]
      if (record.kind === 'vault') data.entries.push(await decodeItem(record))
      else data.profile = await this.decode<Profile>(record)
      if ((index + 1) % 32 === 0) await this.tick()
    }
    const conflicts = (await db.conflicts()).filter((x) => x.kind === 'vault')
    const resolved = new Set(
      data.entries.flatMap((entry) => entry.item.resolvedConflicts ?? [])
    )
    for (let index = 0; index < conflicts.length; index++) {
      const record = conflicts[index]
      if (!resolved.has(record.key)) data.conflicts.push(await decodeItem(record))
      if ((index + 1) % 32 === 0) await this.tick()
    }
    for (const job of (await db.db.getAll('migration_jobs')).filter(
      (x) => x.kind === 'file-import' && x.stage !== 'complete'
    )) {
      const sealed = await db.db.get('local_private', `file-job:${job.id}`)
      ensure(sealed, 'RECOVERY_INCOMPLETE', '中断的文件任务缺少密钥封装。')
      const plan = decodeCanonical<FilePlan>(
        await open(localKey, sealed.ciphertext, ['dmsg/file-job/1', db.name, job.id])
      )
      data.imports.push({
        id: job.id,
        name: plan.manifest.name,
        size: plan.manifest.size,
        completed: plan.manifest.chunks.length,
        total: job.total
      })
    }
    await this.ready()
    return data
  }
  async saveProfile(profile: Profile) {
    const clean = z
      .object({
        name: z.string().trim().min(1).max(100),
        bio: z.string().max(500),
        link: z.string().max(2048),
        contact: z.enum(['closed', 'invitation']),
        publicFields: z.array(z.enum(['name', 'bio', 'link'])).max(3),
        avatarFile: z.string().max(160).optional()
      })
      .strict()
      .parse(profile)
    if (clean.link)
      ensure(
        new URL(clean.link).protocol === 'https:',
        'INVALID_INPUT',
        '公开链接须使用 HTTPS。'
      )
    const { db } = await this.ready()
    if (clean.avatarFile) {
      const record = (await db.db.get('objects', clean.avatarFile)) as
        EncryptedObject | undefined
      ensure(record?.kind === 'vault' && !record.tombstone, 'INVALID_INPUT')
      const file = (await this.decode<Item>(record)).file
      ensure(
        file &&
          file.size <= 2 * 1024 * 1024 &&
          ['image/png', 'image/jpeg', 'image/webp'].includes(file.mime),
        'INVALID_INPUT'
      )
    }
    const existing = (await db.heads('profile'))[0]
    return this.write('profile', clean, existing?.id, existing?.revision)
  }
  async importFile(file: File, resumeId?: string) {
    return this.importFileStorage(file, resumeId, 'vault')
  }
  private async importFileStorage(
    file: File,
    resumeId: string | undefined,
    kind: 'vault' | 'migration_part'
  ) {
    const { db, localKey, meta, lease } = await this.ready()
    ensure(meta.account, 'RECOVERY_INCOMPLETE', '请先绑定账户并启用内容根。')
    ensure(file.size <= MAX_FILE, 'QUOTA_EXCEEDED', '单个文件上限为 100 MiB。')
    ensure(file.name.length <= 255, 'INVALID_INPUT', '文件名过长。')
    let plan: FilePlan
    if (resumeId) {
      ensure(/^[0-9a-f]{64}$/.test(resumeId), 'INVALID_INPUT')
      const sealed = await db.db.get('local_private', `file-job:${resumeId}`)
      ensure(sealed, 'NOT_FOUND')
      plan = decodeCanonical<FilePlan>(
        await open(localKey, sealed.ciphertext, ['dmsg/file-job/1', db.name, resumeId])
      )
      ensure(
        plan.manifest.name === file.name && plan.manifest.size === file.size,
        'INTEGRITY_FAILED',
        '请选择原来的文件，文件名和大小需要一致。'
      )
    } else {
      plan = {
        manifest: {
          id: id(),
          version: id(),
          name: file.name,
          mime: file.type || 'application/octet-stream',
          size: file.size,
          sha256: '',
          chunks: [],
          key: b64(random())
        },
        plainDigests: []
      }
    }
    const manifest = plan.manifest,
      fileKey = unb64(manifest.key),
      fileId = manifest.id,
      version = manifest.version,
      count = Math.ceil(file.size / CHUNK_SIZE),
      overall = sha256.create()
    const persistPlan = async (chunk?: Chunk) => {
      const ciphertext = await seal(localKey, canonical(plan), [
        'dmsg/file-job/1',
        db.name,
        version
      ])
      await this.ready()
      await db.checkpointFile(
        version,
        ciphertext,
        {
          id: version,
          kind: 'file-import',
          stage: 'encrypting',
          completed: manifest.chunks.length,
          total: count
        },
        chunk,
        lease
      )
    }
    await persistPlan()
    try {
      for (let i = 0; i < count; i++) {
        if (kind === 'migration_part')
          ensure(!this.legacyPaused, 'LEGACY_PAUSED', '迁移已暂停，可继续同一文件。')
        await this.tick()
        const plaintext = new Uint8Array(
          await file.slice(i * CHUNK_SIZE, (i + 1) * CHUNK_SIZE).arrayBuffer()
        )
        overall.update(plaintext)
        if (i < manifest.chunks.length) {
          ensure(
            hash(plaintext) === plan.plainDigests[i],
            'INTEGRITY_FAILED',
            '原文件已变化，不能复用已有加密版本。请作为新文件导入。'
          )
          const existing = (await db.db.get('chunks', manifest.chunks[i].id)) as
            Chunk | undefined
          ensure(
            existing && hash(unb64(existing.ciphertext)) === manifest.chunks[i].digest,
            'INTEGRITY_FAILED',
            '已保存的文件块损坏，未继续导入。'
          )
          plaintext.fill(0)
          continue
        }
        const iv = new Uint8Array(12)
        iv.set([0x64, 0x4d, 0x73, 0x67])
        new DataView(iv.buffer).setBigUint64(4, BigInt(i))
        const ciphertext = await seal(
          fileKey,
          plaintext,
          ['dmsg/file-chunk/1', fileId, version, i, count, plaintext.length],
          iv
        )
        const chunk: Chunk = {
          id: `${fileId}:${version}:${i}`,
          ciphertext,
          digest: hash(unb64(ciphertext))
        }
        manifest.chunks.push({ id: chunk.id, digest: chunk.digest, size: plaintext.length })
        plan.plainDigests.push(hash(plaintext))
        plaintext.fill(0)
        await persistPlan(chunk)
        this.progress({ stage: '正在分块加密', completed: i + 1, total: count })
      }
      manifest.sha256 = hex(overall.digest())
      // The manifest is encrypted as part of a separately keyed immutable vault
      // version. Its key is independent of the chunk key, including empty files.
      const item: Item = {
        type: 'file',
        title: file.name,
        tags: [],
        body: '',
        username: '',
        secret: '',
        url: '',
        favorite: false,
        createdAt: Date.now(),
        updatedAt: Date.now(),
        file: manifest
      }
      const prior = await db.getHead(fileId)
      if (prior)
        ensure(
          (await this.decode<Item>(prior)).file?.sha256 === manifest.sha256,
          'IDEMPOTENCY_CONFLICT'
        )
      const result = prior ?? (await this.write(kind, item, fileId))
      await db.completeFile(
        version,
        {
          id: version,
          kind: 'file-import',
          stage: 'complete',
          completed: count,
          total: count,
          objectKey: result.key
        },
        lease
      )
      return result
    } finally {
      fileKey.fill(0)
      overall.destroy()
    }
  }
  private async verifyFile(
    record: EncryptedObject,
    root?: Uint8Array,
    chunks?: Map<string, Chunk>,
    collectParts = true
  ): Promise<{ manifest: FileManifest; parts: Uint8Array<ArrayBuffer>[] }> {
    const item = await this.decode<Item>(record, root)
    ensure(item.file, 'INTEGRITY_FAILED')
    return this.verifyFileManifest(item.file, chunks, collectParts)
  }
  private async verifyFileManifest(
    manifest: FileManifest,
    chunks?: Map<string, Chunk>,
    collectParts = true
  ): Promise<{ manifest: FileManifest; parts: Uint8Array<ArrayBuffer>[] }> {
    ensure(
      Number.isSafeInteger(manifest.size) &&
        manifest.size >= 0 &&
        manifest.size <= MAX_FILE &&
        Array.isArray(manifest.chunks) &&
        manifest.chunks.length === Math.ceil(manifest.size / CHUNK_SIZE),
      'INTEGRITY_FAILED'
    )
    const parts: Uint8Array<ArrayBuffer>[] = [],
      overall = sha256.create(),
      key = unb64(manifest.key)
    let decodedSize = 0
    try {
      for (let i = 0; i < manifest.chunks.length; i++) {
        const ref = manifest.chunks[i],
          chunk = chunks
            ? chunks.get(ref.id)
            : ((await this.db!.db.get('chunks', ref.id)) as Chunk | undefined)
        ensure(
          chunk &&
            chunk.id === `${manifest.id}:${manifest.version}:${i}` &&
            chunk.digest === ref.digest &&
            hash(unb64(chunk.ciphertext)) === ref.digest,
          'INTEGRITY_FAILED',
          '文件缺块或校验失败。'
        )
        const plain = await open(key, chunk.ciphertext, [
          'dmsg/file-chunk/1',
          manifest.id,
          manifest.version,
          i,
          manifest.chunks.length,
          ref.size
        ])
        ensure(
          plain.length === ref.size &&
            ref.size === Math.min(CHUNK_SIZE, manifest.size - i * CHUNK_SIZE),
          'INTEGRITY_FAILED'
        )
        overall.update(plain)
        decodedSize += plain.length
        if (collectParts) parts.push(plain)
        else plain.fill(0)
        this.progress({
          stage: '正在验证文件',
          completed: i + 1,
          total: manifest.chunks.length
        })
        if (this.lease) await this.tick()
      }
      ensure(
        hex(overall.digest()) === manifest.sha256 && decodedSize === manifest.size,
        'INTEGRITY_FAILED',
        '完整文件摘要不一致。'
      )
      return { manifest, parts }
    } finally {
      overall.destroy()
      key.fill(0)
    }
  }
  async downloadFile(objectKey: string) {
    return this.downloadStoredFile(objectKey, false)
  }
  private async downloadStoredFile(objectKey: string, legacy: boolean) {
    const { db } = await this.ready(),
      record = (await db.db.get('objects', objectKey)) as EncryptedObject
    ensure(
      record?.kind === 'vault' || (legacy && record?.kind === 'migration_part'),
      'NOT_FOUND'
    )
    const { manifest, parts } = await this.verifyFile(record)
    await this.ready()
    return {
      name: manifest.name,
      blob: new Blob(parts, { type: 'application/octet-stream' }),
      sha256: manifest.sha256
    }
  }
  async readRequest(envelope: { enc: string; ciphertext: string }, requestId: string) {
    const { bundle, meta } = await this.ready()
    const payload = decodeCanonical(
      await hpkeOpen(unb64(bundle.hpke), envelope, [
        'dmsg/external-request/1',
        meta.subjectId,
        meta.deviceId,
        requestId
      ]),
      131072
    )
    await this.ready()
    return payload
  }
  /** Signs with the session key; login must work before the workspace is unlocked. */
  async authSign(message: Uint8Array) {
    ensure(message.length <= 1048576, 'QUOTA_EXCEEDED')
    return ed25519.sign(message, this.authSeed)
  }

  /** Account journals contain public requests/signatures, never private keys.
   * Keep them encrypted and separate from content archives and the SW outbox. */
  async controlGet(key: string): Promise<string | null> {
    ensure(/^[a-z0-9:-]{1,150}$/.test(key), 'INVALID_INPUT')
    const { db, localKey } = await this.ready()
    const row = await db.db.get('local_private', `control:${key}`)
    if (!row) return null
    return new TextDecoder('utf-8', { fatal: true }).decode(
      await open(localKey, row.ciphertext, ['dmsg/control-journal/1', db.name, key])
    )
  }

  async controlPut(key: string, value: string) {
    ensure(/^[a-z0-9:-]{1,150}$/.test(key) && utf8(value).length <= 200000, 'INVALID_INPUT')
    const { db, lease, localKey } = await this.ready()
    const ciphertext = await seal(localKey, utf8(value), [
      'dmsg/control-journal/1',
      db.name,
      key
    ])
    await db.guardedPut('local_private', { id: `control:${key}`, ciphertext }, lease)
  }

  /** Device signatures over approval digests and domain-separated COSE structures. */
  async deviceSign(message: Uint8Array) {
    ensure(
      message instanceof Uint8Array && message.length > 0 && message.length <= 262144,
      'INVALID_INPUT'
    )
    const { bundle } = await this.ready()
    const key = unb64(bundle.signing)
    try {
      return ed25519.sign(message, key)
    } finally {
      key.fill(0)
    }
  }

  private async candidate(context: RootContext, value?: RootMaterial): Promise<RootMaterial> {
    const { db, lease, localKey } = await this.ready()
    const key = `account-root:${context.account}:${context.opId}`
    const aad = ['dmsg/root-candidate/2', db.name, key]
    if (value) {
      await db.guardedPut(
        'local_private',
        { id: key, ciphertext: await seal(localKey, canonical(value), aad) },
        lease
      )
      return value
    }
    const row = await db.db.get('local_private', key)
    const saved = row
      ? decodeCanonical<RootMaterial>(await open(localKey, row.ciphertext, aad))
      : rootMaterial(context)
    ensure(equal(canonical(saved.context), canonical(context)), 'IDEMPOTENCY_CONFLICT')
    if (!row) await this.candidate(context, saved)
    return saved
  }

  /** The transport public key a recovery derivation of this generation is encrypted to. */
  async prepareAccountRoot(context: RootContext) {
    return rootTransport(await this.candidate(context))
  }

  async wrapAccountRoot(
    context: RootContext,
    recipients: { deviceId: string; hpkePublic: string }[],
    recoveryKey: RecoveryKey
  ) {
    const material = await this.candidate(context)
    const { bundle, meta } = await this.ready()
    const previous =
      meta.account?.id === context.account &&
      meta.account.rootBytesDigest &&
      meta.account.rootUploadId
        ? {
            digest: meta.account.rootBytesDigest,
            uploadId: meta.account.rootUploadId,
            generation: meta.rootGeneration,
            root: unb64(bundle.root)
          }
        : null
    ensure(!meta.account || previous, 'RECOVERY_INCOMPLETE')
    const signing = unb64(bundle.signing)
    try {
      const data = await wrapRoot(
        material,
        recipients,
        recoveryKey,
        { deviceId: meta.deviceId, seed: signing },
        previous
      )
      material.bytes = b64(data)
      await this.candidate(context, material)
      return data
    } finally {
      signing.fill(0)
      previous?.root.fill(0)
    }
  }

  /** Open the current bundle with this device's envelope. */
  async openAccountRoot(context: RootContext, bundles: Uint8Array[], bundleDigest: string) {
    const material = await this.candidate(context),
      { bundle, meta } = await this.ready()
    const seed = unb64(bundle.hpke)
    try {
      await this.candidate(
        context,
        await openRoot(material, { deviceId: meta.deviceId, hpkeSeed: seed }, bundles, bundleDigest)
      )
    } finally {
      seed.fill(0)
    }
  }

  /** Open the current bundle's recovery envelope with the derived vetKD key. */
  async recoverAccountRoot(
    context: RootContext,
    key: RecoveryKey,
    encryptedKey: Uint8Array,
    bundles: Uint8Array[],
    bundleDigest: string
  ) {
    const material = await this.candidate(context)
    await this.candidate(
      context,
      await recoverRoot(material, key, encryptedKey, bundles, bundleDigest)
    )
  }

  /** Make the opened or wrapped candidate this workspace's content root. A
   * first binding renames the subject to the account; no content exists yet. */
  async activateAccountRoot(input: {
    context: RootContext
    digest: string
    uploadId: string
    homeUser: string
    issuer: string
  }) {
    const source = await this.ready(),
      material = await this.candidate(input.context)
    ensure(material.bytes, 'RECOVERY_INCOMPLETE')
    const meta: WorkspaceMeta = {
      ...source.meta,
      subjectId: input.context.account,
      account: {
        id: input.context.account,
        issuer: input.issuer,
        homeUser: input.homeUser,
        rootDigest: input.digest,
        rootBytesDigest: hash(unb64(material.bytes)),
        rootUploadId: input.uploadId
      },
      rootGeneration: input.context.generation,
      registered: true
    }
    const next: Bundle = { ...source.bundle, root: material.root, roots: { ...material.roots } }
    if (source.meta.account) {
      ensure(
        source.meta.account.id === input.context.account &&
          source.meta.rootGeneration <= meta.rootGeneration,
        'VERSION_CONFLICT',
        '不能把已绑定的工作区转换到另一个账户。'
      )
      if (source.meta.rootGeneration === meta.rootGeneration)
        ensure(
          source.meta.account.rootDigest === input.digest &&
            equal(unb64(source.bundle.root), unb64(material.root)),
          'INTEGRITY_FAILED',
          '已恢复的根与链上当前根不一致。'
        )
      next.roots = {
        ...next.roots,
        ...source.bundle.roots,
        ...(source.meta.rootGeneration < meta.rootGeneration
          ? { [source.meta.rootGeneration]: source.bundle.root }
          : {})
      }
    } else
      ensure(
        (await source.db.db.count('objects')) === 0,
        'VERSION_CONFLICT',
        '未绑定的工作台不应含有内容。'
      )
    meta.rootHistory = Object.keys(next.roots!).length
      ? await seal(unb64(next.root), canonical(next.roots), [
          'dmsg/root-history/1',
          meta.subjectId,
          meta.rootGeneration
        ])
      : undefined
    if (!meta.rootHistory) delete meta.rootHistory
    const envelope = await source.db.envelope()
    envelope.privateBundle = await seal(source.localKey, canonical(next), [
      'dmsg/device-bundle/1',
      source.db.name
    ])
    await source.db.replaceKeys(meta, envelope, source.lease)
    this.bundle = next
    this.meta = meta
    return meta
  }

  /** Self-held Agent Delegation controller keys live in the vault, so every
   * device holding the root can sign with them. */
  async controllerKey(
    account: string,
    generation: number,
    action: 'create' | 'public' | 'sign',
    message?: Uint8Array
  ) {
    const { db, meta } = await this.ready()
    ensure(
      meta.account?.id === account && Number.isInteger(generation) && generation > 0,
      'AUTH_REQUIRED'
    )
    const objectId = hash(canonical(['dmsg/agent-controller/1', account, generation]))
    let record = await db.getHead(objectId, 'vault')
    if (!record) {
      ensure(action === 'create', 'NOT_FOUND', '此代 controller key 不在本机 vault 中。')
      record = await this.write(
        'vault',
        {
          type: 'key',
          title: `dMsg controller #${generation}`,
          tags: ['dmsg-controller'],
          body: `Agent Delegation controller key of ${account}, generation ${generation}.`,
          username: '',
          secret: b64(random()),
          url: '',
          favorite: false,
          createdAt: Date.now(),
          updatedAt: Date.now()
        } satisfies Item,
        objectId
      )
    }
    const item = await this.decode<Item>(record)
    ensure(!record.tombstone && item.type === 'key', 'NOT_FOUND')
    const seed = unb64(item.secret)
    try {
      ensure(seed.length === 32, 'INTEGRITY_FAILED')
      const publicKey = ed25519.getPublicKey(seed)
      if (action === 'sign') {
        ensure(message instanceof Uint8Array && message.length === 32, 'INVALID_INPUT')
        return { publicKey, signature: ed25519.sign(message, seed) }
      }
      return { publicKey, signature: new Uint8Array() }
    } finally {
      seed.fill(0)
    }
  }
}
