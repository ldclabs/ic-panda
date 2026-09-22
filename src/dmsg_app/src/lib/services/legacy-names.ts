import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import {
  verifySnapshotProof,
  foldSnapshotRow,
  type SnapshotProof,
  type SnapshotScope
} from '@dmsg/legacy'
import type {
  _SERVICE,
  LegacySnapshot,
  LegacyReservation
} from '../canisters/generated/handle'
import { canonicalHandle, legacyEntryDigest, progressValue } from '../protocol/handle'
import { canonical, digest, equal } from '../protocol/codec'
import { controlResult } from './account'
import { ensure } from '../errors'

async function sourceRows(
  proofs: SnapshotProof[],
  rootKey: Uint8Array,
  canister: string,
  scope: SnapshotScope
) {
  ensure(proofs.length > 0 && proofs.length <= 10000, 'INVALID_INPUT')
  let cursor: string | null = null,
    rolling: Uint8Array | null = null,
    header: Awaited<ReturnType<typeof verifySnapshotProof>>['page'] | null = null
  const rows: [string, Uint8Array][] = []
  for (const [index, proof] of proofs.entries()) {
    const { page } = await verifySnapshotProof(proof, {
      rootKey,
      canister,
      caller: proofs[0].caller,
      scope,
      after: cursor
    })
    if (header)
      ensure(
        equal(header.inventory_digest, page.inventory_digest) &&
          equal(header.initial_digest, page.initial_digest) &&
          header.count === page.count &&
          header.freeze.epoch === page.freeze.epoch,
        'INTEGRITY_FAILED'
      )
    else {
      header = page
      rolling = page.initial_digest
    }
    for (const [key, bytes] of page.entries) rolling = foldSnapshotRow(rolling!, key, bytes)
    rows.push(...page.entries)
    ensure(
      page.complete === (index === proofs.length - 1),
      'RECOVERY_INCOMPLETE',
      '来源分页没有完整闭合。'
    )
    cursor = page.next[0] ?? null
  }
  ensure(
    header && BigInt(rows.length) === header.count && equal(rolling!, header.inventory_digest),
    'RECOVERY_INCOMPLETE'
  )
  return { header, rows }
}
export async function prepareLegacyNames(input: {
  names: SnapshotProof[]
  authorities: SnapshotProof[]
  rootKey: Uint8Array
  source: string
  identity: string
  expectedNames: number
  cutover: Uint8Array
}) {
  ensure(
    Number.isSafeInteger(input.expectedNames) && input.expectedNames > 0,
    'INVALID_INPUT',
    '须使用部署盘点中的非零名称数量，不能用空快照放行。'
  )
  const names = await sourceRows(input.names, input.rootKey, input.source, { Names: null })
  const authorities = await sourceRows(input.authorities, input.rootKey, input.identity, {
    Authorities: null
  })
  ensure(
    input.cutover.length === 32 &&
      input.cutover.some((v) => v !== 0) &&
      equal(names.header.freeze.cutover_id[0], input.cutover) &&
      names.rows.length === input.expectedNames &&
      equal(names.header.freeze.cutover_id[0], authorities.header.freeze.cutover_id[0]),
    'VERSION_CONFLICT',
    '名称数量或冻结批次与盘点不一致。'
  )
  const auth = new Map<string, { principal: Principal; admins: Principal[] }>()
  for (const [key, bytes] of authorities.rows) {
    const [name, principal, roles] = IDL.decode(
      [IDL.Text, IDL.Principal, IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Int8))],
      bytes
    ) as unknown as [string, Principal, [Principal, number][]]
    ensure(name === key && !auth.has(name), 'INTEGRITY_FAILED')
    auth.set(name, {
      principal,
      admins: roles.filter(([, role]) => role === 1).map(([p]) => p)
    })
  }
  const entries: LegacyReservation[] = []
  let rolling = new Uint8Array(32)
  for (const [key, bytes] of names.rows) {
    const [name, owner, namedPrincipal] = IDL.decode(
      [IDL.Text, IDL.Principal, IDL.Principal],
      bytes
    ) as unknown as [string, Principal, Principal]
    ensure(
      name === key &&
        canonicalHandle(name) === name &&
        !owner.isAnonymous() &&
        owner.toUint8Array().length > 0,
      'INTEGRITY_FAILED',
      '原名称或 owner 需要先核对。'
    )
    const authority = auth.get(name)
    ensure(
      !authority || authority.principal.toText() === namedPrincipal.toText(),
      'INTEGRITY_FAILED'
    )
    const admins = authority?.admins ?? []
    ensure(
      admins.length <= 16 && new Set(admins.map((p) => p.toText())).size === admins.length,
      'INTEGRITY_FAILED'
    )
    const entry: LegacyReservation = {
      handle: name,
      legacy_owner: owner,
      legacy_name_principal: [namedPrincipal],
      frozen_admins: admins,
      quarantined: owner.toText() === namedPrincipal.toText() && admins.length === 0
    }
    entries.push(entry)
    rolling = Uint8Array.from(legacyEntryDigest(rolling, entry))
    auth.delete(name)
  }
  ensure(
    auth.size === 0,
    'RECOVERY_INCOMPLETE',
    '存在未对应注册表的旧名称身份，须先隔离并解决来源差异。'
  )
  const [height, tip] = IDL.decode(
    [IDL.Nat64, IDL.Vec(IDL.Nat8)],
    names.header.source_context
  ) as unknown as [bigint, Uint8Array]
  ensure(tip.length === 32 && height >= 0n, 'INTEGRITY_FAILED')
  const snapshot: LegacySnapshot = {
    source_canister: Principal.fromText(input.source),
    snapshot_id: digest('dmsg/legacy-import-snapshot/v1', [
      Principal.fromText(input.source).toUint8Array(),
      names.header.freeze.epoch,
      names.header.freeze.cutover_id[0],
      tip,
      rolling,
      Principal.fromText(input.identity).toUint8Array(),
      authorities.header.freeze.epoch
    ]),
    freeze_version: names.header.freeze.epoch,
    event_tip: tip,
    count: BigInt(entries.length),
    entries_digest: rolling
  }
  return {
    snapshot,
    entries,
    quarantined: entries.filter((e) => e.quarantined).map((e) => e.handle)
  }
}
/** Caller supplies a controller actor; retries inspect the authoritative prefix. */
export async function importLegacyNames(
  registry: _SERVICE,
  input: { snapshot: LegacySnapshot; entries: LegacyReservation[] },
  onProgress: (count: number) => void = () => {}
) {
  let state = await registry.snapshot_progress()
  if (!state.snapshot.length) {
    controlResult(await registry.begin_legacy_snapshot(input.snapshot))
    state = await registry.snapshot_progress()
  }
  ensure(
    state.snapshot.length &&
      equal(
        canonical((progressValue({ ...state, snapshot: [input.snapshot] }) as any).snapshot),
        canonical((progressValue(state) as any).snapshot)
      ),
    'VERSION_CONFLICT'
  )
  const count = Number(state.imported)
  ensure(Number.isSafeInteger(count) && count <= input.entries.length, 'INTEGRITY_FAILED')
  let rolling = new Uint8Array(32)
  for (const entry of input.entries.slice(0, count))
    rolling = Uint8Array.from(legacyEntryDigest(rolling, entry))
  ensure(
    equal(rolling, Uint8Array.from(state.rolling_digest)) &&
      (state.last_handle[0] ?? null) === (input.entries[count - 1]?.handle ?? null),
    'INTEGRITY_FAILED',
    '已导入前缀与输入清单不一致。'
  )
  for (let index = count; index < input.entries.length; index += 256) {
    state = controlResult(
      await registry.import_legacy_handles(
        input.snapshot.snapshot_id,
        input.entries.slice(index, index + 256)
      )
    )
    onProgress(Number(state.imported))
  }
  state = controlResult(await registry.seal_legacy_snapshot())
  ensure(
    state.sealed &&
      state.imported === input.snapshot.count &&
      equal(
        Uint8Array.from(state.rolling_digest),
        Uint8Array.from(input.snapshot.entries_digest)
      ),
    'INTEGRITY_FAILED'
  )
  return state
}
