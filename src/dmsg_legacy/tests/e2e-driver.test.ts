import { readFileSync, writeFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { HttpAgent } from '@icp-sdk/core/agent'
import { Ed25519KeyIdentity } from '@icp-sdk/core/identity'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import {
  collectSnapshot,
  digest,
  encodeArchive,
  managerConsentDigest,
  memberClaimDigest,
  memberClaimSchema,
  observation,
  pack,
  readOfferLink,
  sealTransfer,
  pairingFingerprint,
  sharedProposalSchema,
  type LegacyArchive,
  type SnapshotProof
} from '../src'

// The old site's side of the desktop end-to-end test
// (tests/dmsg_integration/tests/desktop.rs, `desktop_legacy`): one action per
// run, as the `/legacy` page performs it with an old identity against the
// frozen canisters. DMSG_LEGACY_DRIVER names the input file.
const input = process.env.DMSG_LEGACY_DRIVER
  ? JSON.parse(readFileSync(process.env.DMSG_LEGACY_DRIVER, 'utf8'))
  : null
const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString('hex')
const unhex = (text: string) => new Uint8Array(Buffer.from(text, 'hex'))
const b64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64url')
const wire = (p: SnapshotProof) => ({
  ...p,
  args: b64(p.args),
  nonce: b64(p.nonce),
  requestId: b64(p.requestId),
  certificate: b64(p.certificate),
  reply: b64(p.reply)
})
const messageType = IDL.Record({
  id: IDL.Nat32,
  kind: IDL.Nat8,
  reply_to: IDL.Nat32,
  created_by: IDL.Principal,
  created_at: IDL.Nat64,
  payload: IDL.Vec(IDL.Nat8)
})

async function agent(seed: number) {
  const agent = await HttpAgent.create({
    host: input.gateway,
    identity: Ed25519KeyIdentity.generate(new Uint8Array(32).fill(seed)),
    shouldFetchRootKey: false
  })
  await agent.fetchRootKey()
  expect(hex(new Uint8Array(agent.rootKey!))).toBe(input.root_key_hex)
  return agent
}

async function run() {
  const caller = await agent(input.seed)
  const channels = [input.channel]
  switch (input.action) {
    case 'source': {
      const control = await collectSnapshot(
        caller,
        input.channel,
        { ChannelAuthority: input.channel_id },
        channels
      )
      const names = await collectSnapshot(caller, input.identity, { Authorities: null }, [
        input.identity
      ])
      return {
        format: 'dmsg-legacy-shared-source/1',
        source: input.channel,
        identity: input.identity,
        cutover: input.cutover_hex,
        input: {
          channel: input.channel_id,
          authority: wire(control.proofs[0]!),
          authorities: names.proofs.map(wire)
        }
      }
    }
    case 'approve': {
      // As the page does: recompute the intent before the old identity signs.
      const request = input.request
      const proposal = sharedProposalSchema.parse(request.proposal)
      const bytes =
        request.kind === 'manager'
          ? managerConsentDigest(proposal, request.identity)
          : memberClaimDigest(memberClaimSchema.parse(request.claim))
      expect(hex(bytes)).toBe(request.digest)
      const result = await collectSnapshot(
        caller,
        proposal.source,
        { Attestation: { digest: bytes, expires_at: BigInt(request.expires_at) } },
        channels
      )
      return { proof: wire(result.proofs[0]!), expires_at: request.expires_at }
    }
    case 'archive': {
      // The owner's archive of the frozen channel, observed as the
      // collector observes it, sealed to the desktop's offer.
      const rows = await collectSnapshot(
        caller,
        input.channel,
        { Messages: input.channel_id },
        channels
      )
      const key = `${input.channel}/channel/${input.channel_id}`
      const objects: LegacyArchive['inventory']['objects'] = []
      const add = (k: string, kind: any, value: unknown) => {
        const bytes = pack(observation(value))
        objects.push({
          key: k,
          kind,
          bytes,
          digest: digest(bytes),
          trust: 'query_observation',
          observedAt: 1
        })
      }
      const owner = (await caller.getPrincipal()).toText()
      add(`${input.channel}/identity/${owner}`, 'identity', {
        id: Principal.fromText(owner),
        name: 'Old Owner',
        image: ''
      })
      add(key, 'channel', {
        id: input.channel_id,
        canister: Principal.fromText(input.channel),
        name: 'Frozen channel',
        dek: unhex(input.wrapped_hex),
        paid: 0n,
        gas: 0n
      })
      for (const [, value] of rows.entries) {
        const [id, found] = IDL.decode([IDL.Nat32, IDL.Opt(messageType)], value) as any
        if (found.length) add(`${key}/message/${id}`, 'message', found[0])
      }
      const parameters = pack(
        observation({ canister: Principal.fromText(input.channel), id: input.channel_id })
      )
      const archive: LegacyArchive = {
        format: 'dmsg-legacy-archive/1',
        inventory: {
          format: 'dmsg-legacy-inventory/1',
          principal: owner,
          messageCanister: input.channel,
          mode: 'Local',
          snapshot: 'pre_migration',
          objects,
          gaps: [],
          calls: []
        },
        keys: [
          {
            source: key,
            principal: owner,
            purpose: 'channel_kek',
            coseKey: unhex(input.kek_hex),
            parameters
          },
          {
            source: key,
            principal: owner,
            purpose: 'channel_dek',
            coseKey: unhex(input.dek_hex),
            parameters
          }
        ],
        checks: [],
        createdAt: Date.now()
      }
      const link = new URL(input.link)
      const offer = readOfferLink(link.hash, link.origin)
      const sealed = await sealTransfer(encodeArchive(archive), offer, {
        origin: offer.origin,
        target: offer.target,
        fingerprint: pairingFingerprint(offer)
      })
      writeFileSync(input.file, sealed)
      return {
        principal: owner,
        messages: objects.filter((o) => o.kind === 'message').length
      }
    }
    case 'frozen': {
      const authority = await collectSnapshot(
        caller,
        input.channel,
        { ChannelAuthority: input.channel_id },
        channels
      )
      const messages = await collectSnapshot(
        caller,
        input.channel,
        { Messages: input.channel_id },
        channels
      )
      writeFileSync(
        input.file,
        pack([
          {
            channel: `${input.channel}/channel/${input.channel_id}`,
            authority: authority.proofs[0]!,
            messages: messages.proofs,
            folder: []
          }
        ])
      )
      return { pages: messages.proofs.length }
    }
  }
  throw new Error(`unknown action ${input.action}`)
}

describe.skipIf(!input)('legacy site driver', () => {
  it('performs one old-site action', async () => {
    writeFileSync(input.out, JSON.stringify(await run()))
  }, 120000)
})
