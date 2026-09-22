import {
  Certificate,
  lookupResultToBuffer,
  requestIdOf,
  type HttpAgent
} from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { sha256 } from '@noble/hashes/sha2.js'
import { binary, digest, pack, principal, requireLegacy } from './base'

const blob = IDL.Vec(IDL.Nat8)
export const snapshotScopeIDL = IDL.Variant({
  Names: IDL.Null,
  Authorities: IDL.Null,
  Profile: IDL.Null,
  Channels: IDL.Null,
  Channel: IDL.Nat32,
  ChannelAuthority: IDL.Nat32,
  Attestation: IDL.Record({ digest: blob, expires_at: IDL.Nat64 }),
  Messages: IDL.Nat32
})
const statusIDL = IDL.Record({
  mode: IDL.Variant({ Active: IDL.Null, Draining: IDL.Null, ReadOnly: IDL.Null }),
  epoch: IDL.Nat64,
  cutover_id: IDL.Opt(blob),
  draining_at: IDL.Opt(IDL.Nat64),
  readonly_at: IDL.Opt(IDL.Nat64),
  pending: IDL.Nat64,
  unresolved_after_upgrade: IDL.Nat64,
  baseline_needed: IDL.Bool,
  baseline_evidence: IDL.Opt(blob)
})
export const snapshotPageIDL = IDL.Record({
  schema: IDL.Nat8,
  source: IDL.Principal,
  subject: IDL.Principal,
  freeze: statusIDL,
  scope: snapshotScopeIDL,
  source_context: blob,
  count: IDL.Nat64,
  initial_digest: blob,
  inventory_digest: blob,
  entries: IDL.Vec(IDL.Tuple(IDL.Text, blob)),
  next: IDL.Opt(IDL.Text),
  complete: IDL.Bool
})
const resultIDL = IDL.Variant({ Ok: snapshotPageIDL, Err: IDL.Text })
export type SnapshotScope =
  | { Names: null }
  | { Authorities: null }
  | { Profile: null }
  | { Channels: null }
  | { Channel: number }
  | { ChannelAuthority: number }
  | { Messages: number }
  | { Attestation: { digest: Uint8Array; expires_at: bigint } }
export interface SnapshotProof {
  format: 'dmsg-legacy-snapshot-proof/1'
  canister: string
  caller: string
  args: Uint8Array
  expiry: string
  nonce: Uint8Array
  requestId: Uint8Array
  certificate: Uint8Array
  reply: Uint8Array
}
interface Page {
  schema: number
  source: Principal
  subject: Principal
  scope: SnapshotScope
  source_context: Uint8Array
  freeze: {
    mode: { ReadOnly?: null }
    epoch: bigint
    cutover_id: Uint8Array[]
    draining_at: bigint[]
    readonly_at: bigint[]
    pending: bigint
    unresolved_after_upgrade: bigint
    baseline_needed: boolean
  }
  count: bigint
  initial_digest: Uint8Array
  inventory_digest: Uint8Array
  entries: [string, Uint8Array][]
  next: string[]
  complete: boolean
}
const same = (a: Uint8Array, b: Uint8Array) =>
  a.length === b.length && a.every((v, i) => v === b[i])
