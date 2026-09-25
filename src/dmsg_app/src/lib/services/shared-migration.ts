import { readCloudSecurity } from './cloud-security'
import type { CloudSession } from './cloud-session'
import { legacyHistoryScopeSchema, type LegacyHistoryGrant } from '../protocol/shared-history'
import { IC_ROOT_KEY } from '@icp-sdk/core/agent'
import {
  controlsLegacyIdentity,
  managerConsentDigest,
  memberClaimDigest,
  proposalDigest,
  sharedProposalSchema,
  sharedSourceKey,
  verifyLegacyAttestation,
  verifySharedSource,
  type SharedProposal,
  type SharedSource,
  type SharedSourceInput,
  type SnapshotProof,
  type MemberClaim
} from '@dmsg/legacy'
import { AccountClient } from './account'
import { CloudClient } from './relay'
import { ChannelClient } from './channel'
import { verifyChannelEvent } from './channel-proof'
import { signCloudCommand, type CloudAction, type CloudSigned } from '../protocol/cloud'
import { b64, canonical, equal, hash, hex, id, unb64, unhex } from '../protocol/codec'
import type { CloudSecurityEvidence } from './cloud-security'
import { ensure } from '../errors'

export type LegacyProofWire = Omit<
  SnapshotProof,
  'args' | 'nonce' | 'requestId' | 'certificate' | 'reply'
