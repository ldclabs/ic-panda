import { config } from '../config'
import { actionToCandid } from '../protocol/app-action'
import { canonical as sdkCanonical } from '@dmsg/sdk'
import { registeredApplication } from './registration'
import { services } from './ic'
import type { _SERVICE as CoseService } from '../canisters/generated/cose'
import type {
  SignRequest,
  AppActionSignRequest,
  ExecutionResult
} from '../canisters/generated/user'
import { AccountClient, controlResult } from './account'
import {
  coseClient,
  prepareSign,
  signBytes,
  signingRequest,
  type PreparedExecution
} from './cose'
import {
  assertRequestUnchanged,
  requestStatement,
  type PendingRequest,
  type SignatureRequest
} from '../protocol/requests'
import { decodeControl, encodeControl } from '../protocol/account'
import {
  b64,
  canonical,
  decodeCanonical,
  digest,
  equal,
  hash,
  hex,
  unb64,
  unhex,
  utf8
} from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { statementPurpose, verifyDocumentArtifact } from '../protocol/statements'
import { certifiedValue } from './certified'
import { listRequests, setRequestState } from '../requests'
import { ensure } from '../errors'

interface Journal {
  format: 'dmsg-signing-journal/2'
  method: 'sign' | 'sign_app_action'
  externalId: string
  digest: string
  account: string
  origin: string
  executionId: string
  operation: string
  stage: 'authorized' | 'unknown' | 'complete' | 'failed' | 'result_expired'
  receipt?: string
  keyDescriptorCbor?: string
  executionCertificateCbor?: string
  artifact?: { cose_sign1: string; cose_key: string }
  error?: string
}
export class SigningClient {
  private prepared: PreparedExecution | null = null
  private external: PendingRequest | null = null
  constructor(
    readonly account: AccountClient,
    readonly cose: CoseService,
    private readonly sourceLive: (request: PendingRequest) => Promise<void>
  ) {}
  private key(id: string) {
    return `formal:${id}`
  }
  async journal(id: string): Promise<Journal | null> {
    const value = await this.account.crypto.call('controlGet', this.key(id))
    return value ? JSON.parse(value) : null
  }
  private save(job: Journal) {
    return this.account.crypto.call(
      'controlPut',
      this.key(job.externalId),
      JSON.stringify(job)
    )
  }
  async usage(account: string) {
    const raw = xidBytes(account)
    const usage = controlResult(await this.account.user.refresh_execution_entitlement(raw))
    const proof = await certifiedValue(
      controlResult(
        await this.account.user.get_execution_usage_certified(raw, usage.month_utc)
      ),
      this.account.agent,
      this.account.home.toText(),
      digest('dmsg/commerce/usage-key/v1', [raw, usage.month_utc])
    )
    const value = decodeCanonical<Record<string, any>>(proof.value)
    ensure(
      equal(canonical(value), canonical({ ...usage, account_id: raw })) &&
        usage.valid_until_ms > BigInt(Date.now()) &&
        usage.held_units + usage.charged_units <= usage.allowed_units,
      'INTEGRITY_FAILED'
    )
    return {
      allowed: usage.allowed_units.toString(),
      held: usage.held_units.toString(),
      charged: usage.charged_units.toString(),
      remaining: (usage.allowed_units - usage.held_units - usage.charged_units).toString(),
      month: usage.month_utc
    }
  }
  private async registered(request: PendingRequest, payload?: SignatureRequest) {
    if (!request.bridge) {
      ensure(
        typeof chrome !== 'undefined' &&
          request.source.origin === `chrome-extension://${chrome.runtime.id}`,
        'FORBIDDEN'
      )
      return
    }
    const app = await registeredApplication(
      await services(),
      request.bridge.appId,
      request.source.origin,
      request.kind === 'action' ? 'SignAction' : 'SignDocument'
    )
    ensure(
      app.config_version.toString() === request.bridge.appVersion &&
        request.bridge.accountId === this.account.meta.account?.id,
      'POLICY_STALE'
    )
    if (payload) {
      const profile =
        payload.statement.content.kind === 'app_action'
          ? 'AppActionV1'
          : payload.statement.content.kind === 'text'
            ? 'TextStatementV1'
            : payload.statement.content.kind === 'digest'
              ? 'DigestStatementV1'
              : 'FileStatementV1'
      ensure(app.profiles.includes(profile), 'FORBIDDEN')
    }
  }
  async prepare(
    request: PendingRequest,
    payload: SignatureRequest,
    maxCycles = 100000000000n
  ) {
    assertRequestUnchanged(payload, request)
    await this.registered(request, payload)
    ensure(!(await this.journal(request.id)), 'Pending', '此请求已有执行记录，请对账原操作。')
    await this.sourceLive(request)
    const state = await this.account.refresh(payload.accountId)
    ensure(
      state.device &&
        state.device.input.capabilities.some((c) => 'FormalApprove' in c) &&
        !state.info.sensitive_policy.frozen,
      'Forbidden',
      '当前设备没有正式批准能力，或账户已暂停签名。'
    )
    const statement = requestStatement(payload)
    ensure(
      state.info.sensitive_policy.allowed_purposes.some(
        (p) => Object.keys(p)[0] === statementPurpose(statement.content)
      ),
      'FORBIDDEN'
    )
    if (statement.content.kind === 'app_action') {
      ensure(
        request.kind === 'action' &&
          statement.content.action.origin === request.source.origin &&
          statement.content.action.app_id === request.bridge?.appId,
        'INTEGRITY_FAILED'
      )
      controlResult(
        await this.account.user.inspect_app_action(
          xidBytes(payload.accountId),
          actionToCandid(statement.content.action)
        )
      )
    }
    const usage = await this.usage(payload.accountId)
    ensure(BigInt(usage.remaining) > 0n, 'QuotaExceeded', '本月可用正式执行额度不足。')
    const key = await coseClient(this.account.user, this.cose).publicKey(
      xidBytes(payload.accountId),
      {
        kind: 'signing',
        purpose:
          statementPurpose(statement.content) === 'AppAction'
            ? 'app_action'
            : statementPurpose(statement.content) === 'Statement'
              ? 'statement'
              : 'file_attestation'
      }
    )
    ensure(
      key.home_cose.toText() === state.info.home_cose.toText() &&
        equal(Uint8Array.from(key.account_id), xidBytes(payload.accountId)) &&
        key.derivation_version === 2 &&
        key.key_generation === 1n &&
        'Ed25519' in key.algorithm &&
        Object.keys(key.environment)[0].toLowerCase() === this.account.meta.environment &&
        (config.environment === 'local'
          ? ['key_1', 'test_key_1', 'dfx_test_key'].includes(key.master_key_name)
          : key.master_key_name === 'key_1'),
      'INTEGRITY_FAILED'
    )
    await this.sourceLive(request)
    this.prepared = prepareSign(
      {
        homeUser: this.account.home,
        accountId: xidBytes(payload.accountId),
        issuer: state.info.issuer,
        deviceId: unhex(this.account.meta.deviceId),
        securityEpoch: state.info.security_epoch,
        sequence: state.device.next_sequence,
        expiresAt: BigInt(payload.expiresAt),
        maxCycles
      },
      {
        origin: request.source.origin,
        key: {
          algorithm: 'Ed25519',
          kid: Uint8Array.from(key.key_id),
          publicKeyFingerprint: Uint8Array.from(key.public_key_fingerprint)
        },
        statement: requestStatement(payload)
      }
    )
    this.external = structuredClone(request)
    return {
      usage,
      fingerprint: hex(Uint8Array.from(key.public_key_fingerprint)),
      keyId: hex(Uint8Array.from(key.key_id)),
      executionId: this.prepared.requestId,
      maxCycles: maxCycles.toString(),
      toBeSignedDigest: hash(this.prepared.toBeSigned!)
    }
  }
  async execute() {
    const prepared = this.prepared,
      request = this.external
    ensure(prepared && request, 'INVALID_INPUT')
    await this.registered(request)
    await this.sourceLive(request)
    const stored = (await listRequests()).find((r) => r.id === request.id)
    ensure(stored?.state === 'awaiting_user' && stored.digest === request.digest, 'EXPIRED')
    const operation = prepared.review.request as SignRequest
    const state = await this.account.refresh(hexAccount(operation.account_id))
    ensure(
      state.device?.next_sequence === operation.approval.sequence &&
        state.info.security_epoch === operation.approval.security_epoch,
      'POLICY_STALE'
    )
    let job: Journal | null = null
    try {
      const result = await prepared.approveAndExecute(
        this.account.user,
        (data) => this.account.crypto.call('deviceSign', data),
        async (approved) => {
          ensure(approved.kind !== 'derive_root', 'INTEGRITY_FAILED')
          await this.sourceLive(request)
          job = {
            format: 'dmsg-signing-journal/2',
            method: approved.kind,
            externalId: request.id,
            digest: request.digest,
            account: hexAccount(approved.request.account_id),
            origin: request.source.origin,
            executionId: prepared.requestId,
            operation: encodeControl(approved.kind, [approved.request]),
            stage: 'authorized'
          }
          await this.account.crypto.call('formalAuthorize', {
            requestId: request.id,
            digest: request.digest,
            executionId: prepared.requestId,
            journal: JSON.stringify(job)
          })
          await this.sourceLive(request)
          job.stage = 'unknown'
          await this.save(job)
          await setRequestState(request.id, 'execution_unknown')
        }
      )
      ensure(job, 'INTEGRITY_FAILED')
      return this.finish(job, result)
    } catch (error) {
      if (job) {
        const persisted =
          (await this.journal(request.id).catch(() => null)) ?? (job as Journal)
        if (persisted.stage === 'complete') {
          await setRequestState(request.id, 'signed')
          throw error
        }
        // A returned error may follow a persisted authorization. Keep the original
        // operation until get_execution/reconcile reports a stored terminal result.
        persisted.error = error instanceof Error ? error.message : 'EXECUTION_UNKNOWN'
        persisted.stage = 'unknown'
        await this.save(persisted)
        await setRequestState(request.id, 'execution_unknown')
      }
      throw error
    }
  }
  async resume(id: string) {
    const record = (await listRequests()).find((r) => r.id === id)
    ensure(record && !['cancelled', 'rejected', 'expired'].includes(record.state), 'FORBIDDEN')
    const job = await this.journal(id)
    ensure(job && job.account === this.account.meta.account?.id, 'AUTH_REQUIRED')
    if (job.stage === 'complete') {
      await this.account.crypto.call('formalHistory', {
        requestId: job.externalId,
        value: JSON.stringify(job)
      })
      await setRequestState(job.externalId, 'signed')
      return job
    }
    if (job.stage === 'failed' || job.stage === 'result_expired') {
      await setRequestState(job.externalId, job.stage, job.error)
      return job
    }
    const request = decodeControl(job.method, job.operation)[0] as
      SignRequest | AppActionSignRequest
    const response = await this.account.user.get_execution(
      xidBytes(job.account),
      unhex(job.executionId)
    )
    let result: ExecutionResult
    if ('Ok' in response) {
      result = response.Ok
      if (!(
        'Completed' in result.outcome ||
        'Failed' in result.outcome ||
        'ResultExpired' in result.outcome
      ))
        result = controlResult(
          await this.account.user.reconcile_execution(
            xidBytes(job.account),
            unhex(job.executionId)
          )
        )
    } else {
      ensure('ResultExpired' in response.Err, 'EXECUTION_UNKNOWN')
      const external = (await listRequests()).find((r) => r.id === id)
      ensure(
        external && Date.now() < Number(request.approval.expires_at),
        'RESULT_EXPIRED',
        '原请求已过期，不能自动重新签署。'
      )
      await this.sourceLive(external)
      result = controlResult(
        await (job.method === 'sign_app_action'
          ? this.account.user.sign_app_action(request as AppActionSignRequest)
          : this.account.user.sign(request as SignRequest))
      )
    }
    return this.finish(job, result)
  }
  private async finish(job: Journal, result: ExecutionResult) {
    const current = await this.journal(job.externalId)
    if (
      current?.stage === 'complete' &&
      current.executionId === job.executionId &&
      current.digest === job.digest
    )
      return current
    ensure(
      equal(Uint8Array.from(result.request_id), unhex(job.executionId)),
      'INTEGRITY_FAILED'
    )
    if ('Failed' in result.outcome) {
      job.stage = 'failed'
      job.error = Object.keys(result.outcome.Failed)[0]
      await this.save(job)
      await setRequestState(job.externalId, 'failed', job.error)
      return job
    }
    if ('ResultExpired' in result.outcome) {
      job.stage = 'result_expired'
      await this.save(job)
      await setRequestState(job.externalId, 'result_expired')
      return job
    }
    ensure(
      'Completed' in result.outcome && 'Signature' in result.outcome.Completed,
      'EXECUTION_UNKNOWN'
    )
    Object.assign(job, await this.evidence(job, result))
    job.stage = 'complete'
    await this.save(job)
    await this.account.crypto.call('formalHistory', {
      requestId: job.externalId,
      value: JSON.stringify(job)
    })
    await setRequestState(job.externalId, 'signed')
    return job
  }
  private async evidence(job: Journal, result: ExecutionResult) {
    ensure(
      'Completed' in result.outcome && 'Signature' in result.outcome.Completed,
      'EXECUTION_UNKNOWN'
    )
    const output = result.outcome.Completed.Signature,
      artifact = verifyDocumentArtifact(output.artifact),
      request = signingRequest(
        decodeControl(job.method, job.operation)[0] as SignRequest | AppActionSignRequest
      )
    ensure(
      equal(artifact.toBeSigned, signBytes(request)),
      'INTEGRITY_FAILED',
      '签名对象与已批准的最终载荷不一致。'
    )
    const receipt = controlResult(
      await this.account.user.get_execution_receipt(
        xidBytes(job.account),
        unhex(job.executionId)
      )
    )
    const proof = await certifiedValue(
      receipt,
      this.account.agent,
      this.account.home.toText(),
      Uint8Array.from([
        ...utf8('execution/'),
        ...xidBytes(job.account),
        ...unhex(job.executionId)
      ])
    )
    const value = decodeCanonical<Record<string, unknown>>(proof.value)
    ensure(
      value.public_key_fingerprint instanceof Uint8Array &&
        equal(value.public_key_fingerprint, artifact.keyFingerprint) &&
        value.device_id instanceof Uint8Array &&
        equal(value.device_id, Uint8Array.from(request.approval.device_id)) &&
        BigInt(value.max_cycles as number | bigint) === request.max_cycles &&
        result.cycles_cost_upper_bound <= request.max_cycles,
      'INTEGRITY_FAILED'
    )
    ensure(
      artifact.statement.issuer === this.account.meta.account!.issuer &&
        equal(artifact.keyFingerprint, Uint8Array.from(request.key.public_key_fingerprint)) &&
        value.schema === 1 &&
        value.status === 'Completed' &&
        value.origin === job.origin &&
        value.issuer === this.account.meta.account!.issuer &&
        value.account_id instanceof Uint8Array &&
        equal(value.account_id, xidBytes(job.account)) &&
        value.request_id instanceof Uint8Array &&
        equal(value.request_id, unhex(job.executionId)) &&
        value.signature_digest instanceof Uint8Array &&
        equal(value.signature_digest, unhex(hash(artifact.signature))) &&
        value.to_be_signed_digest instanceof Uint8Array &&
        equal(value.to_be_signed_digest, unhex(hash(artifact.toBeSigned))),
      'INTEGRITY_FAILED'
    )
    const artifactView = {
      cose_sign1: b64(Uint8Array.from(output.artifact.cose_sign1)),
      cose_key: b64(Uint8Array.from(output.artifact.cose_key))
    }
    const receiptView = b64(
      canonical({
        value: proof.value,
        certificate: Uint8Array.from(receipt.certificate),
        witness: Uint8Array.from(receipt.entries[0].witness)
      })
    )
    const key = output.key
    const keyDescriptorCbor = b64(
      sdkCanonical({
        key_id: Uint8Array.from(key.key_id),
        account_id: Uint8Array.from(key.account_id),
        purpose: Object.keys(key.purpose)[0],
        algorithm: Object.keys(key.algorithm)[0],
        home_cose: key.home_cose.toUint8Array(),
        master_key_name: key.master_key_name,
        environment: Object.keys(key.environment)[0],
        derivation_version: BigInt(key.derivation_version),
        key_generation: key.key_generation,
        public_key: Uint8Array.from(key.public_key),
        public_key_fingerprint: Uint8Array.from(key.public_key_fingerprint)
      })
    )
    const executionCertificateCbor = b64(
      sdkCanonical({
        schema: BigInt(receipt.schema),
        canister: receipt.canister.toUint8Array(),
        certificate: Uint8Array.from(receipt.certificate),
        entries: receipt.entries.map((e) => ({
          key: Uint8Array.from(e.key),
          witness: Uint8Array.from(e.witness),
          value: e.value.length ? Uint8Array.from(e.value[0]!) : null
        }))
      })
    )
    return {
      artifact: artifactView,
      receipt: receiptView,
      keyDescriptorCbor,
      executionCertificateCbor
    }
  }
  async freshResult(id: string) {
    const job = await this.journal(id)
    ensure(
      job?.stage === 'complete' && job.account === this.account.meta.account?.id,
      'AUTH_REQUIRED'
    )
    const result = controlResult(
      await this.account.user.get_execution(xidBytes(job.account), unhex(job.executionId))
    )
    ensure(
      equal(Uint8Array.from(result.request_id), unhex(job.executionId)),
      'INTEGRITY_FAILED'
    )
    return { ...job, ...(await this.evidence(job, result)) }
  }
}
function hexAccount(bytes: Uint8Array | number[]) {
  return xidText(Uint8Array.from(bytes))
}
import { xidText } from '../protocol/identity'
