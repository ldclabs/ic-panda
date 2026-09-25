import { AccountClient } from './account'
import type { CloudClient } from './relay'
import { CloudSession } from './cloud-session'
import {
  mergeCloudEvidence,
  readCloudSecurity,
  readCloudSecurityBatch
} from './cloud-security'
import {
  channelAccountEvidence,
  channelDevice,
  verifyChannelEvent,
  type ChannelTrust,
  type StoredChannelEvent
} from './channel-proof'
import {
  advanceChannel,
  activationSchema,
  channelControlSchema,
  channelMessageSchema,
  readable,
  recipientDigest,
  startChannel,
  type ChannelControl,
  type ChannelLedger,
  type EpochRecipient,
  type RotationLease
} from '../protocol/channel'
import {
  CLOUD_PROTOCOL,
  readCloudCommand,
  signCloudCommand,
  type CloudAction,
  type CloudContext,
  type CloudSigned
} from '../protocol/cloud'
import {
  b64,
  canonical,
  decodeCanonical,
  digest,
  equal,
  hash,
  hex,
  id,
  unb64
} from '../protocol/codec'
import { ed25519 } from '../crypto/primitives'
import { xidBytes } from '../protocol/identity'
import type { ChannelUpload } from '../crypto/channel'
import { ensure } from '../errors'

