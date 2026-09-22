import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { sha256 } from '@noble/hashes/sha2.js'
import { z } from 'zod'
import { digest, pack, principal, requireLegacy } from './base'
import { verifySnapshotSeries, type SnapshotProof } from './snapshot'

const blob = IDL.Vec(IDL.Nat8)
const authorityIDL = IDL.Record({
  id: IDL.Nat32,
  managers: IDL.Vec(IDL.Principal),
  members: IDL.Vec(IDL.Principal),
  message_start: IDL.Nat32,
  latest_message_id: IDL.Nat32,
  dek_digest: blob,
  storage: IDL.Opt(IDL.Tuple(IDL.Principal, IDL.Nat32))
})
const hex = (value: Uint8Array) =>
  Array.from(value, (b) => b.toString(16).padStart(2, '0')).join('')
const bytes = (value: string) => {
  requireLegacy(/^[0-9a-f]{64}$/.test(value), 'corrupt', 'Invalid shared migration digest')
  return Uint8Array.from(value.match(/../g)!, (b) => parseInt(b, 16))
}
const scopedDigest = (domain: string, value: unknown) => sha256(pack([1, domain, value]))
const hash = z.string().regex(/^[0-9a-f]{64}$/)
const account = z.string().regex(/^[0-9a-v]{19}[0g]$/)
const uint = z.number().int().nonnegative().safe()
export const sharedProposalSchema = z.strictObject({
  format: z.literal('dmsg-legacy-shared-proposal/1'),
  source: z.string().max(64),
  channel: uint.max(0xffffffff),
  source_digest: hash,
  version: uint.min(1),
  owner: account,
  channel_id: hash,
  genesis_digest: hash,
  nonce: hash
})
export type SharedProposal = z.infer<typeof sharedProposalSchema>
export const proposalDigest = (proposal: SharedProposal) =>
  hex(scopedDigest('dmsg/legacy-shared-proposal/v1', sharedProposalSchema.parse(proposal)))
export const managerConsentDigest = (proposal: SharedProposal, manager: string) =>
  scopedDigest('dmsg/legacy-manager-consent/v1', [
    proposalDigest(proposal),
    principal(manager)
  ])
export const memberClaimSchema = z.strictObject({
  format: z.literal('dmsg-legacy-member-claim/1'),
  proposal_digest: hash,
  member: z.string().max(64),
  account,
  device: hash,
  hpke_pub: hash,
  security_epoch: uint
})
export type MemberClaim = z.infer<typeof memberClaimSchema>
export const memberClaimDigest = (claim: MemberClaim) =>
  scopedDigest('dmsg/legacy-member-claim/v1', memberClaimSchema.parse(claim))
export const sharedSourceKey = (source: string, channel: number) =>
  hex(scopedDigest('dmsg/legacy-channel-key/v1', [principal(source), channel]))
export interface SharedSourceInput {
  channel: number
  authority: SnapshotProof
  authorities: SnapshotProof[]
}
export interface SharedSourceTrust {
  rootKey: Uint8Array
  source: string
  identity: string
  cutover: Uint8Array
}
export interface SharedSource {
  source: string
  identity: string
  channel: number
  cutover: string
  epoch: string
  identityEpoch: string
  digest: string
  managers: string[]
  members: string[]
  named: Record<string, string[]>
  messageStart: number
  messageEnd: number
  dekDigest: string
}
/** Verifies full frozen ACL and name-role evidence; no message bytes or secrets
 * are needed for coordination. Named identities require a frozen administrator. */
