import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { expect, it } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import { Header } from '@ldclabs/cose-ts/header'
import { Encrypt0Message } from '@ldclabs/cose-ts/encrypt0'
import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { sha256 } from '@noble/hashes/sha2.js'
import {
  archiveContentId,
  createPairing,
  decodeArchive,
  digest,
  encodeArchive,
  managerConsentDigest,
  memberClaimDigest,
  observation,
  offerLink,
  openArchiveFile,
  openTransfer,
  pack,
  pairingFingerprint,
  proposalDigest,
  readObservation,
  sealTransfer,
  sharedHistoryArchive,
  sharedSourceKey,
  TRANSFER_CHUNK,
  unpack,
  verifyArchiveContent,
  type LegacyArchive,
  type SharedSource
} from '../src'

// tests/vectors/client/legacy-v1.json: the Rust client opens the same
// transfer and archive. DMSG_WRITE_CLIENT_VECTORS=1 writes a new file; HPKE
// is randomized, so the file is checked by opening it, not by regenerating.
const path = resolve(import.meta.dirname, '../../../tests/vectors/client/legacy-v1.json')
const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString('hex')
const unhex = (text: string) => new Uint8Array(Buffer.from(text, 'hex'))
const b64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64url')
const unb64 = (text: string) => new Uint8Array(Buffer.from(text, 'base64url'))

const owner = Principal.fromUint8Array(new Uint8Array([1, 2, 3])).toText()
const message = 'nscli-qiaaa-aaaaj-qa4pa-cai'
const profile = 'ijyxz-wyaaa-aaaaj-qa4qa-cai'
const channels = 'nvdn4-5qaaa-aaaaj-qa4pq-cai'
const bucket = '532er-faaaa-aaaaj-qncpa-cai'
const ledger = 'druyg-tyaaa-aaaaq-aactq-cai'
const device = 'd1'.repeat(32)

async function seal(key: AesGcmKey, data: Uint8Array, iv: number) {
  return new Encrypt0Message(
    data,
    new Header().setParam(1, 3),
    new Header().setParam(5, new Uint8Array(12).fill(iv))
  ).toBytes(key)
}

function object(
  key: string,
  kind: LegacyArchive['inventory']['objects'][number]['kind'],
  value: unknown,
  trust: 'query_observation' | 'local_cache' = 'query_observation'
) {
  const bytes = pack(observation(value))
  return { key, kind, bytes, digest: digest(bytes), trust, observedAt: 1759999999000 }
}

/** One Local-mode archive: two channels (one without keys), every message
 * kind, a complete and an incomplete attachment, an avatar and an
 * entitlement, plus one exporter claim that offline checking contradicts. */
async function archive(): Promise<LegacyArchive> {
  const kek = AesGcmKey.fromSecret(new Uint8Array(32).fill(8))
  const dek = AesGcmKey.fromSecret(new Uint8Array(32).fill(9))
  const wrapped = await seal(kek, dek.toBytes(), 1)
  const first = `${channels}/channel/1`,
    second = `${channels}/channel/2`
  const notes = new TextEncoder().encode('Notes kept since 2024.\n')
  const avatar = Uint8Array.from([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13])
  const reference = pack({
    canister: Principal.fromText(bucket).toUint8Array(),
    id: 7,
    name: 'notes.txt',
    size: notes.length,
    type: 'text/plain'
  })
  const sent = (id: number, kind: number, payload: Uint8Array, channel = first) =>
    object(`${channel}/message/${id}`, 'message', {
      id,
      reply_to: 0,
      kind,
      created_at: 1700000000000n + BigInt(id),
      created_by: Principal.fromText(owner),
      payload
    })
  const tampered = await seal(dek, pack('changed'), 5)
  tampered[tampered.length - 1]! ^= 1
  const objects = [
    object(`${message}/identity/${owner}`, 'identity', {
      id: Principal.fromText(owner),
      name: 'Old Panda',
      image: `https://${bucket}.icp0.io/f/9`,
      username: ['old_panda']
    }),
    object(`${profile}/profile/${owner}`, 'profile', { bio: 'Building on ICP.' }),
    object(first, 'channel', {
      id: 1,
      canister: Principal.fromText(channels),
      name: 'Old friends',
      dek: wrapped,
      paid: 1000000000n,
      gas: 5n
    }),
    object(second, 'channel', {
      id: 2,
      canister: Principal.fromText(channels),
      name: 'No keys',
      dek: await seal(kek, dek.toBytes(), 2),
      paid: 0n,
      gas: 0n
    }),
    sent(1, 1, pack('Channel created')),
    sent(2, 0, await seal(dek, pack('Hello from the old app'), 3)),
    sent(3, 0, await seal(dek, pack(['See the file', 'File', reference]), 4)),
    sent(4, 0, new Uint8Array()),
    sent(5, 0, tampered),
    sent(1, 0, await seal(dek, pack('Unreadable here'), 6), second),
    object(`${bucket}/file/7`, 'file', {
      info: { id: 7, name: 'notes.txt', size: BigInt(notes.length), parent: 3 },
      ciphertext: await seal(dek, notes, 7),
      complete: true,
      channel: first
    }),
    object(`${bucket}/file/8`, 'file', {
      info: { id: 8, name: 'draft.bin', size: 1024n, parent: 3 },
      ciphertext: new Uint8Array(16).fill(1),
      complete: false,
      channel: first
    }),
    object(`${bucket}/avatar/9`, 'avatar', {
      info: { id: 9, name: 'avatar.png', size: BigInt(avatar.length) },
      bytes: avatar,
      digest: digest(avatar),
      originalUrl: `https://${bucket}.icp0.io/f/9`
    }),
    object(`${ledger}/balance/${owner}`, 'entitlement', {
      symbol: 'PANDA',
      balance: 123n
    }),
    object('local/cache/settings', 'local', { theme: 'dark' }, 'local_cache')
  ]
  const parameters = pack(observation({ canister: Principal.fromText(channels), id: 1 }))
  return {
    format: 'dmsg-legacy-archive/1',
    inventory: {
      format: 'dmsg-legacy-inventory/1',
      principal: owner,
      messageCanister: message,
      mode: 'Local',
      snapshot: 'pre_migration',
      objects,
      gaps: [
        {
          source: `${owner}/static-ecdh`,
          code: 'missing',
          detail: 'No static ECDH key was cached'
        }
      ],
      calls: [{ canister: message, method: 'my_iv', at: 1759999999000, outcome: 'ok' }]
    },
    keys: [
      {
        source: first,
        principal: owner,
        purpose: 'channel_kek',
        coseKey: kek.toBytes(),
        parameters
      },
      {
        source: first,
        principal: owner,
        purpose: 'channel_dek',
        coseKey: dek.toBytes(),
        parameters
      }
    ],
    checks: [
      {
        source: `${first}/message/1`,
        digest: objects[4]!.digest,
        result: 'aead_verified',
        plaintextDigest: null
      },
      {
        source: `${first}/message/2`,
        digest: objects[5]!.digest,
        result: 'aead_verified',
        plaintextDigest: digest(
          pack({ deleted: false, text: 'Hello from the old app', file: null })
        )
      }
    ],
    createdAt: 1760000000000
  }
}

