import { ChannelVault } from './channel'
import { sha256 } from '@noble/hashes/sha2.js'
import { LegacyVault } from './legacy'
import { ContentEngine, type DownloadedContent, type CloudRevision } from './content'
import type { ContentJob, ContentUpload } from '../protocol/content'
import {
  openContentManifest,
  storedUploadPlanSchema,
  uploadPlanSchema
} from '../protocol/content'
import { hmac } from '@noble/hashes/hmac.js'
import { z } from 'zod'
import { config, CHUNK_SIZE, MAX_FILE } from '../config'
import {
  currentWorkspace,
  registerWorkspace,
  removeWorkspaceDatabase,
  WorkspaceDB
} from '../db'
import { ensure } from '../errors'
import { xidBytes } from '../protocol/identity'
import {
  rootMaterial,
  rootTransport,
  wrapRoot,
  openRoot,
  type RootContext,
  type RootKey,
  type RootMaterial
} from './root'
import type {
  Channel,
  Chunk,
  EncryptedObject,
  FileManifest,
  Item,
  Lease,
  LocalEnvelope,
  Message,
  ObjectKind,
  Profile,
  RecoveryArchive,
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
  defaultKdf,
  derive,
  ed25519,
  hpkeOpen,
  hpkePublic,
  hpkeSeal,
  open,
  passwordKey,
  recoveryContext,
  recoverySeeds,
  seal
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
  transport: string
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
    const old = (await db.heads()).find((r) => r.id === manifest.id && r.kind === kind)
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
      buckets: ['532er-faaaa-aaaaj-qncpa-cai', 'sb6zj-3aaaa-aaaaj-qndla-cai'],
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
  async legacyOpenGrant(
    grant: import('../protocol/shared-history').LegacyHistoryGrant,
    recoveryCode?: string
  ) {
    if (!recoveryCode) return this.channelVault().openLegacyGrant(grant)
    const { meta } = await this.ready(),
      code = unhex(recoveryCode.trim().toLowerCase().replaceAll(/[-\s]/g, ''))
    const seeds = recoverySeeds(
      code,
      meta.environment,
      grant.scope.account,
      grant.scope.recovery_generation
    )
    try {
      return await this.channelVault().openLegacyGrant(grant, seeds.hpke)
    } finally {
      code.fill(0)
      seeds.hpke.fill(0)
      seeds.signing.fill(0)
    }
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
    return new ContentEngine({
      ready: () => this.ready(),
      decode: <T>(record: EncryptedObject) => this.decode<T>(record),
      verifyFile: (record, chunks) => this.verifyFile(record, undefined, chunks, false),
      tick: () => this.tick()
    })
  }
  async contentPrepare(key: string) {
    return this.contentEngine().prepare(key)
  }
  async contentReplan(key: string, generation: number, uploadIds: string[]) {
    return this.contentEngine().replan(key, generation, uploadIds)
  }
  async contentJobs() {
    return this.contentEngine().jobs()
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
  async contentReceive(input: {
    objects: DownloadedContent[]
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
    let record = (await db.heads()).find((r) => r.kind === 'inbox' && r.id === objectId)
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
      record = (await db.heads()).find((r) => r.id === objectId && r.kind === 'inbox')
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
    const record = (await db.heads()).find((r) => r.id === objectId && r.kind === 'commerce')
    if (value === undefined)
      return record ? (await this.decode<{ key: string; value: string }>(record)).value : null
    await this.write('commerce', { key, value }, objectId, record?.revision ?? null)
    return value
  }
  async commerceJournals() {
    const { db } = await this.ready(),
      result: { key: string; value: string }[] = []
    for (const record of (await db.heads()).filter((r) => r.kind === 'commerce'))
      result.push(await this.decode(record))
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
    const previous = (await db.heads()).find((r) => r.id === objectId)
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
  async initialize(password: string) {
    ensure(
      (await currentWorkspace()) === null,
      'VERSION_CONFLICT',
      '此浏览器已有工作台，请解锁或恢复。'
    )
    ensure(
      password.length >= 12 && utf8(password).length <= 1024,
      'INVALID_INPUT',
      '请使用至少 12 个字符的独立口令。'
    )
    const subjectId = id(),
      deviceId = id(),
      code = random()
    const recovery = recoverySeeds(code, config.environment, subjectId, 1)
    const bundle: Bundle = {
      root: b64(random()),
      signing: b64(random()),
      hpke: b64(random()),
      transport: b64(random())
    }
    const meta: WorkspaceMeta = {
      subjectId,
      deviceId,
      environment: config.environment,
      createdAt: Date.now(),
      signingPublic: b64(ed25519.getPublicKey(unb64(bundle.signing))),
      hpkePublic: await hpkePublic(unb64(bundle.hpke)),
      transportPublic: b64(ed25519.getPublicKey(unb64(bundle.transport))),
      recoveryPublic: await hpkePublic(recovery.hpke),
      recoverySigningPublic: b64(ed25519.getPublicKey(recovery.signing)),
      recoveryGeneration: 1,
      rootGeneration: 1,
      recoveryChecked: false,
      lastBackupAt: null,
      lastBackupCount: 0,
      registered: false,
      recoveryEnvelope: { enc: '', ciphertext: '' }
    }
    meta.recoveryEnvelope = await hpkeSeal(
      meta.recoveryPublic,
      unb64(bundle.root),
      recoveryContext(meta.environment, subjectId, 1)
    )
    try {
      await this.install(password, meta, bundle, undefined, code)
      return { meta, recoveryCode: hex(code).match(/.{8}/g)!.join('-') }
    } finally {
      code.fill(0)
      recovery.hpke.fill(0)
      recovery.signing.fill(0)
    }
  }
  private async install(
    password: string,
    meta: WorkspaceMeta,
    bundle: Bundle,
    archive?: RecoveryArchive,
    pendingRecoveryCode?: Uint8Array,
    expectedActive?: string
  ) {
    const name = `dmsg:${meta.environment}:${meta.subjectId}:${meta.deviceId}`
    const db = await WorkspaceDB.open(name),
      lease = await db.acquire(this.owner, Date.now(), this.documentId)
    const localKey = random(),
      kdf = defaultKdf()
    let registered = false
    this.progress({ stage: '正在保护本机密钥', completed: 0, total: 1 })
    let luk: Uint8Array | null = null
    try {
      luk = await passwordKey(password, kdf)
      await db.renew(lease)
      const envelope: LocalEnvelope = {
        id: 'local',
        kdf,
        wrappedKey: await seal(luk, localKey, ['dmsg/local-key/1', name]),
        privateBundle: await seal(localKey, canonical(bundle), ['dmsg/device-bundle/1', name])
      }
      const pendingRecovery = pendingRecoveryCode
        ? await seal(localKey, pendingRecoveryCode, ['dmsg/pending-recovery/1', name])
        : null
      const tx = db.db.transaction(
        ['meta', 'key_envelopes', 'objects', 'chunks', 'outbox', 'local_private'],
        'readwrite'
      )
      const current = (await tx.objectStore('meta').get('crypto-owner')) as Lease | undefined
      ensure(
        current &&
          current.owner === lease.owner &&
          current.fence === lease.fence &&
          current.expiresAt > Date.now(),
        'LOCKED'
      )
      ensure(!(await tx.objectStore('meta').get('workspace')), 'VERSION_CONFLICT')
      await tx.objectStore('key_envelopes').put(envelope)
      await tx.objectStore('meta').put({ id: 'workspace', value: meta })
      if (pendingRecovery)
        await tx
          .objectStore('local_private')
          .put({ id: 'pending-recovery', ciphertext: pendingRecovery })
      if (archive) {
        if (archive.meta.cloudSnapshot) {
          await tx.objectStore('meta').put({
            id: 'cloud-snapshot',
            through: archive.meta.cloudSnapshot.through,
            evidence: archive.meta.cloudSnapshot.evidence
          })
          for (const [id, revision] of archive.meta.cloudSnapshot.heads)
            await tx.objectStore('meta').put({ id: `cloud-head:${id}`, revision })
        }
        // Restore immutable history first, deriving heads from authenticated
        // parent links. Never choose a head from attacker-controlled timestamps.
        for (const record of archive.objects) await tx.objectStore('objects').put(record)
        const parents = new Set(
          archive.objects
            .filter((x) => !x.conflict && x.parent)
            .map((x) => `${x.id}:${x.parent}`)
        )
        for (const record of archive.objects.filter(
          (x) => !x.conflict && !parents.has(x.key)
        )) {
          ensure(
            !(await tx.objectStore('meta').get(`head:${record.id}`)),
            'INTEGRITY_FAILED',
            '备份包含未解决的历史分叉。'
          )
          await tx
            .objectStore('meta')
            .put({ id: `head:${record.id}`, revision: record.revision })
          await tx.objectStore('outbox').put({
            id: record.revision,
            objectKey: record.key,
            frame: b64(canonical(record)),
            digest: record.digest,
            state: 'local',
            attempt: 0,
            nextAttempt: 0
          })
        }
        for (const chunk of archive.chunks) await tx.objectStore('chunks').put(chunk)
      }
      await tx.done
      await registerWorkspace(name, expectedActive)
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
      luk?.fill(0)
    }
  }
  async unlock(password: string) {
    const name = await currentWorkspace()
    ensure(name, 'RECOVERY_INCOMPLETE', '此浏览器还没有工作台。')
    const db = await WorkspaceDB.open(name),
      lease = await db.acquire(this.owner, Date.now(), this.documentId)
    let luk: Uint8Array | null = null
    try {
      const envelope = await db.envelope(),
        meta = await db.meta()
      ensure(meta, 'RECOVERY_INCOMPLETE')
      luk = await passwordKey(password, envelope.kdf)
      await db.renew(lease)
      let localKey: Uint8Array
      try {
        localKey = await open(luk, envelope.wrappedKey, ['dmsg/local-key/1', name])
      } catch {
        throw new Error('口令不匹配，或本机密钥封装已损坏。请重试或使用恢复包。')
      }
      const bundle = decodeCanonical<Bundle>(
        await open(localKey, envelope.privateBundle, ['dmsg/device-bundle/1', name])
      )
      ensure(
        b64(ed25519.getPublicKey(unb64(bundle.signing))) === meta.signingPublic &&
          b64(ed25519.getPublicKey(unb64(bundle.transport))) === meta.transportPublic &&
          (await hpkePublic(unb64(bundle.hpke))) === meta.hpkePublic,
        'INTEGRITY_FAILED'
      )
      this.db = db
      this.lease = lease
      this.localKey = localKey
      this.bundle = bundle
      this.meta = meta
      await this.restoreAuxiliary()
      return meta
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
  async tick() {
    const { db, lease } = await this.ready()
    await db.renew(lease)
  }
  async lock() {
    this.localKey?.fill(0)
    this.localKey = null
    this.bundle = null
    this.meta = null
    const db = this.db,
      lease = this.lease
    this.db = null
    this.lease = null
    if (db && lease) {
      await db.release(lease)
      db.db.close()
    }
  }
  async verifyRecovery(code: string) {
    const { meta, bundle, db, lease } = await this.ready()
    const seeds = recoverySeeds(
      unhex(code.trim().toLowerCase().replaceAll(/[-\s]/g, '')),
      meta.environment,
      meta.subjectId,
      meta.recoveryGeneration
    )
    try {
      const root = await hpkeOpen(
        seeds.hpke,
        meta.recoveryEnvelope,
        recoveryContext(meta.environment, meta.subjectId, meta.rootGeneration)
      )
      ensure(equal(root, unb64(bundle.root)), 'INTEGRITY_FAILED', '恢复码不匹配。')
      root.fill(0)
      await this.ready()
      const updated = { ...meta, recoveryChecked: true }
      await db.completeRecovery(updated, lease)
      this.meta = updated
      return true
    } finally {
      seeds.signing.fill(0)
      seeds.hpke.fill(0)
    }
  }
  async pendingRecovery() {
    const { meta, db, localKey } = await this.ready()
    ensure(!meta.recoveryChecked, 'NOT_FOUND')
    const record = await db.db.get('local_private', 'pending-recovery')
    ensure(
      record?.ciphertext,
      'RECOVERY_INCOMPLETE',
      '初始化恢复材料不完整，请清除此扩展的本地数据后重新建立工作台。'
    )
    const code = await open(localKey, record.ciphertext, ['dmsg/pending-recovery/1', db.name])
    try {
      ensure(code.length === 32, 'INTEGRITY_FAILED')
      await this.ready()
      return {
        recoveryCode: hex(code).match(/.{8}/g)!.join('-'),
        backupGenerated: meta.lastBackupAt !== null
      }
    } finally {
      code.fill(0)
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
      return decodeCanonical<T>(await open(key, record.ciphertext, this.aad(record)))
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
    ensure(meta.recoveryChecked, 'RECOVERY_INCOMPLETE', '请先验证恢复码并保存恢复包。')
    ensure(
      canonical(payload).length <= (kind.startsWith('formal_') ? 512 * 1024 : 200000),
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
      return await db.commit(record, base, lease, {
        id: revision,
        objectKey: record.key,
        frame: b64(canonical(record)),
        digest: record.digest,
        state: 'local',
        attempt: 0,
        nextAttempt: 0
      })
    } finally {
      key.fill(0)
    }
  }
  async saveItem(input: { item: Item; id?: string; base?: string | null }) {
    ensure(input.item.type !== 'file', 'INVALID_INPUT')
    const item = itemSchema.parse(input.item)
    ensure(
      utf8(item.body).length + utf8(item.secret).length <= 32768,
      'QUOTA_EXCEEDED',
      '条目正文应小于 32 KiB。'
    )
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
    const { db, lease } = await this.ready(),
      record = (await db.db.get('objects', input.key)) as EncryptedObject | undefined
    ensure(record?.conflict && record.kind === 'vault', 'NOT_FOUND')
    const saved = await this.write(
      'vault',
      await this.decode(record),
      record.id,
      input.base,
      record.tombstone
    )
    // Retain the immutable branch as evidence. Resolution is a new revision.
    if (!saved.conflict)
      await db.guardedPut('meta', { id: `resolved:${record.key}`, value: saved.key }, lease)
    return saved
  }
  async view(): Promise<ViewData> {
    const { db, meta, localKey } = await this.ready(),
      records = await db.heads()
    const data: ViewData = {
      meta,
      entries: [],
      channels: [],
      messages: [],
      profile: null,
      outbox: await db.db.getAll('outbox'),
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
      else if (record.kind === 'channel' && !record.tombstone) {
        const channel = await this.decode<Channel>(record)
        channel.key = ''
        data.channels.push({ record, channel })
      } else if (record.kind === 'message' && !record.tombstone)
        data.messages.push({ record, message: await this.decode<Message>(record) })
      else if (record.kind === 'profile') data.profile = await this.decode<Profile>(record)
      if ((index + 1) % 32 === 0) await this.tick()
    }
    const conflicts = ((await db.db.getAll('objects')) as EncryptedObject[]).filter(
      (x) => x.kind === 'vault' && x.conflict
    )
    for (let index = 0; index < conflicts.length; index++) {
      const record = conflicts[index]
      if (!(await db.db.get('meta', `resolved:${record.key}`)))
        data.conflicts.push(await decodeItem(record))
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
  async createChannel(input: { name: string; type: Channel['type']; recipient: string }) {
    const { meta } = await this.ready()
    ensure(
      ['direct', 'collaboration', 'distribution'].includes(input.type) &&
        input.name.trim().length > 0 &&
        input.name.length <= 100,
      'INVALID_INPUT'
    )
    ensure(
      !input.recipient || /^[0-9a-f]{64}$/.test(input.recipient),
      'INVALID_INPUT',
      '请输入 64 位稳定主体 ID。'
    )
    const channel: Channel = {
      name: input.name.trim(),
      type: input.type,
      members: [
        { subject: meta.subjectId, role: 'owner', accepted: true },
        ...(input.recipient && input.recipient !== meta.subjectId
          ? [{ subject: input.recipient, role: 'member' as const, accepted: false }]
          : [])
      ],
      epoch: 0,
      state: 'draft',
      controlHead: null,
      key: b64(random()),
      createdAt: Date.now()
    }
    return this.write('channel', channel)
  }
  async saveMessage(input: { channelId: string; text: string }) {
    const { db, bundle } = await this.ready()
    ensure(
      input.text.trim().length > 0 && utf8(input.text).length <= 24000,
      'QUOTA_EXCEEDED',
      '消息应小于 24 KiB。'
    )
    const record = (await db.heads()).find(
      (x) => x.id === input.channelId && x.kind === 'channel'
    )
    ensure(record, 'NOT_FOUND')
    const channel = await this.decode<Channel>(record)
    ensure(channel.state === 'draft', 'POLICY_STALE', '需要刷新频道授权与 epoch 后才能发布。')
    const message: Message = {
      channelId: input.channelId,
      text: input.text,
      createdAt: Date.now(),
      epoch: channel.epoch,
      state: 'draft'
    }
    message.signature = b64(
      ed25519.sign(digest('dmsg/local-message/1', message), unb64(bundle.signing))
    )
    return this.write('message', message)
  }
  async saveDraft(input: { channelId: string; text: string }) {
    const { db, localKey, lease } = await this.ready()
    ensure(
      /^[0-9a-f]{64}$/.test(input.channelId) && utf8(input.text).length <= 24000,
      'INVALID_INPUT'
    )
    const key = `composer:${input.channelId}`
    const ciphertext = await seal(
      derive(localKey, ['dmsg/local-private/1']),
      utf8(input.text),
      ['dmsg/draft/1', key, db.name]
    )
    await this.ready()
    await db.guardedPut('local_private', { id: key, ciphertext }, lease)
  }
  async getDraft(channelId: string) {
    const { db, localKey } = await this.ready(),
      key = `composer:${channelId}`,
      record = await db.db.get('local_private', key)
    if (!record) return ''
    const text = new TextDecoder().decode(
      await open(derive(localKey, ['dmsg/local-private/1']), record.ciphertext, [
        'dmsg/draft/1',
        key,
        db.name
      ])
    )
    await this.ready()
    return text
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
    const existing = (await db.heads()).find((x) => x.kind === 'profile')
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
    ensure(meta.recoveryChecked, 'RECOVERY_INCOMPLETE', '请先验证恢复材料。')
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
      const prior = (await db.heads()).find((x) => x.id === fileId)
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
  private async verifyCloudSnapshot(
    meta: WorkspaceMeta,
    root: Uint8Array,
    chunks?: Map<string, Chunk>
  ) {
    const snapshot = meta.cloudSnapshot
    if (!snapshot) return
    ensure(
      snapshot.uploads.length <= 10000 && snapshot.heads.length <= 100000,
      'QUOTA_EXCEEDED'
    )
    const history = meta.rootHistory
      ? decodeCanonical<Record<string, string>>(
          await open(root, meta.rootHistory, [
            'dmsg/root-history/1',
            meta.subjectId,
            meta.rootGeneration
          ])
        )
      : {}
    for (const upload of snapshot.uploads) {
      const plan = storedUploadPlanSchema.parse(upload.plan),
        manifest = unb64(upload.manifest)
      ensure(
        manifest.length === plan.manifest_size &&
          hash(manifest) === plan.manifest_digest &&
          upload.chunkIds.length === plan.chunks.length,
        'INTEGRITY_FAILED'
      )
      for (let index = 0; index < upload.chunkIds.length; index++) {
        const key = upload.chunkIds[index],
          chunk = chunks
            ? chunks.get(key)
            : ((await this.db!.db.get('chunks', key)) as Chunk | undefined)
        ensure(chunk && chunk.digest === plan.chunks[index].digest, 'RECOVERY_INCOMPLETE')
        const bytes = unb64(chunk.ciphertext)
        ensure(
          bytes.length === plan.chunks[index].size &&
            hash(bytes) === plan.chunks[index].digest,
          'INTEGRITY_FAILED'
        )
      }
      if (plan.kind === 'vault' || plan.kind === 'file') {
        const old =
          plan.root_generation === meta.rootGeneration
            ? root
            : history[String(plan.root_generation)]
              ? unb64(history[String(plan.root_generation)])
              : null
        ensure(old, 'RECOVERY_INCOMPLETE')
        const content = await openContentManifest(
          old,
          meta.subjectId,
          uploadPlanSchema.parse(plan),
          manifest
        )
        if (content) await this.verifyFileManifest(content, chunks, false)
      }
      if (this.lease) await this.tick()
    }
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
  async changePassword(input: { current: string; next: string }) {
    const { db, localKey, lease } = await this.ready()
    ensure(input.next.length >= 12, 'INVALID_INPUT', '新口令至少需要 12 个字符。')
    const old = await db.envelope(),
      previous = await passwordKey(input.current, old.kdf)
    try {
      const recovered = await open(previous, old.wrappedKey, ['dmsg/local-key/1', db.name])
      ensure(equal(recovered, localKey), 'INTEGRITY_FAILED')
      recovered.fill(0)
    } finally {
      previous.fill(0)
    }
    await db.renew(lease)
    const kdf = defaultKdf(),
      next = await passwordKey(input.next, kdf)
    try {
      await db.renew(lease)
      const wrappedKey = await seal(next, localKey, ['dmsg/local-key/1', db.name])
      await this.ready()
      await db.guardedPut('key_envelopes', { ...old, kdf, wrappedKey }, lease)
    } finally {
      next.fill(0)
    }
  }
  private async reauthenticate(password: string) {
    const { db, localKey, lease } = await this.ready(),
      envelope = await db.envelope(),
      luk = await passwordKey(password, envelope.kdf)
    try {
      const key = await open(luk, envelope.wrappedKey, ['dmsg/local-key/1', db.name])
      ensure(equal(key, localKey), 'INTEGRITY_FAILED', '口令不匹配。')
      key.fill(0)
      await db.renew(lease)
    } finally {
      luk.fill(0)
    }
  }
  private async partialPlan(plan: FilePlan, chunks?: Map<string, Chunk>) {
    const manifest = plan.manifest,
      count = Math.ceil(manifest.size / CHUNK_SIZE)
    ensure(
      /^[0-9a-f]{64}$/.test(manifest.id) &&
        /^[0-9a-f]{64}$/.test(manifest.version) &&
        Number.isSafeInteger(manifest.size) &&
        manifest.size >= 0 &&
        manifest.size <= MAX_FILE &&
        manifest.chunks.length <= count &&
        plan.plainDigests.length === manifest.chunks.length,
      'INTEGRITY_FAILED'
    )
    const key = unb64(manifest.key)
    try {
      for (let index = 0; index < manifest.chunks.length; index++) {
        const ref = manifest.chunks[index],
          chunk = chunks
            ? chunks.get(ref.id)
            : ((await this.db!.db.get('chunks', ref.id)) as Chunk | undefined)
        ensure(
          ref.id === `${manifest.id}:${manifest.version}:${index}` &&
            ref.size === Math.min(CHUNK_SIZE, manifest.size - index * CHUNK_SIZE) &&
            chunk &&
            hash(unb64(chunk.ciphertext)) === ref.digest,
          'RECOVERY_INCOMPLETE'
        )
        const plain = await open(key, chunk.ciphertext, [
          'dmsg/file-chunk/1',
          manifest.id,
          manifest.version,
          index,
          count,
          ref.size
        ])
        ensure(
          plain.length === ref.size && hash(plain) === plan.plainDigests[index],
          'INTEGRITY_FAILED'
        )
        plain.fill(0)
        if (this.lease) await this.tick()
      }
    } finally {
      key.fill(0)
    }
  }
  private async backupDraft(payload: unknown) {
    const { meta, bundle } = await this.ready()
    const record: EncryptedObject = {
      key: '',
      id: id(),
      revision: id(),
      parent: null,
      subjectId: meta.subjectId,
      deviceId: meta.deviceId,
      generation: meta.rootGeneration,
      kind: 'draft',
      ciphertext: '',
      keyEnvelope: '',
      digest: '',
      tombstone: false,
      conflict: false,
      createdAt: Date.now()
    }
    record.key = `${record.id}:${record.revision}`
    const key = random()
    try {
      record.ciphertext = await seal(key, canonical(payload), this.aad(record))
      record.keyEnvelope = await seal(unb64(bundle.root), key, this.wrapContext(record))
      record.digest = hash(
        canonical([this.aad(record), record.ciphertext, record.keyEnvelope])
      )
      return record
    } finally {
      key.fill(0)
    }
  }
  private async restoreAuxiliary() {
    const { db, localKey, lease } = await this.ready(),
      files = new Map<string, FilePlan>()
    for (const record of (await db.db.getAll('objects')) as EncryptedObject[])
      if (record.kind === 'draft') {
        const draft = await this.decode<{
          format?: string
          source: string
          text: string
          plan?: FilePlan
        }>(record)
        if (
          draft.source?.startsWith('composer:') &&
          !(await db.db.get('local_private', draft.source))
        )
          await this.saveDraft({ channelId: draft.source.slice(9), text: draft.text })
        if (draft.format === 'dmsg-file-import/1' && draft.plan) {
          const plan = draft.plan,
            previous = files.get(plan.manifest.version)
          if (previous)
            ensure(
              previous.manifest.key === plan.manifest.key &&
                previous.manifest.id === plan.manifest.id &&
                previous.manifest.size === plan.manifest.size &&
                previous.plainDigests
                  .slice(0, Math.min(previous.plainDigests.length, plan.plainDigests.length))
                  .every((d, i) => d === plan.plainDigests[i]),
              'INTEGRITY_FAILED'
            )
          if (!previous || previous.manifest.chunks.length < plan.manifest.chunks.length)
            files.set(plan.manifest.version, plan)
        }
      }
    for (const [version, plan] of files) {
      if (
        (await db.db.get('local_private', `file-job:${version}`)) ||
        (await db.db.get('migration_jobs', version))?.stage === 'complete'
      )
        continue
      await this.partialPlan(plan)
      await db.checkpointFile(
        version,
        await seal(localKey, canonical(plan), ['dmsg/file-job/1', db.name, version]),
        {
          id: version,
          kind: 'file-import',
          stage: 'encrypting',
          completed: plan.manifest.chunks.length,
          total: Math.ceil(plan.manifest.size / CHUNK_SIZE)
        },
        undefined,
        lease
      )
    }
  }
  async exportBackup(password: string) {
    await this.reauthenticate(password)
    const { db, meta, bundle, localKey, lease } = await this.ready()
    await this.tick()
    const tx = db.db.transaction(['objects', 'local_private', 'migration_jobs'])
    const objects = (await tx.objectStore('objects').getAll()) as EncryptedObject[],
      drafts = await tx.objectStore('local_private').getAll(),
      jobs = await tx.objectStore('migration_jobs').getAll()
    await tx.done
    await this.tick()
    const missing = jobs.filter((j) => j.stage !== 'complete').map((j) => `unfinished:${j.id}`)
    const vaultFiles = new Set<string>()
    for (const record of objects.filter((o) => o.kind === 'vault')) {
      const item = await this.decode<Item>(record)
      if (item.file) vaultFiles.add(`${item.file.id}:${item.file.version}`)
    }
    for (const record of objects.filter((o) => o.kind === 'formal_message')) {
      const message = await this.decode<any>(record)
      if (message.file && !vaultFiles.has(`${message.file.file_id}:${message.file.version}`))
        missing.push(`channel-file:${message.channel}:${message.file.upload_id}`)
    }
    // Composer drafts use LocalDataKey on disk. Rewrap their plaintext into an
    // archive-only content object so the backup never contains device secrets.
    const composerDrafts = drafts.filter((x) => x.id.startsWith('composer:'))
    for (let index = 0; index < composerDrafts.length; index++) {
      const draft = composerDrafts[index]
      const text = await open(derive(localKey, ['dmsg/local-private/1']), draft.ciphertext, [
        'dmsg/draft/1',
        draft.id,
        db.name
      ])
      if (!text.length) {
        text.fill(0)
        continue
      }
      const record: EncryptedObject = {
        key: '',
        id: id(),
        revision: id(),
        parent: null,
        subjectId: meta.subjectId,
        deviceId: meta.deviceId,
        generation: meta.rootGeneration,
        kind: 'draft',
        ciphertext: '',
        keyEnvelope: '',
        digest: '',
        tombstone: false,
        conflict: false,
        createdAt: Date.now()
      }
      record.key = `${record.id}:${record.revision}`
      const key = random()
      record.ciphertext = await seal(
        key,
        canonical({ source: draft.id, text: new TextDecoder().decode(text) }),
        this.aad(record)
      )
      record.keyEnvelope = await seal(unb64(bundle.root), key, this.wrapContext(record))
      record.digest = hash(
        canonical([this.aad(record), record.ciphertext, record.keyEnvelope])
      )
      objects.push(record)
      text.fill(0)
      key.fill(0)
      if ((index + 1) % 16 === 0) await this.tick()
    }
    const required = new Set<string>()
    for (let index = objects.length - 1; index >= 0; index--)
      if (objects[index].kind === 'draft') {
        const draft = await this.decode<{ format?: string; plan?: FilePlan }>(objects[index])
        if (
          draft.format === 'dmsg-file-import/1' &&
          draft.plan &&
          ((await db.db.get('local_private', `file-job:${draft.plan.manifest.version}`)) ||
            (await db.db.get('migration_jobs', draft.plan.manifest.version))?.stage ===
              'complete')
        )
          objects.splice(index, 1)
      }
    for (const row of drafts.filter((d) => d.id.startsWith('file-job:'))) {
      const version = row.id.slice('file-job:'.length),
        plan = decodeCanonical<FilePlan>(
          await open(localKey, row.ciphertext, ['dmsg/file-job/1', db.name, version])
        )
      await this.partialPlan(plan)
      for (const chunk of plan.manifest.chunks) required.add(chunk.id)
      objects.push(
        await this.backupDraft({
          format: 'dmsg-file-import/1',
          source: `file-import:${version}`,
          text: '',
          plan
        })
      )
    }
    await this.verifyCloudSnapshot(meta, unb64(bundle.root))
    for (const upload of meta.cloudSnapshot?.uploads ?? [])
      for (const ref of upload.chunkIds) required.add(ref)
    const verifiedFiles = new Set<string>(),
      vaultObjects = objects.filter((o) => o.kind === 'vault' || o.kind === 'migration_part')
    for (let index = 0; index < vaultObjects.length; index++) {
      const record = vaultObjects[index]
      const item = await this.decode<Item>(record)
      if (item.file) {
        const fileVersion = hash(canonical(item.file))
        if (!verifiedFiles.has(fileVersion)) {
          await this.verifyFile(record, undefined, undefined, false)
          verifiedFiles.add(fileVersion)
        }
        for (const chunk of item.file.chunks) required.add(chunk.id)
      }
      if ((index + 1) % 16 === 0) await this.tick()
    }
    const chunks: Chunk[] = []
    let chunkIndex = 0
    for (const ref of required) {
      const chunk = (await db.db.get('chunks', ref)) as Chunk | undefined
      ensure(chunk, 'RECOVERY_INCOMPLETE', `备份所需文件块缺失：${ref}`)
      chunks.push(chunk)
      if (++chunkIndex % 16 === 0) await this.tick()
    }
    const pendingRequests = await db.db.getAll('requests')
    for (const request of pendingRequests.filter(
      (r) => !['rejected', 'cancelled', 'expired'].includes(r.state)
    ))
      missing.push(`request:${request.id}`)
    const content = {
      format: 'dmsg-backup/1' as const,
      meta: { ...meta },
      createdAt: Date.now(),
      scope: missing.length ? ('partial' as const) : ('local-inclusive' as const),
      objects,
      chunks,
      missing
    }
    await this.tick()
    const manifestDigest = hash(utf8(JSON.stringify(content))),
      authentication = b64(
        hmac(sha256, derive(unb64(bundle.root), ['dmsg/backup-auth/1']), utf8(manifestDigest))
      )
    const archive: RecoveryArchive = { ...content, manifestDigest, authentication }
    const json = JSON.stringify(archive)
    ensure(
      utf8(json).length <= 256 * 1024 * 1024,
      'QUOTA_EXCEEDED',
      '恢复包超过 256 MiB，上限包含全部编码与封装开销；未生成文件。'
    )
    await this.tick()
    const updated = {
      ...meta,
      lastBackupAt: archive.createdAt,
      lastBackupCount: objects.length
    }
    await db.guardedPut('meta', { id: 'workspace', value: updated }, lease)
    this.meta = updated
    return {
      blob: new Blob([json], { type: 'application/json' }),
      name: `dmsg-${new Date(archive.createdAt).toISOString().slice(0, 10)}.dmsg`,
      count: objects.length,
      missing,
      scope: archive.scope
    }
  }
  async restore(input: { file: File; code: string; password: string }) {
    ensure(!this.bundle, 'LOCKED', '恢复前请先锁定当前工作台。')
    ensure(
      (await currentWorkspace()) === null,
      'VERSION_CONFLICT',
      '此浏览器配置已经有工作台。请在空白浏览器配置中恢复，以免隐藏原有数据。'
    )
    ensure(
      input.file.size <= 256 * 1024 * 1024 && input.password.length >= 12,
      'INVALID_INPUT',
      '请选择有效备份，并设置至少 12 个字符的新口令。'
    )
    const archive = JSON.parse(await input.file.text()) as RecoveryArchive
    ensure(
      archive.format === 'dmsg-backup/1' &&
        Array.isArray(archive.objects) &&
        Array.isArray(archive.chunks) &&
        Array.isArray(archive.missing) &&
        archive.objects.length <= 10000 &&
        archive.chunks.length <= 10000,
      'UNSUPPORTED_PROTOCOL'
    )
    const { manifestDigest, authentication, ...content } = archive
    ensure(
      hash(utf8(JSON.stringify(content))) === manifestDigest,
      'INTEGRITY_FAILED',
      '恢复包摘要不一致。'
    )
    const meta = archive.meta
    ensure(
      ['local', 'staging', 'production'].includes(meta.environment) &&
        (/^[0-9a-f]{64}$/.test(meta.subjectId) ||
          (meta.account?.id === meta.subjectId && xidBytes(meta.subjectId).length === 12)) &&
        Number.isSafeInteger(meta.rootGeneration) &&
        meta.rootGeneration > 0,
      'INVALID_INPUT'
    )
    const code = unhex(input.code.trim().toLowerCase().replaceAll(/[-\s]/g, '')),
      seeds = recoverySeeds(code, meta.environment, meta.subjectId, meta.recoveryGeneration)
    const root = await hpkeOpen(
      seeds.hpke,
      meta.recoveryEnvelope,
      recoveryContext(meta.environment, meta.subjectId, meta.rootGeneration)
    )
    code.fill(0)
    seeds.signing.fill(0)
    seeds.hpke.fill(0)
    try {
      ensure(
        equal(
          hmac(sha256, derive(root, ['dmsg/backup-auth/1']), utf8(manifestDigest)),
          unb64(authentication)
        ),
        'INTEGRITY_FAILED',
        '恢复包清单认证失败。'
      )
      const uniqueObjects = new Set(archive.objects.map((o) => o.key)),
        chunkMap = new Map(archive.chunks.map((c) => [c.id, c]))
      ensure(
        uniqueObjects.size === archive.objects.length &&
          chunkMap.size === archive.chunks.length,
        'INTEGRITY_FAILED'
      )
      this.meta = meta
      await this.verifyCloudSnapshot(meta, root, chunkMap)
      for (let index = 0; index < archive.objects.length; index++) {
        const record = archive.objects[index]
        ensure(
          record.subjectId === meta.subjectId &&
            record.key === `${record.id}:${record.revision}` &&
            /^[0-9a-f]{64}$/.test(record.id) &&
            /^[0-9a-f]{64}$/.test(record.revision),
          'INTEGRITY_FAILED'
        )
        const payload = await this.decode<Item>(record, root)
        if (
          record.kind === 'draft' &&
          (payload as unknown as { format?: string }).format === 'dmsg-file-import/1'
        )
          await this.partialPlan((payload as unknown as { plan: FilePlan }).plan, chunkMap)
        if ((record.kind === 'vault' || record.kind === 'migration_part') && payload.file) {
          const result = await this.verifyFile(record, root, chunkMap)
          for (const part of result.parts) part.fill(0)
        }
        this.progress({
          stage: '正在验证恢复内容',
          completed: index + 1,
          total: archive.objects.length
        })
      }
      // A fresh device receives fresh keys. Offline recovery is not on-chain
      // device approval, and the old authentication private key is not copied.
      const bundle: Bundle = {
        root: b64(root),
        signing: b64(random()),
        hpke: b64(random()),
        transport: b64(random())
      }
      if (meta.rootHistory)
        bundle.roots = decodeCanonical(
          await open(root, meta.rootHistory, [
            'dmsg/root-history/1',
            meta.subjectId,
            meta.rootGeneration
          ])
        )
      const restored: WorkspaceMeta = {
        ...meta,
        restoredFrom: { manifestDigest, device: meta.deviceId, at: Date.now() },
        deviceId: id(),
        registered: false,
        createdAt: Date.now(),
        signingPublic: b64(ed25519.getPublicKey(unb64(bundle.signing))),
        hpkePublic: await hpkePublic(unb64(bundle.hpke)),
        transportPublic: b64(ed25519.getPublicKey(unb64(bundle.transport))),
        recoveryChecked: true
      }
      await this.install(input.password, restored, bundle, archive)
      await this.restoreAuxiliary()
      return { meta: restored, count: archive.objects.length, missing: archive.missing }
    } finally {
      root.fill(0)
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
  async authSign(message: Uint8Array) {
    const { bundle } = await this.ready()
    ensure(message.length <= 1048576, 'QUOTA_EXCEEDED')
    return ed25519.sign(message, unb64(bundle.transport))
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

  async deviceSign(message: Uint8Array) {
    ensure(message instanceof Uint8Array && message.length === 32, 'INVALID_INPUT')
    const { bundle } = await this.ready()
    const key = unb64(bundle.signing)
    try {
      return ed25519.sign(message, key)
    } finally {
      key.fill(0)
    }
  }

  async contentSign(message: Uint8Array) {
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
    const aad = ['dmsg/root-candidate/1', db.name, key]
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

  async prepareAccountRoot(context: RootContext) {
    return rootTransport(await this.candidate(context))
  }

  async wrapAccountRoot(context: RootContext, key: RootKey, encryptedKey: Uint8Array) {
    const material = await this.candidate(context)
    const { bundle, meta } = await this.ready()
    // Initial conversion must not pretend the R0 root was an account root.
    const previous =
      meta.account?.id === context.account &&
      meta.account.rootDigest &&
      meta.account.rootUploadId
        ? {
            digest: meta.account.rootDigest,
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
        key,
        encryptedKey,
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

  async openAccountRoot(
    context: RootContext,
    key: RootKey,
    encryptedKey: Uint8Array,
    bundles: Uint8Array[],
    expectedDigest: string
  ) {
    const material = await this.candidate(context)
    await this.candidate(
      context,
      await openRoot(material, key, encryptedKey, bundles, expectedDigest)
    )
  }

  async activateAccountRoot(input: {
    context: RootContext
    digest: string
    uploadId: string
    homeUser: string
    issuer: string
    password: string
  }) {
    await this.reauthenticate(input.password)
    const source = await this.ready(),
      material = await this.candidate(input.context)
    ensure(material.bytes && hash(unb64(material.bytes)) === input.digest, 'INTEGRITY_FAILED')
    const rootBundle = decodeCanonical<{
      payload: { recovery: WorkspaceMeta['recoveryEnvelope'] }
    }>(unb64(material.bytes))
    const meta: WorkspaceMeta = {
      ...source.meta,
      subjectId: input.context.account,
      account: {
        id: input.context.account,
        issuer: input.issuer,
        homeUser: input.homeUser,
        rootDigest: input.digest,
        rootUploadId: input.uploadId
      },
      rootGeneration: input.context.generation,
      recoveryGeneration: input.context.recoveryGeneration,
      recoveryPublic: input.context.recoveryPublic,
      recoverySigningPublic: input.context.recoverySigningPublic,
      recoveryEnvelope: rootBundle.payload.recovery,
      recoveryChecked: true,
      registered: true,
      lastBackupAt: null,
      lastBackupCount: 0,
      rootBundles: [
        ...new Set([
          ...(source.meta.account ? (source.meta.rootBundles ?? []) : []),
          ...(material.bundles ?? []),
          material.bytes
        ])
      ]
    }
    ensure(meta.rootBundles!.length <= 256, 'QUOTA_EXCEEDED')
    const next: Bundle = {
      ...source.bundle,
      root: material.root,
      roots: { ...material.roots }
    }
    if (source.meta.account?.id === input.context.account) {
      ensure(source.meta.rootGeneration <= meta.rootGeneration, 'VERSION_CONFLICT')
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
      meta.rootHistory = await seal(unb64(next.root), canonical(next.roots), [
        'dmsg/root-history/1',
        meta.subjectId,
        meta.rootGeneration
      ])
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
    ensure(
      !source.meta.account,
      'VERSION_CONFLICT',
      '不能把已关联的正式工作区转换到另一个账户。'
    )
    if (Object.keys(next.roots!).length)
      meta.rootHistory = await seal(unb64(next.root), canonical(next.roots), [
        'dmsg/root-history/1',
        meta.subjectId,
        meta.rootGeneration
      ])
    // Verify the entire source, including files, before constructing any target.
    const backup = await this.exportBackup(input.password)
    ensure(
      !backup.missing.length,
      'RECOVERY_INCOMPLETE',
      '请先完成未完成的文件或请求，再转换工作区。'
    )
    const archive = JSON.parse(await backup.blob.text()) as RecoveryArchive
    const originalKeys = new Set(
      ((await source.db.db.getAll('objects')) as EncryptedObject[]).map((o) => o.key)
    )
    const composer = (await source.db.db.getAll('local_private')).filter((row) =>
      row.id.startsWith('composer:')
    )
    meta.account!.sourceDigest = hash(
      canonical([
        archive.objects.filter((o) => originalKeys.has(o.key)).map((o) => [o.key, o.digest]),
        archive.chunks.map((c) => [c.id, c.digest]),
        composer.map((row) => [row.id, row.ciphertext])
      ])
    )
    const remap = (kind: string, value: string) =>
      hex(
        digest('dmsg/local-conversion/1', [
          input.context.account,
          input.context.opId,
          kind,
          value
        ])
      )
    const records: EncryptedObject[] = []
    for (const record of archive.objects) {
      await this.tick()
      const payload = await this.decode<Record<string, unknown>>(record)
      const syntheticDraft = record.kind === 'draft' && !originalKeys.has(record.key)
      const sourceId = syntheticDraft ? `draft:${String(payload.source)}` : record.id
      const sourceRevision = syntheticDraft
        ? `${sourceId}:${meta.account!.sourceDigest}`
        : record.revision
      if (record.kind === 'message')
        payload.channelId = remap('object', payload.channelId as string)
      if (record.kind === 'profile' && typeof payload.avatarFile === 'string') {
        const [objectId, revision] = payload.avatarFile.split(':')
        payload.avatarFile = `${remap('object', objectId)}:${remap('revision', revision)}`
      }
      if (
        record.kind === 'migration' &&
        payload.format === 'dmsg-legacy-storage/1' &&
        Array.isArray(payload.frozenProofParts)
      )
        payload.frozenProofParts = (payload.frozenProofParts as string[]).map((key) => {
          const [id, revision] = key.split(':')
          return `${remap('object', id)}:${remap('revision', revision)}`
        })
      if (record.kind === 'migration' && payload.format === 'dmsg-legacy-storage/1')
        payload.parts = (payload.parts as string[]).map((key) => {
          const [objectId, revision] = key.split(':')
          return `${remap('object', objectId)}:${remap('revision', revision)}`
        })
      if (
        record.kind === 'draft' &&
        typeof payload.source === 'string' &&
        payload.source.startsWith('composer:')
      )
        payload.source = `composer:${remap('object', payload.source.slice(9))}`
      if (record.kind === 'channel') {
        const channel = payload as unknown as Channel
        channel.members = channel.members.map((m) => ({
          ...m,
          subject: m.subject === source.meta.subjectId ? meta.subjectId : m.subject
        }))
        channel.state = 'draft'
      }
      const target: EncryptedObject = {
        ...record,
        id: remap('object', sourceId),
        revision: remap('revision', sourceRevision),
        parent: record.parent ? remap('revision', record.parent) : null,
        subjectId: meta.subjectId,
        generation: meta.rootGeneration,
        deviceId: meta.deviceId
      }
      target.key = `${target.id}:${target.revision}`
      const key = random()
      try {
        target.ciphertext = await seal(key, canonical(payload), this.aad(target))
        target.keyEnvelope = await seal(unb64(next.root), key, this.wrapContext(target))
        target.digest = hash(
          canonical([this.aad(target), target.ciphertext, target.keyEnvelope])
        )
        ensure(
          equal(await open(key, target.ciphertext, this.aad(target)), canonical(payload)),
          'INTEGRITY_FAILED'
        )
      } finally {
        key.fill(0)
      }
      records.push(target)
      this.progress({
        stage: '正在验证正式工作区副本',
        completed: records.length,
        total: archive.objects.length
      })
    }
    const name = `dmsg:${meta.environment}:${meta.subjectId}:${meta.deviceId}`
    const targetDB = await WorkspaceDB.open(name)
    const previous = await targetDB.meta()
    targetDB.db.close()
    if (previous) {
      // A crash may leave a complete target before the registry switch. Verify
      // its encrypted contents with the same root, then activate that copy.
      ensure(
        previous.account?.rootDigest === input.digest &&
          previous.account?.sourceDigest === meta.account!.sourceDigest,
        'VERSION_CONFLICT'
      )
      const target = await WorkspaceDB.open(name)
      try {
        const envelope = await target.envelope(),
          luk = await passwordKey(input.password, envelope.kdf)
        try {
          const local = await open(luk, envelope.wrappedKey, ['dmsg/local-key/1', name])
          const savedBundle = decodeCanonical<Bundle>(
            await open(local, envelope.privateBundle, ['dmsg/device-bundle/1', name])
          )
          local.fill(0)
          ensure(
            savedBundle.root === next.root && savedBundle.signing === next.signing,
            'INTEGRITY_FAILED'
          )
        } finally {
          luk.fill(0)
        }
        const saved = (await target.db.getAll('objects')) as EncryptedObject[]
        ensure(saved.length === records.length, 'RECOVERY_INCOMPLETE')
        const keys = new Set(records.map((r) => r.key))
        for (const record of saved) {
          ensure(
            keys.has(record.key) && record.subjectId === meta.subjectId,
            'INTEGRITY_FAILED'
          )
          ensure(
            record.generation === meta.rootGeneration &&
              hash(canonical([this.aad(record), record.ciphertext, record.keyEnvelope])) ===
                record.digest,
            'INTEGRITY_FAILED'
          )
          const key = await open(
            unb64(next.root),
            record.keyEnvelope,
            this.wrapContext(record)
          )
          try {
            await open(key, record.ciphertext, this.aad(record))
          } finally {
            key.fill(0)
          }
        }
        const chunks = (await target.db.getAll('chunks')) as Chunk[]
        ensure(
          chunks.length === archive.chunks.length &&
            chunks.every(
              (c) =>
                hash(unb64(c.ciphertext)) === c.digest &&
                archive.chunks.some((old) => old.id === c.id && old.digest === c.digest)
            ),
          'INTEGRITY_FAILED'
        )
      } finally {
        target.db.close()
      }
      await registerWorkspace(name, source.db.name)
      await this.lock()
      return this.unlock(input.password)
    }
    // All writes into the new database commit together. The original encrypted
    // database remains registered as a retained, inactive copy after the switch.
    await this.install(
      input.password,
      meta,
      next,
      { ...archive, meta, objects: records },
      undefined,
      source.db.name
    )
    await source.db.release(source.lease)
    source.db.db.close()
    source.localKey.fill(0)
    for (const record of records.filter((r) => r.kind === 'draft')) {
      const draft = await this.decode<{ source: string; text: string }>(record)
      if (draft.source.startsWith('composer:'))
        await this.saveDraft({ channelId: draft.source.slice(9), text: draft.text })
    }
    return meta
  }

  async accountRecovery(input: {
    account: string
    generation: number
    action: 'generate' | 'prove' | 'clear'
    code?: string
    message?: Uint8Array
    publicKey?: string
  }) {
    xidBytes(input.account)
    ensure(Number.isSafeInteger(input.generation) && input.generation > 0, 'INVALID_INPUT')
    const { meta } = await this.ready()
    const key = `recovery:${input.account}:${input.generation}`
    if (input.action === 'clear') {
      await this.controlPut(key, 'null')
      return { code: '', signingPublic: '', hpkePublic: '', signature: new Uint8Array() }
    }
    let code: Uint8Array
    if (input.action === 'generate') {
      const saved = await this.controlGet(key)
      code = saved && saved !== 'null' ? unhex(JSON.parse(saved)) : random()
      // Only the unfinished setup code is retained, under LocalDataKey.
      await this.controlPut(key, JSON.stringify(hex(code)))
    } else {
      ensure(input.code && input.message?.length === 32 && input.publicKey, 'INVALID_INPUT')
      code = unhex(input.code.trim().toLowerCase().replaceAll(/[-\s]/g, ''))
    }
    const seeds = recoverySeeds(code, meta.environment, input.account, input.generation)
    try {
      const signingPublic = b64(ed25519.getPublicKey(seeds.signing))
      if (input.action === 'prove')
        ensure(signingPublic === input.publicKey, 'INTEGRITY_FAILED', '账户恢复码不匹配。')
      return {
        code: input.action === 'generate' ? hex(code).match(/.{8}/g)!.join('-') : '',
        signingPublic,
        hpkePublic: await hpkePublic(seeds.hpke),
        signature:
          input.action === 'prove'
            ? ed25519.sign(input.message!, seeds.signing)
            : new Uint8Array()
      }
    } finally {
      code.fill(0)
      seeds.signing.fill(0)
      seeds.hpke.fill(0)
    }
  }
}
