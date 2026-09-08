import { sha256 } from '@noble/hashes/sha2.js'
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
    pendingRecoveryCode?: Uint8Array
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
        b64(ed25519.getPublicKey(unb64(bundle.signing))) === meta.signingPublic,
        'INTEGRITY_FAILED'
      )
      this.db = db
      this.lease = lease
      this.localKey = localKey
      this.bundle = bundle
      this.meta = meta
      return meta
    } catch (error) {
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
    const key = await open(
      root ?? unb64(this.bundle!.root),
      record.keyEnvelope,
      this.wrapContext(record)
    )
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
    ensure(canonical(payload).length <= 200000, 'QUOTA_EXCEEDED', '条目超过大小限制。')
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
        publicFields: z.array(z.enum(['name', 'bio', 'link'])).max(3)
      })
      .strict()
      .parse(profile)
    if (clean.link)
      ensure(
        new URL(clean.link).protocol === 'https:',
        'INVALID_INPUT',
        '公开链接须使用 HTTPS。'
      )
    const { db } = await this.ready(),
      existing = (await db.heads()).find((x) => x.kind === 'profile')
    return this.write('profile', clean, existing?.id, existing?.revision)
  }
  async importFile(file: File, resumeId?: string) {
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
      const result = prior ?? (await this.write('vault', item, fileId))
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
    ensure(
      item.file &&
        item.file.size <= MAX_FILE &&
        item.file.chunks.length === Math.ceil(item.file.size / CHUNK_SIZE),
      'INTEGRITY_FAILED'
    )
    const manifest = item.file,
      parts: Uint8Array<ArrayBuffer>[] = [],
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
    const { db } = await this.ready(),
      record = (await db.db.get('objects', objectKey)) as EncryptedObject
    ensure(record?.kind === 'vault', 'NOT_FOUND')
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
    const verifiedFiles = new Set<string>(),
      vaultObjects = objects.filter((o) => o.kind === 'vault')
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
      '此备份超过当前本地导出上限，请分批导出文件。'
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
        /^[0-9a-f]{64}$/.test(meta.subjectId) &&
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
        if (record.kind === 'vault' && payload.file) {
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
      const restored: WorkspaceMeta = {
        ...meta,
        deviceId: id(),
        registered: false,
        createdAt: Date.now(),
        signingPublic: b64(ed25519.getPublicKey(unb64(bundle.signing))),
        hpkePublic: await hpkePublic(unb64(bundle.hpke)),
        transportPublic: b64(ed25519.getPublicKey(unb64(bundle.transport))),
        recoveryChecked: true
      }
      await this.install(input.password, restored, bundle, archive)
      for (const record of archive.objects.filter((x) => x.kind === 'draft')) {
        const draft = await this.decode<{ source: string; text: string }>(record)
        if (draft.source.startsWith('composer:'))
          await this.saveDraft({ channelId: draft.source.slice(9), text: draft.text })
      }
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
}
