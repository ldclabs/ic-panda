import { describe, expect, it } from 'vitest'
import {
  createPairing,
  pairingFingerprint,
  sealTransfer,
  openTransfer,
  offerLink,
  readOfferLink,
  TRANSFER_CHUNK,
  pack,
  unpack,
  decodeArchive,
  encodeArchive,
  validateArchive,
  verifyArchiveContent,
  digest,
  type LegacyArchive
} from '../src'

const target = 'a'.repeat(64),
  origin = 'https://dmsg.net' as const
describe('bounded legacy migration transfer', () => {
  it('allows a slow export and import within one hour, but rejects expired or overlong pairings', async () => {
    const now = 1760000000000
    const session = await createPairing(origin, target, now)
    expect(session.offer.expiresAt).toBe(now + 3_600_000)
    const expected = { origin, target, fingerprint: pairingFingerprint(session.offer) }
    const plain = new Uint8Array([1, 2, 3])
    const encoded = await sealTransfer(plain, session.offer, expected, now + 45 * 60_000)
    expect(
      (await openTransfer(encoded, session, expected, now + 59 * 60_000)).plain
    ).toEqual(plain)
    await expect(
      openTransfer(encoded, session, expected, now + 3_600_000)
    ).rejects.toMatchObject({ code: 'permission' })
    const tooLong = offerLink({ ...session.offer, expiresAt: now + 3_600_001 })
    expect(() => readOfferLink(new URL(tooLong).hash, origin, now)).toThrow()
  })

  it('binds both origins, fingerprint, expiry and recipient, authenticating every ordered chunk', async () => {
    const session = await createPairing(origin, target)
    const expected = { origin, target, fingerprint: pairingFingerprint(session.offer) }
    const plain = new Uint8Array(TRANSFER_CHUNK + 15).fill(7)
    const encoded = await sealTransfer(plain, session.offer, expected)
    expect((await openTransfer(encoded, session, expected)).plain).toEqual(plain)
    await expect(
      openTransfer(encoded, session, { ...expected, origin: 'https://evil.example' })
    ).rejects.toMatchObject({ code: 'permission' })
    await expect(
      openTransfer(encoded, session, {
        ...expected,
        target: 'b'.repeat(64)
      })
    ).rejects.toMatchObject({ code: 'permission' })
    await expect(
      openTransfer(encoded, session, { ...expected, fingerprint: '0'.repeat(64) })
    ).rejects.toMatchObject({ code: 'permission' })
    await expect(
      openTransfer(encoded, session, expected, session.offer.expiresAt)
    ).rejects.toMatchObject({ code: 'permission' })
    const other = await createPairing(origin, target)
    await expect(
      openTransfer(encoded, { ...session, seed: other.seed }, expected)
    ).rejects.toMatchObject({ code: 'permission' })
    for (const mutate of [
      (v: any) => v.frames.reverse(),
      (v: any) => (v.frames[1].ciphertext[0] ^= 1),
      (v: any) => (v.frames[1] = v.frames[0]),
      (v: any) => v.frames.pop(),
      (v: any) => v.size--,
      (v: any) => (v.transfer = '0'.repeat(64))
    ]) {
      const changed: any = unpack(encoded)
      mutate(changed)
      await expect(openTransfer(pack(changed), session, expected)).rejects.toThrow()
    }
  })
  it('carries the offer in a canonical fragment checked against the page origin', async () => {
    const { offer } = await createPairing(origin, target)
    const link = offerLink(offer)
    expect(link.startsWith('https://dmsg.net/legacy#pair=')).toBe(true)
    const hash = link.slice(link.indexOf('#'))
    expect(readOfferLink(hash, origin)).toEqual(offer)
    expect(() => readOfferLink(hash, 'https://panda.fans')).toThrow()
    expect(() => readOfferLink(hash, origin, offer.expiresAt)).toThrow()
    expect(() => readOfferLink('#pair=' + hash.slice(6, -2), origin)).toThrow()
    expect(() =>
      readOfferLink(offerLink({ ...offer, target: 'chrome-extension://' + 'a'.repeat(32) }), origin)
    ).toThrow()
  })
  it('rejects malformed CBOR before recursive decoding', () => {
    expect(() => unpack(new Uint8Array(10000).fill(0x81))).toThrow()
    expect(() => unpack(new Uint8Array([0x9a, 0xff, 0xff, 0xff, 0xff]))).toThrow()
    expect(() => unpack(new Uint8Array([0x9f, 0xff]))).toThrow()
    expect(() => unpack(new Uint8Array([0xa2, 0x61, 0x78, 1, 0x61, 0x78, 2]))).toThrow()
  })
  it('retains query/cache trust and refuses duplicate/substituted sources or mismatched key identity', async () => {
    const bytes = pack({ payload: { bytes: pack('system') }, kind: 1 })
    const archive: LegacyArchive = {
      format: 'dmsg-legacy-archive/1',
      inventory: {
        format: 'dmsg-legacy-inventory/1',
        principal: 'aaaaa-aa',
        messageCanister: 'aaaaa-aa',
        mode: 'Local',
        snapshot: 'pre_migration',
        objects: [
          {
            key: 'aaaaa-aa/channel/1/message/1',
            kind: 'message',
            bytes,
            digest: digest(bytes),
            trust: 'query_observation',
            observedAt: 1
          }
        ],
        gaps: [],
        calls: []
      },
      keys: [],
      checks: [],
      createdAt: 1
    }
    expect(decodeArchive(encodeArchive(archive))).toEqual(archive)
    const checked = await verifyArchiveContent(archive)
    expect(checked.messages[0]?.text).toBe('system')
    expect(checked.checks[0]?.result).toBe('source_plaintext')
    expect(() =>
      validateArchive({
        ...archive,
        inventory: {
          ...archive.inventory,
          objects: [...archive.inventory.objects, ...archive.inventory.objects]
        }
      })
    ).toThrow()
    expect(() =>
      validateArchive({
        ...archive,
        inventory: {
          ...archive.inventory,
          objects: [{ ...archive.inventory.objects[0], digest: '0'.repeat(64) }]
        }
      })
    ).toThrow()
    expect(() =>
      validateArchive({
        ...archive,
        keys: [
          {
            source: 'x',
            principal: '2vxsx-fae',
            purpose: 'channel_dek',
            coseKey: new Uint8Array([1]),
            parameters: new Uint8Array()
          }
        ]
      })
    ).toThrow()
  })
})