const shared = (archive: LegacyArchive): SharedSource => ({
  source: channels,
  identity: '2rgax-kyaaa-aaaap-anvba-cai',
  channel: 1,
  cutover: '08'.repeat(32),
  epoch: '3',
  identityEpoch: '4',
  digest: 'a1'.repeat(32),
  managers: [owner],
  members: [owner, '2vxsx-fae'],
  named: {},
  messageStart: 1,
  messageEnd: 6,
  dekDigest: digest(
    readObservation(
      archive.inventory.objects.find((o) => o.key === `${channels}/channel/1`)!.bytes
    ).dek
  )
})

async function generate() {
  const value = await archive()
  const plain = encodeArchive(value)
  const session = await createPairing('https://dmsg.net', device, 1760000000000)
  const fingerprint = pairingFingerprint(session.offer)
  const expected = { origin: session.offer.origin, target: device, fingerprint }
  const file = await sealTransfer(plain, session.offer, expected, 1760000000000)
  const otherTarget = { ...session.offer, target: 'd2'.repeat(32) }
  const otherOrigin = { ...session.offer, origin: 'https://panda.fans' as const }
  const otherNonce = { ...session.offer, nonce: 'e1'.repeat(32) }
  const sealed = async (offer: typeof session.offer) =>
    b64(
      await sealTransfer(
        plain,
        offer,
        {
          origin: offer.origin,
          target: offer.target,
          fingerprint: pairingFingerprint(offer)
        },
        1760000000000
      )
    )
  // Two frames, so reordering and duplication are observable.
  const large = new Uint8Array(TRANSFER_CHUNK + 1000).fill(7)
  const framed = await sealTransfer(large, session.offer, expected, 1760000000000)
  return {
    format: 'dmsg-client-vectors/legacy-v1',
    generator: 'src/dmsg_legacy/tests/vectors.test.ts',
    transfer: {
      seed_hex: hex(session.seed),
      offer: session.offer,
      offer_cbor_hex: hex(pack(session.offer)),
      fingerprint,
      link: offerLink(session.offer),
      now: 1760000000000,
      plain_sha256: digest(plain),
      file_b64: b64(file),
      rejected: [
        { case: 'other_target', file_b64: await sealed(otherTarget) },
        { case: 'other_origin', file_b64: await sealed(otherOrigin) },
        { case: 'other_nonce', file_b64: await sealed(otherNonce) }
      ],
      framed: {
        plain_sha256: digest(large),
        file_b64: b64(framed),
        // Applied to the decoded transfer, re-encoded, then refused.
        mutations: ['reverse', 'duplicate', 'drop_last', 'flip', 'shrink']
      }
    }
  }
}

