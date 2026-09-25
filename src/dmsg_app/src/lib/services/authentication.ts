import { Principal } from '@icp-sdk/core/principal'
import {
  canonical as sdkCanonical,
  decodeCanonical as sdkDecode,
  type AuthenticationRequest
} from '@dmsg/sdk'
import { base64, unbase64, browserOperationDigest, hex } from '@dmsg/sdk/browser'
import { verifyAuthenticationCertificate, type CertifiedBatch } from '@dmsg/sdk/ic'
import type {
  Approval,
  AuthenticationRequest as CandidRequest
} from '../canisters/generated/user'
import { AccountClient, controlResult } from './account'
import { services } from './ic'
import { registeredApplication } from './registration'
import { decodeControl, encodeControl } from '../protocol/account'
import { digest, equal, unhex } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import type { PendingRequest } from '../protocol/requests'
import { validateAuthenticationPayload, type AuthenticationPayload } from '../bridge-requests'
import { assertLiveSource, listRequests, setRequestState } from '../requests'
import { ensure } from '../errors'

interface Journal {
  account: string
  home: string
  caller: string
  origin: string
  digest: string
  requestCbor: string
  operation: string
  stage: 'authorized' | 'unknown' | 'complete' | 'failed' | 'result_expired'
}
function candidRequest(request: AuthenticationRequest): CandidRequest {
  return {
    ...request,
    version: Number(request.version),
    environment: { [request.environment]: null } as CandidRequest['environment'],
    purpose: { [request.purpose]: null } as CandidRequest['purpose'],
    receiver: Principal.fromUint8Array(request.receiver)
  }
}
function proofView(
  batch: import('../canisters/generated/user').CertifiedBatch
): CertifiedBatch {
  return {
    schema: BigInt(batch.schema),
    canister: batch.canister.toUint8Array(),
    certificate: Uint8Array.from(batch.certificate),
    entries: batch.entries.map((e) => ({
      key: Uint8Array.from(e.key),
      value: e.value.length ? Uint8Array.from(e.value[0]!) : null,
      witness: Uint8Array.from(e.witness)
    }))
  }
}

