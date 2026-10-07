import type { DeriveRootRequest, ExecutionResult } from '../canisters/generated/user'
import type { _SERVICE as CoseService, KeyDescriptor } from '../canisters/generated/cose'
import { b64, canonical, equal, hash, hex, id, unb64, unhex } from '../protocol/codec'
import { decodeControl, encodeControl } from '../protocol/account'
import { xidBytes } from '../protocol/identity'
import {
  bundleDigests,
  parseRootBundle,
  readRootBundle,
  recoveryKeyOf,
  ROOT_SUITE,
  type RecoveryKey,
  type RootBundle,
  type RootContext
} from '../crypto/root'
import { ensure } from '../errors'
import { prepareRootDerivation, recordedExecution, submitDerivation } from './cose'
import { AccountClient, controlResult } from './account'
import type { CloudClient } from './relay'
import { CloudSession, type AccountState } from './cloud-session'
import { config } from '../config'

export interface RootJob {
  version: 2
  account: string
  home: string
  opId: string
  expectedGeneration: number
  stage: 'reserve' | 'wrap' | 'upload' | 'commit' | 'committed'
  context?: RootContext
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
  return result.outcome.Completed
}
/** Every retry preserves the original reservation, request and bundle bytes. */
export class AccountRootClient {
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudClient,
    readonly cose: CoseService
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
  /** The executor's content-root key for a generation, checked against the
   * account's home, the deployment and any build-time pin. */
  async recoveryKey(accountId: string, generation: number, homeCose: string): Promise<RecoveryKey> {
    const key: KeyDescriptor = controlResult(
      await this.cose.root_public_key(xidBytes(accountId), BigInt(generation))
    )
    const publicKey = Uint8Array.from(key.public_key)
    ensure(
      key.home_cose.toText() === homeCose &&
        key.key_generation === BigInt(generation) &&
        key.derivation_version === 2 &&
        equal(Uint8Array.from(key.account_id), xidBytes(accountId)) &&
        Object.keys(key.environment)[0].toLowerCase() === this.account.meta.environment &&
        publicKey.length === 96 &&
        hex(Uint8Array.from(key.public_key_fingerprint)) === hash(publicKey) &&
        (config.environment === 'local'
          ? ['key_1', 'test_key_1', 'dfx_test_key'].includes(key.master_key_name)
          : key.master_key_name === 'key_1') &&
        (!config.coseRootPublicKey || config.coseRootPublicKey === b64(publicKey)),
      'INTEGRITY_FAILED',
      '密钥服务返回的恢复公钥与配置不一致。'
    )
    const descriptor = { homeCose, keyName: key.master_key_name, publicKey: b64(publicKey) }
    recoveryKeyOf(descriptor)
    return descriptor
  }
  async restartExpired(accountId: string) {
    const job = await this.job(accountId)
    ensure(job && job.stage !== 'committed', 'NOT_FOUND')
    if (await this.account.pending()) await this.account.resume()
    ensure(!(await this.account.pending()), 'EXECUTION_UNKNOWN')
    const { info, verified } = await this.account.refresh(accountId)
    if (job.bytes && info.current_root[0] && this.matches(job.bytes, info.current_root[0]))
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
    await this.account.crypto.call(
      'controlPut',
      `root-history:${job.opId}`,
      JSON.stringify(job)
    )
    await this.account.crypto.call('controlPut', `root:${accountId}`, 'null')
    return this.run(accountId)
  }
  private matches(bytes: string, ref: { bundle_digest: Uint8Array | number[] }) {
    return (
      hex(Uint8Array.from(ref.bundle_digest)) ===
      hex(bundleDigests(parseRootBundle(unb64(bytes))).bundleDigest)
    )
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
  /** Download the committed bundle and every bundle its previous chain names. */
  private async fetchBundles(accountId: string, state: AccountState) {
    const ref = state.info.current_root[0]
    ensure(ref, 'RECOVERY_INCOMPLETE')
    const session = await this.session(accountId, state)
    const base = `/v1/accounts/${accountId}`,
      expected = hex(Uint8Array.from(ref.bundle_digest))
    const response = (await session.get(`${base}/root`)) as {
      root: { upload_id: string; digest: string; root_generation: number }
    }
    ensure(
      /^[0-9a-f]{64}$/.test(response.root.digest) &&
        response.root.root_generation === Number(ref.generation) &&
        /^[0-9a-f]{64}$/.test(response.root.upload_id),
      'INTEGRITY_FAILED'
    )
    const bundles = [
      await this.cloud.getChunk(
        `${base}/objects/${response.root.upload_id}/chunks/manifest`,
        response.root.digest,
        await session.context(),
        session.sign
      )
    ]
    const current = readRootBundle(bundles[0], expected)
    ensure(
      current.body.context.account === accountId &&
        current.body.context.environment === this.account.meta.environment &&
        current.body.context.generation === Number(ref.generation),
      'INTEGRITY_FAILED'
    )
    let cursor = current
    while (cursor.body.previous) {
      const link = cursor.body.previous
      ensure(
        bundles.length < 256 &&
          link.generation < cursor.body.context.generation &&
          /^[0-9a-f]{64}$/.test(link.uploadId),
        'RECOVERY_INCOMPLETE'
      )
      const data = await this.cloud.getChunk(
        `${base}/objects/${link.uploadId}/chunks/manifest`,
        link.digest,
        await session.context(),
        session.sign
      )
      cursor = parseRootBundle(data)
      bundles.push(data)
    }
    return { current, bundles, expected, uploadId: response.root.upload_id }
  }
  private async committedJob(accountId: string, current: RootBundle, bundles: Uint8Array[], uploadId: string) {
    const job: RootJob = {
      version: 2,
      account: accountId,
      home: this.account.home.toText(),
      opId: current.body.context.opId,
      expectedGeneration: 0,
      context: current.body.context,
      stage: 'committed',
      bytes: b64(bundles[0]),
      plan: { upload_id: uploadId }
    }
    await this.save(job)
    return job
  }
  private async settleStaleJob(accountId: string, state: AccountState) {
    const pending = await this.job(accountId)
    if (!pending || pending.stage === 'committed') return state
    // A different device may have won the reservation. The certified current
    // generation makes this old expected-generation CAS impossible to commit.
    ensure(
      (state.info.current_root[0]?.generation ?? 0n) > BigInt(pending.expectedGeneration),
      'Pending',
      '请先对账并完成原根操作。'
    )
    if (await this.account.pending()) await this.account.resume()
    ensure(!(await this.account.pending()), 'EXECUTION_UNKNOWN')
    await this.account.crypto.call('controlPut', `root-history:${pending.opId}`, JSON.stringify(pending))
    await this.account.crypto.call('controlPut', `root:${accountId}`, 'null')
    return this.account.refresh(accountId)
  }
  /** Open the committed root with this device's own envelope. */
  async openCurrent(accountId: string): Promise<RootJob> {
    let state = await this.account.refresh(accountId)
    ensure(state.info.current_root[0] && state.device && !state.device.revoked_at.length, 'DeviceNotApproved')
    state = await this.settleStaleJob(accountId, state)
    const { current, bundles, expected, uploadId } = await this.fetchBundles(accountId, state)
    await this.account.crypto.call('openAccountRoot', current.body.context, bundles, expected)
    return this.committedJob(accountId, current, bundles, uploadId)
  }
  /** After a completed login recovery, derive the committed generation's
   * vetKD key once and open the recovery envelope. The caller then rotates. */
  async recoverCurrent(accountId: string): Promise<RootJob> {
    const account = this.account,
      crypto = account.crypto
    let state = await account.refresh(accountId)
    const ref = state.info.current_root[0],
      recovered = state.info.recovered_device[0]
    ensure(
      ref &&
        state.device &&
        !state.device.revoked_at.length &&
        recovered &&
        hex(Uint8Array.from(recovered[0])) === account.meta.deviceId &&
        recovered[1] === ref.generation,
      'DeviceNotApproved',
      '只有刚完成恢复的设备可以派生当前根。'
    )
    state = await this.settleStaleJob(accountId, state)
    const { current, bundles, expected, uploadId } = await this.fetchBundles(accountId, state)
    const context = current.body.context,
      recoveryKey = await this.recoveryKey(accountId, context.generation, state.info.home_cose.toText())
    ensure(
      equal(canonical(recoveryKey), canonical(current.body.recoveryKey)),
      'INTEGRITY_FAILED',
      '根包的恢复公钥与密钥服务的描述不一致。'
    )
    const journalKey = `recover-root:${accountId}:${expected}`
    const saved = await crypto.call('controlGet', journalKey)
    const approve = async () => {
      state = await account.refresh(accountId)
      ensure(
        state.device &&
          state.info.current_root[0] &&
          hex(Uint8Array.from(state.info.current_root[0].bundle_digest)) === expected,
        'POLICY_STALE'
      )
      const prepared = prepareRootDerivation(
        {
          homeUser: account.home,
          accountId: xidBytes(accountId),
          issuer: state.info.issuer,
          deviceId: unhex(account.meta.deviceId),
          securityEpoch: state.info.security_epoch,
          sequence: state.device.next_sequence,
          expiresAt: BigInt(Date.now() + 300000)
        },
        BigInt(context.generation),
        await crypto.call('prepareAccountRoot', context),
        BigInt(config.rootDerivationMaxCycles)
      )
      const request = prepared.request
      request.approval.signature = await crypto.call('deviceSign', prepared.approvalMessage)
      await crypto.call('controlPut', journalKey, JSON.stringify(encodeControl('derive_root', [request])))
      return request
    }
    let request = saved
      ? (decodeControl('derive_root', JSON.parse(saved) as string)[0] as DeriveRootRequest)
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
          `recover-root-history:${hex(Uint8Array.from(request.approval.request_id))}`,
          JSON.stringify(encodeControl('derive_root', [request]))
        )
        request = await approve()
      }
      result = await submitDerivation(account.user, request)
    }
    const output = completedDerivation(result, request.approval.request_id)
    const key = output.key
    ensure(
      key.home_cose.toText() === recoveryKey.homeCose &&
        key.master_key_name === recoveryKey.keyName &&
        b64(Uint8Array.from(key.public_key)) === recoveryKey.publicKey &&
        key.key_generation === BigInt(context.generation),
      'INTEGRITY_FAILED'
    )
    await crypto.call(
      'recoverAccountRoot',
      context,
      recoveryKey,
      Uint8Array.from(output.encrypted_key),
      bundles,
      expected
    )
    return this.committedJob(accountId, current, bundles, uploadId)
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
        '请先在此设备读取当前内容根，不能用空白根覆盖已有内容。'
      )
      job = {
        version: 2,
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
      !!job!.bytes && !!state.info.current_root[0] && this.matches(job!.bytes, state.info.current_root[0])
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
      ensure(slot && hex(Uint8Array.from(slot.op_id)) === job.opId, 'VERSION_CONFLICT')
      job.context = {
        account: accountId,
        environment: account.meta.environment,
        generation: Number(slot.generation),
        opId: job.opId,
        securityEpoch: Number(slot.security_epoch)
      }
      job.stage = 'wrap'
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
    if (!job.bytes) {
      progress('wrap')
      // The recipients are the certified active devices of the reserved epoch;
      // the commit rejects any other set.
      const recipients = state.info.devices
        .filter(([, device]) => !device.revoked_at.length)
        .map(([key, device]) => ({
          deviceId: hex(Uint8Array.from(key)),
          hpkePublic: b64(Uint8Array.from(device.input.hpke_pub))
        }))
      const recoveryKey = await this.recoveryKey(
        accountId,
        context.generation,
        state.info.home_cose.toText()
      )
      job.bytes = b64(await crypto.call('wrapAccountRoot', context, recipients, recoveryKey))
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
    const digests = bundleDigests(parseRootBundle(bytes))
    state = await account.mutate(accountId, {
      CommitRoot: {
        expected_generation: BigInt(job.expectedGeneration),
        op_id: unhex(job.opId),
        root: {
          generation: BigInt(context.generation),
          suite: ROOT_SUITE,
          bundle_digest: digests.bundleDigest,
          recipients_digest: digests.recipientsDigest,
          body_digest: digests.bodyDigest
        }
      }
    })
    ensure(committed(), 'INTEGRITY_FAILED')
    job.stage = 'committed'
    await this.save(job)
    progress('committed')
    return job
  }
  /** Make a committed job's root this workspace's root. */
  async activate(accountId: string, job: RootJob) {
    const state = await this.account.refresh(accountId)
    ensure(job.stage === 'committed' && job.context && job.bytes && job.plan, 'RECOVERY_INCOMPLETE')
    const ref = state.info.current_root[0]
    ensure(ref && this.matches(job.bytes, ref), 'POLICY_STALE')
    return this.account.crypto.call('activateAccountRoot', {
      context: job.context,
      digest: hex(Uint8Array.from(ref.bundle_digest)),
      uploadId: String(job.plan.upload_id),
      homeUser: this.account.home.toText(),
      issuer: state.info.issuer
    })
  }
}
