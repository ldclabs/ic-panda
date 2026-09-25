import {
  legacyHistoryScopeSchema,
  type LegacyHistoryScope,
  type LegacyHistoryGrant
} from '../protocol/shared-history'
import { emptyFile } from '../protocol/content'
import { ensure } from '../errors'
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
  unhex
} from '../protocol/codec'
import {
  activationSchema,
  channelMessageSchema,
  recipientDigest,
  type Activation,
  type ChannelLedger,
  type EpochRecipient,
  type RotationLease
} from '../protocol/channel'
import { derive, hpkeOpen, hpkeSeal, open, seal } from './primitives'
import { MAX_FORMAL_OBJECT_BYTES } from '../config'
import type {
  EncryptedObject,
  ObjectKind,
  WorkspaceMeta,
  FileManifest,
  Item,
  Chunk,
  Lease
} from '../models'
import { prefixRange, type WorkspaceDB } from '../db'

export interface ChannelUpload {
  plan: {
    upload_id: string
    object_id: string
    version_id: string
    root_generation: number
    epoch: number
    control_head: string
    kind: 'envelopes' | 'file'
    chunks: { digest: string; size: number }[]
    manifest_digest: string
    manifest_size: number
    expires_at: number
  }
  chunks: string[]
  manifest: string
  recipients: string[]
  fileRecord?: string
}
interface PrivateChannel {
  format: 'dmsg-channel-client/1'
  id: string
  name: string
  genesis: string
  ledger: ChannelLedger | null
  keys: Record<string, string>
  candidate?: { lease: RotationLease; key: string; groups: string[] }
}
// Relay positions are local routing metadata, not synchronized content.
interface Cursors {
  control: number
  message: number
}
interface ChannelFileRef {
  upload_id: string
  file_id: string
  version: string
  epoch: number
  manifest_digest: string
  name: string
  size: number
  mime: string
  key: string
}
interface PrivateChannelFile {
  sourceRecord?: string
  channel: string
  reference: ChannelFileRef
  manifest: FileManifest
  upload?: ChannelUpload
}
interface ChannelPort {
  ready(): Promise<{
    db: WorkspaceDB
    meta: WorkspaceMeta
    bundle: { hpke: string }
    localKey: Uint8Array
    lease: Lease
  }>
  decode<T>(record: EncryptedObject): Promise<T>
  write(
    kind: ObjectKind,
    value: unknown,
    id?: string,
    base?: string | null
  ): Promise<EncryptedObject>
  cacheFile(
    manifest: FileManifest,
    chunks: Map<string, Chunk>,
    hidden?: boolean
  ): Promise<{ blob: Blob; name: string; key: string }>
  tick(): Promise<unknown>
}
const envelopeContext = (
  channel: string,
  activation: Pick<
    Activation,
    'epoch' | 'lease_id' | 'fencing' | 'expected_head' | 'recipients_digest'
  >,
  recipient: string
) => [
  'dmsg/channel-epoch-envelope/1',
  channel,
  activation.epoch,
  activation.lease_id,
  activation.fencing,
  activation.expected_head,
  activation.recipients_digest,
  recipient
]
const confirmation = (channel: string, epoch: number, key: Uint8Array) =>
  hex(digest('dmsg/channel-epoch-confirmation/v1', [channel, epoch, key]))
