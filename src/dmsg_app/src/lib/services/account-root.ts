import type { DeriveRootRequest, ExecutionResult } from '../canisters/generated/user'
import { b64, canonical, equal, hash, hex, id, unb64, unhex } from '../protocol/codec'
import {
  decodeControl,
  decodeControlResult,
  encodeControl,
  encodeControlResult
} from '../protocol/account'
import { xidBytes } from '../protocol/identity'
import { readRootBundle, type RootContext, type RootKey } from '../crypto/root'
import { ensure } from '../errors'
import { prepareRootDerivation, recordedExecution } from './cose'
import { AccountClient, controlResult } from './account'
import type { CloudClient } from './relay'
import { CloudSession, type AccountState } from './cloud-session'
import { config } from '../config'

export interface RootJob {
  version: 1
  account: string
  home: string
  opId: string
  expectedGeneration: number
  stage: 'reserve' | 'derive' | 'wrap' | 'upload' | 'commit' | 'committed'
  context?: RootContext
  derive?: string
  result?: string
  bytes?: string
  plan?: Record<string, unknown>
}
/** Accepts only a completed derivation for exactly this request. */
function completedDerivation(result: ExecutionResult, requestId: Uint8Array | number[]) {
  ensure(
    equal(Uint8Array.from(result.request_id), Uint8Array.from(requestId)),
    'INTEGRITY_FAILED'
  )
  if ('Failed' in result.outcome) controlResult({ Err: result.outcome.Failed })
  if ('ResultExpired' in result.outcome) controlResult({ Err: { ResultExpired: null } })
  ensure(
    'Completed' in result.outcome,
    'EXECUTION_UNKNOWN',
    '根派生尚未成功完成，请按原请求对账。'
  )
  return result
}
/** The derived key must be this account's content-root key for the bundle context. */
function derivedRootKey(result: ExecutionResult, context: RootContext) {
  ensure(
    'Completed' in result.outcome && 'EncryptedRootKey' in result.outcome.Completed,
    'INTEGRITY_FAILED'
  )
  const output = result.outcome.Completed.EncryptedRootKey,
    key = output.key
  ensure(
    key.home_cose.toText() === context.homeCose &&
      key.key_generation === BigInt(context.generation) &&
      key.derivation_version === 2 &&
      'VetKdBls12381' in key.algorithm &&
      'ContentRoot' in key.purpose &&
      Object.keys(key.environment)[0].toLowerCase() === context.environment &&
      equal(Uint8Array.from(key.account_id), xidBytes(context.account)),
    'INTEGRITY_FAILED'
  )
  const descriptor: RootKey = {
    publicKey: b64(Uint8Array.from(key.public_key)),
    fingerprint: hex(Uint8Array.from(key.public_key_fingerprint)),
    keyId: hex(Uint8Array.from(key.key_id)),
    keyName: key.master_key_name
  }
  return { descriptor, encryptedKey: Uint8Array.from(output.encrypted_key) }
}
/** Every retry preserves the original reservation, transport key, execution
 * request and bundle bytes. A candidate is not a content-writing authority. */
