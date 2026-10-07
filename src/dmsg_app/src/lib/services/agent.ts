import { config } from '../config'
import type {
  DelegationAuthority,
  HostedController,
  PrincipalInfo,
  PrincipalType
} from '../canisters/generated/user'
import { AccountClient, controlResult } from './account'
import type { CloudSession } from './cloud-session'
import { controllerPopMessage } from '../protocol/account'
import { b64 } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import {
  agentId,
  envelope,
  eventHash,
  eventText,
  grantEvent,
  nextNonce,
  revokeEvent,
  type DelegationEvent,
  type Envelope,
  type GrantInput
} from '../protocol/agent'
import { ensure } from '../errors'

const JOBS = 'agent:jobs'

/** Durable record of one locally signed event, from signature to service acceptance. */
export interface AgentJob {
  format: 'dmsg-agent-journal/2'
  id: string
  account: string
  generation: number
  kind: DelegationEvent['type']
  delegationId: string
  envelope: Envelope
  stage: 'signed' | 'accepted' | 'failed'
  credential?: Record<string, unknown>
  error?: string
}

export type Authority =
  { kind: 'all' } | { kind: 'restricted'; scopes: string[]; audiences: string[] }

export interface Credential {
  id: string
  subject: string
  scopes: string[]
  audiences: string[]
  status: 'active' | 'suspended' | 'expired' | 'revoked'
  controller: string
  expires_at?: number
  accepted_at: number
}

export const isCurrent = (c: HostedController) => c.retired_at.length === 0

/**
 * Agent Delegation principal management for the unlocked settings page. The
 * controller keys are ordinary vault entries, so every device holding the
 * root can sign; registration proves possession to the user home, and every
 * signed event is journaled before it is submitted.
 */
