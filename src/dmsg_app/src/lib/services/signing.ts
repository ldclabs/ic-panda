import { actionToCandid } from '../protocol/app-action'
import { canonical as sdkCanonical } from 'dmsg-sdk'
import { registeredApplication } from './registration'
import { services } from './ic'
import type { AttestRequest, AppActionAttestRequest, SignedArtifact } from '../canisters/generated/user'
import { AccountClient, controlResult } from './account'
import {
  checkArtifact,
  deviceKeyThumbprint,
  prepareAttest,
  recordedAttestation,
  signBytes,
  submitAttestation,
  type AttestOperation,
  type PreparedAttestation
} from './cose'
import {
  assertRequestUnchanged,
  requestStatement,
  type PendingRequest,
  type SignatureRequest
} from '../protocol/requests'
import { decodeControl, encodeControl } from '../protocol/account'
import { b64, canonical, decodeCanonical, digest, equal, hash, hex, unb64, unhex, utf8 } from '../protocol/codec'
import { xidBytes, xidText } from '../protocol/identity'
import { statementPurpose } from '../protocol/statements'
import { certifiedValue } from './certified'
import { getRequest, setRequestState } from '../requests'
import { ensure } from '../errors'

interface Journal {
  format: 'dmsg-signing-journal/3'
  method: 'attest' | 'attest_app_action'
  externalId: string
  digest: string
  account: string
  origin: string
  executionId: string
  operation: string
  stage: 'authorized' | 'unknown' | 'complete' | 'failed' | 'result_expired'
  receipt?: string
  executionCertificateCbor?: string
  artifact?: { cose_sign1: string; cose_key: string }
  error?: string
}
export class SigningClient {
  private prepared: PreparedAttestation | null = null
  private external: PendingRequest | null = null
  constructor(
    readonly account: AccountClient,
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
  async prepare(request: PendingRequest, payload: SignatureRequest) {
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
    await this.sourceLive(request)
    const devicePublicKey = unb64(this.account.meta.signingPublic)
    this.prepared = prepareAttest(
      {
        homeUser: this.account.home,
        accountId: xidBytes(payload.accountId),
        issuer: state.info.issuer,
        deviceId: unhex(this.account.meta.deviceId),
        securityEpoch: state.info.security_epoch,
        sequence: state.device.next_sequence,
        expiresAt: BigInt(payload.expiresAt)
      },
      { origin: request.source.origin, statement: requestStatement(payload), devicePublicKey }
    )
    this.external = structuredClone(request)
    return {
      usage,
      fingerprint: hex(this.prepared.kid),
      executionId: this.prepared.requestId,
      toBeSignedDigest: hash(this.prepared.toBeSigned)
    }
  }
  async execute() {
    const prepared = this.prepared,
      request = this.external
    ensure(prepared && request, 'INVALID_INPUT')
    await this.registered(request)
    await this.sourceLive(request)
    const stored = await getRequest(request.id)
    ensure(stored?.state === 'awaiting_user' && stored.digest === request.digest, 'EXPIRED')
    const operation = prepared.review.request
    const state = await this.account.refresh(accountText(operation.account_id))
    ensure(
      state.device?.next_sequence === operation.approval.sequence &&
        state.info.security_epoch === operation.approval.security_epoch,
      'POLICY_STALE'
    )
    let job: Journal | null = null
    try {
      const artifact = await prepared.approveAndExecute(
        this.account.user,
        (data) => this.account.crypto.call('deviceSign', data),
        async (approved) => {
          await this.sourceLive(request)
          job = {
            format: 'dmsg-signing-journal/3',
            method: approved.kind,
            externalId: request.id,
            digest: request.digest,
            account: accountText(approved.request.account_id),
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
      return this.finish(job, artifact)
    } catch (error) {
      if (job) {
        const persisted =
          (await this.journal(request.id).catch(() => null)) ?? (job as Journal)
        if (persisted.stage === 'complete') {
          await setRequestState(request.id, 'signed')
          throw error
        }
        // A returned error may follow a persisted authorization. Keep the original
        // operation until get_attestation reports the stored artifact.
        persisted.error = error instanceof Error ? error.message : 'EXECUTION_UNKNOWN'
        persisted.stage = 'unknown'
        await this.save(persisted)
        await setRequestState(request.id, 'execution_unknown')
      }
      throw error
    }
  }
  private operation(job: Journal): AttestOperation {
    return {
      kind: job.method,
      request: decodeControl(job.method, job.operation)[0]
    } as AttestOperation
  }
  async resume(id: string) {
    const record = await getRequest(id)
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
    const operation = this.operation(job)
    let artifact = await recordedAttestation(
      this.account.user,
      xidBytes(job.account),
      unhex(job.executionId)
    )
    if (!artifact) {
      // Without a stored artifact, an expired approval can never run; record a terminal state.
      if (Date.now() >= Number(operation.request.approval.expires_at)) {
        job.stage = 'result_expired'
        await this.save(job)
        await setRequestState(job.externalId, 'result_expired')
        return job
      }
      const external = await getRequest(id)
      ensure(external, 'NOT_FOUND')
      await this.sourceLive(external)
      const kid = deviceKeyThumbprint(unb64(this.account.meta.signingPublic))
      try {
        artifact = await submitAttestation(
          this.account.user,
          operation,
          signBytes(operation.request, kid),
          kid
        )
      } catch (error) {
        if (error instanceof Error && 'code' in error && typeof error.code === 'string' && error.code !== 'EXECUTION_UNKNOWN') {
          job.stage = 'failed'
          job.error = error.code
          await this.save(job)
          await setRequestState(job.externalId, 'failed', job.error)
          return job
        }
        throw error
      }
    }
    return this.finish(job, artifact)
  }
  private async finish(job: Journal, artifact: SignedArtifact) {
    const current = await this.journal(job.externalId)
    if (
      current?.stage === 'complete' &&
      current.executionId === job.executionId &&
      current.digest === job.digest
    )
      return current
    Object.assign(job, await this.evidence(job, artifact))
    job.stage = 'complete'
    await this.save(job)
    await this.account.crypto.call('formalHistory', {
      requestId: job.externalId,
      value: JSON.stringify(job)
    })
    await setRequestState(job.externalId, 'signed')
    return job
  }
  private async evidence(job: Journal, artifact: SignedArtifact) {
    const request = this.operation(job).request as AttestRequest | AppActionAttestRequest,
      kid = deviceKeyThumbprint(unb64(this.account.meta.signingPublic)),
      checked = checkArtifact(artifact, signBytes(request, kid), kid)
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
      checked.statement.issuer === this.account.meta.account!.issuer &&
        value.schema === 2 &&
        value.origin === job.origin &&
        value.issuer === this.account.meta.account!.issuer &&
        value.account_id instanceof Uint8Array &&
        equal(value.account_id, xidBytes(job.account)) &&
        value.request_id instanceof Uint8Array &&
        equal(value.request_id, unhex(job.executionId)) &&
        value.device_id instanceof Uint8Array &&
        equal(value.device_id, Uint8Array.from(request.approval.device_id)) &&
        BigInt(value.security_epoch as number | bigint) === request.approval.security_epoch &&
        BigInt(value.expires_at as number | bigint) === request.approval.expires_at &&
        value.public_key_fingerprint instanceof Uint8Array &&
        equal(value.public_key_fingerprint, kid) &&
        value.signature_digest instanceof Uint8Array &&
        equal(value.signature_digest, unhex(hash(checked.signature))) &&
        value.to_be_signed_digest instanceof Uint8Array &&
        equal(value.to_be_signed_digest, unhex(hash(checked.toBeSigned))),
      'INTEGRITY_FAILED'
    )
    const artifactView = {
      cose_sign1: b64(Uint8Array.from(artifact.cose_sign1)),
      cose_key: b64(Uint8Array.from(artifact.cose_key))
    }
    const receiptView = b64(
      canonical({
        value: proof.value,
        certificate: Uint8Array.from(receipt.certificate),
        witness: Uint8Array.from(receipt.entries[0].witness)
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
    return { artifact: artifactView, receipt: receiptView, executionCertificateCbor }
  }
  async freshResult(id: string) {
    const job = await this.journal(id)
    ensure(
      job?.stage === 'complete' && job.account === this.account.meta.account?.id,
      'AUTH_REQUIRED'
    )
    const artifact = await recordedAttestation(
      this.account.user,
      xidBytes(job.account),
      unhex(job.executionId)
    )
    ensure(artifact, 'RESULT_EXPIRED')
    return { ...job, ...(await this.evidence(job, artifact)) }
  }
}
function accountText(bytes: Uint8Array | number[]) {
  return xidText(Uint8Array.from(bytes))
}
