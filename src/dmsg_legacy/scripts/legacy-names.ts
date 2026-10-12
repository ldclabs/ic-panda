import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { Tagged, digest, equal } from 'dmsg-sdk/codec'
import { DmsgError } from 'dmsg-sdk/errors'
import {
  verifySnapshotProof,
  foldSnapshotRow,
  type SnapshotProof,
  type SnapshotScope
} from '../src'

// The dmsg_handle types used, as in src/dmsg_handle/dmsg_handle.did.
export interface LegacyReservation {
  frozen_admins: Principal[]
  handle: string
  legacy_name_principal: [] | [Principal]
  legacy_owner: Principal
  quarantined: boolean
}
export interface LegacySnapshot {
  event_tip: Uint8Array
  source_canister: Principal
  count: bigint
  entries_digest: Uint8Array
  snapshot_id: Uint8Array
  freeze_version: bigint
}
export const LegacyReservationType = IDL.Record({
  frozen_admins: IDL.Vec(IDL.Principal),
  handle: IDL.Text,
  legacy_name_principal: IDL.Opt(IDL.Principal),
  legacy_owner: IDL.Principal,
  quarantined: IDL.Bool
})
export const LegacySnapshotType = IDL.Record({
  event_tip: IDL.Vec(IDL.Nat8),
  source_canister: IDL.Principal,
  count: IDL.Nat64,
  entries_digest: IDL.Vec(IDL.Nat8),
  snapshot_id: IDL.Vec(IDL.Nat8),
  freeze_version: IDL.Nat64
})

function ensure(condition: unknown, code: string, message?: string): asserts condition {
  if (!condition) throw new DmsgError(code, message)
}

/** A Candid value as the canonical value dmsg_handle digests (its serde form). */
function candidValue(type: IDL.Type, value: any): unknown {
  if (type instanceof IDL.RecClass) {
    const inner = type.getType()
    ensure(inner, 'UNSUPPORTED_PROTOCOL')
    return candidValue(inner, value)
  }
  if (type instanceof IDL.OptClass) {
    ensure(Array.isArray(value) && value.length <= 1, 'INVALID_INPUT')
    return value.length ? candidValue(type._type, value[0]) : null
  }
  if (type instanceof IDL.VecClass)
    return type._type.name === 'nat8'
      ? Uint8Array.from(value)
      : Array.from(value as unknown[], (v) => candidValue(type._type, v))
  if (type instanceof IDL.TupleClass)
    return type._fields.map(([, child], i) => candidValue(child, value[i]))
  if (type instanceof IDL.RecordClass)
    return Object.fromEntries(
      type._fields.map(([name, child]) => [name, candidValue(child, value[name])])
    )
  if (type instanceof IDL.VariantClass) {
    const entries = Object.entries(value)
    ensure(entries.length === 1, 'INVALID_INPUT')
    const [name, childValue] = entries[0],
      child = type._fields.find(([key]) => key === name)?.[1]
    ensure(child, 'UNSUPPORTED_PROTOCOL')
    return child.name === 'null' ? name : { [name]: candidValue(child, childValue) }
  }
  if (typeof value === 'bigint' && value > 0xffffffffffffffffn) {
    ensure(value <= (1n << 128n) - 1n && type.name === 'nat', 'INVALID_INPUT')
    let hex = value.toString(16)
    if (hex.length % 2) hex = '0' + hex
    return new Tagged(
      2,
      Uint8Array.from(hex.match(/../g)!, (b) => parseInt(b, 16))
    )
  }
  return value instanceof Principal ? value.toUint8Array() : value
}

function canonicalHandle(value: string) {
  const name = value.toLowerCase()
  ensure(
    /^[a-z0-9][a-z0-9_]{0,18}$/.test(name),
    'INVALID_INPUT',
    '名称应为 1–19 个字母、数字或下划线，且不能以下划线开头。'
  )
  return name
}

/** dmsg_handle's fold of one imported entry into the snapshot digest. */
export const legacyEntryDigest = (previous: Uint8Array, value: LegacyReservation) =>
  digest('dmsg/legacy-entry/v1', [previous, candidValue(LegacyReservationType, value)])

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