> & { args: string; nonce: string; requestId: string; certificate: string; reply: string }
export const encodeLegacyProof = (p: SnapshotProof): LegacyProofWire => ({
  ...p,
  args: b64(p.args),
  nonce: b64(p.nonce),
  requestId: b64(p.requestId),
  certificate: b64(p.certificate),
  reply: b64(p.reply)
})
export const decodeLegacyProof = (p: LegacyProofWire): SnapshotProof => ({
  ...p,
  args: unb64(p.args, 2048),
  nonce: unb64(p.nonce, 32),
  requestId: unb64(p.requestId, 32),
  certificate: unb64(p.certificate, 65536),
  reply: unb64(p.reply, 196608)
})
export interface SharedSourceFile {
  format: 'dmsg-legacy-shared-source/1'
  source: string
  identity: string
  cutover: string
  input: { channel: number; authority: LegacyProofWire; authorities: LegacyProofWire[] }
}
export interface LegacyApproval {
  proof: LegacyProofWire
  expires_at: number
}
export interface MigrationDraft {
  format: 'dmsg-shared-migration-draft/1'
  proposal: SharedProposal
  source: SharedSourceFile['input']
  genesis: { signed: CloudSigned; evidence: CloudSecurityEvidence; approved_at: number }
  source_file: SharedSourceFile
}
export interface SharedView {
  key: string
  proposal: SharedProposal
  proposal_digest: string
  source: SharedSource
  material: SharedSourceFile['input']
  genesis: MigrationDraft['genesis']
  stage: 'reserving' | 'pending' | 'committed' | 'active'
  votes: Record<string, { account: string; manager: string; approval: LegacyApproval }>
  grants: Record<
    string,
    {
      grant: LegacyHistoryGrant
      signed: CloudSigned
      evidence: CloudSecurityEvidence
      stored_at: number
    }
  >
  claims: Record<
    string,
    {
      claim: MemberClaim
      approval: LegacyApproval
      joined: boolean
      acceptance: { signed: CloudSigned; evidence: CloudSecurityEvidence; stored_at: number }
    }
  >
}
export class SharedMigrationClient {
  readonly channels: ChannelClient
  readonly session: CloudSession
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudClient,
    readonly accountId: string,
    readonly legacy: {
      channels: string[]
      identity: string
      cutover: string
      rootKey?: string
    }
  ) {
    this.channels = new ChannelClient(account, cloud, accountId)
    this.session = this.channels.session
  }
  private root() {
    return unhex(this.legacy.rootKey ?? IC_ROOT_KEY)
  }
  async source(file: SharedSourceFile) {
    ensure(
      file.format === 'dmsg-legacy-shared-source/1' &&
        this.legacy.channels.includes(file.source) &&
        file.identity === this.legacy.identity &&
        file.cutover === this.legacy.cutover,
      'FORBIDDEN',
      '来源不在已审核的冻结配置中。'
    )
    const input: SharedSourceInput = {
      channel: file.input.channel,
      authority: decodeLegacyProof(file.input.authority),
      authorities: file.input.authorities.map(decodeLegacyProof)
    }
    return verifySharedSource(input, {
      rootKey: this.root(),
      source: file.source,
      identity: file.identity,
      cutover: unhex(file.cutover)
    })
  }
  async prepare(file: SharedSourceFile, version = 1): Promise<MigrationDraft> {
    const source = await this.source(file),
      channel = id(),
      context = await this.session.context()
    const signed = await signCloudCommand(
      context,
      'dmsg/channel/genesis/v1',
      { channel_id: channel, type: 'collaboration', nonce: id() },
      this.session.sign
    )
    const proposal: SharedProposal = {
      format: 'dmsg-legacy-shared-proposal/1',
      source: source.source,
      channel: source.channel,
      source_digest: source.digest,
      version,
      owner: this.accountId,
      channel_id: channel,
      genesis_digest: hash(unb64(signed.cose_sign1)),
      nonce: id()
    }
    const draft: MigrationDraft = {
      format: 'dmsg-shared-migration-draft/1',
      proposal,
      source: file.input,
      genesis: {
        signed,
        evidence: this.session.state!.verified.evidence,
        approved_at: Date.now()
      },
      source_file: file
    }
    await this.account.crypto.call('channelRemember', {
      channel,
      name: `旧频道 ${source.channel} 的继承方案`,
      genesis: proposal.genesis_digest
    })
    await this.account.crypto.call('channelJob', channel, 'legacy-draft', draft)
    return draft
  }
  async validateDraft(draft: MigrationDraft) {
    ensure(draft.format === 'dmsg-shared-migration-draft/1', 'INVALID_INPUT')
    const source = await this.source(draft.source_file),
      proposal = sharedProposalSchema.parse(draft.proposal)
    ensure(
      proposal.source_digest === source.digest &&
        proposal.source === source.source &&
        proposal.channel === source.channel &&
        equal(canonical(draft.source), canonical(draft.source_file.input)) &&
        hash(unb64(draft.genesis.signed.cose_sign1)) === proposal.genesis_digest,
      'INTEGRITY_FAILED'
    )
    await this.session.context()
    const prefix = this.session.state!.info.issuer.slice(0, -this.accountId.length)
    const checked = await verifyChannelEvent(
      {
        signed: draft.genesis.signed,
        evidence: draft.genesis.evidence,
        stored_at: draft.genesis.approved_at
      },
      { home: this.account.home.toText(), namespace: prefix, agent: this.account.agent }
    )
    ensure(
      checked.account === proposal.owner &&
        checked.body.action === 'dmsg/channel/genesis/v1' &&
        checked.body.payload.channel_id === proposal.channel_id,
      'INTEGRITY_FAILED'
    )
    return source
  }
  async managerChallenge(draft: MigrationDraft, manager: string) {
    const source = await this.validateDraft(draft)
    ensure(source.managers.includes(manager), 'FORBIDDEN')
    return {
      format: 'dmsg-legacy-approval-request/1',
      kind: 'manager',
      proposal: draft.proposal,
      identity: manager,
      digest: hex(managerConsentDigest(draft.proposal, manager)),
      expires_at: Date.now() + 7 * 86400000 - 60000
    }
  }
  private async validateApproval(
    source: SharedSource,
    identity: string,
    digest: Uint8Array,
    approval: LegacyApproval,
    at = Date.now()
  ) {
    const caller = await verifyLegacyAttestation(
      decodeLegacyProof(approval.proof),
      digest,
      approval.expires_at,
      source,
      this.root(),
      at
    )
    ensure(
      controlsLegacyIdentity(source, identity, caller),
      'FORBIDDEN',
      '需要冻结时的该身份控制者；共享名称只接受冻结管理员。'
    )
  }
  private async post(
    key: string,
    action: CloudAction,
    payload: Record<string, unknown>,
    route: string,
    channel: string
  ) {
    const journalKey = `legacy-http:${key}:${route}`,
      prior = await this.account.crypto.call('channelJob', channel, journalKey)
    let resume = prior && !prior.complete ? prior : null
    if (resume) {
      const status = (await this.session.get(
        `/v1/legacy/${key}/operations/${resume.requestId}`
      )) as { found: boolean; result: unknown }
      ensure(typeof status.found === 'boolean', 'INTEGRITY_FAILED')
      if (!equal(canonical(resume.payload), canonical(payload))) {
        // Read the canonical directory before replacing a lost-reply journal.
        // The old signed request may still be processing until its deadline.
        if (status.found) await this.view(key)
        else
          ensure(
            !resume.deadline || Date.now() > resume.deadline + 60000,
            'EXECUTION_UNKNOWN',
            '原共享操作仍可能完成，请稍后对账。'
          )
        await this.account.crypto.call(
          'channelJob',
          channel,
          `${journalKey}:${resume.requestId}`,
          resume
        )
        resume = null
      }
    }
    const requestId = resume ? resume.requestId : id(),
      context = await this.session.context(requestId)
    const signed = await signCloudCommand(context, action, payload, this.session.sign)
    await this.account.crypto.call('channelJob', channel, journalKey, {
      requestId,
      payload,
      deadline: context.deadline,
      complete: false
    })
    const result = await this.cloud.post(
      `/v1/legacy/${key}/${route}`,
      signed,
      context,
      this.session.sign
    )
    await this.account.crypto.call('channelJob', channel, journalKey, {
      requestId,
      payload,
      deadline: context.deadline,
      complete: true
    })
    return this.validateView(result as SharedView)
  }
  async propose(draft: MigrationDraft, manager: string, approval: LegacyApproval) {
    const source = await this.validateDraft(draft)
    ensure(draft.proposal.owner === this.accountId, 'FORBIDDEN')
    await this.validateApproval(
      source,
      manager,
      managerConsentDigest(draft.proposal, manager),
      approval
    )
    return this.post(
      sharedSourceKey(source.source, source.channel),
      'dmsg/legacy/propose/v1',
      {
        proposal: draft.proposal,
        source: draft.source,
        genesis: draft.genesis,
        manager,
        approval
      },
      'proposals',
      draft.proposal.channel_id
    )
  }
  async consent(draft: MigrationDraft, manager: string, approval: LegacyApproval) {
    const source = await this.validateDraft(draft)
    await this.validateApproval(
      source,
      manager,
      managerConsentDigest(draft.proposal, manager),
      approval
    )
    await this.account.crypto.call('channelRemember', {
      channel: draft.proposal.channel_id,
      name: `旧频道 ${source.channel} 的继承方案`,
      genesis: draft.proposal.genesis_digest
    })
    return this.post(
      sharedSourceKey(source.source, source.channel),
      'dmsg/legacy/consent/v1',
      { proposal_digest: proposalDigest(draft.proposal), manager, approval },
      'consents',
      draft.proposal.channel_id
    )
  }
  async validateView(view: SharedView) {
    const file: SharedSourceFile = {
      format: 'dmsg-legacy-shared-source/1',
      source: view.proposal.source,
      identity: this.legacy.identity,
      cutover: this.legacy.cutover,
      input: view.material
    }
    const source = await this.validateDraft({
      format: 'dmsg-shared-migration-draft/1',
      proposal: view.proposal,
      source: view.material,
      genesis: view.genesis,
      source_file: file
    })
    ensure(
      ['reserving', 'pending', 'committed', 'active'].includes(view.stage),
      'INTEGRITY_FAILED'
    )
    view = { ...view, source }
    ensure(
      view.key === sharedSourceKey(source.source, source.channel) &&
        source.digest === view.proposal.source_digest &&
        proposalDigest(view.proposal) === view.proposal_digest,
      'INTEGRITY_FAILED'
    )
    for (const [manager, vote] of Object.entries(view.votes)) {
      ensure(manager === vote.manager && source.managers.includes(manager), 'INTEGRITY_FAILED')
      ensure(
        vote.approval.expires_at > Date.now() || ['committed', 'active'].includes(view.stage),
        'EXPIRED',
        '管理员同意已过期，需要对同一方案重新批准。'
      )
      await this.validateApproval(
        source,
        manager,
        managerConsentDigest(view.proposal, manager),
        vote.approval,
        ['committed', 'active'].includes(view.stage)
          ? Math.min(Date.now(), vote.approval.expires_at - 1)
          : Date.now()
      )
    }
    if (['committed', 'active'].includes(view.stage))
      ensure(
        source.managers.every((m) => view.votes[m]),
        'RECOVERY_INCOMPLETE'
      )
    for (const [member, value] of Object.entries(view.claims)) {
      ensure(
        member === value.claim.member &&
          source.members.includes(member) &&
          value.claim.proposal_digest === view.proposal_digest,
        'INTEGRITY_FAILED'
      )
      await this.validateApproval(
        source,
        member,
        memberClaimDigest(value.claim),
        value.approval,
        Math.min(Date.now(), value.approval.expires_at - 1)
      )
      const accepted = await verifyChannelEvent(value.acceptance, {
        home: this.account.home.toText(),
        namespace: this.session.state!.info.issuer.slice(0, -this.accountId.length),
        agent: this.account.agent
      })
      ensure(
        accepted.body.action === 'dmsg/legacy/claim/v1' &&
          accepted.account === value.claim.account &&
          accepted.deviceId === value.claim.device &&
          equal(canonical(accepted.body.payload.claim), canonical(value.claim)),
        'INTEGRITY_FAILED'
      )
    }
    await this.account.crypto.call(
      'channelJob',
      view.proposal.channel_id,
      'legacy-directory',
      view
    )
    return view
  }
  async view(key: string) {
    return this.validateView((await this.session.get(`/v1/legacy/${key}`)) as SharedView)
  }
  async commit(view: SharedView) {
    await this.validateView(view)
    ensure(
      view.proposal.owner === this.accountId &&
        view.source.managers.every((manager) => view.votes[manager]),
      'FORBIDDEN'
    )
    return this.post(
      view.key,
      'dmsg/legacy/commit/v1',
      { proposal_digest: view.proposal_digest },
      'commit',
      view.proposal.channel_id
    )
  }
  async activate(view: SharedView) {
    view = await this.view(view.key)
    ensure(
      ['committed', 'active'].includes(view.stage) && view.proposal.owner === this.accountId,
      'FORBIDDEN'
    )
    const context = await this.session.context(),
      payload = { source_key: view.key, proposal_digest: view.proposal_digest }
    await this.cloud.post(
      `/v1/channels/${view.proposal.channel_id}/legacy-activate`,
      await signCloudCommand(context, 'dmsg/legacy/activate/v1', payload, this.session.sign),
      context,
      this.session.sign
    )
    await this.channels.pullControl(view.proposal.channel_id)
    await this.channels.rotate(view.proposal.channel_id)
    return this.view(view.key)
  }
  async claimChallenge(view: SharedView, member: string) {
    await this.validateView(view)
    await this.session.context()
    ensure(view.source.members.includes(member), 'FORBIDDEN')
    const claim: MemberClaim = {
      format: 'dmsg-legacy-member-claim/1',
      proposal_digest: view.proposal_digest,
      member,
      account: this.accountId,
      device: this.account.meta.deviceId,
      hpke_pub: hex(unb64(this.account.meta.hpkePublic)),
      security_epoch: this.session.state!.verified.securityEpoch
    }
    return {
      format: 'dmsg-legacy-approval-request/1',
      kind: 'member',
      proposal: view.proposal,
      claim,
      identity: member,
      digest: hex(memberClaimDigest(claim)),
      expires_at: Date.now() + 7 * 86400000 - 60000
    }
  }
  async claim(view: SharedView, claim: MemberClaim, approval: LegacyApproval) {
    await this.validateView(view)
    await this.validateApproval(view.source, claim.member, memberClaimDigest(claim), approval)
    return this.post(
      view.key,
      'dmsg/legacy/claim/v1',
      { claim, approval },
      'claims',
      view.proposal.channel_id
    )
  }
  async shareHistory(view: SharedView, archiveKey: string, member: string, grantId = id()) {
    view = await this.view(view.key)
    const claim = view.claims[member]
    ensure(claim?.joined, 'FORBIDDEN')
    const ledger = await this.channels.pullControl(view.proposal.channel_id)
    ensure(['owner', 'admin'].includes(ledger.members[this.accountId]?.role), 'FORBIDDEN')
    const target = await readCloudSecurity(this.account.user, this.account.agent, {
      accountId: claim.claim.account,
      issuer:
        this.session.state!.info.issuer.slice(0, -this.accountId.length) + claim.claim.account,
      homeUser: this.account.home.toText()
    })
    await this.cloud.publishSecurity(target.evidence)
    const device = target.devices.find(
      (d) =>
        hex(Uint8Array.from(d.input.device_id)) === claim.claim.device &&
        !d.revoked_at.length &&
        d.input.capabilities.some((c) => 'ContentSign' in c)
    )
    const recovery = target.snapshot.recovery_hpke_pub,
      generation = Number(target.snapshot.recovery_root_version)
    ensure(device && recovery instanceof Uint8Array && generation > 0, 'AUTH_REQUIRED')
    const scope = {
      format: 'dmsg-legacy-history-grant/1' as const,
      directory_key: view.key,
      proposal_digest: view.proposal_digest,
      source_digest: view.source.digest,
      grant_id: grantId,
      channel_id: view.proposal.channel_id,
      epoch: ledger.epoch,
      control_head: ledger.head,
      member,
      account: claim.claim.account,
      device: claim.claim.device,
      security_epoch: target.securityEpoch,
      recovery_generation: generation
    }
    const recipients = [
      {
        recipient: `device:${scope.account}:${scope.device}`,
        hpke_pub: hex(Uint8Array.from(device.input.hpke_pub))
      },
      { recipient: `recovery:${scope.account}:${generation}`, hpke_pub: hex(recovery) }
    ]
    const prepared = await this.account.crypto.call('legacyPrepareGrant', {
      archiveKey,
      source: view.source,
      scope,
      recipients
    })
    for (const upload of prepared.uploads)
      await this.channels.uploadPrepared(view.proposal.channel_id, upload)
    return this.post(
      view.key,
      'dmsg/legacy/history/v1',
      prepared.grant as unknown as Record<string, unknown>,
      'history',
      view.proposal.channel_id
    )
  }
  async receiveHistory(view: SharedView, grantId: string, recoveryCode?: string) {
    view = await this.view(view.key)
    const stored = view.grants[grantId]
    ensure(stored, 'NOT_FOUND')
    const checked = await verifyChannelEvent(stored, {
        home: this.account.home.toText(),
        namespace: this.session.state!.info.issuer.slice(0, -this.accountId.length),
        agent: this.account.agent
      }),
      grant = stored.grant,
      scope = legacyHistoryScopeSchema.parse(grant.scope)
    ensure(
      checked.body.action === 'dmsg/legacy/history/v1' &&
        equal(canonical(checked.body.payload), canonical(grant)) &&
        scope.account === this.accountId &&
        (!!recoveryCode || scope.device === this.account.meta.deviceId) &&
        scope.directory_key === view.key &&
        scope.proposal_digest === view.proposal_digest &&
        scope.source_digest === view.source.digest &&
        view.claims[scope.member]?.claim.account === this.accountId,
      'FORBIDDEN'
    )
    await this.channels.pullControl(view.proposal.channel_id)
    const opened = await this.account.crypto.call('legacyOpenGrant', grant, recoveryCode),
      parts: string[] = []
    for (const [index, file] of opened.files.entries()) {
      const base = `/v1/channels/${view.proposal.channel_id}/objects/${file.upload_id}`
      const descriptor = (await this.session.get(base)) as any
      const owner = await verifyChannelEvent(descriptor, {
        home: this.account.home.toText(),
        namespace: this.session.state!.info.issuer.slice(0, -this.accountId.length),
        agent: this.account.agent
      })
      ensure(
        owner.account === checked.account &&
          owner.body.action === 'dmsg/upload/reserve/v1' &&
          equal(canonical(owner.body.payload), canonical(descriptor.plan)) &&
          descriptor.plan.epoch === scope.epoch &&
          descriptor.plan.manifest_digest === file.manifest_digest,
        'INTEGRITY_FAILED'
      )
      const manifestBytes = await this.cloud.getChunk(
        `${base}/chunks/manifest`,
        file.manifest_digest,
        await this.session.context(),
        this.session.sign
      )
      const manifest = await this.account.crypto.call(
        'legacyGrantManifest',
        opened.recordId,
        index,
        manifestBytes
      )
      ensure(
        manifest.chunks.length === descriptor.plan.chunks.length &&
          manifest.chunks.every(
            (c: { digest: string }, i: number) => c.digest === descriptor.plan.chunks[i].digest
          ),
        'INTEGRITY_FAILED'
      )
      const chunks: Uint8Array[] = []
      for (let i = 0; i < descriptor.plan.chunks.length; i++)
        chunks.push(
          await this.cloud.getChunk(
            `${base}/chunks/${i}`,
            descriptor.plan.chunks[i].digest,
            await this.session.context(),
            this.session.sign
          )
        )
      parts.push(
        (await this.account.crypto.call('legacyGrantPart', opened.recordId, index, chunks)).key
      )
    }
    return this.account.crypto.call(
      'legacyReceiveScoped',
      parts,
      view.source,
      scope.archive_digest
    )
  }
  async joined(view: SharedView) {
    return this.post(
      view.key,
      'dmsg/legacy/joined/v1',
      { proposal_digest: view.proposal_digest },
      'joined',
      view.proposal.channel_id
    )
  }
}