export class AgentClient {
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudSession | null
  ) {}

  principalId(account: string) {
    ensure(config.principalOrigin, 'UNAVAILABLE', '尚未配置 principal 地址。')
    return `${config.principalOrigin}/${account}`
  }

  async principal(account: string): Promise<PrincipalInfo | null> {
    const result = await this.account.user.get_principal(xidBytes(account))
    if ('Err' in result && 'NotFound' in result.Err) return null
    const info = controlResult(result)
    ensure(info.principal_id === this.principalId(account), 'INTEGRITY_FAILED')
    return info
  }

  enable(account: string, type: 'Person' | 'Organization' | 'Team' | 'Project' | 'Other') {
    return this.account.mutate(account, {
      EnablePrincipal: { principal_type: { [type]: null } as PrincipalType }
    })
  }

  /** The key of a generation, created in the vault on first use. */
  async controllerKey(account: string, generation: number) {
    const { publicKey } = await this.account.crypto.call(
      'controllerKey',
      account,
      generation,
      'create'
    )
    return { publicKey, agentId: agentId(publicKey) }
  }

  async register(
    account: string,
    input: { authority: Authority; name?: string; supersedes: number[] }
  ) {
    const principal = await this.principal(account)
    ensure(principal, 'NOT_FOUND', '请先启用 principal。')
    const generation = (principal.state.controllers.at(-1)?.generation ?? 0) + 1
    const { publicKey } = await this.controllerKey(account, generation)
    const delegation: DelegationAuthority =
      input.authority.kind === 'all'
        ? { Unrestricted: null }
        : {
            Restricted: {
              scopes: input.authority.scopes,
              audiences: input.authority.audiences
            }
          }
    await this.account.mutate(account, async (_, requestId) => ({
      RegisterController: {
        generation,
        public_key: publicKey,
        name: input.name ? [input.name] : [],
        delegation,
        supersedes: input.supersedes,
        proof: (
          await this.account.crypto.call(
            'controllerKey',
            account,
            generation,
            'sign',
            controllerPopMessage(
              this.account.home,
              xidBytes(account),
              generation,
              delegation,
              input.supersedes,
              requestId
            )
          )
        ).signature
      }
    }))
    return generation
  }

  retire(account: string, generation: number) {
    return this.account.mutate(account, { RetireController: { generation } })
  }

  compromise(account: string, generation: number, invalidFrom: number) {
    return this.account.mutate(account, {
      MarkControllerCompromised: { generation, invalid_from: BigInt(invalidFrom) }
    })
  }

  rename(account: string, generation: number, name: string) {
    return this.account.mutate(account, {
      RenameController: { generation, name: name ? [name] : [] }
    })
  }

  async publish(account: string) {
    return controlResult(await this.account.user.publish_principal(xidBytes(account)))
  }

  private key(id: string) {
    return `agent:${id}`
  }

  async job(id: string): Promise<AgentJob | null> {
    const value = await this.account.crypto.call('controlGet', this.key(id))
    return value ? (JSON.parse(value) as AgentJob) : null
  }

  async jobs(): Promise<AgentJob[]> {
    const ids = JSON.parse(
      (await this.account.crypto.call('controlGet', JOBS)) ?? '[]'
    ) as string[]
    const jobs = await Promise.all(ids.map((id) => this.job(id)))
    return jobs.filter((job): job is AgentJob => job !== null)
  }

  private async save(job: AgentJob) {
    await this.account.crypto.call('controlPut', this.key(job.id), JSON.stringify(job))
    const ids = JSON.parse(
      (await this.account.crypto.call('controlGet', JOBS)) ?? '[]'
    ) as string[]
    if (!ids.includes(job.id))
      await this.account.crypto.call('controlPut', JOBS, JSON.stringify([job.id, ...ids].slice(0, 32)))
  }

  async grant(account: string, generation: number, input: Omit<GrantInput, 'principalId'>) {
    return this.sign(account, generation, (actor, nonce, createdAt) =>
      grantEvent(actor, nonce, { ...input, principalId: this.principalId(account) }, createdAt)
    )
  }

  async revoke(account: string, generation: number, id: string) {
    return this.sign(account, generation, (actor, nonce, createdAt) =>
      revokeEvent(actor, nonce, id, this.principalId(account), createdAt)
    )
  }

  /** Sign with the vault key, journal the envelope, then submit it. */
  private async sign(
    account: string,
    generation: number,
    build: (actor: string, nonce: number, createdAt: number) => DelegationEvent
  ) {
    ensure(typeof chrome !== 'undefined' && chrome.runtime?.id, 'FORBIDDEN')
    const principal = await this.principal(account)
    const controller = principal?.state.controllers.find((c) => c.generation === generation)
    ensure(controller && isCurrent(controller), 'FORBIDDEN', '只能使用当前 controller 签名。')
    const { device } = await this.account.refresh(account)
    ensure(
      device && device.input.capabilities.some((c) => 'FormalApprove' in c),
      'Forbidden',
      '当前设备没有正式批准能力。'
    )
    const nonceKey = `agent:nonce:${account}:${generation}`
    const last = BigInt((await this.account.crypto.call('controlGet', nonceKey)) ?? '0')
    // A new binding starts at valid_from; the signer may not backdate before it.
    const createdAt = Math.max(Date.now(), Number(controller.valid_from))
    const nonce = nextNonce(last, createdAt)
    const event = build(agentId(Uint8Array.from(controller.public_key)), nonce, createdAt)
    const hash = eventHash(eventText(event))
    const { publicKey, signature } = await this.account.crypto.call(
      'controllerKey',
      account,
      generation,
      'sign',
      hash
    )
    ensure(
      b64(publicKey) === b64(Uint8Array.from(controller.public_key)),
      'INTEGRITY_FAILED',
      '本机 vault 中的 controller key 与已登记的公钥不一致。'
    )
    await this.account.crypto.call('controlPut', nonceKey, String(nonce))
    const signed = envelope(event, hash, signature)
    const job: AgentJob = {
      format: 'dmsg-agent-journal/2',
      id: signed.hash,
      account,
      generation,
      kind: event.type,
      delegationId: String(event.payload.id),
      envelope: signed,
      stage: 'signed'
    }
    await this.save(job)
    return this.submit(job)
  }

  /** Resubmit a signed envelope the service has not confirmed. */
  async resume(id: string) {
    const job = await this.job(id)
    ensure(job && job.account === this.account.meta.account?.id, 'AUTH_REQUIRED')
    return job.stage === 'signed' ? this.submit(job) : job
  }

  /** Exact resubmission is idempotent; a stale envelope needs a new signature. */
  private async submit(job: AgentJob) {
    ensure(config.agentOrigin, 'UNAVAILABLE', '尚未配置 delegation 服务地址。')
    // Even past the live window the service still answers an accepted envelope.
    const response = await fetch(`${config.agentOrigin}/v1/delegations`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(job.envelope),
      signal: AbortSignal.timeout(20_000)
    })
    const body = (await response.json().catch(() => null)) as Record<string, any> | null
    if (response.ok && body) {
      ensure(
        body.id === job.delegationId && body.event_id === job.envelope.hash,
        'INTEGRITY_FAILED'
      )
      job.credential = body
      job.stage = 'accepted'
      delete job.error
    } else {
      const code = body?.error?.code ?? `HTTP ${response.status}`
      job.error = code
      // A transient failure says nothing about an earlier acceptance. Exact
      // resubmission remains a lookup even after the new-admission window.
      if (response.status < 500 && ![408, 429].includes(response.status)) job.stage = 'failed'
    }
    await this.save(job)
    return job
  }

  async credentials(account: string): Promise<Credential[]> {
    ensure(this.cloud, 'UNAVAILABLE', '尚未配置云端服务。')
    const credentials: Credential[] = []
    let cursor: string | undefined
    do {
      const page: { result: Credential[]; next_cursor?: string } = await this.cloud.get(
        `/v1/accounts/${account}/delegations?limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`
      )
      credentials.push(...page.result)
      cursor = page.next_cursor
    } while (cursor)
    return credentials
  }

  /** After a controller change is published, drop the service's document cache. */
  async refreshService(account: string) {
    ensure(this.cloud, 'UNAVAILABLE', '尚未配置云端服务。')
    const context = await this.cloud.context()
    return this.cloud.cloud.postRaw(
      `/v1/accounts/${account}/principal/refresh`,
      null,
      context,
      this.cloud.sign
    )
  }
}