interface Operation {
  action: CloudAction
  payload: Record<string, unknown>
  path: string
  context: CloudContext
  signed: CloudSigned
  result?: any
}
export interface ChannelInvitation {
  format: 'dmsg-channel-invitation/1'
  channel: string
  genesis: string
  head: string
  invitation: string
  account: string
  name: string
  expiresAt: number
}
export interface OwnerTransferPacket {
  format: 'dmsg-channel-owner-transfer/1'
  channel: string
  genesis: string
  context: CloudContext
  payload: {
    expected_head: string
    expected_acl_version: number
    action: { type: 'transfer'; account: string; device: string }
  }
  evidence: import('./cloud-security').CloudSecurityEvidence
  proposedAt: number
  proposalSignature: string
  acceptance?: string
  targetEvidence?: import('./cloud-security').CloudSecurityEvidence
  acceptedAt?: number
}
function ownerUnsigned(packet: Pick<OwnerTransferPacket, 'context' | 'payload'>) {
  const c = packet.context
  return {
    protocol: CLOUD_PROTOCOL,
    action: 'dmsg/channel/control/v1',
    security_epoch: c.securityEpoch,
    request_id: c.requestId,
    deadline: c.deadline,
    payload: packet.payload,
    issuer: c.issuer,
    account_id: c.accountId,
    device_id: c.deviceId
  }
}
export class ChannelClient {
  readonly session: CloudSession
  private ledgers = new Map<string, ChannelLedger>()
  private verifiedEvents = new Map<string, string>()
  // Controls already replayed by this client: the stored cursor and its ledger.
  private replayed = new Map<string, { cursor: number; ledger: ChannelLedger }>()
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudClient,
    readonly accountId: string
  ) {
    this.session = new CloudSession(account, cloud, accountId)
  }
  private base(channel: string) {
    ensure(/^[0-9a-f]{64}$/.test(channel), 'INVALID_INPUT')
    return `/v1/channels/${channel}`
  }
  refresh() {
    return this.session.refresh()
  }
  private trust(): ChannelTrust {
    const state = this.session.state
    ensure(state && state.info.issuer.endsWith(this.accountId), 'AUTH_REQUIRED')
    return {
      home: this.account.home.toText(),
      namespace: state.info.issuer.slice(0, -this.accountId.length),
      agent: this.account.agent
    }
  }
  private async post(
    channel: string,
    key: string,
    action: CloudAction,
    payload: Record<string, unknown>,
    suffix: string
  ) {
    let job = (await this.account.crypto.call('channelJob', channel, key)) as Operation | null
    if (job)
      ensure(
        job.action === action &&
          job.path === this.base(channel) + suffix &&
          equal(canonical(job.payload), canonical(payload)),
        'IDEMPOTENCY_CONFLICT'
      )
    if (job?.result !== undefined) return job.result
    const requestId = job?.context.requestId ?? id()
    const status = await this.session.find(`${this.base(channel)}/operations/${requestId}`)
    if (status?.found) {
      ensure(job, 'INTEGRITY_FAILED')
      job.result = status.result
      await this.account.crypto.call('channelJob', channel, key, job)
      return job.result
    }
    const context = await this.session.context(requestId)
    if (job)
      ensure(
        job.context.deviceId === context.deviceId,
        'AUTH_REQUIRED',
        '此操作属于原设备；先核对原结果，不自动替换发送者。'
      )
    if (
      !job ||
      job.context.deadline <= Date.now() ||
      job.context.securityEpoch !== context.securityEpoch
    ) {
      ensure(
        !(
          job &&
          action === 'dmsg/channel/control/v1' &&
          (payload.action as any)?.type === 'transfer'
        ),
        'EXPIRED',
        '目标 owner 的接受已过期，需要重新核对双方批准。'
      )
      job = {
        action,
        payload,
        path: this.base(channel) + suffix,
        context,
        signed: await signCloudCommand(context, action, payload, this.session.sign)
      }
      await this.account.crypto.call('channelJob', channel, key, job)
    }
    if (action === 'dmsg/channel/genesis/v1') {
      const saved = await this.known(channel)
      await this.account.crypto.call('channelRemember', {
        channel,
        name: saved.name,
        genesis: hash(unb64(job.signed.cose_sign1)),
        replaceUncommitted: true
      })
    }
    const result = await this.cloud.post(
      job.path,
      job.signed,
      { ...context, requestId: job.context.requestId },
      this.session.sign
    )
    job.result = result
    await this.account.crypto.call('channelJob', channel, key, job)
    return result
  }
  async resume(channel: string, key: string) {
    const job = (await this.account.crypto.call(
      'channelJob',
      channel,
      key
    )) as Operation | null
    ensure(job?.action && job.path.startsWith(this.base(channel)), 'NOT_FOUND')
    const result = await this.post(
      channel,
      key,
      job.action,
      job.payload,
      job.path.slice(this.base(channel).length)
    )
    await this.pullControl(channel)
    if (job.action === 'dmsg/channel/message/v1') {
      const pending = await this.account.crypto.call(
        'channelJob',
        channel,
        `message:${job.payload.message_id}`
      )
      if (pending)
        await this.account.crypto.call(
          'channelJob',
          channel,
          `message:${job.payload.message_id}`,
          { ...pending, state: 'stored', receipt: result }
        )
      await this.sync(channel)
    }
    return result
  }
  private contentRootReady() {
    ensure(
      this.session.state &&
        'Ready' in this.session.state.info.vault_write_state &&
        this.session.state.info.current_root[0]?.generation ===
          BigInt(this.account.meta.rootGeneration),
      'REKEY_REQUIRED',
      '请先完成账户换根并重新连接，再保存新频道密钥或消息。'
    )
  }
  async create(name: string, type: ChannelLedger['type'], channel = id()) {
    const key = 'genesis',
      existing = (await this.account.crypto.call(
        'channelJob',
        channel,
        key
      )) as Operation | null
    const payload = existing?.payload ?? { channel_id: channel, type, nonce: id() }
    if (!existing) {
      const context = await this.session.context(),
        signed = await signCloudCommand(
          context,
          'dmsg/channel/genesis/v1',
          payload,
          this.session.sign
        )
      await this.account.crypto.call('channelRemember', {
        channel,
        name,
        genesis: hash(unb64(signed.cose_sign1))
      })
      await this.account.crypto.call('channelJob', channel, key, {
        action: 'dmsg/channel/genesis/v1',
        payload,
        path: this.base(channel),
        context,
        signed
      })
    }
    await this.post(channel, key, 'dmsg/channel/genesis/v1', payload, '')
    await this.pullControl(channel)
    return channel
  }
  known(channel: string) {
    return this.account.crypto.call('channelGet', channel)
  }
  async join(invitation: ChannelInvitation) {
    ensure(
      invitation.format === 'dmsg-channel-invitation/1' &&
        invitation.account === this.accountId &&
        invitation.expiresAt > Date.now(),
      'FORBIDDEN'
    )
    await this.account.crypto.call('channelRemember', {
      channel: invitation.channel,
      name: invitation.name,
      genesis: invitation.genesis
    })
    await this.pullControl(invitation.channel, invitation.invitation, invitation.head)
    const ledger = (await this.known(invitation.channel)).ledger!
    ensure(ledger.invitations[invitation.invitation]?.account === this.accountId, 'FORBIDDEN')
    await this.post(
      invitation.channel,
      `accept:${invitation.invitation}`,
      'dmsg/channel/control/v1',
      {
        expected_head: ledger.head,
        expected_acl_version: ledger.acl_version,
        action: { type: 'accept', invitation_id: invitation.invitation }
      },
      '/control'
    )
    return this.pullControl(invitation.channel)
  }
  async control(channel: string, action: ChannelControl['action'], operation = id()) {
    await this.pullControl(channel)
    const state = (await this.known(channel)).ledger!
    const input = channelControlSchema.parse({
      expected_head: state.head,
      expected_acl_version: state.acl_version,
      action
    })
    advanceChannel(state, input, this.accountId, '00'.repeat(32), Date.now())
    await this.post(
      channel,
      `control:${operation}`,
      'dmsg/channel/control/v1',
      input,
      '/control'
    )
    return this.pullControl(channel)
  }
  async invite(channel: string, account: string, role: 'member' | 'publisher' = 'member') {
    xidBytes(account)
    const invitation = id(),
      expiresAt = Date.now() + 86400000
    await this.control(
      channel,
      { type: 'invite', invitation_id: invitation, account, role, expires_at: expiresAt },
      invitation
    )
    const saved = await this.known(channel)
    return {
      format: 'dmsg-channel-invitation/1',
      channel,
      genesis: saved.genesis,
      head: saved.ledger!.head,
      invitation,
      account,
      name: saved.name,
      expiresAt
    } satisfies ChannelInvitation
  }
  private async consumeEvent(
    previous: ChannelLedger | null,
    event: StoredChannelEvent,
    genesis: string
  ) {
    ensure(hash(unb64(event.signed.cose_sign1)) === event.head, 'INTEGRITY_FAILED')
    const cached = this.ledgers.get(event.head),
      proof = JSON.stringify(event)
    if (cached && this.verifiedEvents.get(event.head) === proof) {
      ensure(event.control_seq === cached.control_seq, 'UNVERIFIED_HEAD')
      const body = readCloudCommand(event.signed).body
      ensure(
        previous
          ? cached.channel_id === previous.channel_id &&
              cached.control_seq === previous.control_seq + 1 &&
              body.payload.expected_head === previous.head
          : cached.control_seq === 0 && event.head === genesis,
        'UNVERIFIED_HEAD'
      )
      return structuredClone(cached)
    }
    const verified = await verifyChannelEvent(event, this.trust()),
      body = verified.body
    let state: ChannelLedger
    if (!previous) {
      ensure(
        event.control_seq === 0 &&
          event.head === genesis &&
          body.action === 'dmsg/channel/genesis/v1',
        'UNVERIFIED_HEAD'
      )
      state = startChannel(body.payload, verified.account, event.head)
    } else {
      ensure(event.control_seq === previous.control_seq + 1, 'UNVERIFIED_HEAD')
      if (body.action === 'dmsg/channel/control/v1') {
        const control = channelControlSchema.parse(body.payload)
        if (control.action.type === 'transfer') {
          ensure(event.acceptance_evidence, 'AUTH_REQUIRED')
          const target = await channelDevice(
            event.acceptance_evidence,
            control.action.account,
            control.action.device,
            event.stored_at,
            this.trust()
          )
          const { signature, ...action } = control.action
          ensure(
            ed25519.verify(
              unb64(signature),
              digest('dmsg/channel/owner-acceptance/v1', {
                ...verified.unsigned,
                payload: { ...control, action }
              }),
              target.device.signing_pub
            ),
            'AUTH_REQUIRED'
          )
        }
        state = advanceChannel(
          previous,
          control,
          verified.account,
          event.head,
          event.stored_at
        )
      } else if (body.action === 'dmsg/channel/reactivate/v1') {
        ensure(
          previous.owner === verified.account &&
            previous.status === 'archived' &&
            body.payload.expected_head === previous.head,
          'UNVERIFIED_HEAD'
        )
        state = {
          ...structuredClone(previous),
          status: 'rotation_required',
          control_seq: event.control_seq,
          acl_version: previous.acl_version + 1,
          head: event.head
        }
      } else {
        ensure(body.action === 'dmsg/channel/activate/v1', 'UNSUPPORTED_PROTOCOL')
        const activation = activationSchema.parse(body.payload),
          coordinator = previous.members[verified.account]
        ensure(
          coordinator?.can_rotate &&
            (previous.epoch === 0 || coordinator.joined_epoch <= previous.epoch) &&
            previous.status !== 'archived' &&
            activation.expected_head === previous.head &&
            activation.epoch === previous.epoch + 1 &&
            new Set(activation.envelopes.map((e) => e.recipient)).size ===
              activation.envelopes.length,
          'UNVERIFIED_HEAD'
        )
        state = {
          ...structuredClone(previous),
          epoch: activation.epoch,
          status: 'active',
          head: event.head,
          control_seq: event.control_seq,
          manifest: null
        }
      }
    }
    this.ledgers.set(state.head, structuredClone(state))
    this.verifiedEvents.set(state.head, proof)
    return state
  }
  async pullControl(channel: string, invitation?: string, pinnedHead?: string) {
    await this.session.context()
    const saved = await this.known(channel),
      replayed = this.replayed.get(channel)
    let ledger: ChannelLedger | null = null,
      cursor = 0,
      pinSeen = !pinnedHead
    if (replayed?.cursor === saved.controlCursor) {
      // Stored controls are unchanged since this client verified them.
      ledger = structuredClone(replayed.ledger)
      cursor = replayed.cursor
      pinSeen ||= this.ledgers.get(pinnedHead!)?.channel_id === channel
    } else
      for (const row of await this.account.crypto.call('channelControls', channel)) {
        ledger = await this.consumeEvent(ledger, row.value, saved.genesis)
        cursor = row.cursor
        if (ledger.head === pinnedHead) pinSeen = true
      }
    const extra = invitation ? `&invitation=${invitation}` : ''
    for (let page = 0; page < 10000; page++) {
      const result = await this.session.get(
        `${this.base(channel)}/sync?kind=control&after=${cursor}&limit=5${extra}`
      )
      ensure(
        Array.isArray(result.entries) &&
          result.entries.length <= 5 &&
          Number.isSafeInteger(result.next_cursor) &&
          result.next_cursor >= cursor,
        'INTEGRITY_FAILED'
      )
      for (const row of result.entries) {
        ensure(Number.isSafeInteger(row.cursor) && row.cursor > cursor, 'INTEGRITY_FAILED')
        ledger = await this.consumeEvent(ledger, row.value, saved.genesis)
        cursor = row.cursor
        if (ledger.head === pinnedHead) pinSeen = true
      }
      ensure(
        result.next_cursor === cursor && ledger?.channel_id === channel,
        'UNVERIFIED_HEAD'
      )
      await this.account.crypto.call('channelAdvance', channel, ledger, cursor, result.entries)
      if (ledger.head === result.head) {
        ensure(pinSeen, 'UNVERIFIED_HEAD', '邀请中确认的控制头不在已验证历史中。')
        this.replayed.set(channel, { cursor, ledger: structuredClone(ledger) })
        const remote = await this.session.get(
          `${this.base(channel)}${invitation ? `?invitation=${invitation}` : ''}`
        )
        ensure(
          remote.head === ledger.head &&
            remote.control_seq === ledger.control_seq &&
            remote.epoch === ledger.epoch &&
            remote.acl_version === ledger.acl_version,
          'UNVERIFIED_HEAD'
        )
        if (remote.status === 'rotation_required' && ledger.status === 'active')
          ledger = { ...ledger, status: 'rotation_required' }
        else ensure(remote.status === ledger.status, 'UNVERIFIED_HEAD')
        if (remote.members)
          ensure(
            equal(canonical(remote.members), canonical(ledger.members)) &&
              remote.owner === ledger.owner,
            'UNVERIFIED_HEAD'
          )
        await this.account.crypto.call('channelAdvance', channel, ledger, cursor, [])
        return ledger
      }
      ensure(result.entries.length > 0, 'UNVERIFIED_HEAD', '控制日志存在缺口。')
    }
    throw new Error('QUOTA_EXCEEDED')
  }
  private async currentRecipients(ledger: ChannelLedger) {
    const recipients: EpochRecipient[] = [],
      versions: Record<string, number> = {},
      expires: number[] = []
    const accounts = Object.keys(ledger.members),
      trust = this.trust()
    const records = await readCloudSecurityBatch(
      this.account.user,
      this.account.agent,
      accounts.map((accountId) => ({
        accountId,
        issuer: `${trust.namespace}${accountId}`,
        homeUser: trust.home
      }))
    )
    for (const evidence of mergeCloudEvidence(records.map((record) => record.evidence)))
      await this.cloud.publishSecurity(evidence)
    for (const [index, account] of accounts.entries()) {
      const record = records[index]
      versions[account] = record.securityEpoch
      expires.push(record.expiresAt)
      for (const device of record.devices)
        if (
          !device.revoked_at.length &&
          device.input.capabilities.some((c) => 'ContentSign' in c)
        )
          recipients.push({
            recipient: `device:${account}:${hex(Uint8Array.from(device.input.device_id))}`,
            hpke_pub: hex(Uint8Array.from(device.input.hpke_pub))
          })
      const pub = record.snapshot.recovery_hpke_pub,
        generation = Number(record.snapshot.recovery_root_version)
      ensure(
        pub instanceof Uint8Array &&
          pub.length === 32 &&
          Number.isSafeInteger(generation) &&
          generation > 0,
        'RECOVERY_INCOMPLETE'
      )
      recipients.push({ recipient: `recovery:${account}:${generation}`, hpke_pub: hex(pub) })
    }
    return {
      recipients: recipients.sort((a, b) => a.recipient.localeCompare(b.recipient)),
      versions,
      expiresAt: Math.min(...expires)
    }
  }
  async uploadPrepared(channel: string, upload: ChannelUpload) {
    await this.pullControl(channel)
    this.contentRootReady()
    return this.upload(channel, upload)
  }
  private async upload(channel: string, upload: ChannelUpload) {
    const base = this.base(channel),
      plan = upload.plan
    let status = await this.session.find(`${base}/uploads/${plan.upload_id}`)
    if (status) ensure(equal(canonical(status.plan), canonical(plan)), 'IDEMPOTENCY_CONFLICT')
    if (status?.status === 'committed') return
    if (!status || status.quota_state === 'pending')
      status = await this.post(
        channel,
        `upload:${plan.upload_id}`,
        'dmsg/upload/reserve/v1',
        plan,
        '/uploads'
      )
    for (let offset = 0; offset < plan.chunks.length; offset += 8) {
      const context = await this.session.context()
      await Promise.all(
        plan.chunks.slice(offset, offset + 8).map(async (_ref, i) => {
          const index = offset + i
          const bytes = upload.fileRecord
            ? await this.account.crypto.call('channelFileChunk', upload.fileRecord, index)
            : unb64(upload.chunks[index])
          return this.cloud.putChunk(
            `${base}/uploads/${plan.upload_id}/chunks/${index}`,
            bytes,
            { ...context, requestId: id() },
            this.session.sign
          )
        })
      )
    }
    await this.cloud.putChunk(
      `${base}/uploads/${plan.upload_id}/chunks/manifest`,
      unb64(upload.manifest),
      await this.session.context(),
      this.session.sign
    )
    const result = await this.post(
      channel,
      `finalize:${plan.upload_id}`,
      'dmsg/upload/finalize/v1',
      { upload_id: plan.upload_id },
      '/uploads/finalize'
    )
    ensure(
      result.upload_id === plan.upload_id && result.digest === plan.manifest_digest,
      'INTEGRITY_FAILED'
    )
  }
  async rotate(channel: string) {
    let ledger = await this.pullControl(channel)
    this.contentRootReady()
    let current = await this.currentRecipients(ledger)
    const observedHead = ledger.head
    ledger = await this.pullControl(channel)
    if (ledger.head !== observedHead) current = await this.currentRecipients(ledger)
    if (ledger.status === 'active') return this.openEpoch(channel, ledger.epoch)
    ensure(ledger.status === 'rotation_required', 'EPOCH_ROTATING')
    let pending = (await this.account.crypto.call('channelJob', channel, 'rotation')) as {
      head: string
      lease_id: string
      lease?: RotationLease
    } | null
    if (
      !pending ||
      pending.head !== ledger.head ||
      (pending.lease && pending.lease.expires_at <= Date.now())
    ) {
      pending = { head: ledger.head, lease_id: id() }
      await this.account.crypto.call('channelJob', channel, 'rotation', pending)
    }
    const result = (await this.post(
      channel,
      `lease:${pending.lease_id}`,
      'dmsg/channel/lease/v1',
      { expected_head: ledger.head, lease_id: pending.lease_id },
      '/rotation/lease'
    )) as { lease: RotationLease; recipients: EpochRecipient[] }
    ensure(
      equal(canonical(result.recipients), canonical(current.recipients)) &&
        recipientDigest(current.recipients) === result.lease.recipients_digest &&
        equal(canonical(current.versions), canonical(result.lease.security_versions)) &&
        result.lease.expires_at <= current.expiresAt &&
        result.lease.expires_at > Date.now(),
      'POLICY_STALE'
    )
    pending.lease = result.lease
    await this.account.crypto.call('channelJob', channel, 'rotation', pending)
    const prepared = await this.account.crypto.call(
      'channelRotation',
      channel,
      result.lease,
      current.recipients
    )
    for (const upload of prepared.uploads) await this.upload(channel, upload)
    await this.post(
      channel,
      `activate:${result.lease.id}`,
      'dmsg/channel/activate/v1',
      prepared.activation,
      '/rotation/activate'
    )
    await this.pullControl(channel)
    return this.openEpoch(channel, prepared.activation.epoch)
  }
  private async descriptor(channel: string, upload: string, uploader?: string) {
    const result = await this.session.get(`${this.base(channel)}/objects/${upload}`),
      checked = await verifyChannelEvent(result, this.trust())
    ensure(
      checked.body.action === 'dmsg/upload/reserve/v1' &&
        equal(canonical(checked.body.payload), canonical(result.plan)) &&
        result.plan.upload_id === upload,
      'INTEGRITY_FAILED'
    )
    if (uploader) ensure(checked.account === uploader, 'AUTH_REQUIRED')
    return result.plan
  }
  async openEpoch(channel: string, epoch: number) {
    const saved = await this.known(channel),
      ledger = saved.ledger
    ensure(ledger && readable(ledger, this.accountId, epoch), 'FORBIDDEN')
    const controls = await this.account.crypto.call('channelControls', channel)
    const row = controls.find((r) => {
      const body = readCloudCommand(r.value.signed).body
      return body.action === 'dmsg/channel/activate/v1' && body.payload.epoch === epoch
    })
    ensure(row, 'NOT_FOUND')
    const activation = activationSchema.parse(readCloudCommand(row.value.signed).body.payload)
    if (
      !activation.envelopes.some(
        (e) => e.recipient === `device:${this.accountId}:${this.account.meta.deviceId}`
      )
    )
      return this.openHistory(channel, epoch)
    const before = this.ledgers.get(activation.expected_head)
    ensure(before, 'UNVERIFIED_HEAD')
    const recipients: EpochRecipient[] = [],
      accounts = new Set<string>()
    let cursor = 0
    while (accounts.size < Object.keys(before.members).length) {
      const result = await this.session.get(
        `${this.base(channel)}/epochs/${epoch}/security?after=${cursor}`
      )
      ensure(
        result.epoch === epoch &&
          result.head === row.value.head &&
          result.recipients_digest === activation.recipients_digest &&
          result.count === Object.keys(before.members).length &&
          Array.isArray(result.entries) &&
          result.entries.length > 0 &&
          result.entries.length <= 5,
        'INTEGRITY_FAILED'
      )
      for (const entry of result.entries) {
        const { account, evidence } = entry.value
        ensure(
          before.members[account] && !accounts.has(account) && entry.cursor > cursor,
          'INTEGRITY_FAILED'
        )
        const record = await channelAccountEvidence(
          evidence,
          account,
          row.value.stored_at,
          this.trust()
        )
        for (const [id, d] of record.devices)
          if (d.revoked_at === null && d.input.capabilities.includes('ContentSign'))
            recipients.push({
              recipient: `device:${account}:${hex(id)}`,
              hpke_pub: hex(d.input.hpke_pub)
            })
        ensure(
          record.snapshot.recovery_hpke_pub instanceof Uint8Array &&
            BigInt(record.snapshot.recovery_root_version) > 0n,
          'INTEGRITY_FAILED'
        )
        recipients.push({
          recipient: `recovery:${account}:${record.snapshot.recovery_root_version}`,
          hpke_pub: hex(record.snapshot.recovery_hpke_pub)
        })
        accounts.add(account)
        cursor = entry.cursor
      }
      ensure(cursor === result.next_cursor, 'INTEGRITY_FAILED')
      await this.account.crypto.call(
        'channelJob',
        channel,
        `epoch-evidence:${epoch}:${cursor}`,
        result
      )
    }
    recipients.sort((a, b) => a.recipient.localeCompare(b.recipient))
    ensure(
      recipientDigest(recipients) === activation.recipients_digest &&
        recipients.length === activation.envelopes.length &&
        recipients.every((r) => activation.envelopes.some((e) => e.recipient === r.recipient)),
      'INTEGRITY_FAILED'
    )
    const reference = activation.envelopes.find(
      (e) => e.recipient === `device:${this.accountId}:${this.account.meta.deviceId}`
    )
    ensure(reference, 'RECOVERY_INCOMPLETE', '此设备不在该代接收集合中，需要独立历史授权。')
    const plan = await this.descriptor(channel, reference.upload_id)
    ensure(
      plan.kind === 'envelopes' &&
        plan.epoch === epoch &&
        plan.control_head === activation.expected_head &&
        plan.chunks[reference.chunk]?.digest === reference.digest &&
        plan.chunks[reference.chunk]?.size === reference.size,
      'INTEGRITY_FAILED'
    )
    const manifest = decodeCanonical<any>(
      await this.cloud.getChunk(
        `${this.base(channel)}/objects/${reference.upload_id}/chunks/manifest`,
        plan.manifest_digest,
        await this.session.context(),
        this.session.sign
      )
    )
    ensure(
      manifest.format === 'dmsg-channel-epoch/1' &&
        manifest.channel === channel &&
        manifest.epoch === epoch &&
        manifest.key_confirmation === activation.key_confirmation &&
        manifest.recipients_digest === activation.recipients_digest,
      'INTEGRITY_FAILED'
    )
    const envelope = await this.cloud.getChunk(
      `${this.base(channel)}/objects/${reference.upload_id}/chunks/${reference.chunk}`,
      reference.digest,
      await this.session.context(),
      this.session.sign
    )
    this.contentRootReady()
    await this.account.crypto.call('channelInstall', channel, activation, envelope)
    return this.known(channel)
  }
  async send(channel: string, text: string, messageId = id()) {
    const ledger = await this.pullControl(channel)
    this.contentRootReady()
    if (!(await this.known(channel)).readableEpochs.includes(ledger.epoch))
      await this.openEpoch(channel, ledger.epoch)
    const job = await this.account.crypto.call('channelMessage', channel, text, messageId)
    const receipt = await this.post(
      channel,
      `send:${messageId}`,
      'dmsg/channel/message/v1',
      job.payload,
      '/messages'
    )
    const operation = (await this.account.crypto.call(
      'channelJob',
      channel,
      `send:${messageId}`
    )) as Operation
    ensure(
      receipt.digest === hash(unb64(operation.signed.cose_sign1)) &&
        Number.isSafeInteger(receipt.seq) &&
        receipt.seq > 0,
      'INTEGRITY_FAILED'
    )
    await this.account.crypto.call('channelJob', channel, `message:${messageId}`, {
      ...job,
      state: 'stored',
      receipt
    })
    await this.sync(channel)
    return { messageId, receipt }
  }
  async sendFile(channel: string, objectKey: string, messageId = id(), text = '') {
    await this.pullControl(channel)
    this.contentRootReady()
    const upload = await this.account.crypto.call(
      'channelFilePrepare',
      channel,
      messageId,
      objectKey
    )
    await this.upload(channel, upload)
    return this.send(channel, text, messageId)
  }
  async downloadFile(channel: string, seq: number) {
    await this.pullControl(channel)
    this.contentRootReady()
    const row = (await this.account.crypto.call('channelMessages', channel)).find(
      (m) => m.seq === seq
    )
    ensure(row?.file, 'NOT_FOUND')
    const ref = row.file,
      plan = await this.descriptor(channel, ref.upload_id, row.account)
    ensure(
      plan.kind === 'file' &&
        plan.object_id === ref.file_id &&
        plan.version_id === ref.version &&
        plan.epoch === ref.epoch &&
        plan.manifest_digest === ref.manifest_digest,
      'INTEGRITY_FAILED'
    )
    const bytes = await this.cloud.getChunk(
      `${this.base(channel)}/objects/${ref.upload_id}/chunks/manifest`,
      ref.manifest_digest,
      await this.session.context(),
      this.session.sign
    )
    const manifest = await this.account.crypto.call('channelFileManifest', channel, seq, bytes)
    ensure(
      plan.chunks.length === Math.max(1, manifest.chunks.length) &&
        plan.chunks.every(
          (c: any, i: number) =>
            !manifest.chunks.length || c.digest === manifest.chunks[i].digest
        ),
      'INTEGRITY_FAILED'
    )
    const chunks: Uint8Array[] = []
    for (let index = 0; index < plan.chunks.length; index++)
      chunks.push(
        await this.cloud.getChunk(
          `${this.base(channel)}/objects/${ref.upload_id}/chunks/${index}`,
          plan.chunks[index].digest,
          await this.session.context(),
          this.session.sign
        )
      )
    return this.account.crypto.call('channelFileDownload', channel, seq, chunks)
  }
  async sync(channel: string, backfill = false) {
    const ledger = await this.pullControl(channel),
      saved = await this.known(channel),
      readableEpochs = new Set(saved.readableEpochs)
    let after = backfill ? 0 : saved.messageCursor
    for (let page = 0; page < 10000; page++) {
      const result = await this.session.get(
        `${this.base(channel)}/sync?after=${after}&limit=20`
      )
      ensure(
        result.head === ledger.head &&
          Number.isSafeInteger(result.next_seq) &&
          result.next_seq >= after &&
          Number.isSafeInteger(result.latest_seq) &&
          result.latest_seq >= result.next_seq &&
          Array.isArray(result.entries) &&
          result.entries.length <= 20,
        'UNVERIFIED_HEAD'
      )
      const rows = [],
        seen = new Set<number>()
      ensure(
        result.next_seq - after <= 20 && Array.isArray(result.skipped),
        'INTEGRITY_FAILED'
      )
      for (const skipped of result.skipped) {
        ensure(
          Number.isSafeInteger(skipped.seq) &&
            skipped.seq > after &&
            skipped.seq <= result.next_seq &&
            !seen.has(skipped.seq) &&
            Number.isSafeInteger(skipped.epoch) &&
            skipped.epoch > 0 &&
            skipped.epoch <= ledger.epoch &&
            !readable(ledger, this.accountId, skipped.epoch),
          'INTEGRITY_FAILED'
        )
        seen.add(skipped.seq)
      }
      for (const message of result.entries) {
        const checked = await verifyChannelEvent(
            {
              signed: message.signed,
              evidence: message.evidence,
              stored_at: message.receipt.stored_at
            },
            this.trust()
          ),
          payload = channelMessageSchema.parse(checked.body.payload)
        ensure(
          checked.body.action === 'dmsg/channel/message/v1' &&
            payload.channel_id === channel &&
            message.epoch === payload.epoch &&
            message.receipt.seq === message.seq &&
            message.receipt.digest === hash(unb64(message.signed.cose_sign1)) &&
            message.seq > after &&
            message.seq <= result.next_seq &&
            !seen.has(message.seq),
          'INTEGRITY_FAILED'
        )
        const historical = this.ledgers.get(payload.control_head),
          author = historical?.members[checked.account]
        ensure(
          historical &&
            author &&
            historical.epoch === payload.epoch &&
            historical.acl_version === payload.acl_version &&
            (historical.type !== 'distribution' || author.role !== 'member') &&
            readable(ledger, this.accountId, payload.epoch),
          'FORBIDDEN'
        )
        seen.add(message.seq)
        if (!readableEpochs.has(payload.epoch))
          for (const epoch of (await this.openEpoch(channel, payload.epoch)).readableEpochs)
            readableEpochs.add(epoch)
        rows.push({
          account: checked.account,
          device: checked.deviceId,
          value: payload,
          seq: message.seq,
          evidence: message
        })
      }
      ensure(
        seen.size === result.next_seq - after,
        'INTEGRITY_FAILED',
        '可读取的消息区间存在缺口。'
      )
      ensure(
        result.next_seq > after || result.next_seq === result.latest_seq,
        'INTEGRITY_FAILED'
      )
      if (rows.length) this.contentRootReady()
      await this.account.crypto.call(
        'channelReceive',
        channel,
        rows,
        result.next_seq,
        backfill
      )
      after = result.next_seq
      if (after === result.latest_seq)
        return {
          through: after,
          messages: await this.account.crypto.call('channelMessages', channel)
        }
    }
    throw new Error('QUOTA_EXCEEDED')
  }
  async grantHistory(
    channel: string,
    target: string,
    from: number,
    to: number,
    grantId = id()
  ) {
    const ledger = await this.pullControl(channel)
    this.contentRootReady()
    ensure(
      ledger.status === 'active' &&
        ['owner', 'admin'].includes(ledger.members[this.accountId]?.role) &&
        ledger.members[target] &&
        Number.isSafeInteger(from) &&
        Number.isSafeInteger(to) &&
        from > 0 &&
        to >= from &&
        to <= ledger.epoch &&
        to - from < 1000,
      'FORBIDDEN'
    )
    const readableEpochs = new Set((await this.known(channel)).readableEpochs)
    for (let epoch = from; epoch <= to; epoch++) {
      ensure(readable(ledger, this.accountId, epoch), 'FORBIDDEN')
      if (!readableEpochs.has(epoch))
        for (const opened of (await this.openEpoch(channel, epoch)).readableEpochs)
          readableEpochs.add(opened)
    }
    const current = await this.currentRecipients({
      ...ledger,
      members: { [target]: ledger.members[target] }
    })
    const prepared = await this.account.crypto.call(
      'channelHistory',
      channel,
      target,
      from,
      to,
      current.recipients,
      grantId
    )
    for (const upload of prepared.uploads) await this.upload(channel, upload)
    const fresh = await this.currentRecipients({
      ...ledger,
      members: { [target]: ledger.members[target] }
    })
    ensure(
      equal(canonical(current.versions), canonical(fresh.versions)) &&
        equal(canonical(current.recipients), canonical(fresh.recipients)),
      'POLICY_STALE'
    )
    const action = {
      type: 'history',
      account: target,
      from_epoch: from,
      to_epoch: to,
      envelope_upload: prepared.uploads[0].plan.upload_id
    }
    await this.post(
      channel,
      `history:${grantId}`,
      'dmsg/channel/control/v1',
      { expected_head: ledger.head, expected_acl_version: ledger.acl_version, action },
      '/control'
    )
    return this.pullControl(channel)
  }
  private async openHistory(channel: string, epoch: number) {
    const controls = await this.account.crypto.call('channelControls', channel)
    const row = [...controls].reverse().find((r) => {
      const b = readCloudCommand(r.value.signed).body
      const a = b.payload.action as any
      return (
        b.action === 'dmsg/channel/control/v1' &&
        a?.type === 'history' &&
        a.account === this.accountId &&
        a.from_epoch <= epoch &&
        epoch <= a.to_epoch
      )
    })
    ensure(row, 'RECOVERY_INCOMPLETE', '此设备尚无相应历史授权。')
    const event = await verifyChannelEvent(row.value, this.trust()),
      control = channelControlSchema.parse(event.body.payload)
    ensure(control.action.type === 'history', 'INTEGRITY_FAILED')
    const action = control.action,
      first = await this.descriptor(channel, action.envelope_upload, event.account)
    const encoded = await this.cloud.getChunk(
      `${this.base(channel)}/objects/${action.envelope_upload}/chunks/manifest`,
      first.manifest_digest,
      await this.session.context(),
      this.session.sign
    )
    const manifest = decodeCanonical<any>(encoded),
      scope = manifest.scope
    ensure(
      manifest.format === 'dmsg-channel-history/1' &&
        scope?.channel === channel &&
        scope.target === this.accountId &&
        scope.from === action.from_epoch &&
        scope.to === action.to_epoch &&
        scope.head === control.expected_head &&
        first.epoch === scope.epoch &&
        first.control_head === scope.head &&
        first.kind === 'envelopes' &&
        Array.isArray(manifest.envelopes) &&
        manifest.envelopes.length <= 192 &&
        row.value.acceptance_evidence,
      'INTEGRITY_FAILED'
    )
    const target = await channelAccountEvidence(
        row.value.acceptance_evidence,
        this.accountId,
        row.value.stored_at,
        this.trust()
      ),
      recipients: EpochRecipient[] = []
    for (const [id, d] of target.devices)
      if (d.revoked_at === null && d.input.capabilities.includes('ContentSign'))
        recipients.push({
          recipient: `device:${this.accountId}:${hex(id)}`,
          hpke_pub: hex(d.input.hpke_pub)
        })
    ensure(target.snapshot.recovery_hpke_pub instanceof Uint8Array, 'INTEGRITY_FAILED')
    recipients.push({
      recipient: `recovery:${this.accountId}:${target.snapshot.recovery_root_version}`,
      hpke_pub: hex(target.snapshot.recovery_hpke_pub)
    })
    recipients.sort((a, b) => a.recipient.localeCompare(b.recipient))
    ensure(recipientDigest(recipients) === scope.recipients_digest, 'POLICY_STALE')
    const recipient = `device:${this.accountId}:${this.account.meta.deviceId}`
    const references = manifest.envelopes.filter((r: any) => r.recipient === recipient)
    ensure(
      references.length > 0 && references.length <= 32,
      'RECOVERY_INCOMPLETE',
      '此设备未包含在历史授权中，请由持钥管理员重新授权。'
    )
    const downloads = []
    for (const ref of references) {
      const plan = await this.descriptor(channel, ref.upload_id, event.account)
      ensure(
        plan.kind === 'envelopes' &&
          plan.epoch === scope.epoch &&
          plan.control_head === scope.head &&
          plan.manifest_digest === first.manifest_digest &&
          plan.chunks[ref.chunk]?.digest === ref.digest &&
          plan.chunks[ref.chunk]?.size === ref.size,
        'INTEGRITY_FAILED'
      )
      const bytes = await this.cloud.getChunk(
        `${this.base(channel)}/objects/${ref.upload_id}/chunks/${ref.chunk}`,
        ref.digest,
        await this.session.context(),
        this.session.sign
      )
      downloads.push({ ...ref, bytes })
    }
    const confirmations: [number, string][] = []
    for (const r of controls) {
      const body = readCloudCommand(r.value.signed).body
      if (body.action === 'dmsg/channel/activate/v1') {
        const a = activationSchema.parse(body.payload)
        if (a.epoch >= scope.from && a.epoch <= scope.to)
          confirmations.push([a.epoch, a.key_confirmation])
      }
    }
    this.contentRootReady()
    await this.account.crypto.call(
      'channelHistoryInstall',
      channel,
      scope,
      downloads,
      confirmations
    )
    return this.known(channel)
  }
  async prepareOwnerTransfer(channel: string, target: string, targetDevice?: string) {
    const ledger = await this.pullControl(channel)
    ensure(
      ledger.owner === this.accountId && ledger.members[target] && target !== this.accountId,
      'FORBIDDEN'
    )
    const targetState = await readCloudSecurity(this.account.user, this.account.agent, {
      accountId: target,
      issuer: `${this.trust().namespace}${target}`,
      homeUser: this.trust().home
    })
    const device = targetState.devices.find(
      (d) =>
        !d.revoked_at.length &&
        d.input.capabilities.some((c) => 'ContentSign' in c) &&
        (!targetDevice || hex(Uint8Array.from(d.input.device_id)) === targetDevice)
    )
    ensure(device, 'AUTH_REQUIRED')
    const context = await this.session.context(),
      saved = await this.known(channel)
    const packet: OwnerTransferPacket = {
      format: 'dmsg-channel-owner-transfer/1',
      channel,
      genesis: saved.genesis,
      context,
      payload: {
        expected_head: ledger.head,
        expected_acl_version: ledger.acl_version,
        action: {
          type: 'transfer',
          account: target,
          device: hex(Uint8Array.from(device.input.device_id))
        }
      },
      evidence: this.session.state!.verified.evidence,
      proposedAt: Date.now(),
      proposalSignature: ''
    }
    packet.proposalSignature = b64(
      await this.session.sign(digest('dmsg/channel/owner-proposal/v1', ownerUnsigned(packet)))
    )
    await this.account.crypto.call(
      'channelJob',
      channel,
      `owner-proposal:${context.requestId}`,
      packet
    )
    return packet
  }
  async acceptOwnerTransfer(packet: OwnerTransferPacket) {
    ensure(
      packet.format === 'dmsg-channel-owner-transfer/1' &&
        packet.payload.action.account === this.accountId &&
        packet.payload.action.device === this.account.meta.deviceId &&
        packet.context.deadline > Date.now(),
      'FORBIDDEN'
    )
    const ledger = await this.pullControl(packet.channel),
      local = await this.known(packet.channel)
    ensure(
      local.genesis === packet.genesis &&
        ledger.head === packet.payload.expected_head &&
        ledger.acl_version === packet.payload.expected_acl_version &&
        ledger.owner === packet.context.accountId,
      'UNVERIFIED_HEAD'
    )
    const owner = await channelDevice(
      packet.evidence,
      packet.context.accountId,
      packet.context.deviceId,
      packet.proposedAt,
      this.trust()
    )
    ensure(
      BigInt(owner.snapshot.security_epoch) === BigInt(packet.context.securityEpoch) &&
        packet.context.issuer === owner.snapshot.issuer &&
        ed25519.verify(
          unb64(packet.proposalSignature),
          digest('dmsg/channel/owner-proposal/v1', ownerUnsigned(packet)),
          owner.device.signing_pub
        ),
      'AUTH_REQUIRED'
    )
    ensure(
      this.session.state?.device &&
        !this.session.state.device.revoked_at.length &&
        this.session.state.device.input.capabilities.some((c) => 'ContentSign' in c),
      'AUTH_REQUIRED'
    )
    const accepted = {
      ...packet,
      acceptance: b64(
        await this.session.sign(
          digest('dmsg/channel/owner-acceptance/v1', ownerUnsigned(packet))
        )
      ),
      targetEvidence: this.session.state.verified.evidence,
      acceptedAt: Date.now()
    }
    await this.account.crypto.call(
      'channelJob',
      packet.channel,
      `owner-acceptance:${packet.context.requestId}`,
      accepted
    )
    return accepted
  }
  async commitOwnerTransfer(packet: OwnerTransferPacket) {
    const original = (await this.account.crypto.call(
      'channelJob',
      packet.channel,
      `owner-proposal:${packet.context.requestId}`
    )) as OwnerTransferPacket | null
    ensure(
      original &&
        equal(canonical(ownerUnsigned(original)), canonical(ownerUnsigned(packet))) &&
        packet.context.accountId === this.accountId &&
        packet.context.deadline > Date.now() &&
        packet.acceptance &&
        packet.targetEvidence &&
        packet.acceptedAt,
      'AUTH_REQUIRED'
    )
    const ledger = await this.pullControl(packet.channel)
    ensure(
      ledger.head === packet.payload.expected_head && ledger.owner === this.accountId,
      'UNVERIFIED_HEAD'
    )
    const target = await channelDevice(
      packet.targetEvidence,
      packet.payload.action.account,
      packet.payload.action.device,
      packet.acceptedAt,
      this.trust()
    )
    ensure(
      ed25519.verify(
        unb64(packet.acceptance),
        digest('dmsg/channel/owner-acceptance/v1', ownerUnsigned(packet)),
        target.device.signing_pub
      ),
      'AUTH_REQUIRED'
    )
    const payload = {
        ...packet.payload,
        action: { ...packet.payload.action, signature: packet.acceptance }
      },
      key = `control:${packet.context.requestId}`
    const existing = await this.account.crypto.call('channelJob', packet.channel, key)
    if (!existing)
      await this.account.crypto.call('channelJob', packet.channel, key, {
        action: 'dmsg/channel/control/v1',
        payload,
        context: packet.context,
        path: this.base(packet.channel) + '/control',
        signed: await signCloudCommand(
          packet.context,
          'dmsg/channel/control/v1',
          payload,
          this.session.sign
        )
      })
    await this.post(packet.channel, key, 'dmsg/channel/control/v1', payload, '/control')
    return this.pullControl(packet.channel)
  }
  async reactivate(channel: string) {
    const ledger = await this.pullControl(channel),
      current = await this.session.get(this.base(channel))
    ensure(
      ledger.status === 'archived' && ledger.owner === this.accountId && current.billing,
      'FORBIDDEN'
    )
    await this.post(
      channel,
      `reactivate:${current.billing.billing_version}`,
      'dmsg/channel/reactivate/v1',
      {
        expected_head: ledger.head,
        expected_billing_version: current.billing.billing_version
      },
      '/reactivate'
    )
    await this.pullControl(channel)
    return this.rotate(channel)
  }
  async prepareSponsor(channel: string, payer: string, transitionId = id()) {
    const ledger = await this.pullControl(channel),
      current = await this.session.get(this.base(channel))
    ensure(ledger.owner === this.accountId && current.billing, 'FORBIDDEN')
    const context = await this.session.context()
    const payload = {
      transition_id: transitionId,
      to_payer: payer,
      expected_billing_version: current.billing.billing_version
    }
    // Preparation can advance multiple bounded pages. Its immutable transition
    // is authoritative; a fresh command may resume without changing the payer.
    const signed = await signCloudCommand(
      context,
      'dmsg/channel/billing/prepare/v1',
      payload,
      this.session.sign
    )
    await this.account.crypto.call(
      'channelJob',
      channel,
      `sponsor-prepare:${transitionId}`,
      payload
    )
    const transition: any = await this.cloud.post(
      `${this.base(channel)}/billing/prepare`,
      signed,
      context,
      this.session.sign
    )
    ensure(transition.state === 'prepared', 'EXECUTION_UNKNOWN')
    const target = await readCloudSecurity(this.account.user, this.account.agent, {
      accountId: payer,
      issuer: this.trust().namespace + payer,
      homeUser: this.trust().home
    })
    const device = target.devices.find(
      (d) => !d.revoked_at.length && d.input.capabilities.some((c) => 'ContentSign' in c)
    )
    ensure(device, 'AUTH_REQUIRED')
    const intent = {
      transition_id: transitionId,
      expected_billing_version: current.billing.billing_version,
      manifest_digest: transition.manifest_digest,
      bytes: transition.bytes,
      slot_id: transition.slot_id,
      to_payer: payer,
      device: hex(Uint8Array.from(device.input.device_id))
    }
    const approval = await this.session.context(),
      unsigned = {
        protocol: CLOUD_PROTOCOL,
        action: 'dmsg/channel/billing/transfer/v1',
        security_epoch: approval.securityEpoch,
        request_id: approval.requestId,
        deadline: approval.deadline,
        issuer: approval.issuer,
        account_id: approval.accountId,
        device_id: approval.deviceId,
        payload: intent
      }
    const packet = {
      format: 'dmsg-channel-sponsor/1',
      channel,
      genesis: (await this.known(channel)).genesis,
      context: approval,
      unsigned,
      evidence: this.session.state!.verified.evidence,
      proposedAt: Date.now(),
      ownerSignature: b64(
        await this.session.sign(digest('dmsg/channel/sponsor-proposal/v1', unsigned))
      )
    }
    await this.account.crypto.call(
      'channelJob',
      channel,
      `sponsor:${approval.requestId}`,
      packet
    )
    return packet
  }
  async acceptSponsor(packet: any) {
    await this.session.context()
    ensure(
      packet.format === 'dmsg-channel-sponsor/1' &&
        packet.unsigned.payload.to_payer === this.accountId &&
        packet.unsigned.payload.device === this.account.meta.deviceId &&
        packet.context.deadline > Date.now(),
      'FORBIDDEN'
    )
    const owner = await channelDevice(
      packet.evidence,
      packet.context.accountId,
      packet.context.deviceId,
      packet.proposedAt,
      this.trust()
    )
    ensure(
      ed25519.verify(
        unb64(packet.ownerSignature),
        digest('dmsg/channel/sponsor-proposal/v1', packet.unsigned),
        owner.device.signing_pub
      ),
      'AUTH_REQUIRED'
    )
    ensure(
      packet.unsigned.action === 'dmsg/channel/billing/transfer/v1' &&
        packet.unsigned.account_id === packet.context.accountId &&
        packet.unsigned.request_id === packet.context.requestId &&
        packet.unsigned.deadline === packet.context.deadline,
      'INTEGRITY_FAILED'
    )
    return {
      ...packet,
      acceptance: b64(
        await this.session.sign(digest('dmsg/channel/billing-acceptance/v1', packet.unsigned))
      ),
      targetEvidence: this.session.state!.verified.evidence,
      acceptedAt: Date.now()
    }
  }
  async commitSponsor(packet: any) {
    await this.session.context()
    const saved = await this.account.crypto.call(
      'channelJob',
      packet.channel,
      `sponsor:${packet.context.requestId}`
    )
    ensure(
      saved &&
        equal(canonical(saved.unsigned), canonical(packet.unsigned)) &&
        packet.context.accountId === this.accountId &&
        packet.context.deadline > Date.now(),
      'AUTH_REQUIRED'
    )
    const target = await channelDevice(
      packet.targetEvidence,
      packet.unsigned.payload.to_payer,
      packet.unsigned.payload.device,
      packet.acceptedAt,
      this.trust()
    )
    ensure(
      ed25519.verify(
        unb64(packet.acceptance),
        digest('dmsg/channel/billing-acceptance/v1', packet.unsigned),
        target.device.signing_pub
      ),
      'AUTH_REQUIRED'
    )
    const payload = { ...packet.unsigned.payload, signature: packet.acceptance },
      signed = await signCloudCommand(
        packet.context,
        'dmsg/channel/billing/transfer/v1',
        payload,
        this.session.sign
      )
    const key = `sponsor-commit:${packet.context.requestId}`
    if (!(await this.account.crypto.call('channelJob', packet.channel, key)))
      await this.account.crypto.call('channelJob', packet.channel, key, {
        action: 'dmsg/channel/billing/transfer/v1',
        payload,
        context: packet.context,
        signed,
        path: `${this.base(packet.channel)}/billing/transfer`
      })
    const result = await this.post(
      packet.channel,
      key,
      'dmsg/channel/billing/transfer/v1',
      payload,
      '/billing/transfer'
    )
    ensure(
      (result as any).billing_account === packet.unsigned.payload.to_payer,
      'INTEGRITY_FAILED'
    )
    return result
  }
  async cancelSponsor(channel: string, transition: string) {
    return this.post(
      channel,
      `sponsor-cancel:${transition}`,
      'dmsg/channel/billing/cancel/v1',
      { transition_id: transition },
      '/billing/cancel'
    )
  }
  async hints(channel: string, available: () => void) {
    const ticket = id(),
      value = await this.post(
        channel,
        `ticket:${ticket}`,
        'dmsg/channel/ws-ticket/v1',
        { ticket },
        '/ws-ticket'
      )
    ensure(value.ticket === ticket && value.expires_at > Date.now(), 'EXPIRED')
    const url = new URL(`${this.base(channel)}/ws`, this.cloud.origin)
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
    url.searchParams.set('ticket', ticket)
    const socket = new WebSocket(url)
    socket.onmessage = (event) => {
      try {
        const value = JSON.parse(event.data)
        if (value.type === 'available') available()
      } catch {
        socket.close()
      }
    }
    return socket
  }
}