export class AuthenticationClient {
  constructor(
    readonly account: AccountClient,
    readonly sourceLive = assertLiveSource
  ) {}
  async journal(id: string): Promise<Journal | null> {
    const value = await this.account.crypto.call('controlGet', `auth:${id}`)
    return value ? JSON.parse(value) : null
  }
  private save(id: string, journal: Journal) {
    return this.account.crypto.call('controlPut', `auth:${id}`, JSON.stringify(journal))
  }
  private async current(id: string) {
    const record = (await listRequests()).find((r) => r.id === id)
    ensure(
      record?.kind === 'authentication' &&
        record.bridge?.accountId === this.account.meta.account?.id,
      'FORBIDDEN'
    )
    return record
  }
  async prepare(record: PendingRequest, payload: AuthenticationPayload) {
    const current = await this.current(record.id)
    ensure(
      current.digest === record.digest && current.state === 'awaiting_user',
      'INTEGRITY_FAILED'
    )
    const app = await registeredApplication(
      await services(),
      record.bridge!.appId,
      record.source.origin,
      'Authenticate'
    )
    const command = {
      method: 'authenticate' as const,
      appId: record.bridge!.appId,
      operationId: record.id,
      publicKey: record.bridge!.publicKey,
      payload: payload.requestCbor,
      resultDigest: null
    }
    const request = await validateAuthenticationPayload(command, record.source, app)
    ensure(
      record.digest === record.bridge!.operationDigest &&
        record.digest === hex(browserOperationDigest(command, record.source.origin)),
      'INTEGRITY_FAILED'
    )
    ensure(app.config_version.toString() === record.bridge!.appVersion, 'POLICY_STALE')
    const state = await this.account.refresh(record.bridge!.accountId)
    ensure(
      state.device?.input.capabilities.some((c) => 'FormalApprove' in c) &&
        !state.info.sensitive_policy.frozen &&
        'Active' in state.info.status,
      'FORBIDDEN'
    )
    await this.sourceLive(record)
    return { request, account: record.bridge!.accountId, app: app.app_id }
  }
  async execute(record: PendingRequest, payload: AuthenticationPayload) {
    ensure(!(await this.journal(record.id)), 'PENDING')
    const review = await this.prepare(record, payload)
    const { info, device } = await this.account.refresh(review.account)
    ensure(device, 'DEVICE_NOT_APPROVED')
    const approval: Approval = {
      device_id: unhex(this.account.meta.deviceId),
      security_epoch: info.security_epoch,
      sequence: device.next_sequence,
      request_id: review.request.operation_id,
      expires_at: review.request.expires_at_ms,
      signature: new Uint8Array()
    }
    const message = digest('dmsg/device-approval/v2', [
      this.account.home.toUint8Array(),
      xidBytes(review.account),
      'dmsg/authentication/approve/v1',
      approval.device_id,
      approval.security_epoch,
      approval.sequence,
      approval.request_id,
      approval.expires_at,
      digest('dmsg/authentication/approve/v1', review.request)
    ])
    approval.signature = await this.account.crypto.call('deviceSign', message)
    await this.sourceLive(record)
    const job: Journal = {
      account: review.account,
      home: this.account.home.toText(),
      caller: this.account.caller.toText(),
      origin: record.source.origin,
      digest: record.digest,
      requestCbor: payload.requestCbor,
      operation: encodeControl('approve_authentication', [
        xidBytes(review.account),
        candidRequest(review.request),
        approval
      ]),
      stage: 'authorized'
    }
    await this.save(record.id, job)
    // A cancellation before this CAS wins; afterwards only reconciliation is allowed.
    await setRequestState(record.id, 'authorized')
    return this.dispatch(record.id, job)
  }
  private async dispatch(id: string, job: Journal) {
    const args = decodeControl('approve_authentication', job.operation) as [
      Uint8Array,
      CandidRequest,
      Approval
    ]
    try {
      const result = await this.account.user.approve_authentication(...args)
      if ('Err' in result) {
        job.stage = 'failed'
        await this.save(id, job)
        await setRequestState(id, 'failed')
        controlResult(result)
      }
    } catch (error) {
      if (job.stage !== 'failed') {
        job.stage = 'unknown'
        await this.save(id, job)
        await setRequestState(id, 'execution_unknown')
      }
      throw error
    }
    return this.readResult(id, job)
  }
  private async readResult(id: string, job: Journal) {
    const request = sdkDecode(unbase64(job.requestCbor)) as unknown as AuthenticationRequest
    const result = await this.account.user.authentication_certificate(
      xidBytes(job.account),
      request.operation_id
    )
    if ('Err' in result && ('Expired' in result.Err || 'ResultExpired' in result.Err)) {
      job.stage = 'result_expired'
      await this.save(id, job)
      await setRequestState(id, 'result_expired')
      controlResult(result)
    }
    const proof = proofView(controlResult(result))
    ensure(
      this.account.agent.rootKey &&
        job.home === this.account.home.toText() &&
        job.caller === this.account.caller.toText(),
      'FORBIDDEN'
    )
    const verified = await verifyAuthenticationCertificate(
      proof,
      request,
      this.account.home.toUint8Array(),
      this.account.agent.rootKey,
      BigInt(Date.now())
    )
    ensure(equal(verified.account_id, xidBytes(job.account)), 'INTEGRITY_FAILED')
    job.stage = 'complete'
    await this.save(id, job)
    await setRequestState(id, 'signed')
    return { proof: base64(sdkCanonical(proof)) }
  }
  async result(id: string) {
    const job = await this.journal(id)
    ensure(job?.stage === 'complete', 'PENDING')
    const record = await this.current(id)
    ensure(
      record.digest === job.digest && record.source.origin === job.origin,
      'INTEGRITY_FAILED'
    )
    return this.readResult(id, job)
  }
  async resume(id: string) {
    const job = await this.journal(id),
      record = await this.current(id)
    ensure(
      job && ['authorized', 'execution_unknown', 'signed', 'returned'].includes(record.state),
      'FORBIDDEN'
    )
    ensure(
      job.digest === record.digest &&
        job.home === this.account.home.toText() &&
        job.caller === this.account.caller.toText(),
      'FORBIDDEN'
    )
    try {
      return await this.readResult(id, job)
    } catch (error) {
      const request = sdkDecode(unbase64(job.requestCbor)) as unknown as AuthenticationRequest
      // Reuse only the original persisted approval, and only on explicit user reconciliation.
      if (
        !(error instanceof Error) ||
        error.message !== 'NotFound' ||
        Date.now() >= Number(request.expires_at_ms) ||
        job.stage === 'result_expired' ||
        job.stage === 'complete'
      )
        throw error
      await this.sourceLive(record)
      return this.dispatch(id, job)
    }
  }
}
