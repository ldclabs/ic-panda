import { WorkspaceDB, currentWorkspace } from '../db'
import type { CipherDispatch } from './background'
import { AccountClient } from './account'
import { CloudClient, RelayError, verifyCloudProfile } from './relay'
import {
  canonical,
  decodeCanonical,
  equal,
  hash,
  hex,
  id,
  unb64,
  utf8
} from '../protocol/codec'
import { MAX_CIPHER_CHUNK } from '../config'
import {
  readCloudCommand,
  signCloudHttp,
  signCloudCommand,
  verifyCloudCommand,
  type CloudAction,
  type CloudContext,
  type CloudProfile,
  type CloudSigned
} from '../protocol/cloud'
import {
  readObject,
  uploadPlanSchema,
  storedUploadPlanSchema,
  type ContentJob,
  type ContentUpload,
  type StoredUploadPlan
} from '../protocol/content'
import type { EncryptedObject } from '../models'
import type { CloudRevision } from '../crypto/content'
import { ensure } from '../errors'

type State = Awaited<ReturnType<AccountClient['refresh']>>
export class ContentClient {
  private state: State | null = null
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudClient,
    readonly accountId: string
  ) {}
  private sign = (bytes: Uint8Array) => this.account.crypto.call('contentSign', bytes)
  private base() {
    return `/v1/accounts/${this.accountId}`
  }
  async refresh() {
    this.state = await this.account.refresh(this.accountId)
    await this.cloud.publishSecurity(this.state.verified.evidence)
    return this.state
  }
  private async context(requestId = id()): Promise<CloudContext> {
    if (!this.state || this.state.verified.expiresAt < Date.now() + 10000) await this.refresh()
    return {
      accountId: this.accountId,
      issuer: this.state!.info.issuer,
      deviceId: this.account.meta.deviceId,
      securityEpoch: this.state!.verified.securityEpoch,
      requestId,
      deadline: Math.min(Date.now() + 45000, this.state!.verified.expiresAt)
    }
  }
  private async get(path: string) {
    return this.cloud.get(path, await this.context(), this.sign) as Promise<any>
  }
  private async post(path: string, action: CloudAction, payload: Record<string, unknown>) {
    const context = await this.context()
    return this.cloud.post(
      path,
      await signCloudCommand(context, action, payload, this.sign),
      context,
      this.sign
    ) as Promise<any>
  }
  private verify(signed: CloudSigned, action: CloudAction) {
    ensure(this.state, 'AUTH_REQUIRED')
    const parsed = readCloudCommand(signed)
    const device = this.state.verified.devices.find(
      (d) => hex(Uint8Array.from(d.input.device_id)) === hex(parsed.kid)
    )
    ensure(device, 'INTEGRITY_FAILED', '缺少签名设备的认证记录。')
    return verifyCloudCommand(
      signed,
      { issuer: this.state.info.issuer, deviceId: hex(parsed.kid) },
      Uint8Array.from(device.input.signing_pub),
      action
    )
  }
  async membership() {
    return this.get(`${this.base()}/membership`)
  }
  async refreshMembership() {
    return this.cloud.postRaw(
      `${this.base()}/membership/refresh`,
      null,
      await this.context(),
      this.sign
    )
  }
  async acknowledgeNotice(noticeId: string) {
    return this.post(`${this.base()}/membership/notice-ack`, 'dmsg/membership/notice-ack/v1', {
      notice_id: noticeId
    })
  }
  async quota() {
    return this.get(`${this.base()}/quota`)
  }
  async profile() {
    const value = await this.get(`${this.base()}/profile`)
    const saved = await this.account.crypto.call(
      'controlGet',
      `profile-head:${this.accountId}`
    )
    const known = saved ? JSON.parse(saved) : null
    if (!value) {
      ensure(!known, 'INTEGRITY_FAILED', '中继遗漏已确认的公开资料。')
      return null
    }
    this.verify(value.signed, 'dmsg/profile/v1')
    const parsed = readCloudCommand(value.signed),
      device = this.state!.verified.devices.find(
        (d) => hex(Uint8Array.from(d.input.device_id)) === hex(parsed.kid)
      )!
    const profile = verifyCloudProfile(
      value,
      { ...(await this.context()), deviceId: hex(parsed.kid) },
      Uint8Array.from(device.input.signing_pub)
    )
    ensure(
      !known ||
        (value.version >= known.version &&
          (value.version !== known.version || value.hash === known.hash)),
      'INTEGRITY_FAILED',
      '公开资料版本回退或分叉。'
    )
    await this.account.crypto.call(
      'controlPut',
      `profile-head:${this.accountId}`,
      JSON.stringify({ version: value.version, hash: value.hash })
    )
    return { value, profile }
  }
  async publishProfile(profile: CloudProfile) {
    await this.refresh()
    const key = `profile-job:${this.accountId}`
    const saved = await this.account.crypto.call('controlGet', key)
    let job = saved ? JSON.parse(saved) : null
    if (job)
      ensure(
        equal(canonical(job.payload), canonical(profile)),
        'Pending',
        '先对账上一次公开资料修改，再提交新内容。'
      )
    else job = { requestId: id(), payload: profile }
    const status = await this.get(`${this.base()}/operations/${job.requestId}`).catch(
      (cause) => {
        if (cause instanceof RelayError && cause.code === 'NOT_FOUND') return { found: false }
        throw cause
      }
    )
    let receipt = status.result
    if (!status.found) {
      const context = await this.context(job.requestId)
      if (
        !job.signed ||
        job.deadline <= Date.now() ||
        readCloudCommand(job.signed).body.security_epoch !== context.securityEpoch
      ) {
        job.signed = await signCloudCommand(context, 'dmsg/profile/v1', job.payload, this.sign)
        job.deadline = context.deadline
      }
      await this.account.crypto.call('controlPut', key, JSON.stringify(job))
      receipt = await this.cloud.post(`${this.base()}/profile`, job.signed, context, this.sign)
    }
    ensure(
      receipt?.version === profile.version &&
        receipt.hash === hash(unb64(job.signed.cose_sign1)),
      'INTEGRITY_FAILED'
    )
    const result = await this.profile()
    ensure(result && result.profile.version >= profile.version, 'INTEGRITY_FAILED')
    if (result.profile.version === profile.version)
      ensure(equal(canonical(result.profile), canonical(profile)), 'INTEGRITY_FAILED')
    await this.account.crypto.call('controlPut', key, 'null')
    return result
  }
  async resumeProfile() {
    const saved = await this.account.crypto.call('controlGet', `profile-job:${this.accountId}`)
    const job = saved ? JSON.parse(saved) : null
    return job ? this.publishProfile(job.payload) : this.profile()
  }
  private async upload(upload: ContentUpload) {
    const plan = uploadPlanSchema.parse(upload.plan),
      base = this.base()
    let previous: any = null
    try {
      previous = await this.get(`${base}/uploads/${plan.upload_id}`)
    } catch (cause) {
      if (!(cause instanceof RelayError && cause.code === 'NOT_FOUND')) throw cause
    }
    if (previous)
      ensure(equal(canonical(previous.plan), canonical(plan)), 'IDEMPOTENCY_CONFLICT')
    if (previous?.status === 'committed') return
    ensure(
      !previous || previous.status === 'staging',
      'RESULT_EXPIRED',
      '上传已结束，请先对账原版本。'
    )
    if (!previous || previous.quota_state === 'pending')
      await this.post(`${base}/uploads`, 'dmsg/upload/reserve/v1', plan)
    for (let index = 0; index < plan.chunks.length; index++) {
      const bytes = await this.account.crypto.call('contentChunk', upload, index)
      await this.cloud.putChunk(
        `${base}/uploads/${plan.upload_id}/chunks/${index}`,
        bytes,
        await this.context(),
        this.sign
      )
    }
    await this.cloud.putChunk(
      `${base}/uploads/${plan.upload_id}/chunks/manifest`,
      unb64(upload.manifest),
      await this.context(),
      this.sign
    )
    const result = await this.post(`${base}/uploads/finalize`, 'dmsg/upload/finalize/v1', {
      upload_id: plan.upload_id
    })
    ensure(
      result.upload_id === plan.upload_id &&
        result.digest === plan.manifest_digest &&
        result.object_id === plan.object_id &&
        result.version_id === plan.version_id,
      'INTEGRITY_FAILED'
    )
  }
  private async usableJob(job: ContentJob, generation: number) {
    const replace: string[] = []
    const now = Date.now()
    for (const upload of job.uploads) {
      const plan = upload.plan
      const previous = (await this.get(`${this.base()}/uploads/${plan.upload_id}`).catch(
        (cause) => {
          if (cause instanceof RelayError && cause.code === 'NOT_FOUND') return null
          throw cause
        }
      )) as any
      if (previous)
        ensure(equal(canonical(previous.plan), canonical(plan)), 'IDEMPOTENCY_CONFLICT')
      // A committed historical file remains readable under its retained old
      // root; only the vault revision must use the newly approved root.
      const oldRoot =
        plan.root_generation !== generation &&
        (plan.kind === 'vault' || previous?.status !== 'committed')
      const unavailable =
        previous?.status === 'aborted' ||
        previous?.quota_state === 'released' ||
        (previous?.status !== 'committed' &&
          (plan.expires_at <= now + 300000 ||
            (previous?.lease_expires_at ?? Number.MAX_SAFE_INTEGER) <= now + 300000))
      if (!oldRoot && !unavailable) continue
      if (previous?.status === 'staging')
        await this.post(`${this.base()}/uploads/cancel`, 'dmsg/upload/cancel/v1', {
          upload_id: plan.upload_id
        })
      replace.push(plan.upload_id)
    }
    return replace.length
      ? (this.account.crypto.call(
          'contentReplan',
          job.recordKey,
          generation,
          replace
        ) as Promise<ContentJob>)
      : job
  }
  async push(key: string) {
    await this.context()
    const state = this.state!
    ensure(
      'Ready' in state.info.vault_write_state,
      'REKEY_REQUIRED',
      '账户需要完成换根，请先前往设备与认证处理。'
    )
    let job = (await this.account.crypto.call('contentPrepare', key)) as ContentJob
    if (job.revision.result) {
      await this.account.crypto.call('contentAcknowledge', job)
      return job.revision.result
    }
    const stored = await this.get(`${this.base()}/operations/${job.revision.requestId}`).catch(
      (cause) => {
        if (cause instanceof RelayError && cause.code === 'NOT_FOUND') return { found: false }
        throw cause
      }
    )
    if (stored.found) return this.ack(job, stored.result)
    ensure(state.info.current_root[0], 'REKEY_REQUIRED')
    job = await this.usableJob(job, Number(state.info.current_root[0].generation))
    for (const upload of job.uploads) await this.upload(upload)
    const vault = job.uploads.find((u) => u.plan.kind === 'vault')!,
      record = readObject(unb64(vault.object!))
    const context = await this.context(job.revision.requestId)
    // Immutable IDs/payload are retained. Only expired proof timestamps may be
    // renewed after checking the original operation and each upload status.
    const previous = job.revision.signed
      ? readCloudCommand({ cose_sign1: job.revision.signed })
      : null
    if (
      !previous ||
      (job.revision.deadline ?? 0) <= Date.now() ||
      previous.body.security_epoch !== context.securityEpoch
    ) {
      const payload = await this.revisionPayload(job, record)
      const signed = await signCloudCommand(
        context,
        'dmsg/vault/revision/v1',
        payload,
        this.sign
      )
      job.revision.signed = signed.cose_sign1
      job.revision.deadline = context.deadline
      await this.account.crypto.call('contentSave', job)
    }
    const result = await this.cloud.post(
      `${this.base()}/vault`,
      { cose_sign1: job.revision.signed! },
      context,
      this.sign
    )
    return this.ack(job, result)
  }
  private async revisionPayload(job: ContentJob, record: EncryptedObject) {
    if (job.revision.signed)
      return readCloudCommand({ cose_sign1: job.revision.signed }).body.payload
    let restore = false
    if (record.parent && !record.tombstone) {
      const current = await this.get(`${this.base()}/vault/${record.id}`)
      const body = this.verify(current.signed, 'dmsg/vault/revision/v1')
      restore = body.payload.tombstone === true && body.payload.revision_id === record.parent
    }
    return {
      item_id: record.id,
      revision_id: record.revision,
      base_revision: record.parent,
      upload_id: record.tombstone
        ? null
        : job.uploads.find((u) => u.plan.kind === 'vault')!.plan.upload_id,
      tombstone: record.tombstone,
      restore
    }
  }
  private async ack(job: ContentJob, value: any) {
    const record = readObject(unb64(job.uploads.find((u) => u.plan.kind === 'vault')!.object!))
    ensure(
      value?.revision_id === record.revision &&
        typeof value.conflict === 'boolean' &&
        typeof value.tombstone === 'boolean' &&
        value.tombstone === record.tombstone &&
        /^[0-9a-f]{64}$/.test(value.head),
      'INTEGRITY_FAILED'
    )
    job.revision.result = {
      revision_id: value.revision_id,
      head: value.head,
      conflict: value.conflict,
      tombstone: value.tombstone
    }
    await this.account.crypto.call('contentAcknowledge', job)
    return job.revision.result
  }
  async prepareBackground() {
    const state = await this.refresh()
    ensure('Ready' in state.info.vault_write_state, 'REKEY_REQUIRED')
    const pending = (await this.account.crypto.call('contentPending')).map((j) =>
      decodeCanonical<EncryptedObject>(unb64(j.frame), MAX_CIPHER_CHUNK)
    )
    const pendingIds = new Set(pending.map((record) => record.revision))
    const eligible = pending
      .filter(
        (r) =>
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
          ].includes(r.kind) && !pendingIds.has(r.parent ?? '')
      )
      .slice(0, 25)
    ensure(state.info.current_root[0], 'REKEY_REQUIRED')
    let queued = 0
    for (const record of eligible) {
      let job = (await this.account.crypto.call('contentPrepare', record.key)) as ContentJob
      if (job.revision.result) {
        await this.account.crypto.call('contentAcknowledge', job)
        continue
      }
      const stored = await this.get(
        `${this.base()}/operations/${job.revision.requestId}`
      ).catch((cause) => {
        if (cause instanceof RelayError && cause.code === 'NOT_FOUND') return { found: false }
        throw cause
      })
      if (stored.found) {
        await this.ack(job, stored.result)
        continue
      }
      job = await this.usableJob(job, Number(state.info.current_root[0].generation))
      for (const upload of job.uploads) await this.upload(upload)
      const context = await this.context(job.revision.requestId)
      const payload = await this.revisionPayload(job, record)
      const signed = await signCloudCommand(
        context,
        'dmsg/vault/revision/v1',
        payload,
        this.sign
      )
      job.revision.signed = signed.cose_sign1
      job.revision.deadline = context.deadline
      await this.account.crypto.call('contentSave', job)
      const body = canonical(signed),
        base = this.base()
      const dispatch: CipherDispatch = {
        id: job.revision.requestId,
        format: 'dmsg-cipher-dispatch/1',
        account: this.accountId,
        origin: this.cloud.origin,
        deadline: context.deadline,
        state: 'queued',
        attempts: 0,
        recordKey: record.key,
        requestId: context.requestId,
        revision: record.revision,
        tombstone: record.tombstone,
        command: signed.cose_sign1,
        postProof: await signCloudHttp(
          context,
          new URL(`${base}/vault`, this.cloud.origin),
          'POST',
          body,
          this.sign
        ),
        statusProof: await signCloudHttp(
          context,
          new URL(`${base}/operations/${context.requestId}`, this.cloud.origin),
          'GET',
          new Uint8Array(),
          this.sign
        )
      }
      const name = await currentWorkspace()
      ensure(name, 'LOCKED')
      const db = await WorkspaceDB.open(name)
      try {
        await db.db.put('meta', { id: `dispatch:${dispatch.id}`, value: dispatch })
      } finally {
        db.db.close()
      }
      queued++
    }
    if (typeof chrome !== 'undefined' && chrome.runtime?.id)
      await chrome.runtime.sendMessage({ type: 'dmsg-flush-ciphertext' })
    return queued
  }
  async pushPending() {
    const pending = (await this.account.crypto.call('contentPending'))
      .map((j) => decodeCanonical<EncryptedObject>(unb64(j.frame), MAX_CIPHER_CHUNK))
      .filter((r) =>
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
        ].includes(r.kind)
      )
    const ids = new Set(pending.map((record) => record.revision)),
      children = new Map<string, EncryptedObject[]>(),
      ordered = pending.filter((record) => !record.parent || !ids.has(record.parent))
    for (const record of pending) {
      if (record.parent && ids.has(record.parent)) {
        const list = children.get(record.parent) ?? []
        list.push(record)
        children.set(record.parent, list)
      }
    }
    for (let index = 0; index < ordered.length; index++)
      ordered.push(...(children.get(ordered[index].revision) ?? []))
    ensure(ordered.length === pending.length, 'INTEGRITY_FAILED', '本地版本存在循环依赖。')
    for (const record of ordered) await this.push(record.key)
    return ordered.length
  }
  async pull() {
    await this.refresh()
    const start = await this.post(`${this.base()}/exports`, 'dmsg/export/start/v1', {
      export_id: id()
    })
    ensure(
      start.account === this.accountId &&
        Number.isSafeInteger(start.through) &&
        start.scope === 'cloud_snapshot',
      'INTEGRITY_FAILED'
    )
    const objects: StoredUploadPlan[] = [],
      revisions: CloudRevision[] = [],
      evidence: unknown[] = [start]
    const counts = new Map<string, number>()
    let cursor = 0,
      evidenceBytes = utf8(JSON.stringify(start)).length
    for (;;) {
      const page = await this.get(
        `${this.base()}/exports/${start.export_id}?after=${cursor}&limit=100`
      )
      ensure(
        Array.isArray(page.entries) &&
          page.entries.length <= 100 &&
          typeof page.inventory_end === 'boolean',
        'INTEGRITY_FAILED'
      )
      for (const entry of page.entries) {
        ensure(
          Number.isSafeInteger(entry.cursor) &&
            entry.cursor > cursor &&
            entry.cursor <= start.through,
          'INTEGRITY_FAILED'
        )
        cursor = entry.cursor
        counts.set(entry.kind, (counts.get(entry.kind) ?? 0) + 1)
        evidenceBytes += utf8(JSON.stringify(entry)).length + 1
        ensure(evidenceBytes <= 8 * 1024 * 1024, 'QUOTA_EXCEEDED')
        evidence.push(entry)
        if (entry.kind === 'object') {
          ensure(!entry.missing_chunks?.length, 'RECOVERY_INCOMPLETE', '云端快照有缺块。')
          const plan = storedUploadPlanSchema.parse(entry.plan),
            signed = this.verify(entry.signature, 'dmsg/upload/reserve/v1')
          ensure(
            equal(canonical(signed.payload), canonical(plan)) &&
              entry.value.upload_id === plan.upload_id &&
              entry.value.digest === plan.manifest_digest,
            'INTEGRITY_FAILED'
          )
          let missing = await this.account.crypto.call('contentMissing', plan)
          if (!missing.manifest) {
            const manifest = await this.cloud.getChunk(
              `${this.base()}/objects/${plan.upload_id}/chunks/manifest`,
              plan.manifest_digest,
              await this.context(),
              this.sign
            )
            await this.account.crypto.call('contentCache', plan, 'manifest', manifest)
            missing = await this.account.crypto.call('contentMissing', plan)
          }
          for (const index of missing.chunks) {
            const data = await this.cloud.getChunk(
              `${this.base()}/objects/${plan.upload_id}/chunks/${index}`,
              plan.chunks[index].digest,
              await this.context(),
              this.sign
            )
            await this.account.crypto.call('contentCache', plan, index, data)
          }
          objects.push(plan)
          ensure(objects.length <= 10000, 'QUOTA_EXCEEDED')
        } else if (entry.kind === 'revision') {
          const signed = this.verify(entry.value.signed, 'dmsg/vault/revision/v1')
          const payload = signed.payload as unknown as CloudRevision
          for (const field of [
            'item_id',
            'revision_id',
            'base_revision',
            'upload_id',
            'tombstone',
            'restore'
          ])
            ensure(
              equal(canonical(signed.payload[field]), canonical(entry.value[field])),
              'INTEGRITY_FAILED'
            )
          ensure(typeof entry.value.conflict === 'boolean', 'INTEGRITY_FAILED')
          revisions.push({ ...payload, conflict: entry.value.conflict })
          ensure(revisions.length <= 100000, 'QUOTA_EXCEEDED')
        }
      }
      ensure(
        page.next_cursor === cursor && (page.inventory_end || page.entries.length > 0),
        'INTEGRITY_FAILED',
        '导出游标没有推进。'
      )
      if (page.inventory_end) break
    }
    for (const count of start.counts)
      ensure(counts.get(count.kind) === count.n, 'RECOVERY_INCOMPLETE', '云端快照数量不一致。')
    return this.account.crypto.call('contentReceive', {
      objects,
      revisions,
      through: start.through,
      evidence: JSON.stringify(evidence)
    })
  }
}