export class AccountRootClient {
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudClient
  ) {}
  async job(account: string): Promise<RootJob | null> {
    const data = await this.account.crypto.call('controlGet', `root:${account}`)
    return data ? JSON.parse(data) : null
  }
  private save(job: RootJob) {
    return this.account.crypto.call('controlPut', `root:${job.account}`, JSON.stringify(job))
  }
  private async session(accountId: string, state: AccountState) {
    const session = new CloudSession(this.account, this.cloud, accountId)
    await session.adopt(state)
    return session
  }
  async restartExpired(accountId: string) {
    const job = await this.job(accountId)
    ensure(job && job.stage !== 'committed', 'NOT_FOUND')
    if (await this.account.pending()) await this.account.resume()
    ensure(!(await this.account.pending()), 'EXECUTION_UNKNOWN')
    const { info, verified } = await this.account.refresh(accountId)
    if (
      job.bytes &&
      info.current_root[0] &&
      hex(Uint8Array.from(info.current_root[0].bundle_digest)) === hash(unb64(job.bytes))
    )
      return this.run(accountId)
    ensure(
      Number(info.current_root[0]?.generation ?? 0n) === job.expectedGeneration,
      'VERSION_CONFLICT'
    )
    const slot = info.root_slot[0]
    ensure(
      !slot ||
        (hex(Uint8Array.from(slot.op_id)) === job.opId &&
          BigInt(verified.certifiedAt) >= slot.expires_at),
      'Pending',
      '仍有有效根预留，请继续原操作。'
    )
    // Retain old bytes and encrypted transport state for reconciliation/export.
    await this.account.crypto.call(
      'controlPut',
      `root-history:${job.opId}`,
      JSON.stringify(job)
    )
    await this.account.crypto.call('controlPut', `root:${accountId}`, 'null')
    return this.run(accountId)
  }
  async rotate(accountId: string, progress: (stage: RootJob['stage']) => void = () => {}) {
    const state = await this.account.refresh(accountId),
      previous = await this.job(accountId)
    ensure(
      state.info.current_root[0] &&
        this.account.meta.account?.rootDigest ===
          hex(Uint8Array.from(state.info.current_root[0].bundle_digest)),
      'RECOVERY_INCOMPLETE'
    )
    ensure(!previous || previous.stage === 'committed', 'Pending', '请先完成上一次根操作。')
    if (previous)
      await this.account.crypto.call(
        'controlPut',
        `root-history:${previous.opId}`,
        JSON.stringify(previous)
      )
    await this.account.crypto.call('controlPut', `root:${accountId}`, 'null')
    return this.run(accountId, progress)
  }
  async openCurrent(accountId: string): Promise<RootJob> {
    const account = this.account,
      crypto = account.crypto
    const pending = await this.job(accountId)
    let state = await account.refresh(accountId)
    const ref = state.info.current_root[0]
    ensure(ref && state.device && !state.device.revoked_at.length, 'DeviceNotApproved')
    if (pending && pending.stage !== 'committed') {
      // A different device may have won the reservation. The certified current
      // generation makes this old expected-generation CAS impossible to commit.
      ensure(
        ref.generation > BigInt(pending.expectedGeneration),
        'Pending',
        '请先对账并完成原根操作。'
      )
      if (await account.pending()) await account.resume()
      ensure(!(await account.pending()), 'EXECUTION_UNKNOWN')
      await crypto.call('controlPut', `root-history:${pending.opId}`, JSON.stringify(pending))
      await crypto.call('controlPut', `root:${accountId}`, 'null')
      state = await account.refresh(accountId)
    }
    const session = await this.session(accountId, state)
    const base = `/v1/accounts/${accountId}`,
      expected = hex(Uint8Array.from(ref.bundle_digest))
    const response = (await session.get(`${base}/root`)) as {
      root: { upload_id: string; digest: string; root_generation: number }
    }
    ensure(
      response.root.digest === expected &&
        response.root.root_generation === Number(ref.generation) &&
        /^[0-9a-f]{64}$/.test(response.root.upload_id),
      'INTEGRITY_FAILED'
    )
    const bundles = [
      await this.cloud.getChunk(
        `${base}/objects/${response.root.upload_id}/chunks/manifest`,
        expected,
        await session.context(),
        session.sign
      )
    ]
    const current = readRootBundle(bundles[0], expected)
    ensure(
      current.payload.context.account === accountId &&
        current.payload.context.homeCose === state.info.home_cose.toText() &&
        current.payload.context.environment === account.meta.environment &&
        current.payload.context.generation === Number(ref.generation),
      'INTEGRITY_FAILED'
    )
    let cursor = current
    while (cursor.payload.previous) {
      const link = cursor.payload.previous
      ensure(
        bundles.length < 256 &&
          link.generation < cursor.payload.context.generation &&
          /^[0-9a-f]{64}$/.test(link.uploadId),
        'RECOVERY_INCOMPLETE'
      )
      const data = await this.cloud.getChunk(
        `${base}/objects/${link.uploadId}/chunks/manifest`,
        link.digest,
        await session.context(),
        session.sign
      )
      cursor = readRootBundle(data, link.digest)
      bundles.push(data)
    }
    const journalKey = `open-root:${accountId}:${expected}`
    const previous = await crypto.call('controlGet', journalKey)
    const encoded = previous ? (JSON.parse(previous) as string) : null
    const approve = async () => {
      state = await account.refresh(accountId)
      ensure(state.device && !state.device.revoked_at.length, 'DeviceNotApproved')
      ensure(
        state.info.current_root[0] &&
          hex(Uint8Array.from(state.info.current_root[0].bundle_digest)) === expected,
        'POLICY_STALE'
      )
      const transport = await crypto.call('prepareAccountRoot', current.payload.context)
      const prepared = prepareRootDerivation(
        {
          homeUser: account.home,
          accountId: xidBytes(accountId),
          issuer: state.info.issuer,
          deviceId: unhex(account.meta.deviceId),
          securityEpoch: state.info.security_epoch,
          sequence: state.device.next_sequence,
          expiresAt: BigInt(Date.now() + 300000),
          maxCycles: BigInt(config.rootDerivationMaxCycles)
        },
        { kind: 'current', generation: ref.generation },
        transport
      )
      const request = prepared.review.request as DeriveRootRequest
      request.approval.signature = await crypto.call('deviceSign', prepared.approvalMessage)
      await crypto.call(
        'controlPut',
        journalKey,
        JSON.stringify(encodeControl('derive_root', [request]))
      )
      return request
    }
    let request = encoded
      ? (decodeControl('derive_root', encoded)[0] as DeriveRootRequest)
      : await approve()
    let result = await recordedExecution(
      account.user,
      xidBytes(accountId),
      Uint8Array.from(request.approval.request_id)
    )
    if (!result) {
      if (request.approval.expires_at <= BigInt(Date.now())) {
        const fresh = await account.refresh(accountId)
        // An unconsumed sequence plus a certified time past the deadline proves
        // the original approval can no longer start. Unknown consumed results
        // must stay in reconciliation instead of silently spending again.
        ensure(
          BigInt(fresh.verified.certifiedAt) >= request.approval.expires_at &&
            fresh.device?.next_sequence === request.approval.sequence,
          'RESULT_EXPIRED',
          '原派生请求的执行结果尚不能确认，请先对账。'
        )
        await crypto.call(
          'controlPut',
          `open-root-history:${hex(Uint8Array.from(request.approval.request_id))}`,
          JSON.stringify(encodeControl('derive_root', [request]))
        )
        request = await approve()
      }
      result = controlResult(await account.user.derive_root(request))
    }
    const { descriptor, encryptedKey } = derivedRootKey(
      completedDerivation(result, request.approval.request_id),
      current.payload.context
    )
    state = await account.refresh(accountId)
    ensure(
      state.info.current_root[0] &&
        hex(Uint8Array.from(state.info.current_root[0].bundle_digest)) === expected,
      'POLICY_STALE'
    )
    await crypto.call(
      'openAccountRoot',
      current.payload.context,
      descriptor,
      encryptedKey,
      bundles,
      expected
    )
    const job: RootJob = {
      version: 1,
      account: accountId,
      home: account.home.toText(),
      opId: current.payload.context.opId,
      expectedGeneration: 0,
      context: current.payload.context,
      stage: 'committed',
      bytes: b64(bundles[0]),
      plan: { upload_id: response.root.upload_id }
    }
    await this.save(job)
    return job
  }
  async run(accountId: string, progress: (stage: RootJob['stage']) => void = () => {}) {
    const account = this.account,
      crypto = account.crypto
    let state = await account.refresh(accountId)
    let job = await this.job(accountId)
    if (!job) {
      const current = state.info.current_root[0]
      ensure(
        !current ||
          (account.meta.account?.id === accountId &&
            account.meta.account.rootDigest === hex(Uint8Array.from(current.bundle_digest))),
        'RECOVERY_INCOMPLETE',
        '请先在此设备恢复当前内容根，不能用空白根覆盖已有内容。'
      )
      job = {
        version: 1,
        account: accountId,
        home: account.home.toText(),
        opId: id(),
        expectedGeneration: Number(current?.generation ?? 0n),
        stage: 'reserve'
      }
      await this.save(job)
    }
    ensure(job.home === account.home.toText(), 'INTEGRITY_FAILED')
    const committed = () =>
      job!.bytes &&
      state.info.current_root[0] &&
      hex(Uint8Array.from(state.info.current_root[0].bundle_digest)) ===
        hash(unb64(job!.bytes))
    if (committed()) {
      ensure(
        state.info.current_root[0]!.generation === BigInt(job.context!.generation),
        'INTEGRITY_FAILED'
      )
      const pending = await account.pending()
      if (pending?.method === 'mutate_account' && pending.account === accountId) {
        const request = decodeControl('mutate_account', pending.args)[0] as {
          command: { CommitRoot?: { op_id: Uint8Array } }
        }
        if (
          request.command.CommitRoot &&
          hex(Uint8Array.from(request.command.CommitRoot.op_id)) === job.opId
        )
          await account.resume()
      }
      job.stage = 'committed'
      await this.save(job)
      progress(job.stage)
      return job
    }
    ensure(
      Number(state.info.current_root[0]?.generation ?? 0n) === job.expectedGeneration,
      'VERSION_CONFLICT'
    )
    const pending = await account.pending()
    if (pending) {
      ensure(pending.method === 'mutate_account' && pending.account === accountId, 'Pending')
      const mutation = decodeControl('mutate_account', pending.args)[0] as {
        command: { ReserveRoot?: { op_id: Uint8Array }; CommitRoot?: { op_id: Uint8Array } }
      }
      const op = mutation.command.ReserveRoot?.op_id ?? mutation.command.CommitRoot?.op_id
      ensure(op && hex(Uint8Array.from(op)) === job.opId, 'Pending')
      await account.resume()
      state = await account.refresh(accountId)
      if (committed()) {
        job.stage = 'committed'
        await this.save(job)
        progress(job.stage)
        return job
      }
    }
    if (!job.context) {
      progress('reserve')
      let slot = state.info.root_slot[0]
      if (slot && slot.expires_at <= BigInt(state.verified.certifiedAt)) slot = undefined
      if (!slot) {
        state = await account.mutate(accountId, {
          ReserveRoot: {
            expected_generation: BigInt(job.expectedGeneration),
            op_id: unhex(job.opId)
          }
        })
        slot = state.info.root_slot[0]
      }
      const recovery = state.info.recovery[0]
      ensure(
        slot && recovery && hex(Uint8Array.from(slot.op_id)) === job.opId,
        'VERSION_CONFLICT'
      )
      job.context = {
        account: accountId,
        homeCose: state.info.home_cose.toText(),
        environment: account.meta.environment,
        generation: Number(slot.generation),
        opId: job.opId,
        recoveryGeneration: Number(recovery.generation),
        recoveryPublic: b64(Uint8Array.from(recovery.hpke_pub)),
        recoverySigningPublic: b64(Uint8Array.from(recovery.signing_pub))
      }
      job.stage = 'derive'
      await this.save(job)
    }
    const context = job.context,
      slot = state.info.root_slot[0]
    ensure(
      slot &&
        hex(Uint8Array.from(slot.op_id)) === job.opId &&
        slot.expires_at > BigInt(Date.now()) &&
        slot.security_epoch === state.info.security_epoch,
      'VERSION_CONFLICT',
      '根预留已变化或过期；保留原候选与操作记录，请先对账。'
    )
    if (!job.result) {
      progress('derive')
      const transport = await crypto.call('prepareAccountRoot', context)
      if (!job.derive) {
        ensure(state.device && !state.device.revoked_at.length, 'DeviceNotApproved')
        const prepared = prepareRootDerivation(
          {
            homeUser: account.home,
            accountId: xidBytes(accountId),
            issuer: state.info.issuer,
            deviceId: unhex(account.meta.deviceId),
            securityEpoch: state.info.security_epoch,
            sequence: state.device.next_sequence,
            expiresAt: BigInt(Date.now() + 300000),
            maxCycles: BigInt(config.rootDerivationMaxCycles)
          },
          { kind: 'candidate', generation: BigInt(context.generation), opId: unhex(job.opId) },
          transport
        )
        const request = prepared.review.request as DeriveRootRequest
        request.approval.signature = await crypto.call('deviceSign', prepared.approvalMessage)
        job.derive = encodeControl('derive_root', [request])
        await this.save(job)
      }
      const request = decodeControl('derive_root', job.derive)[0] as DeriveRootRequest
      const result = completedDerivation(
        (await recordedExecution(
          account.user,
          xidBytes(accountId),
          Uint8Array.from(request.approval.request_id)
        )) ?? controlResult(await account.user.derive_root(request)),
        request.approval.request_id
      )
      job.result = encodeControlResult('derive_root', { Ok: result })
      job.stage = 'wrap'
      await this.save(job)
    }
    if (!job.bytes) {
      progress('wrap')
      const { descriptor, encryptedKey } = derivedRootKey(
        controlResult(
          decodeControlResult('derive_root', job.result) as
            { Ok: ExecutionResult } | { Err: unknown }
        ),
        context
      )
      job.bytes = b64(await crypto.call('wrapAccountRoot', context, descriptor, encryptedKey))
      job.stage = 'upload'
      await this.save(job)
    }
    progress('upload')
    state = await account.refresh(accountId)
    const session = await this.session(accountId, state)
    const bytes = unb64(job.bytes),
      placeholder = canonical([]),
      base = `/v1/accounts/${accountId}`
    if (!job.plan) {
      job.plan = {
        upload_id: id(),
        object_id: id(),
        version_id: id(),
        root_generation: context.generation,
        epoch: 0,
        control_head: null,
        kind: 'root',
        chunks: [{ digest: hash(placeholder), size: placeholder.length }],
        manifest_digest: hash(bytes),
        manifest_size: bytes.length,
        expires_at: Date.now() + 86400000
      }
      await this.save(job)
    }
    const upload = (await session.find(`${base}/uploads/${job.plan.upload_id}`)) as {
      status: string
      plan: Record<string, unknown>
    } | null
    if (upload)
      ensure(equal(canonical(upload.plan), canonical(job.plan)), 'IDEMPOTENCY_CONFLICT')
    // The persisted plan and upload ID are immutable across fresh PoP calls.
    else await session.post(`${base}/uploads`, 'dmsg/upload/reserve/v1', job.plan)
    if (upload?.status !== 'committed') {
      await this.cloud.putChunk(
        `${base}/uploads/${job.plan.upload_id}/chunks/0`,
        placeholder,
        await session.context(),
        session.sign
      )
      await this.cloud.putChunk(
        `${base}/uploads/${job.plan.upload_id}/chunks/manifest`,
        bytes,
        await session.context(),
        session.sign
      )
      const result = await session.post(
        `${base}/uploads/finalize`,
        'dmsg/upload/finalize/v1',
        {
          upload_id: job.plan.upload_id
        }
      )
      ensure(result.digest === hash(bytes), 'INTEGRITY_FAILED')
    }
    await this.cloud.getChunk(
      `${base}/objects/${job.plan.upload_id}/chunks/manifest`,
      hash(bytes),
      await session.context(),
      session.sign
    )
    job.stage = 'commit'
    await this.save(job)
    progress('commit')
    state = await account.mutate(accountId, {
      CommitRoot: {
        expected_generation: BigInt(job.expectedGeneration),
        op_id: unhex(job.opId),
        root: {
          generation: BigInt(context.generation),
          suite: 'dmsg-root-v1',
          home_cose: state.info.home_cose,
          derivation_version: 2,
          key_generation: BigInt(context.generation),
          bundle_digest: unhex(hash(bytes)),
          recovery_generation: BigInt(context.recoveryGeneration)
        }
      }
    })
    ensure(committed(), 'INTEGRITY_FAILED')
    job.stage = 'committed'
    await this.save(job)
    progress('committed')
    return job
  }
}
