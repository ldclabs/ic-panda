import { config } from '../config'
import type { _SERVICE as CoseService } from '../canisters/generated/cose'
import type {
  AgentEventSignRequest,
  DelegationAuthority,
  ExecutionResult,
  HostedController,
  PrincipalInfo,
  PrincipalType
} from '../canisters/generated/user'
import { AccountClient, controlResult } from './account'
import type { CloudSession } from './cloud-session'
import { recordedExecution } from './cose'
import { decodeControl, encodeControl } from '../protocol/account'
import { digest, equal, hex, unhex } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import {
  agentExecutionKind,
  agentId,
  envelope,
  eventHash,
  eventText,
  executionApprovalMessage,
  grantEvent,
  nextNonce,
  revokeEvent,
  type DelegationEvent,
  type Envelope,
  type GrantInput
} from '../protocol/agent'
import { ed25519 } from '../crypto/primitives'
import { ensure } from '../errors'

const MAX_CYCLES = 100_000_000_000n
const WINDOW = 300_000
const JOBS = 'agent:jobs'

/** Durable record of one hosted signature, from approval to service acceptance. */
export interface AgentJob {
  format: 'dmsg-agent-journal/1'
  account: string
  executionId: string
  generation: number
  kind: DelegationEvent['type']
  delegationId: string
  request: string
  stage: 'unknown' | 'signed' | 'accepted' | 'failed'
  envelope?: Envelope
  credential?: Record<string, unknown>
  error?: string
}

export type Authority = { kind: 'all' } | { kind: 'restricted'; scopes: string[]; audiences: string[] }

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
 * Agent Delegation principal management for the unlocked settings page. Every
 * signature is an explicit settings action approved by this device; the
 * encrypted journal is written before dispatch so an unknown outcome is
 * reconciled with the same request instead of signing again.
 */