async function expectations(vectors: any) {
  const opened = await openTransfer(
    unb64(vectors.transfer.file_b64),
    { offer: vectors.transfer.offer, seed: unhex(vectors.transfer.seed_hex) },
    {
      origin: vectors.transfer.offer.origin,
      target: vectors.transfer.offer.target,
      fingerprint: vectors.transfer.fingerprint
    },
    vectors.transfer.now
  )
  expect(digest(opened.plain)).toBe(vectors.transfer.plain_sha256)
  const value = decodeArchive(opened.plain)
  const checked = await verifyArchiveContent(value)
  const file = await openArchiveFile(value, `${bucket}/file/7`)
  const source = shared(value)
  const scoped = await sharedHistoryArchive(value, source)
  const proposal = {
    format: 'dmsg-legacy-shared-proposal/1' as const,
    source: channels,
    channel: 1,
    source_digest: source.digest,
    version: 1,
    owner: 'c2a41dj7n47abbo00000',
    channel_id: 'c1'.repeat(32),
    genesis_digest: 'e2'.repeat(32),
    nonce: 'f1'.repeat(32)
  }
  const claim = {
    format: 'dmsg-legacy-member-claim/1' as const,
    proposal_digest: proposalDigest(proposal),
    member: '2vxsx-fae',
    account: 'c2a41dj7n47abbo00000',
    device,
    hpke_pub: 'b1'.repeat(32),
    security_epoch: 2
  }
  return {
    archive: {
      content_id: archiveContentId(value),
      partial: checked.partial,
      checks: checked.checks,
      gaps: checked.gaps.map((g) => ({ source: g.source, code: g.code })),
      messages: checked.messages,
      file: { source: `${bucket}/file/7`, name: file.name, sha256: digest(file.bytes) }
    },
    shared: {
      source,
      scoped: {
        content_id: archiveContentId(scoped),
        objects: scoped.inventory.objects.map((o) => o.key),
        gaps: scoped.inventory.gaps.map((g) => ({ source: g.source, code: g.code })),
        keys: scoped.keys.map((k) => ({
          purpose: k.purpose,
          parameters_hex: hex(k.parameters)
        }))
      },
      source_key: sharedSourceKey(channels, 1),
      proposal,
      proposal_digest: proposalDigest(proposal),
      manager_consent_hex: hex(managerConsentDigest(proposal, owner)),
      claim,
      claim_digest_hex: hex(memberClaimDigest(claim)),
      source_digest: {
        input: {
          source: channels,
          channel: 1,
          epoch: '3',
          cutover_hex: '08'.repeat(32),
          authority_hex: 'aabb',
          identity: '2rgax-kyaaa-aaaap-anvba-cai',
          identity_epoch: '4',
          names: [['old_panda', 'ccdd']]
        },
        // sha256(CBOR([1, domain, [source, channel, epoch, cutover, authority,
        // identity, identityEpoch, sha256hex(CBOR(names))]])), as verifySharedSource.
        digest: hex(
          sha256(
            pack([
              1,
              'dmsg/legacy-channel-source/v1',
              [
                channels,
                1,
                3n,
                unhex('08'.repeat(32)),
                unhex('aabb'),
                '2rgax-kyaaa-aaaap-anvba-cai',
                4n,
                digest(pack([['old_panda', unhex('ccdd')]]))
              ]
            ])
          )
        )
      }
    }
  }
}

it('opens the legacy vectors the Rust client checks', async () => {
  if (process.env.DMSG_WRITE_CLIENT_VECTORS || !existsSync(path)) {
    const vectors: any = await generate()
    Object.assign(vectors, await expectations(vectors))
    writeFileSync(path, JSON.stringify(vectors, null, 2) + '\n')
  }
  const vectors = JSON.parse(readFileSync(path, 'utf8'))
  expect(await expectations(vectors)).toEqual({
    archive: vectors.archive,
    shared: vectors.shared
  })
  const session = {
    offer: vectors.transfer.offer,
    seed: unhex(vectors.transfer.seed_hex)
  }
  const expected = {
    origin: session.offer.origin,
    target: session.offer.target,
    fingerprint: vectors.transfer.fingerprint
  }
  expect(hex(pack(session.offer))).toBe(vectors.transfer.offer_cbor_hex)
  expect(offerLink(session.offer)).toBe(vectors.transfer.link)
  for (const rejected of vectors.transfer.rejected)
    await expect(
      openTransfer(unb64(rejected.file_b64), session, expected, vectors.transfer.now),
      rejected.case
    ).rejects.toThrow()
  const framed = unb64(vectors.transfer.framed.file_b64)
  const opened = await openTransfer(framed, session, expected, vectors.transfer.now)
  expect(digest(opened.plain)).toBe(vectors.transfer.framed.plain_sha256)
  const mutations: Record<string, (v: any) => void> = {
    reverse: (v) => v.frames.reverse(),
    duplicate: (v) => (v.frames[1] = v.frames[0]),
    drop_last: (v) => v.frames.pop(),
    flip: (v) => (v.frames[1].ciphertext[0] ^= 1),
    shrink: (v) => v.size--
  }
  for (const name of vectors.transfer.framed.mutations) {
    const changed: any = unpack(framed)
    mutations[name]!(changed)
    await expect(
      openTransfer(pack(changed), session, expected, vectors.transfer.now),
      name
    ).rejects.toThrow()
  }
}, 30000)
