import { Principal } from '@icp-sdk/core/principal'
import type { _SERVICE, Registration, HandleIntent } from '../canisters/generated/handle'
import { handlePrice, encodeHandle, decodeHandle } from '../protocol/handle'
import { digest, canonical } from '../protocol/codec'
import type { WalletClient } from './wallet'
import { AccountClient, controlResult } from './account'
import {
  canonicalHandle,
  encodeClaim,
  decodeClaim,
  legacyClaimDigest
} from '../protocol/handle'
import { id, unhex, equal, decodeCanonical, utf8 } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { ensure } from '../errors'
import { certifiedLeaf } from './certified'

interface ClaimJob {
  version: 1
  name: string
  account: string
  sourceOwner: string
  registry: string
  args: string
  phase: 'prepared' | 'authorized' | 'unknown' | 'claimed' | 'rejected'
}
export class HandleClient {
  constructor(
    readonly account: AccountClient,
    readonly registry: _SERVICE,
    readonly registryId: string,
    readonly oldCaller: Principal
  ) {}
  async purchaseJob(): Promise<{
    args: string
    ledger: string
    total: string
    phase: string
  } | null> {
    const value = await this.account.crypto.call(
      'controlGet',
      `handle-purchase:${this.account.meta.account?.id}`
    )
    return value ? JSON.parse(value) : null
  }
  private savePurchase(job: { args: string; ledger: string; total: string; phase: string }) {
    return this.account.crypto.call(
      'controlPut',
      `handle-purchase:${this.account.meta.account?.id}`,
      JSON.stringify(job)
    )
  }
  async preparePurchase(name: string) {
    const prior = await this.purchaseJob()
    ensure(!prior || ['Committed', 'Rejected', 'review'].includes(prior.phase), 'Pending')
    const account = this.account.meta.account?.id
    ensure(account, 'AUTH_REQUIRED')
    name = canonicalHandle(name)
    ensure(!(await this.ownership(name)), 'IdempotencyConflict')
    ensure(
      (await this.registry.snapshot_progress()).sealed,
      'LegacyWriteDisabled',
      '名称快照尚未封存，暂不能购买新名称。'
    )
    // Check before the allowance is approved. A forged reply can only block
    // this purchase; the registry rejects reserved names on its own.
    ensure(
      !controlResult(await this.registry.get_legacy_reservation(name))[0],
      'VersionConflict',
      '此名称在旧名快照中预留，只能由原持有人认领。'
    )
    const cfg = await this.registry.get_handle_config()
    ensure(cfg.home_user.toText() === this.account.home.toText(), 'INTEGRITY_FAILED')
    const total = handlePrice(name)
    ensure(total > cfg.ledger_fee, 'FeeBlocked')
    const registration: Registration = {
      fee: cfg.ledger_fee,
      payer: { owner: this.oldCaller, subaccount: [] },
      intent: {
        handle_canister: Principal.fromText(this.registryId),
        action: { Register: null },
        account_id: xidBytes(account),
        target_account: [],
        handle: name,
        expected_version: 0n,
        op_id: unhex(id()),
        terms_digest: digest('dmsg/handle-charge/v1', [
          cfg.ledger.toUint8Array(),
          { owner: this.oldCaller.toUint8Array(), subaccount: null },
          total - cfg.ledger_fee,
          cfg.ledger_fee
        ])
      }
    }
    const job = {
      args: encodeHandle('register_handle', [registration]),
      ledger: cfg.ledger.toText(),
      total: total.toString(),
      phase: 'review'
    }
    await this.savePurchase(job)
    return job
  }
  async purchase(wallet: WalletClient, block?: bigint) {
    const job = await this.purchaseJob()
    ensure(job && wallet.owner.toText() === this.oldCaller.toText(), 'AUTH_REQUIRED')
    const [input] = decodeHandle('register_handle', job.args) as [Registration]
    ensure(
      input.payer.owner.toText() === wallet.owner.toText() &&
        equal(
          Uint8Array.from(input.intent.account_id),
          xidBytes(this.account.meta.account!.id)
        ),
      'FORBIDDEN'
    )
    const prior = await this.registry.get_handle_operation(
      input.intent.account_id,
      input.intent.op_id
    )
    let result
    if ('Ok' in prior) {
      result = ['Committed', 'Rejected'].some((k) => k in prior.Ok.phase)
        ? prior.Ok
        : controlResult(
            await (block === undefined
              ? this.registry.commit_handle(input.intent.account_id, input.intent.op_id)
              : this.registry.reconcile_handle_charge(
                  input.intent.account_id,
                  input.intent.op_id,
                  block
                ))
          )
    } else {
      ensure('NotFound' in prior.Err, 'EXECUTION_UNKNOWN')
      ensure(
        job.phase !== 'Rejected',
        'VersionConflict',
        '此购买请求已被拒绝，请重新核对名称和固定收费。'
      )
      await wallet.approveHandle(
        job.ledger,
        this.registryId,
        BigInt(job.total),
        input.fee,
        Array.from(input.intent.op_id, (x) => x.toString(16).padStart(2, '0')).join('')
      )
      await this.account.mutate(this.account.meta.account!.id, {
        AuthorizeHandle: { intent: input.intent }
      })
      job.phase = 'unknown'
      await this.savePurchase(job)
      const response = await this.registry.register_handle(input)
      // Only a definite canister rejection permits a new purchase. A transport
      // failure or unknown charge keeps this operation for reconciliation.
      if ('Err' in response && !('ExecutionUnknown' in response.Err)) {
        job.phase = 'Rejected'
        await this.savePurchase(job)
      }
      result = controlResult(response)
    }
    ensure(
      equal(
        canonical(result.registration.intent.terms_digest),
        canonical(input.intent.terms_digest)
      ),
      'INTEGRITY_FAILED'
    )
    job.phase = Object.keys(result.phase)[0]
    await this.savePurchase(job)
    return result
  }
  async prepareTransfer(name: string, target: string) {
    const owner = this.account.meta.account!.id,
      record = await this.ownership(name)
    ensure(
      record && equal(record.owner_account, xidBytes(owner)) && target !== owner,
      'FORBIDDEN'
    )
    const op = unhex(id()),
      terms = digest('dmsg/handle-transfer/v1', [
        Principal.fromText(this.registryId).toUint8Array(),
        canonicalHandle(name),
        xidBytes(owner),
        xidBytes(target),
        record.version,
        op
      ])
    const from: HandleIntent = {
      handle_canister: Principal.fromText(this.registryId),
      account_id: xidBytes(owner),
      target_account: [xidBytes(target)],
      handle: canonicalHandle(name),
      expected_version: BigInt(record.version),
      op_id: op,
      terms_digest: terms,
      action: { Transfer: null }
    }
    const accept: HandleIntent = {
      ...from,
      account_id: xidBytes(target),
      target_account: [xidBytes(owner)],
      action: { AcceptTransfer: null }
    }
    return encodeHandle('transfer_handle', [from, accept])
  }
  inspectTransfer(packet: string) {
    ensure(packet.length < 8000, 'INVALID_INPUT')
    const [from, accept] = decodeHandle('transfer_handle', packet) as [
      HandleIntent,
      HandleIntent
    ]
    const terms = digest('dmsg/handle-transfer/v1', [
      Principal.fromText(this.registryId).toUint8Array(),
      from.handle,
      Uint8Array.from(from.account_id),
      Uint8Array.from(accept.account_id),
      from.expected_version,
      Uint8Array.from(from.op_id)
    ])
    ensure(
      'Transfer' in from.action &&
        'AcceptTransfer' in accept.action &&
        from.handle_canister.toText() === this.registryId &&
        accept.handle_canister.toText() === this.registryId &&
        from.handle === accept.handle &&
        canonicalHandle(from.handle) === from.handle &&
        from.expected_version === accept.expected_version &&
        equal(Uint8Array.from(from.op_id), Uint8Array.from(accept.op_id)) &&
        from.target_account[0] &&
        accept.target_account[0] &&
        equal(Uint8Array.from(from.target_account[0]), Uint8Array.from(accept.account_id)) &&
        equal(Uint8Array.from(accept.target_account[0]), Uint8Array.from(from.account_id)) &&
        equal(terms, Uint8Array.from(from.terms_digest)) &&
        equal(terms, Uint8Array.from(accept.terms_digest)),
      'INTEGRITY_FAILED'
    )
    return { from, accept }
  }
  async transfer(packet: string, commit: boolean) {
    const { from, accept } = this.inspectTransfer(packet),
      own = xidBytes(this.account.meta.account!.id),
      intent = commit ? from : accept
    ensure(equal(own, Uint8Array.from(intent.account_id)), 'FORBIDDEN')
    await this.account.crypto.call(
      'controlPut',
      `handle-transfer:${this.account.meta.account!.id}`,
      packet
    )
    await this.account.mutate(this.account.meta.account!.id, { AuthorizeHandle: { intent } })
    return commit ? controlResult(await this.registry.transfer_handle(from, accept)) : null
  }
  async legacy(name: string) {
    name = canonicalHandle(name)
    // Plain queries suffice: the claim rechecks the exact frozen record on
    // chain, so a forged reply can only make that claim fail.
    const [records, progress] = await Promise.all([
      this.registry.get_legacy_reservation(name),
      this.registry.snapshot_progress()
    ])
    ensure(progress.sealed && progress.snapshot.length, 'Pending', '旧名称快照尚未完整封存。')
    const reservation = controlResult(records)[0]
    ensure(reservation, 'NOT_FOUND', '冻结快照中没有此名称。')
    ensure(reservation.handle === name, 'INTEGRITY_FAILED')
    ensure(!reservation.quarantined, 'Locked', '此名称已隔离，需先解决原权属。')
    const ownerIsName =
      reservation.legacy_name_principal[0]?.toText() === reservation.legacy_owner.toText()
    ensure(
      ownerIsName
        ? this.oldCaller.toText() !== reservation.legacy_owner.toText() &&
            reservation.frozen_admins.some((p) => p.toText() === this.oldCaller.toText())
        : this.oldCaller.toText() === reservation.legacy_owner.toText(),
      'Forbidden',
      '当前旧身份不是冻结 owner 或冻结管理员。普通委托不能认领。'
    )
    return { snapshot: progress.snapshot[0]!, reservation }
  }
  async ownership(name: string) {
    name = canonicalHandle(name)
    const proof = await certifiedLeaf(
      controlResult(await this.registry.resolve_handle_certified([name])),
      this.account.agent,
      this.registryId,
      utf8(name)
    )
    if (!proof.value) return null
    const record = decodeCanonical<{
      handle: string
      owner_account: Uint8Array
      version: number | bigint
    }>(proof.value)
    ensure(
      record.handle === name &&
        record.owner_account instanceof Uint8Array &&
        record.owner_account.length === 12 &&
        BigInt(record.version) > 0n,
      'INTEGRITY_FAILED'
    )
    return record
  }
  private key() {
    const id = this.account.meta.account?.id
    ensure(id, 'AUTH_REQUIRED')
    return `handle-claim:${id}`
  }
  async job(): Promise<ClaimJob | null> {
    const value = await this.account.crypto.call('controlGet', this.key())
    return value ? JSON.parse(value) : null
  }
  private save(job: ClaimJob) {
    return this.account.crypto.call('controlPut', this.key(), JSON.stringify(job))
  }
  async prepare(name: string) {
    const existing = await this.job()
    ensure(
      !existing || existing.phase === 'claimed' || existing.phase === 'rejected',
      'Pending',
      '请先继续原认领请求。'
    )
    const account = this.account.meta.account?.id
    ensure(account, 'AUTH_REQUIRED')
    const { snapshot, reservation } = await this.legacy(name)
    const intent = {
      handle_canister: Principal.fromText(this.registryId),
      action: { ClaimLegacy: null },
      account_id: xidBytes(account),
      target_account: [] as [],
      handle: reservation.handle,
      expected_version: 0n,
      op_id: unhex(id()),
      terms_digest: legacyClaimDigest(
        Uint8Array.from(snapshot.snapshot_id),
        reservation,
        xidBytes(account)
      )
    }
    const job: ClaimJob = {
      version: 1,
      name: reservation.handle,
      account,
      sourceOwner: this.oldCaller.toText(),
      registry: this.registryId,
      args: encodeClaim(intent, Uint8Array.from(snapshot.snapshot_id)),
      phase: 'prepared'
    }
    await this.save(job)
    return { job, snapshot, reservation }
  }
  async run() {
    const job = await this.job()
    ensure(
      job &&
        job.account === this.account.meta.account?.id &&
        job.registry === this.registryId &&
        job.sourceOwner === this.oldCaller.toText(),
      'AUTH_REQUIRED'
    )
    const [intent, snapshot] = decodeClaim(job.args)
    if (job.phase === 'claimed') return job
    ensure(
      job.phase !== 'rejected',
      'VersionConflict',
      '此认领请求已被拒绝，请重新核对权属与认领预览。'
    )
    if (await this.account.pending()) await this.account.resume()
    ensure(!(await this.account.pending()), 'Pending')
    const ownership = await this.ownership(job.name)
    if (ownership) {
      ensure(
        equal(ownership.owner_account, xidBytes(job.account)) &&
          BigInt(ownership.version) === 1n,
        'IDEMPOTENCY_CONFLICT',
        '名称已归属其它账户或发生后续转移。'
      )
      job.phase = 'claimed'
      await this.save(job)
      return job
    }
    // Reauthorize the same immutable business intent only after certified
    // absence. The canister checks the frozen record; advisory queries must not
    // prevent a prepared claim from reaching that authoritative result.
    await this.account.mutate(job.account, { AuthorizeHandle: { intent } })
    job.phase = 'authorized'
    await this.save(job)
    job.phase = 'unknown'
    await this.save(job)
    const response = await this.registry.claim_legacy_handle(intent, snapshot)
    // Only a definite canister rejection permits fresh arguments. A transport
    // failure or unknown execution keeps the original claim available for retry.
    if ('Err' in response && !('ExecutionUnknown' in response.Err)) {
      job.phase = 'rejected'
      await this.save(job)
    }
    const record = controlResult(response)
    ensure(
      record.handle === job.name &&
        record.version === 1n &&
        equal(Uint8Array.from(record.owner_account), xidBytes(job.account)),
      'INTEGRITY_FAILED'
    )
    job.phase = 'claimed'
    await this.save(job)
    return job
  }
}