const utf8 = (s: string) => new TextEncoder().encode(s)
function timestamp(data: Uint8Array) {
  requireLegacy(data.length > 0 && data.length <= 10, 'corrupt', 'Invalid certificate time')
  let value = 0n
  for (let i = 0; i < data.length; i++) {
    requireLegacy(
      i === data.length - 1 ? !(data[i]! & 128) : Boolean(data[i]! & 128),
      'corrupt',
      'Invalid certificate time'
    )
    value |= BigInt(data[i]! & 127) << BigInt(i * 7)
  }
  return value / 1000000n
}
export async function verifySnapshotProof(
  proof: SnapshotProof,
  trust: {
    rootKey: Uint8Array
    canister: string
    caller: string
    scope: SnapshotScope
    after: string | null
    now?: number
  }
) {
  requireLegacy(
    proof.format === 'dmsg-legacy-snapshot-proof/1' &&
      proof.canister === principal(trust.canister) &&
      proof.caller === principal(trust.caller) &&
      /^[0-9]{1,20}$/.test(proof.expiry) &&
      proof.nonce.length <= 32 &&
      proof.certificate.length <= 65536 &&
      proof.reply.length <= 196608 &&
      proof.args.length <= 2048,
    'corrupt',
    'Invalid source snapshot proof bounds'
  )
  const canister = Principal.fromText(proof.canister),
    caller = Principal.fromText(proof.caller)
  const expectedArgs = new Uint8Array(
    IDL.encode(
      [snapshotScopeIDL, IDL.Opt(IDL.Text)],
      [trust.scope, trust.after === null ? [] : [trust.after]]
    )
  )
  requireLegacy(
    same(expectedArgs, proof.args),
    'permission',
    'Snapshot scope or cursor substitution'
  )
  const request = {
    request_type: 'call',
    canister_id: canister,
    method_name: 'legacy_snapshot',
    arg: proof.args,
    sender: caller,
    ingress_expiry: BigInt(proof.expiry),
    ...(proof.nonce.length ? { nonce: proof.nonce } : {})
  }
  const requestId = requestIdOf(request)
  requireLegacy(same(requestId, proof.requestId), 'corrupt', 'Snapshot request ID mismatch')
  const certificate = await Certificate.create({
    certificate: binary(proof.certificate),
    rootKey: binary(trust.rootKey),
    principal: { canisterId: canister },
    disableTimeVerification: true
  })
  const status = lookupResultToBuffer(
    certificate.lookup_path(['request_status', requestId, 'status'])
  )
  const reply = lookupResultToBuffer(
    certificate.lookup_path(['request_status', requestId, 'reply'])
  )
  const time = lookupResultToBuffer(certificate.lookup_path(['time']))
  requireLegacy(
    status && same(status, utf8('replied')) && reply && same(reply, proof.reply) && time,
    'corrupt',
    'Uncertified snapshot reply'
  )
  const result = IDL.decode([resultIDL], binary(reply))[0] as { Ok?: Page; Err?: string }
  requireLegacy(
    result.Ok,
    'version',
    result.Err ? 'Legacy snapshot is not available or not frozen' : 'Invalid snapshot response'
  )
  const page = result.Ok,
    at = timestamp(time)
  requireLegacy(
    page.schema === 1 &&
      page.source.toText() === proof.canister &&
      page.subject.toText() === proof.caller &&
      'ReadOnly' in page.freeze.mode &&
      page.freeze.pending === 0n &&
      !page.freeze.baseline_needed &&
      page.freeze.readonly_at.length === 1 &&
      page.freeze.readonly_at[0]! <= at &&
      at <= BigInt(trust.now ?? Date.now()) + 5000n &&
      page.count <= 100000n &&
      page.entries.length <= 64 &&
      page.initial_digest.length === 32 &&
      page.inventory_digest.length === 32 &&
      same(pack(page.scope), pack(trust.scope)),
    'corrupt',
    'Snapshot is not an attested frozen scope'
  )
  requireLegacy(
    page.entries.every(
      ([key], i) =>
        key.length <= 128 &&
        (i ? key > page.entries[i - 1]![0] : trust.after === null || key > trust.after)
    ),
    'corrupt',
    'Snapshot page is out of order'
  )
  requireLegacy(
    page.next[0] === (page.entries.at(-1)?.[0] ?? trust.after ?? undefined) &&
      (page.complete || page.entries.length > 0),
    'corrupt',
    'Snapshot cursor did not advance'
  )
  return { page, certifiedAt: at }
}
export function foldSnapshotRow(previous: Uint8Array, key: string, value: Uint8Array) {
  const name = utf8(key),
    length = (n: number) => {
      const bytes = new Uint8Array(8)
      new DataView(bytes.buffer).setBigUint64(0, BigInt(n))
      return bytes
    }
  return sha256
    .create()
    .update(previous)
    .update(length(name.length))
    .update(name)
    .update(length(value.length))
    .update(value)
    .digest()
}
export async function collectSnapshot(
  agent: HttpAgent,
  source: string,
  scope: SnapshotScope,
  allowed: string[]
) {
  requireLegacy(
    allowed.includes(principal(source)) && agent.rootKey,
    'permission',
    'Snapshot source is not allowlisted'
  )
  const caller = (await agent.getPrincipal()).toText(),
    proofs: SnapshotProof[] = [],
    entries: [string, Uint8Array][] = []
  let after: string | null = null,
    accumulated: Uint8Array | null = null,
    descriptor: string | null = null
  for (;;) {
    const args = new Uint8Array(
      IDL.encode([snapshotScopeIDL, IDL.Opt(IDL.Text)], [scope, after === null ? [] : [after]])
    )
    const result = await agent.update(source, { methodName: 'legacy_snapshot', arg: args })
    requireLegacy(result.requestDetails, 'version', 'Agent did not retain request details')
    const request = result.requestDetails
    const proof: SnapshotProof = {
      format: 'dmsg-legacy-snapshot-proof/1',
      canister: source,
      caller,
      args,
      expiry: request.ingress_expiry.toBigInt().toString(),
      nonce: request.nonce ? binary(request.nonce) : new Uint8Array(),
      requestId: requestIdOf(request),
      certificate: binary(result.rawCertificate),
      reply: binary(result.reply)
    }
    const { page } = await verifySnapshotProof(proof, {
      rootKey: agent.rootKey,
      canister: source,
      caller,
      scope,
      after
    })
    const context = digest(
      pack([
        page.source_context,
        page.initial_digest,
        page.inventory_digest,
        page.count.toString(),
        page.freeze.epoch.toString(),
        page.freeze.cutover_id
      ])
    )
    requireLegacy(
      !descriptor || descriptor === context,
      'corrupt',
      'Frozen snapshot changed between pages'
    )
    descriptor = context
    accumulated ??= page.initial_digest
    for (const [key, value] of page.entries)
      accumulated = foldSnapshotRow(accumulated!, key, value)
    proofs.push(proof)
    entries.push(...page.entries)
    requireLegacy(
      proofs.length <= 10000 && entries.length <= 100000,
      'limit',
      'Snapshot page limit'
    )
    if (page.complete) {
      requireLegacy(
        BigInt(entries.length) === page.count && same(accumulated!, page.inventory_digest),
        'corrupt',
        'Snapshot count or rolling digest is incomplete'
      )
      return { format: 'dmsg-legacy-snapshot/1' as const, proofs, entries, digest: descriptor }
    }
    after = page.next[0]!
  }
}

