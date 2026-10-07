import { prepareLegacyNames } from '../scripts/legacy-names'
import { HttpAgent } from '@icp-sdk/core/agent'
import { Ed25519KeyIdentity } from '@icp-sdk/core/identity'
import { collectSnapshot, verifySnapshotProof } from '@dmsg/legacy'
import { IDL } from '@icp-sdk/core/candid'
import {
  compareFrozenArchive,
  verifySharedSource,
  managerConsentDigest,
  verifyLegacyAttestation,
  controlsLegacyIdentity,
  pack,
  observation,
  digest,
  type LegacyArchive
} from '@dmsg/legacy'

;(window as any).legacySnapshotProbe = async (input: {
  gateway: string
  channel: string
  message: string
  identity: string
  profile: string
  channel_id: number
  clean_channel_id: number
}) => {
  const url = new URL(input.gateway)
  if (url.protocol !== 'http:' || !['localhost', '127.0.0.1'].includes(url.hostname))
    throw new Error('Local fixture required')
  const agent = await HttpAgent.create({
    host: input.gateway,
    identity: Ed25519KeyIdentity.generate(new Uint8Array(32).fill(52)),
    shouldFetchRootKey: false,
    verifyQuerySignatures: true
  })
  await agent.fetchRootKey()
  const scope = { Messages: input.channel_id }
  const messages = await collectSnapshot(agent, input.channel, scope, [input.channel])
  const names = await collectSnapshot(agent, input.message, { Names: null }, [input.message])
  const authorities = await collectSnapshot(agent, input.identity, { Authorities: null }, [
    input.identity
  ])
  const control = await collectSnapshot(
    agent,
    input.channel,
    { ChannelAuthority: input.channel_id },
    [input.channel]
  )
  const shared = await verifySharedSource(
    {
      channel: input.channel_id,
      authority: control.proofs[0],
      authorities: authorities.proofs
    },
    {
      rootKey: new Uint8Array(agent.rootKey!),
      source: input.channel,
      identity: input.identity,
      cutover: new Uint8Array(32).fill(8)
    }
  )
  const proposal = {
    format: 'dmsg-legacy-shared-proposal/1' as const,
    source: input.channel,
    channel: input.channel_id,
    source_digest: shared.digest,
    version: 1,
    owner: '0000000000000000000g',
    channel_id: '03'.repeat(32),
    genesis_digest: '04'.repeat(32),
    nonce: '05'.repeat(32)
  }
  const caller = (await agent.getPrincipal()).toText(),
    approvalDigest = managerConsentDigest(proposal, caller),
    expiresAt = Date.now() + 3600000
  const consent = await collectSnapshot(
    agent,
    input.channel,
    { Attestation: { digest: approvalDigest, expires_at: BigInt(expiresAt) } },
    [input.channel]
  )
  if (
    (await verifyLegacyAttestation(
      consent.proofs[0],
      approvalDigest,
      expiresAt,
      shared,
      new Uint8Array(agent.rootKey!)
    )) !== caller ||
    !controlsLegacyIdentity(shared, caller, caller)
  )
    throw new Error('Frozen manager consent did not verify')
  let substituted = false
  try {
    await verifyLegacyAttestation(
      consent.proofs[0],
      managerConsentDigest({ ...proposal, version: 2 }, caller),
      expiresAt,
      shared,
      new Uint8Array(agent.rootKey!)
    )
  } catch {
    substituted = true
  }
  if (!substituted) throw new Error('Approval was reused for another proposal')
  const profile = await collectSnapshot(agent, input.profile, { Profile: null }, [
    input.profile
  ])
  const cleanAuthority = await collectSnapshot(
    agent,
    input.channel,
    { ChannelAuthority: input.clean_channel_id },
    [input.channel]
  )
  const cleanChannel = await collectSnapshot(
    agent,
    input.channel,
    { Channel: input.clean_channel_id },
    [input.channel]
  )
  const cleanMessages = await collectSnapshot(
    agent,
    input.channel,
    { Messages: input.clean_channel_id },
    [input.channel]
  )
  const channelType = IDL.Record({
      id: IDL.Nat32,
      canister: IDL.Principal,
      dek: IDL.Vec(IDL.Nat8)
    }),
    messageType = IDL.Record({
      id: IDL.Nat32,
      kind: IDL.Nat8,
      reply_to: IDL.Nat32,
      created_by: IDL.Principal,
      created_at: IDL.Nat64,
      payload: IDL.Vec(IDL.Nat8)
    })
  const key = `${input.channel}/channel/${input.clean_channel_id}`,
    objects: LegacyArchive['inventory']['objects'] = []
  const add = (key: string, kind: 'channel' | 'message', value: unknown) => {
    const bytes = pack(observation(value))
    objects.push({
      key,
      kind,
      bytes,
      digest: digest(bytes),
      observedAt: Date.now(),
      trust: 'query_observation'
    })
  }
  add(key, 'channel', IDL.decode([channelType], cleanChannel.entries[0][1])[0])
  for (const [, bytes] of cleanMessages.entries) {
    const [id, values] = IDL.decode([IDL.Nat32, IDL.Opt(messageType)], bytes) as unknown as [
      number,
      unknown[]
    ]
    if (values.length) add(`${key}/message/${id}`, 'message', values[0])
  }
  const archive: LegacyArchive = {
    format: 'dmsg-legacy-archive/1',
    inventory: {
      format: 'dmsg-legacy-inventory/1',
      principal: caller,
      messageCanister: input.message,
      mode: 'Local',
      snapshot: 'pre_migration',
      objects,
      gaps: [],
      calls: []
    },
    keys: [],
    checks: [],
    createdAt: Date.now()
  }
  const finalProof = [
      {
        channel: key,
        authority: cleanAuthority.proofs[0],
        messages: cleanMessages.proofs,
        folder: []
      }
    ],
    finalTrust = {
      rootKey: new Uint8Array(agent.rootKey!),
      channels: [input.channel],
      buckets: [],
      cutover: new Uint8Array(32).fill(8)
    }
  const compared = await compareFrozenArchive(archive, finalProof, finalTrust)
  if (!compared.matched) throw new Error('Unchanged frozen history did not match')
  archive.inventory.objects = archive.inventory.objects.filter((o) => o.kind !== 'message')
  if ((await compareFrozenArchive(archive, finalProof, finalTrust)).matched)
    throw new Error('Missing frozen message was accepted')
  const prepared = await prepareLegacyNames({
    names: names.proofs,
    authorities: authorities.proofs,
    rootKey: new Uint8Array(agent.rootKey!),
    source: input.message,
    identity: input.identity,
    expectedNames: 1,
    cutover: new Uint8Array(32).fill(8)
  })
  if (
    prepared.snapshot.count !== 1n ||
    prepared.entries[0].handle !== 'snapshot_owner' ||
    prepared.quarantined.length
  )
    throw new Error('Frozen name import does not match old owner')
  const proof = messages.proofs[0]!
  const trust = {
    rootKey: agent.rootKey!,
    canister: input.channel,
    caller: (await agent.getPrincipal()).toText(),
    scope,
    after: null
  }
  let rejected = 0
  for (const changed of [
    { ...proof, canister: input.message },
    { ...proof, caller: '2vxsx-fae' },
    { ...proof, reply: new Uint8Array([1, 2, 3]) },
    { ...proof, args: new Uint8Array(IDL.encode([IDL.Text], ['forged scope'])) }
  ]) {
    try {
      await verifySnapshotProof(changed, trust)
    } catch {
      rejected++
    }
  }
  const hex = (value: Uint8Array) =>
    Array.from(value, (b) => b.toString(16).padStart(2, '0')).join('')
  const encodeProofs = (values: typeof names.proofs) =>
    values.map((value) => ({
      ...value,
      ...Object.fromEntries(
        ['args', 'nonce', 'requestId', 'certificate', 'reply'].map((key) => [
          key,
          hex(value[key as 'args'])
        ])
      )
    }))
  return {
    operator: {
      root: hex(new Uint8Array(agent.rootKey!)),
      names: encodeProofs(names.proofs),
      authorities: encodeProofs(authorities.proofs)
    },
    frozenDeltaVerified: true,
    sharedConsentVerified: true,
    nameImportVerified: true,
    messages: messages.entries.length,
    pages: messages.proofs.length,
    names: names.entries.length,
    authorities: authorities.entries.length,
    profiles: profile.entries.length,
    rejected
  }
}