export class AgentClient {
  constructor(
    readonly account: AccountClient,
    readonly cose: CoseService,
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

  /** The key of a generation, derived by COSE; shown to the owner before approval. */
  async controllerKey(account: string, generation: number) {
    const key = controlResult(
      await this.cose.public_key(xidBytes(account), { AgentController: { generation } })
    )
    const { info } = await this.account.refresh(account)
    ensure(
      'AgentController' in key.purpose &&
        'Ed25519' in key.algorithm &&
        key.key_generation === BigInt(generation) &&
        equal(Uint8Array.from(key.account_id), xidBytes(account)) &&
        key.home_cose.toText() === info.home_cose.toText() &&
        key.public_key.length === 32,
      'INTEGRITY_FAILED'
    )
    const publicKey = Uint8Array.from(key.public_key)
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
    await this.account.mutate(
      account,
      {
        RegisterController: {
          generation,
          public_key: publicKey,
          name: input.name ? [input.name] : [],
          delegation,
          supersedes: input.supersedes
        }
      },
      undefined,
      'register_controller'
    )
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

  private key(executionId: string) {
    return `agent:${executionId}`
  }

  async job(executionId: string): Promise<AgentJob | null> {
    const value = await this.account.crypto.call('controlGet', this.key(executionId))
    return value ? (JSON.parse(value) as AgentJob) : null
  }

  async jobs(): Promise<AgentJob[]> {
    const ids = JSON.parse((await this.account.crypto.call('controlGet', JOBS)) ?? '[]') as string[]
    const jobs = await Promise.all(ids.map((id) => this.job(id)))
    return jobs.filter((job): job is AgentJob => job !== null)
  }

  private async save(job: AgentJob) {
    await this.account.crypto.call('controlPut', this.key(job.executionId), JSON.stringify(job))
    const ids = JSON.parse((await this.account.crypto.call('controlGet', JOBS)) ?? '[]') as string[]
    if (!ids.includes(job.executionId))
      await this.account.crypto.call(
        'controlPut',
        JOBS,
        JSON.stringify([job.executionId, ...ids].slice(0, 32))
      )
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

  /** Approve, sign in the user home, then submit to the delegation service. */
  private async sign(
    account: string,
    generation: number,
    build: (actor: string, nonce: number, createdAt: number) => DelegationEvent
  ) {
    ensure(typeof chrome !== 'undefined' && chrome.runtime?.id, 'FORBIDDEN')
    const principal = await this.principal(account)
    const controller = principal?.state.controllers.find((c) => c.generation === generation)
    ensure(controller && isCurrent(controller), 'FORBIDDEN', '只能使用当前 controller 签名。')
    const { info, device } = await this.account.refresh(account)
    ensure(
      device &&
        device.input.capabilities.some((c) => 'FormalApprove' in c) &&
        !info.sensitive_policy.frozen &&
        info.sensitive_policy.allowed_purposes.some((p) => 'AgentController' in p),
      'Forbidden',
      '当前设备没有正式批准能力，或账户策略不允许托管 controller 签名。'
    )
    const now = Date.now()
    const last = principal!.last_nonces.find(([g]) => g === generation)?.[1] ?? 0n
    // A new binding starts at valid_from; the signer may not backdate before it.
    const createdAt = Math.max(now, Number(controller.valid_from))
    const event = build(
      agentId(Uint8Array.from(controller.public_key)),
      nextNonce(last, createdAt),
      createdAt
    )
    const text = eventText(event)
    const accountId = xidBytes(account),
      deviceId = unhex(this.account.meta.deviceId),
      origin = `chrome-extension://${chrome.runtime.id}`
    const requestId = digest('dmsg/execution-request/v2', [
      accountId,
      info.security_epoch,
      deviceId,
      device.next_sequence
    ])
    const request: AgentEventSignRequest = {
      account_id: accountId,
      generation,
      event: text,
      origin,
      max_cycles: MAX_CYCLES,
      approval: {
        device_id: deviceId,
        security_epoch: info.security_epoch,
        sequence: device.next_sequence,
        request_id: requestId,
        expires_at: BigInt(now + 240_000),
        signature: new Uint8Array()
      }
    }
    request.approval.signature = await this.account.crypto.call(
      'deviceSign',
      executionApprovalMessage({
        home: this.account.home.toUint8Array(),
        account: accountId,
        deviceId,
        securityEpoch: info.security_epoch,
        sequence: device.next_sequence,
        requestId,
        expiresAt: request.approval.expires_at,
        kind: agentExecutionKind(generation, text, principal!.principal_id, origin),
        maxCycles: MAX_CYCLES
      })
    )
    // Persist before dispatch: termination can follow the home's commit.
    const job: AgentJob = {
      format: 'dmsg-agent-journal/1',
      account,
      executionId: hex(requestId),
      generation,
      kind: event.type,
      delegationId: String(event.payload.id),
      request: encodeControl('sign_agent_event', [request]),
      stage: 'unknown'
    }
    await this.save(job)
    return this.dispatch(job, request)
  }

  private async dispatch(job: AgentJob, request: AgentEventSignRequest) {
    let response: Awaited<ReturnType<AccountClient['user']['sign_agent_event']>>
    try {
      response = await this.account.user.sign_agent_event(request)
    } catch {
      job.error = 'EXECUTION_UNKNOWN'
      await this.save(job)
      return job
    }
    if ('Err' in response) {
      // The home may already have authorized and reserved this execution before
      // COSE rejected dispatch. Keep the original request for reconciliation.
      job.error = Object.keys(response.Err)[0]
      await this.save(job)
      return job
    }
    return this.finish(job, response.Ok)
  }

  /** Reconcile an unknown execution or resubmit a signed envelope. */
  async resume(executionId: string) {
    const job = await this.job(executionId)
    ensure(job && job.account === this.account.meta.account?.id, 'AUTH_REQUIRED')
    if (job.stage === 'signed') return this.submit(job)
    if (job.stage !== 'unknown') return job
    const request = decodeControl('sign_agent_event', job.request)[0] as AgentEventSignRequest
    const result = await recordedExecution(
      this.account.user,
      xidBytes(job.account),
      unhex(job.executionId)
    )
    if (!result) {
      if (Date.now() >= Number(request.approval.expires_at)) {
        job.stage = 'failed'
        job.error = 'Expired'
        await this.save(job)
        return job
      }
      return this.dispatch(job, request)
    }
    return this.finish(job, result)
  }

  private async finish(job: AgentJob, result: ExecutionResult) {
    ensure(equal(Uint8Array.from(result.request_id), unhex(job.executionId)), 'INTEGRITY_FAILED')
    if (!('Completed' in result.outcome)) {
      if ('Failed' in result.outcome) {
        job.stage = 'failed'
        job.error = Object.keys(result.outcome.Failed)[0]
      } else if ('ResultExpired' in result.outcome) {
        job.stage = 'failed'
        job.error = 'ResultExpired'
      } else {
        job.stage = 'unknown'
        job.error = 'Unknown' in result.outcome ? Object.keys(result.outcome.Unknown)[0] : 'Pending'
      }
      await this.save(job)
      return job
    }
    const output = result.outcome.Completed
    ensure('AgentSignature' in output, 'INTEGRITY_FAILED')
    const request = decodeControl('sign_agent_event', job.request)[0] as AgentEventSignRequest
    const signed = output.AgentSignature,
      publicKey = Uint8Array.from(signed.key.public_key),
      hash = eventHash(request.event)
    ensure(
      equal(Uint8Array.from(signed.event_hash), hash) &&
        ed25519.verify(Uint8Array.from(signed.signature), hash, publicKey) &&
        'AgentController' in signed.key.purpose &&
        signed.key.key_generation === BigInt(job.generation),
      'INTEGRITY_FAILED',
      '托管签名与批准的事件不一致。'
    )
    const event = JSON.parse(request.event) as DelegationEvent
    ensure(event.actor === agentId(publicKey), 'INTEGRITY_FAILED')
    job.envelope = envelope(event, hash, Uint8Array.from(signed.signature))
    job.stage = 'signed'
    delete job.error
    await this.save(job)
    return this.submit(job)
  }

  /** Exact resubmission is idempotent; a stale envelope needs a new approval. */
  private async submit(job: AgentJob) {
    ensure(config.agentOrigin && job.envelope, 'UNAVAILABLE', '尚未配置 delegation 服务地址。')
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
      // Retry only what can still succeed: an unavailable service within the window.
      if (response.status < 500 || Date.now() > job.envelope.event.created_at + WINDOW)
        job.stage = 'failed'
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