export class ChannelVault {
  constructor(private port: ChannelPort) {}
  private async load(channel: string) {
    ensure(/^[0-9a-f]{64}$/.test(channel), 'INVALID_INPUT')
    const { db } = await this.port.ready(),
      record = await db.getHead(channel, 'formal_channel')
    ensure(record, 'NOT_FOUND')
    const state = await this.port.decode<PrivateChannel>(record)
    ensure(
      state.format === 'dmsg-channel-client/1' && state.id === channel,
      'INTEGRITY_FAILED'
    )
    return { record, state }
  }
  private async store(record: EncryptedObject, state: PrivateChannel) {
    if (equal(canonical(await this.port.decode(record)), canonical(state))) return record
    return this.port.write('formal_channel', state, record.id, record.revision)
  }
  private async cursors(channel: string): Promise<Cursors> {
    const { db } = await this.port.ready(),
      row = await db.db.get('inbox_cursors', channel)
    return { control: row?.control ?? 0, message: row?.message ?? 0 }
  }
  private async saveCursors(channel: string, cursors: Cursors) {
    const { db, lease } = await this.port.ready()
    await db.guardedPut('inbox_cursors', { id: channel, ...cursors }, lease)
  }
  private async summary(state: PrivateChannel) {
    const cursors = await this.cursors(state.id)
    return {
      id: state.id,
      name: state.name,
      genesis: state.genesis,
      ledger: state.ledger,
      controlCursor: cursors.control,
      messageCursor: cursors.message,
      readableEpochs: Object.keys(state.keys).map(Number)
    }
  }
  async remember(input: {
    channel: string
    name: string
    genesis: string
    replaceUncommitted?: boolean
  }) {
    ensure(
      /^[0-9a-f]{64}$/.test(input.channel) &&
        /^[0-9a-f]{64}$/.test(input.genesis) &&
        input.name.length <= 100,
      'INVALID_INPUT'
    )
    const { db } = await this.port.ready(),
      old = await db.getHead(input.channel, 'formal_channel')
    if (old) {
      const state = await this.port.decode<PrivateChannel>(old)
      if (state.genesis !== input.genesis) {
        ensure(input.replaceUncommitted && !state.ledger, 'UNVERIFIED_HEAD')
        state.genesis = input.genesis
        await this.store(old, state)
      }
      return
    }
    await this.port.write(
      'formal_channel',
      {
        format: 'dmsg-channel-client/1',
        id: input.channel,
        name: input.name,
        genesis: input.genesis,
        ledger: null,
        keys: {}
      } satisfies PrivateChannel,
      input.channel
    )
  }
  async list() {
    const { db } = await this.port.ready(),
      result = []
    for (const record of await db.heads('formal_channel'))
      result.push(await this.summary(await this.port.decode<PrivateChannel>(record)))
    return result
  }
  async get(channel: string) {
    return this.summary((await this.load(channel)).state)
  }
  async advance(channel: string, ledger: ChannelLedger, cursor: number, events: unknown[]) {
    const { record, state } = await this.load(channel),
      cursors = await this.cursors(channel)
    ensure(
      ledger.channel_id === channel &&
        cursor >= cursors.control &&
        (!state.ledger || ledger.control_seq >= state.ledger.control_seq),
      'UNVERIFIED_HEAD'
    )
    for (const event of events as any[]) {
      const objectId = hash(
        canonical(['dmsg/channel-control-record/1', channel, event.value.control_seq])
      )
      await this.immutable('formal_control', objectId, { channel, ...event })
    }
    state.ledger = ledger
    await this.store(record, state)
    if (cursor !== cursors.control)
      await this.saveCursors(channel, { ...cursors, control: cursor })
  }
  private async immutable(kind: ObjectKind, objectId: string, value: unknown) {
    const { db } = await this.port.ready(),
      existing = await db.getHead(objectId, kind)
    if (existing) {
      ensure(
        equal(canonical(await this.port.decode(existing)), canonical(value)),
        'IDEMPOTENCY_CONFLICT'
      )
      return existing
    }
    return this.port.write(kind, value, objectId)
  }
  /** Device-local retry journals, overwritten in place under LocalDataKey. */
  async job(channel: string, key: string, value?: unknown) {
    ensure(/^[0-9a-f]{64}$/.test(channel) && key.length <= 400, 'INVALID_INPUT')
    const { db, localKey, lease } = await this.port.ready(),
      id = `channel-job:${channel}:${key}`,
      aad = ['dmsg/channel-job/1', db.name, id]
    if (value === undefined) {
      const row = await db.db.get('local_private', id)
      return row
        ? decodeCanonical<any>(
            await open(localKey, row.ciphertext, aad),
            MAX_FORMAL_OBJECT_BYTES
          )
        : null
    }
    const encoded = canonical(value)
    ensure(encoded.length <= 450000, 'QUOTA_EXCEEDED')
    await db.guardedPut(
      'local_private',
      { id, ciphertext: await seal(localKey, encoded, aad) },
      lease
    )
    return value
  }
  async rotation(channel: string, lease: RotationLease, recipients: EpochRecipient[]) {
    const { record, state } = await this.load(channel),
      { meta } = await this.port.ready()
    ensure(
      state.ledger &&
        state.ledger.head === lease.head &&
        state.ledger.epoch + 1 === lease.epoch &&
        state.ledger.status !== 'archived' &&
        lease.account === meta.account?.id &&
        lease.device === meta.deviceId &&
        lease.expires_at > Date.now() &&
        recipients.length > 0 &&
        recipients.length <= 600 &&
        new Set(recipients.map((r) => r.recipient)).size === recipients.length &&
        recipientDigest(recipients) === lease.recipients_digest,
      'EPOCH_ROTATING'
    )
    let key: Uint8Array
    if (state.candidate?.lease.id === lease.id) {
      ensure(equal(canonical(state.candidate.lease), canonical(lease)), 'IDEMPOTENCY_CONFLICT')
      key = unb64(state.candidate.key)
    } else {
      key = random()
      state.candidate = { lease, key: b64(key), groups: [] }
      await this.store(record, state)
    }
    const activation: Activation = {
      lease_id: lease.id,
      fencing: lease.fencing,
      expected_head: lease.head,
      epoch: lease.epoch,
      recipients_digest: lease.recipients_digest,
      manifest_upload: '',
      key_confirmation: confirmation(channel, lease.epoch, key),
      envelopes: []
    }
    const uploads: ChannelUpload[] = []
    try {
      for (let offset = 0; offset < recipients.length; offset += 120) {
        const groupKey = `rotation:${lease.id}:${offset}`
        let group = (await this.job(channel, groupKey)) as ChannelUpload | null
        if (!group) {
          const selected = recipients.slice(offset, offset + 120),
            chunks: string[] = []
          for (const recipient of selected) {
            ensure(
              /^(device|recovery):/.test(recipient.recipient) &&
                unhex(recipient.hpke_pub).length === 32,
              'INTEGRITY_FAILED'
            )
            chunks.push(
              b64(
                canonical(
                  await hpkeSeal(
                    b64(unhex(recipient.hpke_pub)),
                    key,
                    envelopeContext(channel, activation, recipient.recipient)
                  )
                )
              )
            )
            if (chunks.length % 32 === 0) await this.port.tick()
          }
          const manifest = b64(
            canonical({
              format: 'dmsg-channel-epoch/1',
              channel,
              epoch: lease.epoch,
              lease_id: lease.id,
              head: lease.head,
              fencing: lease.fencing,
              recipients_digest: lease.recipients_digest,
              key_confirmation: activation.key_confirmation,
              group: offset
            })
          )
          group = {
            plan: {
              upload_id: id(),
              object_id: channel,
              version_id: id(),
              root_generation: 0,
              epoch: lease.epoch,
              control_head: lease.head,
              kind: 'envelopes',
              chunks: chunks.map((value) => ({
                digest: hash(unb64(value)),
                size: unb64(value).length
              })),
              manifest_digest: hash(unb64(manifest)),
              manifest_size: unb64(manifest).length,
              expires_at: Date.now() + 3600000
            },
            chunks,
            manifest,
            recipients: selected.map((r) => r.recipient)
          }
          await this.job(channel, groupKey, group)
        }
        uploads.push(group)
        for (const [chunk, recipient] of group.recipients.entries())
          activation.envelopes.push({
            recipient,
            upload_id: group.plan.upload_id,
            chunk,
            ...group.plan.chunks[chunk]
          })
      }
      activation.manifest_upload = uploads[0].plan.upload_id
      activationSchema.parse(activation)
      return { activation, uploads }
    } finally {
      key.fill(0)
    }
  }
  async history(
    channel: string,
    target: string,
    from: number,
    to: number,
    recipients: EpochRecipient[],
    grantId: string
  ) {
    const { state } = await this.load(channel),
      { meta } = await this.port.ready(),
      ledger = state.ledger
    ensure(
      ledger?.status === 'active' &&
        ['owner', 'admin'].includes(ledger.members[meta.account!.id]?.role) &&
        ledger.members[target] &&
        Number.isSafeInteger(from) &&
        Number.isSafeInteger(to) &&
        from >= 1 &&
        to >= from &&
        to <= ledger.epoch &&
        to - from < 1000 &&
        recipients.length > 0 &&
        recipients.length <= 6,
      'FORBIDDEN'
    )
    for (let epoch = from; epoch <= to; epoch++)
      ensure(state.keys[epoch], 'RECOVERY_INCOMPLETE')
    for (const recipient of recipients)
      ensure(
        recipient.recipient.startsWith(`device:${target}:`) ||
          recipient.recipient.startsWith(`recovery:${target}:`),
        'FORBIDDEN'
      )
    const scope = {
      channel,
      target,
      from,
      to,
      grant_id: grantId,
      head: ledger.head,
      epoch: ledger.epoch,
      recipients_digest: recipientDigest(recipients)
    }
    const existing = await this.job(channel, `history-plan:${grantId}`)
    if (existing) {
      ensure(equal(canonical(existing.scope), canonical(scope)), 'IDEMPOTENCY_CONFLICT')
      const uploads: ChannelUpload[] = []
      for (let i = 0; i < existing.groups; i++)
        uploads.push(await this.job(channel, `history-group:${grantId}:${i}`))
      return { scope, uploads }
    }
    const chunks: string[] = [],
      refs: {
        recipient: string
        from: number
        to: number
        upload_id: string
        chunk: number
        digest: string
        size: number
      }[] = []
    const groups = Array.from(
      { length: Math.ceil((recipients.length * Math.ceil((to - from + 1) / 32)) / 120) },
      () => id()
    )
    for (const recipient of recipients)
      for (let start = from; start <= to; start += 32) {
        const end = Math.min(start + 31, to),
          keys: [number, Uint8Array][] = []
        for (let epoch = start; epoch <= end; epoch++)
          keys.push([epoch, unb64(state.keys[epoch])])
        const plaintext = canonical(keys)
        try {
          const context = [
            'dmsg/channel-history-envelope/1',
            scope,
            recipient.recipient,
            start,
            end
          ]
          const ciphertext = b64(
            canonical(await hpkeSeal(b64(unhex(recipient.hpke_pub)), plaintext, context))
          )
          ensure(unb64(ciphertext).length <= 2048, 'QUOTA_EXCEEDED')
          const index = chunks.length
          refs.push({
            recipient: recipient.recipient,
            from: start,
            to: end,
            upload_id: groups[Math.floor(index / 120)],
            chunk: index % 120,
            digest: hash(unb64(ciphertext)),
            size: unb64(ciphertext).length
          })
          chunks.push(ciphertext)
        } finally {
          plaintext.fill(0)
          keys.forEach(([, key]) => key.fill(0))
        }
        await this.port.tick()
      }
    const manifest = b64(
      canonical({ format: 'dmsg-channel-history/1', scope, envelopes: refs })
    )
    ensure(unb64(manifest).length <= 65536, 'QUOTA_EXCEEDED')
    const uploads: ChannelUpload[] = []
    for (let group = 0; group < groups.length; group++) {
      const bytes = chunks.slice(group * 120, (group + 1) * 120)
      const upload: ChannelUpload = {
        plan: {
          upload_id: groups[group],
          object_id: channel,
          version_id: id(),
          root_generation: 0,
          epoch: ledger.epoch,
          control_head: ledger.head,
          kind: 'envelopes',
          chunks: bytes.map((value) => ({
            digest: hash(unb64(value)),
            size: unb64(value).length
          })),
          manifest_digest: hash(unb64(manifest)),
          manifest_size: unb64(manifest).length,
          expires_at: Date.now() + 3600000
        },
        chunks: bytes,
        manifest,
        recipients: refs.slice(group * 120, (group + 1) * 120).map((r) => r.recipient)
      }
      await this.job(channel, `history-group:${grantId}:${group}`, upload)
      uploads.push(upload)
    }
    await this.job(channel, `history-plan:${grantId}`, { scope, groups: groups.length })
    return { scope, uploads }
  }
  async historyInstall(
    channel: string,
    scope: any,
    references: {
      recipient: string
      from: number
      to: number
      digest: string
      size: number
      bytes: Uint8Array
    }[],
    confirmations: [number, string][]
  ) {
    const { record, state } = await this.load(channel),
      { meta, bundle } = await this.port.ready()
    ensure(
      scope.channel === channel &&
        scope.target === meta.account?.id &&
        Number.isSafeInteger(scope.from) &&
        Number.isSafeInteger(scope.to) &&
        scope.to - scope.from < 1000 &&
        scope.from > 0,
      'FORBIDDEN'
    )
    const recipient = `device:${meta.account!.id}:${meta.deviceId}`,
      pending: Record<string, string> = {},
      checked = new Map(confirmations)
    for (const reference of references) {
      ensure(
        reference.recipient === recipient &&
          reference.size === reference.bytes.length &&
          hash(reference.bytes) === reference.digest &&
          reference.from >= scope.from &&
          reference.to <= scope.to &&
          reference.to - reference.from < 32,
        'INTEGRITY_FAILED'
      )
      const context = [
        'dmsg/channel-history-envelope/1',
        scope,
        recipient,
        reference.from,
        reference.to
      ]
      const plaintext = await hpkeOpen(
        unb64(bundle.hpke),
        decodeCanonical(reference.bytes),
        context
      )
      try {
        const keys = decodeCanonical<[number, Uint8Array][]>(plaintext)
        ensure(keys.length === reference.to - reference.from + 1, 'INTEGRITY_FAILED')
        for (const [index, [epoch, key]] of keys.entries()) {
          ensure(
            epoch === reference.from + index &&
              key.length === 32 &&
              confirmation(channel, epoch, key) === checked.get(epoch) &&
              !pending[epoch],
            'INTEGRITY_FAILED'
          )
          if (state.keys[epoch])
            ensure(equal(unb64(state.keys[epoch]), key), 'IDEMPOTENCY_CONFLICT')
          pending[epoch] = b64(key)
          key.fill(0)
        }
      } finally {
        plaintext.fill(0)
      }
    }
    ensure(Object.keys(pending).length === scope.to - scope.from + 1, 'RECOVERY_INCOMPLETE')
    state.keys = { ...state.keys, ...pending }
    await this.store(record, state)
  }
  async install(channel: string, activation: Activation, envelope: Uint8Array) {
    activationSchema.parse(activation)
    const { record, state } = await this.load(channel),
      { meta, bundle } = await this.port.ready()
    ensure(state.ledger && activation.epoch <= state.ledger.epoch, 'UNVERIFIED_HEAD')
    const recipient = `device:${meta.account?.id}:${meta.deviceId}`,
      reference = activation.envelopes.find((e) => e.recipient === recipient)
    ensure(
      reference && reference.size === envelope.length && reference.digest === hash(envelope),
      'FORBIDDEN'
    )
    const key = await hpkeOpen(
      unb64(bundle.hpke),
      decodeCanonical(envelope),
      envelopeContext(channel, activation, recipient)
    )
    try {
      ensure(
        key.length === 32 &&
          confirmation(channel, activation.epoch, key) === activation.key_confirmation,
        'INTEGRITY_FAILED'
      )
      if (state.keys[activation.epoch])
        ensure(equal(unb64(state.keys[activation.epoch]), key), 'IDEMPOTENCY_CONFLICT')
      else {
        state.keys[activation.epoch] = b64(key)
        await this.store(record, state)
      }
    } finally {
      key.fill(0)
    }
  }
  private async attachment(channel: string, messageId: string) {
    const { db } = await this.port.ready(),
      objectId = hash(canonical(['dmsg/channel-file-source/1', channel, messageId]))
    const record = await db.getHead(objectId, 'formal_file')
    return record
      ? { record, value: await this.port.decode<PrivateChannelFile>(record) }
      : null
  }
  async legacyGrant(
    scope: LegacyHistoryScope,
    fileRecords: string[],
    recipients: EpochRecipient[]
  ): Promise<LegacyHistoryGrant> {
    legacyHistoryScopeSchema.parse(scope)
    const { db } = await this.port.ready(),
      existing = await this.job(scope.channel_id, `legacy-grant:${scope.grant_id}`)
    if (existing) {
      ensure(equal(canonical(existing.scope), canonical(scope)), 'IDEMPOTENCY_CONFLICT')
      return existing
    }
    ensure(
      fileRecords.length > 0 &&
        fileRecords.length <= 8 &&
        recipients.length === 2 &&
        new Set(recipients.map((r) => r.recipient)).size === 2 &&
        recipients.some((r) => r.recipient === `device:${scope.account}:${scope.device}`) &&
        recipients.some(
          (r) => r.recipient === `recovery:${scope.account}:${scope.recovery_generation}`
        ),
      'FORBIDDEN'
    )
    const files: ChannelFileRef[] = []
    for (const key of fileRecords) {
      const record = (await db.db.get('objects', key)) as EncryptedObject | undefined
      ensure(record?.kind === 'formal_file', 'NOT_FOUND')
      const file = await this.port.decode<PrivateChannelFile>(record)
      ensure(
        file.channel === scope.channel_id && file.reference.epoch === scope.epoch,
        'INTEGRITY_FAILED'
      )
      files.push(file.reference)
    }
    const key = random()
    try {
      const manifest = await seal(key, canonical(files), [
        'dmsg/legacy-history-grant/1',
        scope
      ])
      const wrapped = []
      for (const recipient of recipients)
        wrapped.push({
          recipient: recipient.recipient,
          envelope: await hpkeSeal(b64(unhex(recipient.hpke_pub)), key, [
            'dmsg/legacy-history-grant-key/1',
            scope,
            recipient.recipient
          ])
        })
      const grant: LegacyHistoryGrant = {
        scope,
        manifest,
        recipients: wrapped,
        parts: files.map((f) => ({
          upload_id: f.upload_id,
          manifest_digest: f.manifest_digest
        }))
      }
      await this.job(scope.channel_id, `legacy-grant:${scope.grant_id}`, grant)
      return grant
    } finally {
      key.fill(0)
    }
  }
  async openLegacyGrant(grant: LegacyHistoryGrant, recoverySeed?: Uint8Array) {
    const scope = legacyHistoryScopeSchema.parse(grant.scope),
      { meta, bundle } = await this.port.ready()
    ensure(
      meta.account?.id === scope.account && (recoverySeed || meta.deviceId === scope.device),
      'FORBIDDEN'
    )
    const recipient = recoverySeed
        ? `recovery:${scope.account}:${scope.recovery_generation}`
        : `device:${scope.account}:${scope.device}`,
      entries = grant.recipients.filter((r) => r.recipient === recipient)
    ensure(entries.length === 1, 'INTEGRITY_FAILED')
    const key = await hpkeOpen(recoverySeed ?? unb64(bundle.hpke), entries[0].envelope, [
      'dmsg/legacy-history-grant-key/1',
      scope,
      recipient
    ])
    try {
      const files = decodeCanonical<ChannelFileRef[]>(
        await open(key, grant.manifest, ['dmsg/legacy-history-grant/1', scope])
      )
      ensure(
        files.length > 0 &&
          files.length <= 8 &&
          files.length === grant.parts.length &&
          files.every(
            (f, i) =>
              f.epoch === scope.epoch &&
              f.upload_id === grant.parts[i].upload_id &&
              f.manifest_digest === grant.parts[i].manifest_digest &&
              unb64(f.key).length === 32
          ),
        'INTEGRITY_FAILED'
      )
      const objectId = hash(
        canonical(['dmsg/received-legacy-grant/1', scope.directory_key, scope.grant_id])
      )
      const { db } = await this.port.ready()
      const existing = await db.getHead(objectId, 'formal_file')
      if (existing) {
        const saved = await this.port.decode<any>(existing)
        // Download progress adds manifests to this record. Reopening the same
        // device/recovery envelope compares its immutable scope and keys only.
        ensure(
          saved.channel === scope.channel_id &&
            equal(canonical(saved.legacyGrant.scope), canonical(scope)) &&
            equal(canonical(saved.legacyGrant.files), canonical(files)),
          'IDEMPOTENCY_CONFLICT'
        )
      } else
        await this.immutable('formal_file', objectId, {
          channel: scope.channel_id,
          legacyGrant: { scope, files }
        })
      return { recordId: objectId, files: files.map((f) => ({ ...f, key: '' })) }
    } finally {
      key.fill(0)
    }
  }
  async legacyGrantManifest(recordId: string, index: number, ciphertext: Uint8Array) {
    const { db } = await this.port.ready(),
      record = await db.getHead(recordId, 'formal_file')
    ensure(record, 'NOT_FOUND')
    const { legacyGrant } = await this.port.decode<any>(record),
      ref = legacyGrant.files[index] as ChannelFileRef | undefined
    ensure(ref && hash(ciphertext) === ref.manifest_digest, 'INTEGRITY_FAILED')
    const value = decodeCanonical<any>(
      await open(unb64(ref.key), b64(ciphertext), [
        'dmsg/channel-file-manifest/1',
        legacyGrant.scope.channel_id,
        ref.epoch,
        ref.upload_id,
        ref.file_id,
        ref.version
      ])
    )
    ensure(
      value.format === 'dmsg-channel-file/1' &&
        value.channel === legacyGrant.scope.channel_id &&
        value.epoch === ref.epoch &&
        value.file.id === ref.file_id &&
        value.file.version === ref.version &&
        value.file.key === '',
      'INTEGRITY_FAILED'
    )
    legacyGrant.manifests ??= {}
    if (legacyGrant.manifests[index]) {
      ensure(
        equal(
          canonical(legacyGrant.manifests[index]),
          canonical({ ...value.file, key: ref.key })
        ),
        'IDEMPOTENCY_CONFLICT'
      )
      return { ...value.file, key: '' }
    }
    legacyGrant.manifests[index] = { ...value.file, key: ref.key }
    await this.port.write(
      'formal_file',
      { channel: legacyGrant.scope.channel_id, legacyGrant },
      record.id,
      record.revision
    )
    return { ...value.file, key: '' }
  }
  async legacyGrantPart(recordId: string, index: number, ciphertext: Uint8Array[]) {
    const { db } = await this.port.ready(),
      record = await db.getHead(recordId, 'formal_file')
    ensure(record, 'NOT_FOUND')
    const { legacyGrant } = await this.port.decode<any>(record),
      manifest = legacyGrant.manifests?.[index] as FileManifest | undefined
    ensure(manifest && manifest.chunks.length === ciphertext.length, 'INTEGRITY_FAILED')
    const chunks = new Map<string, Chunk>()
    for (const [i, bytes] of ciphertext.entries()) {
      const ref = manifest.chunks[i]
      ensure(hash(bytes) === ref.digest, 'INTEGRITY_FAILED')
      chunks.set(ref.id, { id: ref.id, digest: ref.digest, ciphertext: b64(bytes) })
    }
    const cached = await this.port.cacheFile(manifest, chunks, true)
    return { key: cached.key }
  }
  async filePrepare(channel: string, messageId: string, objectKey: string) {
    const existing = await this.attachment(channel, messageId)
    if (existing?.value.upload) {
      ensure(existing.value.sourceRecord === objectKey, 'IDEMPOTENCY_CONFLICT')
      return { ...existing.value.upload, fileRecord: existing.record.key }
    }
    const { state } = await this.load(channel),
      { db } = await this.port.ready()
    ensure(state.ledger?.status === 'active', 'EPOCH_ROTATING')
    const record = (await db.db.get('objects', objectKey)) as EncryptedObject | undefined
    ensure(record?.kind === 'vault' || record?.kind === 'migration_part', 'NOT_FOUND')
    const item = await this.port.decode<Item>(record),
      file = item.file
    ensure(file && file.size <= 100 * 1024 * 1024, 'INVALID_INPUT', '请选择已完整保存的文件。')
    const chunks = []
    for (const ref of file.chunks) {
      const chunk = (await db.db.get('chunks', ref.id)) as Chunk | undefined
      ensure(chunk && hash(unb64(chunk.ciphertext)) === ref.digest, 'INTEGRITY_FAILED')
      chunks.push({ digest: ref.digest, size: unb64(chunk.ciphertext).length })
    }
    if (!chunks.length) chunks.push({ digest: hash(emptyFile()), size: emptyFile().length })
    const uploadId = id(),
      epoch = state.ledger.epoch,
      key = unb64(file.key)
    const manifest = await seal(
      key,
      canonical({ format: 'dmsg-channel-file/1', channel, epoch, file: { ...file, key: '' } }),
      ['dmsg/channel-file-manifest/1', channel, epoch, uploadId, file.id, file.version]
    )
    key.fill(0)
    const upload: ChannelUpload = {
      plan: {
        upload_id: uploadId,
        object_id: file.id,
        version_id: file.version,
        root_generation: 0,
        epoch,
        control_head: state.ledger.head,
        kind: 'file',
        chunks,
        manifest_digest: hash(unb64(manifest)),
        manifest_size: unb64(manifest).length,
        expires_at: Date.now() + 3600000
      },
      chunks: [],
      manifest,
      recipients: []
    }
    const reference: ChannelFileRef = {
      upload_id: uploadId,
      file_id: file.id,
      version: file.version,
      epoch,
      manifest_digest: upload.plan.manifest_digest,
      name: file.name,
      size: file.size,
      mime: file.mime,
      key: file.key
    }
    const saved = await this.immutable(
      'formal_file',
      hash(canonical(['dmsg/channel-file-source/1', channel, messageId])),
      {
        sourceRecord: objectKey,
        channel,
        reference,
        manifest: file,
        upload
      } satisfies PrivateChannelFile
    )
    return { ...upload, fileRecord: saved.key }
  }
  async fileChunk(recordKey: string, index: number) {
    const { db } = await this.port.ready(),
      record = (await db.db.get('objects', recordKey)) as EncryptedObject | undefined
    ensure(
      record?.kind === 'formal_file' && Number.isSafeInteger(index) && index >= 0,
      'NOT_FOUND'
    )
    const file = await this.port.decode<PrivateChannelFile>(record),
      ref = file.manifest.chunks[index]
    if (!file.manifest.chunks.length && index === 0) return emptyFile()
    ensure(ref, 'NOT_FOUND')
    const chunk = (await db.db.get('chunks', ref.id)) as Chunk | undefined
    ensure(chunk && hash(unb64(chunk.ciphertext)) === ref.digest, 'INTEGRITY_FAILED')
    return unb64(chunk.ciphertext)
  }
  private async receivedFile(channel: string, seq: number) {
    const { db } = await this.port.ready()
    for (const record of await db.heads('formal_message', channel)) {
      const value = await this.port.decode<any>(record)
      if (value.channel === channel && value.seq === seq) {
        ensure(value.file, 'NOT_FOUND')
        return value.file as ChannelFileRef
      }
    }
    throw new Error('NOT_FOUND')
  }
  async fileManifest(channel: string, seq: number, ciphertext: Uint8Array) {
    const ref = await this.receivedFile(channel, seq)
    ensure(hash(ciphertext) === ref.manifest_digest, 'INTEGRITY_FAILED')
    const key = unb64(ref.key)
    try {
      const value = decodeCanonical<{
        format: string
        channel: string
        epoch: number
        file: FileManifest
      }>(
        await open(key, b64(ciphertext), [
          'dmsg/channel-file-manifest/1',
          channel,
          ref.epoch,
          ref.upload_id,
          ref.file_id,
          ref.version
        ])
      )
      ensure(
        value.format === 'dmsg-channel-file/1' &&
          value.channel === channel &&
          value.epoch === ref.epoch &&
          value.file.id === ref.file_id &&
          value.file.version === ref.version &&
          value.file.key === '' &&
          value.file.name === ref.name &&
          value.file.size === ref.size &&
          value.file.mime === ref.mime,
        'INTEGRITY_FAILED'
      )
      const objectId = hash(canonical(['dmsg/channel-received-file/1', channel, seq]))
      await this.immutable('formal_file', objectId, {
        channel,
        reference: ref,
        manifest: { ...value.file, key: ref.key }
      } satisfies PrivateChannelFile)
      return { ...value.file, key: '' }
    } finally {
      key.fill(0)
    }
  }
  async fileDownload(channel: string, seq: number, ciphertext: Uint8Array[]) {
    const { db } = await this.port.ready(),
      objectId = hash(canonical(['dmsg/channel-received-file/1', channel, seq]))
    const record = await db.getHead(objectId, 'formal_file')
    ensure(record, 'NOT_FOUND')
    const stored = await this.port.decode<PrivateChannelFile>(record)
    if (!stored.manifest.chunks.length) {
      ensure(ciphertext.length === 1 && equal(ciphertext[0], emptyFile()), 'INTEGRITY_FAILED')
      ciphertext = []
    }
    ensure(stored.manifest.chunks.length === ciphertext.length, 'INTEGRITY_FAILED')
    const chunks = new Map<string, Chunk>()
    for (const [i, bytes] of ciphertext.entries()) {
      const ref = stored.manifest.chunks[i]
      ensure(hash(bytes) === ref.digest, 'INTEGRITY_FAILED')
      chunks.set(ref.id, { id: ref.id, ciphertext: b64(bytes), digest: ref.digest })
    }
    return this.port.cacheFile(stored.manifest, chunks)
  }
  async message(channel: string, text: string, messageId: string) {
    const existing = await this.job(channel, `message:${messageId}`)
    if (existing) {
      ensure(
        existing.textDigest === hash(new TextEncoder().encode(text)),
        'IDEMPOTENCY_CONFLICT'
      )
      return existing
    }
    const { state } = await this.load(channel),
      { meta } = await this.port.ready(),
      ledger = state.ledger
    ensure(
      ledger?.status === 'active' &&
        ledger.members[meta.account!.id] &&
        (ledger.type !== 'distribution' || ledger.members[meta.account!.id].role !== 'member'),
      'FORBIDDEN'
    )
    ensure(
      text.length >= 0 &&
        new TextEncoder().encode(text).length <= 16000 &&
        /^[0-9a-f]{64}$/.test(messageId),
      'INVALID_INPUT'
    )
    const root = state.keys[ledger.epoch]
    ensure(root, 'RECOVERY_INCOMPLETE', '当前频道密钥尚未解包。')
    const attachment = await this.attachment(channel, messageId)
    ensure(text.length > 0 || attachment, 'INVALID_INPUT')
    if (attachment)
      ensure(
        attachment.value.reference.epoch === ledger.epoch,
        'EPOCH_ROTATING',
        '文件计划属于上一代；请重新准备这一文件版本的投递。'
      )
    const payload = {
      channel_id: channel,
      epoch: ledger.epoch,
      control_head: ledger.head,
      acl_version: ledger.acl_version,
      message_id: messageId,
      client_created_at: Date.now()
    }
    const key = derive(unb64(root), [
      'dmsg/channel-message-key/1',
      channel,
      ledger.epoch,
      messageId,
      meta.deviceId
    ])
    try {
      const ciphertext = await seal(
        key,
        canonical({ text, ...(attachment ? { file: attachment.value.reference } : {}) }),
        ['dmsg/channel-message/1', meta.account!.id, meta.deviceId, payload]
      )
      const job = {
        format: 'dmsg-channel-message-job/1',
        text,
        textDigest: hash(new TextEncoder().encode(text)),
        payload: { ...payload, ciphertext },
        state: 'queued'
      }
      await this.job(channel, `message:${messageId}`, job)
      return job
    } finally {
      key.fill(0)
    }
  }
  async receive(
    channel: string,
    input: {
      account: string
      device: string
      value: unknown
      seq: number
      evidence: unknown
    }[],
    through: number,
    backfill = false
  ) {
    const { state } = await this.load(channel),
      cursors = await this.cursors(channel)
    ensure(
      Number.isSafeInteger(through) && (backfill || through >= cursors.message),
      'UNVERIFIED_HEAD'
    )
    for (const item of input) {
      const payload = channelMessageSchema.parse(item.value),
        root = state.keys[payload.epoch]
      ensure(
        payload.channel_id === channel &&
          item.seq > (backfill ? 0 : cursors.message) &&
          item.seq <= through,
        'INTEGRITY_FAILED'
      )
      ensure(root, 'RECOVERY_INCOMPLETE', '缺少授权历史密钥；游标保持原位置。')
      const { ciphertext, ...aad } = payload,
        key = derive(unb64(root), [
          'dmsg/channel-message-key/1',
          channel,
          payload.epoch,
          payload.message_id,
          item.device
        ])
      try {
        const plaintext = decodeCanonical<{ text: string; file?: ChannelFileRef }>(
          await open(key, ciphertext, [
            'dmsg/channel-message/1',
            item.account,
            item.device,
            aad
          ])
        )
        ensure(
          typeof plaintext.text === 'string' && plaintext.text.length <= 16000,
          'INTEGRITY_FAILED'
        )
        const objectId = hash(
          canonical([
            'dmsg/channel-message-record/1',
            channel,
            item.device,
            payload.message_id
          ])
        )
        await this.immutable('formal_message', objectId, {
          channel,
          ...item,
          text: plaintext.text,
          ...(plaintext.file ? { file: plaintext.file } : {}),
          state: 'received'
        })
      } finally {
        key.fill(0)
      }
    }
    if (!backfill && through !== cursors.message)
      await this.saveCursors(channel, { ...cursors, message: through })
  }
  async pending(channel: string) {
    ensure(/^[0-9a-f]{64}$/.test(channel), 'INVALID_INPUT')
    const { db } = await this.port.ready(),
      prefix = `channel-job:${channel}:`,
      rows = []
    for (const key of await db.db.getAllKeys('local_private', prefixRange(prefix))) {
      const name = String(key).slice(prefix.length),
        value = await this.job(channel, name)
      if (value?.action && value.result === undefined)
        rows.push({
          key: name,
          action: value.action as string,
          requestId: value.context.requestId as string
        })
    }
    return rows
  }
  async controls(channel: string) {
    const { db } = await this.port.ready(),
      rows: any[] = []
    for (const record of await db.heads('formal_control', channel)) {
      const value = await this.port.decode<any>(record)
      if (value.channel === channel) rows.push(value)
    }
    return rows.sort((a, b) => a.value.control_seq - b.value.control_seq)
  }
  async messages(channel: string) {
    const { db } = await this.port.ready(),
      rows: any[] = []
    for (const record of await db.heads('formal_message', channel)) {
      const value = await this.port.decode<any>(record)
      if (value.channel === channel)
        rows.push({ ...value, ...(value.file ? { file: { ...value.file, key: '' } } : {}) })
    }
    return rows.sort((a, b) => a.seq - b.seq)
  }
}