export async function verifySharedSource(
  input: SharedSourceInput,
  trust: SharedSourceTrust
): Promise<SharedSource> {
  const control = await verifySnapshotSeries([input.authority], {
    rootKey: trust.rootKey,
    canister: trust.source,
    cutover: trust.cutover,
    scope: { ChannelAuthority: input.channel }
  })
  const names = await verifySnapshotSeries(input.authorities, {
    rootKey: trust.rootKey,
    canister: trust.identity,
    cutover: trust.cutover,
    scope: { Authorities: null }
  })
  requireLegacy(
    control.rows.length === 1 &&
      control.rows[0]![0] === String(input.channel).padStart(10, '0'),
    'corrupt',
    'Wrong frozen channel scope'
  )
  const authority = IDL.decode([authorityIDL], control.rows[0]![1])[0] as any
  const managers = authority.managers.map((p: Principal) => principal(p)),
    members = authority.members.map((p: Principal) => principal(p))
  requireLegacy(
    authority.id === input.channel &&
      managers.length > 0 &&
      managers.length <= 100 &&
      members.length <= 100 &&
      new Set(managers).size === managers.length &&
      new Set(members).size === members.length &&
      authority.dek_digest.length === 32,
    'corrupt',
    'Invalid frozen membership'
  )
  const named: Record<string, string[]> = {}
  for (const [key, value] of names.rows) {
    const [name, identity, roles] = IDL.decode(
      [IDL.Text, IDL.Principal, IDL.Vec(IDL.Tuple(IDL.Principal, IDL.Int8))],
      value
    ) as unknown as [string, Principal, [Principal, number][]]
    requireLegacy(
      name === key && !named[identity.toText()],
      'corrupt',
      'Duplicate name identity'
    )
    named[identity.toText()] = roles.filter(([, role]) => role === 1).map(([p]) => p.toText())
  }
  const namesDigest = digest(pack(names.rows))
  const sourceDigest = hex(
    scopedDigest('dmsg/legacy-channel-source/v1', [
      trust.source,
      input.channel,
      control.header.freeze.epoch,
      trust.cutover,
      control.rows[0]![1],
      trust.identity,
      names.header.freeze.epoch,
      namesDigest
    ])
  )
  return {
    source: trust.source,
    identity: trust.identity,
    channel: input.channel,
    cutover: hex(trust.cutover),
    epoch: String(control.header.freeze.epoch),
    identityEpoch: String(names.header.freeze.epoch),
    digest: sourceDigest,
    managers,
    members: [...new Set<string>([...members, ...managers])],
    named,
    messageStart: authority.message_start,
    messageEnd: authority.latest_message_id,
    dekDigest: hex(authority.dek_digest)
  }
}
export function controlsLegacyIdentity(
  source: SharedSource,
  identity: string,
  caller: string
) {
  const admins = source.named[identity]
  return admins ? caller !== identity && admins.includes(caller) : caller === identity
}
export async function verifyLegacyAttestation(
  proof: SnapshotProof,
  expectedDigest: Uint8Array,
  expiresAt: number,
  source: SharedSource,
  rootKey: Uint8Array,
  now = Date.now()
) {
  requireLegacy(
    Number.isSafeInteger(expiresAt) && now < expiresAt && expectedDigest.length === 32,
    'version',
    'Legacy approval expired'
  )
  const verified = await verifySnapshotSeries([proof], {
    rootKey,
    canister: source.source,
    cutover: bytes(source.cutover),
    scope: { Attestation: { digest: expectedDigest, expires_at: BigInt(expiresAt) } }
  })
  requireLegacy(
    verified.header.freeze.epoch === BigInt(source.epoch) &&
      verified.rows.length === 1 &&
      verified.rows[0]![0] === proof.caller,
    'permission',
    'Approval source changed'
  )
  const [caller, value, expires] = IDL.decode(
    [IDL.Principal, blob, IDL.Nat64],
    verified.rows[0]![1]
  ) as unknown as [Principal, Uint8Array, bigint]
  requireLegacy(
    caller.toText() === proof.caller &&
      hex(value) === hex(expectedDigest) &&
      expires === BigInt(expiresAt),
    'permission',
    'Approval intent changed'
  )
  return proof.caller
}
