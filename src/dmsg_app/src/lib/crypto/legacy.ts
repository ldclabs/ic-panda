import {
  compareFrozenArchive,
  pack as legacyPack,
  unpack as legacyUnpack,
  type FrozenArchiveProof,
  encodeArchive,
  sharedHistoryArchive,
  type SharedSource,
  createPairing,
  archiveContentId,
  decodeArchive,
  openTransfer,
  pairingFingerprint,
  verifyArchiveContent,
  readObservation,
  openArchiveFile,
  type PairingOffer
} from '@dmsg/legacy'
import { b64, unb64, hash, canonical, decodeCanonical } from '../protocol/codec'
import type { WorkspaceDB } from '../db'
import type { EncryptedObject, Item, Lease, WorkspaceMeta } from '../models'
import { ensure } from '../errors'
import { open, seal } from './primitives'

interface Port {
  ready(): Promise<{
    db: WorkspaceDB
    localKey: Uint8Array
    lease: Lease
    meta: WorkspaceMeta
  }>
  paused(): boolean
  tick(): Promise<unknown>
  decode<T>(record: EncryptedObject): Promise<T>
  write(payload: unknown, id?: string, base?: string): Promise<EncryptedObject>
  importFile(file: File, resumeId?: string): Promise<EncryptedObject>
  downloadFile(key: string): Promise<{ blob: Blob }>
}
interface PairState {
  offer: PairingOffer
  seed?: string
  claimedDigest?: string
  startedAt?: number
  result?: string
  cancelled?: boolean
}
interface LegacyProgress {
  id: string
  nonce: string
  principal: string
  mode: string
  state: 'running' | 'paused' | 'failed' | 'cancelled' | 'complete'
  stage:
    'inventoried' | 'copied' | 'ciphertext_verified' | 'keys_imported' | 'content_verified'
  count: number
  completed: number
  partial: boolean
  archiveDigest: string
  updatedAt: number
  error?: string
  record?: string
}
interface ArchiveRecord {
  format: 'dmsg-legacy-storage/1'
  archiveDigest: string
  contentId: string
  parts: string[]
  principal: string
  mode: string
  objects: number
  gaps: number
  stage: 'content_verified' | 'partial'
  snapshot: 'pre_migration'
  frozenComparison?: Awaited<ReturnType<typeof compareFrozenArchive>> & {
    totalChanges: number
  }
  frozenProofParts?: string[]
}
export class LegacyVault {
  constructor(private readonly port: Port) {}
  private async checkpoint(job: LegacyProgress) {
    const { db, localKey, lease } = await this.port.ready()
    job.updatedAt = Date.now()
    await db.legacyCheckpoint(
      {
        id: `legacy:${job.id}`,
        kind: 'legacy',
        stage: job.state === 'complete' ? 'complete' : job.state,
        completed: job.completed,
        total: job.count
      },
      {
        id: `legacy-job:${job.id}`,
        ciphertext: await seal(localKey, canonical(job), [
          'dmsg/legacy-job/1',
          db.name,
          job.id
        ])
      },
      lease
    )
  }
  async jobs() {
    const { db, localKey } = await this.port.ready(),
      jobs: LegacyProgress[] = []
    for (const row of await db.db.getAll('local_private'))
      if (row.id.startsWith('legacy-job:')) {
        const id = row.id.slice('legacy-job:'.length)
        jobs.push(
          decodeCanonical(
            await open(localKey, row.ciphertext, ['dmsg/legacy-job/1', db.name, id])
          )
        )
      }
    return jobs
  }
  async cancel(id: string) {
    ensure(/^[0-9a-f]{64}$/.test(id), 'INVALID_INPUT')
    const job = (await this.jobs()).find((j) => j.id === id)
    ensure(job && job.state !== 'complete', 'NOT_FOUND')
    job.state = 'cancelled'
    await this.checkpoint(job)
    const { db, localKey, lease } = await this.port.ready()
    const row = await db.db.get('local_private', `legacy-pair:${job.nonce}`)
    if (row) {
      const pair = decodeCanonical<PairState>(
        await open(localKey, row.ciphertext, ['dmsg/legacy-pair/1', db.name, job.nonce])
      )
      pair.cancelled = true
      delete pair.seed
      await db.guardedPut(
        'local_private',
        {
          id: row.id,
          ciphertext: await seal(localKey, canonical(pair), [
            'dmsg/legacy-pair/1',
            db.name,
            job.nonce
          ])
        },
        lease
      )
    }
  }
  async pairs() {
    const { db, localKey } = await this.port.ready(),
      pending = []
    for (const row of await db.db.getAll('local_private')) {
      if (!row.id.startsWith('legacy-pair:')) continue
      const nonce = row.id.slice('legacy-pair:'.length)
      const state = decodeCanonical<PairState>(
        await open(localKey, row.ciphertext, ['dmsg/legacy-pair/1', db.name, nonce])
      )
      if (
        !state.cancelled &&
        !state.result &&
        (state.claimedDigest || state.offer.expiresAt > Date.now())
      )
        pending.push({
          offer: state.offer,
          fingerprint: pairingFingerprint(state.offer),
          started: Boolean(state.claimedDigest)
        })
    }
    return pending
  }
  async pair(origin: PairingOffer['origin'], target: string) {
    const { db, localKey, lease } = await this.port.ready()
    ensure(
      (await this.pairs()).length < 8,
      'QUOTA_EXCEEDED',
      '已有八个未完成配对，请先继续已有任务。'
    )
    if (typeof location !== 'undefined')
      ensure(
        location.protocol === 'chrome-extension:' &&
          target === `chrome-extension://${location.hostname}`,
        'AUTH_REQUIRED'
      )
    const session = await createPairing(origin, target)
    const state: PairState = { offer: session.offer, seed: b64(session.seed) }
    const ciphertext = await seal(localKey, canonical(state), [
      'dmsg/legacy-pair/1',
      db.name,
      session.offer.nonce
    ])
    session.seed.fill(0)
    await db.guardedPut(
      'local_private',
      { id: `legacy-pair:${session.offer.nonce}`, ciphertext },
      lease
    )
    return { offer: session.offer, fingerprint: pairingFingerprint(session.offer) }
  }
  async import(input: { file: File; nonce: string; fingerprint: string; principal: string }) {
    ensure(
      input.file.size > 0 &&
        input.file.size <= 256 * 1024 * 1024 &&
        /^[0-9a-f]{64}$/.test(input.nonce),
      'QUOTA_EXCEEDED'
    )
    const { db, localKey, lease } = await this.port.ready()
    const row = await db.db.get('local_private', `legacy-pair:${input.nonce}`)
    ensure(row, 'NOT_FOUND', '配对不存在，请重新配对。')
    const state = decodeCanonical<PairState>(
      await open(localKey, row.ciphertext, ['dmsg/legacy-pair/1', db.name, input.nonce])
    )
    ensure(!state.cancelled, 'LEGACY_CANCELLED', '此配对已取消；已保存内容和原来源仍保留。')
    const encrypted = new Uint8Array(await input.file.arrayBuffer()),
      transferDigest = hash(encrypted)
    ensure(
      pairingFingerprint(state.offer) === input.fingerprint &&
        (!state.claimedDigest || state.claimedDigest === transferDigest),
      'IDEMPOTENCY_CONFLICT',
      '配对已用于另一个档案。'
    )
    if (state.result) {
      const report = await this.report(state.result)
      ensure(report.principal === input.principal, 'AUTH_REQUIRED')
      return report
    }
    ensure(state.seed, 'NOT_FOUND')
    const authorizedAt = state.startedAt ?? Date.now(),
      seed = unb64(state.seed)
    const opened = await openTransfer(
      encrypted,
      { offer: state.offer, seed },
      {
        origin: state.offer.origin,
        target: state.offer.target,
        fingerprint: input.fingerprint
      },
      authorizedAt
    ).finally(() => seed.fill(0))
    const archive = decodeArchive(opened.plain)
    let progress: LegacyProgress | undefined
    try {
      ensure(
        archive.inventory.principal === input.principal,
        'AUTH_REQUIRED',
        '档案旧身份与本次确认的身份不一致。'
      )
      const archiveDigest = hash(opened.plain),
        contentId = archiveContentId(archive)
      const save = async () => {
        await db.guardedPut(
          'local_private',
          {
            id: `legacy-pair:${input.nonce}`,
            ciphertext: await seal(localKey, canonical(state), [
              'dmsg/legacy-pair/1',
              db.name,
              input.nonce
            ])
          },
          lease
        )
      }
      state.claimedDigest = transferDigest
      state.startedAt = authorizedAt
      await save()
      progress = {
        id: contentId,
        nonce: input.nonce,
        principal: input.principal,
        mode: archive.inventory.mode,
        state: 'running',
        stage: 'inventoried',
        count: archive.inventory.objects.length,
        completed: 0,
        partial: false,
        archiveDigest,
        updatedAt: Date.now()
      }
      await this.checkpoint(progress)
      for (const record of (await db.heads()).filter((r) => r.kind === 'migration')) {
        const previous = await this.port.decode<ArchiveRecord>(record)
        if (
          previous.format === 'dmsg-legacy-storage/1' &&
          (previous.contentId === contentId || previous.archiveDigest === archiveDigest)
        ) {
          state.result = record.key
          delete state.seed
          await save()
          progress.state = 'complete'
          progress.stage =
            previous.stage === 'content_verified' ? 'content_verified' : 'ciphertext_verified'
          progress.completed = progress.count
          progress.partial = previous.gaps > 0
          progress.record = record.key
          await this.checkpoint(progress)
          return this.report(record.key)
        }
      }
      const parts = await this.copyParts(opened.plain, archiveDigest)
      progress.stage = 'copied'
      await this.checkpoint(progress)
      progress.stage = 'ciphertext_verified'
      await this.checkpoint(progress)
      if (archive.keys.length) {
        progress.stage = 'keys_imported'
        await this.checkpoint(progress)
      }
      const pageRows = new Map<number, unknown[]>()
      const verification = await verifyArchiveContent(
        archive,
        async (index, checked, error) => {
          ensure(!this.port.paused(), 'LEGACY_PAUSED', '迁移已暂停，可继续同一文件。')
          const object = archive.inventory.objects[index],
            page = Math.floor(index / 64)
          const rows = pageRows.get(page) ?? []
          rows.push([
            object.key,
            object.digest,
            checked
              ? 'content_verified'
              : error?.code === 'missing_key'
                ? 'ciphertext_verified'
                : 'keys_imported',
            error?.code ?? null
          ])
          pageRows.set(page, rows)
          progress!.completed = index + 1
          if (rows.length === 64 || index + 1 === archive.inventory.objects.length) {
            await db.guardedPut(
              'local_private',
              {
                id: `legacy-progress:${contentId}:${page}`,
                ciphertext: await seal(localKey, canonical(rows), [
                  'dmsg/legacy-progress/1',
                  db.name,
                  contentId,
                  page
                ])
              },
              lease
            )
            await this.checkpoint(progress!)
            pageRows.delete(page)
          }
        }
      )
      const content: ArchiveRecord = {
        format: 'dmsg-legacy-storage/1',
        archiveDigest,
        contentId,
        parts,
        principal: input.principal,
        mode: archive.inventory.mode,
        objects: archive.inventory.objects.length,
        gaps: verification.gaps.length,
        stage: verification.partial ? 'partial' : 'content_verified',
        snapshot: 'pre_migration'
      }
      const result = await this.port.write(content)
      state.result = result.key
      delete state.seed
      await save()
      progress.state = 'complete'
      progress.stage =
        verification.checks.length ===
        archive.inventory.objects.filter(
          (o) => o.kind === 'message' || o.kind === 'file' || o.kind === 'avatar'
        ).length
          ? 'content_verified'
          : archive.keys.length
            ? 'keys_imported'
            : 'ciphertext_verified'
      progress.completed = progress.count
      progress.partial = verification.partial
      progress.record = result.key
      await this.checkpoint(progress)
      return this.report(result.key)
    } catch (error) {
      if (progress) {
        progress.state = this.port.paused() ? 'paused' : 'failed'
        progress.error = this.port.paused() ? 'LEGACY_PAUSED' : 'LEGACY_IMPORT_FAILED'
        await this.checkpoint(progress)
      }
      throw error
    } finally {
      archive.keys.forEach((k) => k.coseKey.fill(0))
      opened.plain.fill(0)
    }
  }
  private async copyParts(encoded: Uint8Array, archiveDigest: string) {
    const { db, localKey } = await this.port.ready()
    const parts: string[] = [],
      partSize = 32 * 1024 * 1024
    for (let offset = 0; offset < encoded.length; offset += partSize) {
      ensure(!this.port.paused(), 'LEGACY_PAUSED', '迁移已暂停，可继续同一文件。')
      await this.port.tick()
      const data = encoded.subarray(offset, offset + partSize),
        name = `legacy-${archiveDigest}-${offset / partSize}.cbor`
      let found: EncryptedObject | undefined
      for (const record of (await db.heads()).filter((r) => r.kind === 'migration_part')) {
        const item = await this.port.decode<Item>(record)
        if (item.file?.name === name) {
          ensure(item.file.sha256 === hash(data), 'INTEGRITY_FAILED')
          found = record
          break
        }
      }
      if (!found) {
        let resume: string | undefined
        for (const row of await db.db.getAll('local_private')) {
          if (!row.id.startsWith('file-job:')) continue
          const version = row.id.slice('file-job:'.length)
          const plan = decodeCanonical<{ manifest: { name: string } }>(
            await open(localKey, row.ciphertext, ['dmsg/file-job/1', db.name, version])
          )
          if (plan.manifest.name === name) {
            resume = version
            break
          }
        }
        found = await this.port.importFile(
          new File([new Uint8Array(data)], name, { type: 'application/cbor' }),
          resume
        )
      }
      parts.push(found.key)
    }
    return parts
  }
  async compareFrozen(
    key: string,
    input: Uint8Array,
    trust: Parameters<typeof compareFrozenArchive>[2]
  ) {
    const stored = await this.stored(key)
    try {
      const proofs = legacyUnpack(input) as FrozenArchiveProof[]
      ensure(Array.isArray(proofs) && proofs.length <= 10000, 'INVALID_INPUT')
      const previousParts: Blob[] = []
      for (const part of stored.value.frozenProofParts ?? [])
        previousParts.push((await this.port.downloadFile(part)).blob)
      const previous: FrozenArchiveProof[] = previousParts.length
        ? (legacyUnpack(
            new Uint8Array(await new Blob(previousParts).arrayBuffer())
          ) as FrozenArchiveProof[])
        : []
      const bySource = new Map(previous.map((p) => [p.channel, p]))
      for (const proof of proofs) bySource.set(proof.channel, proof)
      const combined = [...bySource.values()],
        result = await compareFrozenArchive(stored.archive, combined, trust)
      const { db } = await this.port.ready(),
        record = (await db.db.get('objects', key)) as EncryptedObject
      const proofBytes = legacyPack(combined),
        frozenProofParts = await this.copyParts(proofBytes, hash(proofBytes))
      proofBytes.fill(0)
      const saved = await this.port.write(
        {
          ...stored.value,
          frozenComparison: {
            ...result,
            totalChanges: result.changes.length,
            changes: result.changes.slice(0, 64)
          },
          frozenProofParts
        },
        record.id,
        record.revision
      )
      return this.report(saved.key)
    } finally {
      stored.archive.keys.forEach((k) => k.coseKey.fill(0))
      stored.encoded.fill(0)
    }
  }
  async scoped(key: string, source: SharedSource) {
    const stored = await this.stored(key)
    let scoped: Awaited<ReturnType<typeof sharedHistoryArchive>> | undefined
    let encoded: Uint8Array | undefined
    try {
      scoped = await sharedHistoryArchive(stored.archive, source)
      encoded = encodeArchive(scoped)
      const archiveDigest = hash(encoded),
        parts = await this.copyParts(encoded, archiveDigest)
      return { archiveDigest, parts, principal: scoped.inventory.principal }
    } finally {
      stored.encoded.fill(0)
      stored.archive.keys.forEach((k) => k.coseKey.fill(0))
      scoped?.keys.forEach((k) => k.coseKey.fill(0))
      encoded?.fill(0)
    }
  }
  async receiveScoped(encoded: Uint8Array, source: SharedSource, expectedDigest: string) {
    ensure(hash(encoded) === expectedDigest, 'INTEGRITY_FAILED')
    const archive = decodeArchive(encoded)
    try {
      // Re-filtering validates the frozen key wrapper and prevents an authorized
      // channel grant from importing a master key or another channel's history.
      const scoped = await sharedHistoryArchive(archive, source)
      try {
        ensure(
          archive.keys.length === scoped.keys.length &&
            archive.inventory.objects.length === scoped.inventory.objects.length &&
            archive.keys.every((k) => ['channel_kek', 'channel_dek'].includes(k.purpose)),
          'FORBIDDEN'
        )
      } finally {
        scoped.keys.forEach((k) => k.coseKey.fill(0))
      }
      const contentId = archiveContentId(archive),
        { db } = await this.port.ready()
      for (const record of (await db.heads()).filter((r) => r.kind === 'migration')) {
        const value = await this.port.decode<ArchiveRecord>(record)
        if (value.contentId === contentId) return this.report(record.key)
      }
      const parts = await this.copyParts(encoded, expectedDigest),
        checked = await verifyArchiveContent(archive)
      const record = await this.port.write({
        format: 'dmsg-legacy-storage/1',
        archiveDigest: expectedDigest,
        contentId,
        parts,
        principal: archive.inventory.principal,
        mode: archive.inventory.mode,
        objects: archive.inventory.objects.length,
        gaps: checked.gaps.length,
        stage: checked.partial ? 'partial' : 'content_verified',
        snapshot: 'pre_migration'
      } satisfies ArchiveRecord)
      return this.report(record.key)
    } finally {
      archive.keys.forEach((k) => k.coseKey.fill(0))
      encoded.fill(0)
    }
  }
  async list() {
    const { db } = await this.port.ready(),
      list = []
    for (const record of (await db.heads()).filter(
      (r) => r.kind === 'migration' && !r.tombstone
    )) {
      const value = await this.port.decode<ArchiveRecord>(record)
      if (value.format === 'dmsg-legacy-storage/1') list.push({ key: record.key, ...value })
    }
    return list
  }
  private async stored(key: string) {
    const { db, meta } = await this.port.ready(),
      record = (await db.db.get('objects', key)) as EncryptedObject
    ensure(record?.kind === 'migration', 'NOT_FOUND')
    const value = await this.port.decode<ArchiveRecord>(record)
    ensure(
      value.format === 'dmsg-legacy-storage/1' &&
        value.parts.length > 0 &&
        value.parts.length <= 8,
      'UNSUPPORTED_PROTOCOL'
    )
    const chunks: Blob[] = []
    for (const part of value.parts) {
      chunks.push((await this.port.downloadFile(part)).blob)
      await this.port.tick()
    }
    const blob = new Blob(chunks)
    ensure(blob.size <= 256 * 1024 * 1024, 'QUOTA_EXCEEDED')
    const encoded = new Uint8Array(await blob.arrayBuffer())
    ensure(hash(encoded) === value.archiveDigest, 'INTEGRITY_FAILED')
    return { value, archive: decodeArchive(encoded), encoded, meta }
  }
  async file(key: string, source: string) {
    const data = await this.stored(key)
    try {
      const file = await openArchiveFile(data.archive, source),
        plaintext = file.bytes
      const blob = new Blob([new Uint8Array(plaintext)], { type: 'application/octet-stream' })
      plaintext.fill(0)
      return { name: file.name, blob }
    } finally {
      data.archive.keys.forEach((k) => k.coseKey.fill(0))
      data.encoded.fill(0)
    }
  }
  async report(key: string) {
    const { value, archive, encoded, meta } = await this.stored(key)
    try {
      const verification = await verifyArchiveContent(archive)
      const files = archive.inventory.objects
        .filter((o) => o.kind === 'file')
        .map((o) => {
          const file = readObservation(o.bytes)
          return {
            source: o.key,
            name:
              verification.messages.find((m) => m.file?.source === o.key)?.file?.name ??
              String(file.info?.name ?? 'legacy-file'),
            verified: verification.checks.some(
              (c) => c.source === o.key && c.result === 'aead_verified'
            )
          }
        })
      const avatarObject = archive.inventory.objects.find(
        (o) => o.kind === 'avatar' && verification.checks.some((c) => c.source === o.key)
      )
      const avatar = avatarObject ? readObservation(avatarObject.bytes) : null
      const avatarBytes = avatar?.bytes as Uint8Array | undefined
      const starts = (prefix: number[]) =>
        avatarBytes && prefix.every((b, i) => avatarBytes[i] === b)
      const avatarMime = starts([137, 80, 78, 71, 13, 10, 26, 10])
        ? 'image/png'
        : starts([255, 216, 255])
          ? 'image/jpeg'
          : starts([82, 73, 70, 70]) &&
              new TextDecoder().decode(avatarBytes!.slice(8, 12)) === 'WEBP'
            ? 'image/webp'
            : null
      const identity = archive.inventory.objects.find((o) => o.kind === 'identity')
      const profile = archive.inventory.objects.find((o) => o.kind === 'profile')
      const oldUser = identity ? readObservation(identity.bytes) : null
      const oldProfile = profile ? readObservation(profile.bytes) : null
      const resources = archive.inventory.objects
        .filter((o) => o.kind === 'channel')
        .map((o) => {
          const channel = readObservation(o.bytes)
          return {
            source: o.key,
            paid: String(channel.paid ?? ''),
            gas: String(channel.gas ?? '')
          }
        })
      archive.keys.forEach((k) => k.coseKey.fill(0))
      return {
        key,
        principal: value.principal,
        mode: value.mode,
        count: value.objects,
        stage: verification.partial ? 'partial' : 'content_verified',
        snapshot: value.snapshot,
        gaps: verification.gaps,
        messages: verification.messages,
        files,
        profile: {
          name: String(oldUser?.name ?? ''),
          bio: String(oldProfile?.bio ?? ''),
          image: String(oldUser?.image ?? '')
        },
        resources,
        entitlements: archive.inventory.objects
          .filter((o) => o.kind === 'entitlement')
          .map((o) => ({
            source: o.key,
            value: JSON.stringify(readObservation(o.bytes), (_, v) =>
              typeof v === 'bigint' ? v.toString() : v
            )
          })),
        avatar:
          avatarMime && avatarBytes
            ? {
                blob: new Blob([new Uint8Array(avatarBytes)], { type: avatarMime }),
                name: String(avatar.info?.name ?? 'legacy-avatar')
              }
            : null,
        frozen: value.frozenComparison ?? null,
        recovery: meta.restoredFrom
          ? verification.partial
            ? 'partial_verified'
            : 'verified'
          : 'not_checked'
      }
    } finally {
      encoded.fill(0)
    }
  }
}