export async function verifySnapshotSeries(
  proofs: SnapshotProof[],
  trust: { rootKey: Uint8Array; canister: string; scope: SnapshotScope; cutover: Uint8Array }
) {
  requireLegacy(
    proofs.length > 0 && proofs.length <= 10000 && trust.cutover.length === 32,
    'limit',
    'Invalid frozen proof series'
  )
  let after: string | null = null,
    rolling: Uint8Array | null = null,
    descriptor: string | null = null
  let header: Awaited<ReturnType<typeof verifySnapshotProof>>['page'] | null = null
  const rows: [string, Uint8Array][] = []
  for (const [index, proof] of proofs.entries()) {
    const { page } = await verifySnapshotProof(proof, {
      ...trust,
      caller: proofs[0]!.caller,
      after
    })
    requireLegacy(
      page.freeze.cutover_id.length === 1 && same(page.freeze.cutover_id[0]!, trust.cutover),
      'version',
      'Frozen cutover mismatch'
    )
    const fixed = digest(
      pack([
        page.source_context,
        page.initial_digest,
        page.inventory_digest,
        page.count,
        page.freeze.epoch
      ])
    )
    requireLegacy(
      !descriptor || descriptor === fixed,
      'corrupt',
      'Frozen source changed between pages'
    )
    descriptor = fixed
    header ??= page
    rolling ??= page.initial_digest
    for (const [key, value] of page.entries) rolling = foldSnapshotRow(rolling!, key, value)
    rows.push(...page.entries)
    requireLegacy(
      page.complete === (index === proofs.length - 1),
      'missing',
      'Frozen source page missing or repeated'
    )
    after = page.next[0] ?? null
  }
  requireLegacy(
    header && BigInt(rows.length) === header.count && same(rolling!, header.inventory_digest),
    'missing',
    'Frozen source inventory is incomplete'
  )
  return { header, rows }
}
