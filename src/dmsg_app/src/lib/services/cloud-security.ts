import type { HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import type {
  _SERVICE,
  CertifiedBatch,
  Device,
  SecuritySnapshot
} from '../canisters/generated/user'
import { ensure } from '../errors'
import { b64, canonical, decodeCanonical, digest, equal, hex } from '../protocol/codec'
import { certifiedValue } from './certified'
import { xidBytes } from '../protocol/identity'

export interface CloudSecurityEvidence {
  schema: 1
  canister: string
  certificate: string
  entries: { account_id: string; value: string; witness: string; devices: string }[]
}
export interface CloudSecurityTrust {
  accountId: string
  issuer: string
  homeUser: string
}
type UserQueries = Pick<_SERVICE, 'security_snapshot_batch' | 'get_device_bundle'>
type Bundle = [SecuritySnapshot, Array<[Uint8Array | number[], Device]>]

function variant(value: Record<string, null>, allowed: string[]) {
  const keys = Object.keys(value)
  ensure(
    keys.length === 1 && allowed.includes(keys[0]) && value[keys[0]] === null,
    'UNSUPPORTED_PROTOCOL'
  )
  return keys[0]
}
function blob(value: Uint8Array | number[], size = 32) {
  const result = Uint8Array.from(value)
  ensure(result.length === size, 'INTEGRITY_FAILED')
  return result
}
const capabilities = [
  'ContentSign',
  'VaultUnlock',
  'RootManage',
  'FormalApprove',
  'PaymentOffer'
]

/** Encode all PUBLIC Device fields, including revoked records and sequence values. */
export function encodeDeviceBundle(bundle: Bundle) {
  const map = new Map<Uint8Array, unknown>(),
    seen = new Set<string>()
  ensure(bundle[1].length <= 16, 'INTEGRITY_FAILED')
  for (const [key, device] of bundle[1]) {
    const id = blob(key),
      input = device.input
    ensure(!seen.has(hex(id)) && equal(id, blob(input.device_id)), 'INTEGRITY_FAILED')
    seen.add(hex(id))
    const caps = input.capabilities.map((c) => variant(c, capabilities))
    ensure(caps.length > 0 && new Set(caps).size === caps.length, 'INTEGRITY_FAILED')
    map.set(id, {
      input: {
        device_id: id,
        signing_pub: blob(input.signing_pub),
        hpke_pub: blob(input.hpke_pub),
        role: variant(input.role, ['Administrator', 'Member']),
        capabilities: caps
      },
      added_at: device.added_at,
      added_by: device.added_by.length ? blob(device.added_by[0]!) : null,
      revoked_at: device.revoked_at[0] ?? null,
      next_sequence: device.next_sequence
    })
  }
  const root = digest('dmsg/devices/v1', map)
  ensure(equal(root, blob(bundle[0].devices_root)), 'INTEGRITY_FAILED')
  return { encoded: canonical(map), root }
}

export async function verifyCloudSecurity(
  batch: CertifiedBatch,
  bundle: Bundle,
  agent: Pick<HttpAgent, 'rootKey'>,
  trust: CloudSecurityTrust,
  now = Date.now(),
  allowDisputed = false
) {
  const account = xidBytes(trust.accountId),
    home = Principal.fromText(trust.homeUser)
  ensure(
    batch.schema === 1 &&
      batch.canister.toText() === home.toText() &&
      batch.entries.length === 1 &&
      batch.entries[0].witness.length <= 65536 &&
      (batch.entries[0].value[0]?.length ?? 0) <= 65536,
    'INTEGRITY_FAILED'
  )
  const entry = batch.entries[0],
    { value, certifiedAt, expiresAt } = await certifiedValue(
      batch,
      agent,
      trust.homeUser,
      account,
      now
    )
  const snapshot = decodeCanonical<Record<string, unknown>>(value)
  const { root, encoded } = encodeDeviceBundle(bundle)
  ensure(
    snapshot.schema === 2 &&
      snapshot.issuer === trust.issuer &&
      snapshot.account_id instanceof Uint8Array &&
      equal(snapshot.account_id, account) &&
      snapshot.home_user instanceof Uint8Array &&
      equal(snapshot.home_user, home.toUint8Array()) &&
      snapshot.devices_root instanceof Uint8Array &&
      equal(snapshot.devices_root, root),
    'INTEGRITY_FAILED'
  )
  ensure(
    snapshot.account_status === 'Active' ||
      (allowDisputed && snapshot.account_status === 'RecoveryDisputed'),
    'LOCKED'
  )
  const safe = (v: unknown) => {
    ensure(
      (typeof v === 'number' && Number.isSafeInteger(v) && v >= 0) ||
        (typeof v === 'bigint' && v >= 0n && v <= BigInt(Number.MAX_SAFE_INTEGER)),
      'UNSUPPORTED_PROTOCOL'
    )
    return Number(v)
  }
  const evidence: CloudSecurityEvidence = {
    schema: 1,
    canister: home.toText(),
    certificate: b64(Uint8Array.from(batch.certificate)),
    entries: [
      {
        account_id: trust.accountId,
        value: b64(value),
        witness: b64(Uint8Array.from(entry.witness)),
        devices: b64(encoded)
      }
    ]
  }
  return {
    evidence,
    certifiedAt,
    expiresAt,
    snapshot,
    securityEpoch: safe(snapshot.security_epoch),
    accountVersion: safe(snapshot.account_version),
    devices: bundle[1].map(([, d]) => d)
  }
}

/** A concurrent device change fails the root comparison; callers may query again. */
export async function readCloudSecurity(
  actor: UserQueries,
  agent: HttpAgent,
  trust: CloudSecurityTrust
) {
  return (await readCloudSecurityBatch(actor, agent, [trust]))[0]
}
/** Queries up to 64 accounts per certified batch; all share one certificate check. */
export async function readCloudSecurityBatch(
  actor: UserQueries,
  agent: HttpAgent,
  trusts: CloudSecurityTrust[]
) {
  const result = []
  for (let offset = 0; offset < trusts.length; offset += 64) {
    const group = trusts.slice(offset, offset + 64),
      ids = group.map((trust) => xidBytes(trust.accountId))
    const [batch, bundles] = await Promise.all([
      actor.security_snapshot_batch(ids),
      Promise.all(ids.map((id) => actor.get_device_bundle(id)))
    ])
    ensure('Ok' in batch && batch.Ok.entries.length === group.length, 'POLICY_STALE')
    for (const [index, trust] of group.entries()) {
      const bundle = bundles[index],
        entry = batch.Ok.entries.find((e) => equal(Uint8Array.from(e.key), ids[index]))
      ensure('Ok' in bundle && entry, 'POLICY_STALE')
      result.push(
        await verifyCloudSecurity({ ...batch.Ok, entries: [entry] }, bundle.Ok, agent, trust)
      )
    }
  }
  return result
}
/** Combines verified evidence sharing a certificate into uploads of at most 64 leaves. */
export function mergeCloudEvidence(evidence: CloudSecurityEvidence[]) {
  const groups = new Map<string, CloudSecurityEvidence>()
  for (const item of evidence) {
    const key = `${item.canister}:${item.certificate}`,
      group = groups.get(key) ?? { ...item, entries: [] }
    group.entries.push(...item.entries)
    groups.set(key, group)
  }
  return [...groups.values()].flatMap((group) =>
    Array.from({ length: Math.ceil(group.entries.length / 64) }, (_, i) => ({
      ...group,
      entries: group.entries.slice(i * 64, (i + 1) * 64)
    }))
  )
}
