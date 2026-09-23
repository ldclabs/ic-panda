import { Principal } from '@icp-sdk/core/principal'
import type { _SERVICE } from '../canisters/generated/handle'
import { AccountClient, controlResult } from './account'
import {
  canonicalHandle,
  encodeClaim,
  decodeClaim,
  legacyClaimDigest,
  legacyReservationDigest,
  progressValue
} from '../protocol/handle'
import { id, unhex, equal, canonical, decodeCanonical, utf8 } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { ensure } from '../errors'
import { certifiedValue, certifiedLeaf } from './certified'

interface ClaimJob {
  version: 1
  name: string
  account: string
  sourceOwner: string
  registry: string
  args: string
  phase: 'prepared' | 'authorized' | 'unknown' | 'claimed'
}
export class HandleClient {
  constructor(
    readonly account: AccountClient,
    readonly registry: _SERVICE,
    readonly registryId: string,
    readonly oldCaller: Principal
  ) {}
  async legacy(name: string) {
    name = canonicalHandle(name)
    const {
      progress,
      reservation: records,
      proof
    } = controlResult(await this.registry.get_legacy_reservation_certified(name))
    const snapshot = await certifiedValue(
      proof,
      this.account.agent,
      this.registryId,
      utf8('_legacy_snapshot')
    )
    ensure(
      equal(canonical(decodeCanonical(snapshot.value)), canonical(progressValue(progress))),
      'INTEGRITY_FAILED'
    )
    ensure(progress.sealed && progress.snapshot.length, 'Pending', '旧名称快照尚未完整封存。')
    const entry = await certifiedLeaf(
      proof,
      this.account.agent,
      this.registryId,
      utf8(`_legacy/${name}`)
    )
    const reservation = records[0]
    if (!reservation) {
      ensure(entry.value === null, 'INTEGRITY_FAILED')
      ensure(false, 'NOT_FOUND', '冻结快照中没有此名称。')
    }
    ensure(
      reservation.handle === name &&
        entry.value &&
        equal(entry.value, canonical(legacyReservationDigest(reservation))),
      'INTEGRITY_FAILED'
    )
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
    ensure(!existing || existing.phase === 'claimed', 'Pending', '请先继续原认领请求。')
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
    await this.legacy(job.name)
    // Reauthorize the same immutable business intent only after certified
    // absence. Expiry never creates another claim or changes its target.
    await this.account.mutate(job.account, { AuthorizeHandle: { intent } })
    job.phase = 'authorized'
    await this.save(job)
    job.phase = 'unknown'
    await this.save(job)
    const record = controlResult(await this.registry.claim_legacy_handle(intent, snapshot))
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
