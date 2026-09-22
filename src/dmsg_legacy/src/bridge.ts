import { Aes256Gcm, CipherSuite, HkdfSha256 } from '@hpke/core'
import { DhkemX25519HkdfSha256 } from '@hpke/dhkem-x25519'
import { z } from 'zod'
import { binary, digest, MAX_ARCHIVE, pack, requireLegacy, unpack } from './base'

const suite = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm()
})
const hex = (bytes: Uint8Array) =>
  Array.from(bytes, (n) => n.toString(16).padStart(2, '0')).join('')
const unhex = (value: string) => Uint8Array.from(value.match(/../g)!, (c) => parseInt(c, 16))
const h = z.string().regex(/^[0-9a-f]{64}$/)
const offerSchema = z
  .object({
    version: z.literal('dmsg-legacy-pair/1'),
    nonce: h,
    publicKey: h,
    origin: z.enum(['https://dmsg.net', 'https://panda.fans']),
    target: z.string().regex(/^chrome-extension:\/\/[a-p]{32}$/),
    expiresAt: z.number().int().nonnegative()
  })
  .strict()
export type PairingOffer = z.infer<typeof offerSchema>
export const TRANSFER_CHUNK = 192 * 1024
export const pairingFingerprint = (offer: PairingOffer) =>
  digest(pack(offerSchema.parse(offer)))
export function parseOffer(
  value: unknown,
  origin: string,
  target: string,
  now = Date.now()
): PairingOffer {
  const offer = offerSchema.parse(value)
  requireLegacy(
    offer.origin === origin &&
      offer.target === target &&
      offer.expiresAt > now &&
      offer.expiresAt <= now + 10 * 60 * 1000,
    'permission',
    'Pairing origin, extension target or expiry mismatch'
  )
  return offer
}
export async function createPairing(
  origin: PairingOffer['origin'],
  target: string,
  now = Date.now()
) {
  const seed = crypto.getRandomValues(new Uint8Array(32))
  const keys = await suite.kem.deriveKeyPair(seed)
  const offer = parseOffer(
    {
      version: 'dmsg-legacy-pair/1',
      nonce: hex(crypto.getRandomValues(new Uint8Array(32))),
      publicKey: hex(new Uint8Array(await suite.kem.serializePublicKey(keys.publicKey))),
      origin,
      target,
      expiresAt: now + 10 * 60 * 1000
    },
    origin,
    target,
    now
  )
  return { offer, seed }
}
const frameSchema = z
  .object({
    index: z.number().int().nonnegative(),
    previous: z.string().regex(/^(?:[0-9a-f]{64})?$/),
    enc: z.instanceof(Uint8Array).refine((v) => v.length === 32),
    ciphertext: z
      .instanceof(Uint8Array)
      .refine((v) => v.length >= 16 && v.length <= TRANSFER_CHUNK + 16)
  })
  .strict()
const transferSchema = z
  .object({
    format: z.literal('dmsg-legacy-transfer/1'),
    offer: offerSchema,
    transfer: h,
    count: z
      .number()
      .int()
      .min(1)
      .max(Math.ceil(MAX_ARCHIVE / TRANSFER_CHUNK)),
    size: z.number().int().min(1).max(MAX_ARCHIVE),
    frames: z
      .array(frameSchema)
      .min(1)
      .max(Math.ceil(MAX_ARCHIVE / TRANSFER_CHUNK))
  })
  .strict()
export async function sealTransfer(
  plain: Uint8Array,
  value: PairingOffer,
  expected: { origin: string; target: string; fingerprint: string },
  now = Date.now()
) {
  const offer = parseOffer(value, expected.origin, expected.target, now)
  requireLegacy(
    pairingFingerprint(offer) === expected.fingerprint,
    'permission',
    'Pairing fingerprint was not confirmed'
  )
  requireLegacy(
    plain.length > 0 && plain.length <= MAX_ARCHIVE,
    'limit',
    'Legacy transfer exceeds 256 MiB'
  )
  const transfer = hex(crypto.getRandomValues(new Uint8Array(32))),
    count = Math.ceil(plain.length / TRANSFER_CHUNK)
  const header = {
    format: 'dmsg-legacy-transfer/1' as const,
    offer,
    transfer,
    count,
    size: plain.length
  }
  const frames = []
  let previous = ''
  const recipientPublicKey = await suite.kem.deserializePublicKey(unhex(offer.publicKey))
  for (let index = 0; index < count; index++) {
    const context = pack([header, index, previous])
    const sealed = await suite.seal(
      { recipientPublicKey, info: context },
      binary(plain.subarray(index * TRANSFER_CHUNK, (index + 1) * TRANSFER_CHUNK)),
      context
    )
    const frame = {
      index,
      previous,
      enc: new Uint8Array(sealed.enc),
      ciphertext: new Uint8Array(sealed.ct)
    }
    frames.push(frame)
    previous = digest(pack(frame))
  }
  const result = pack({ ...header, frames })
  requireLegacy(
    result.length <= MAX_ARCHIVE,
    'limit',
    'Encrypted transfer exceeds 256 MiB including framing'
  )
  return result
}
/** The owner persists seed encrypted and consumes nonce atomically with import.
 * This decoder has no storage/network access and returns no authentication key. */
export async function openTransfer(
  bytes: Uint8Array,
  session: { offer: PairingOffer; seed: Uint8Array },
  expected: { origin: string; target: string; fingerprint: string },
  now = Date.now()
) {
  const offer = parseOffer(session.offer, expected.origin, expected.target, now)
  requireLegacy(
    pairingFingerprint(offer) === expected.fingerprint,
    'permission',
    'Pairing fingerprint was not confirmed'
  )
  const value = transferSchema.parse(unpack(bytes))
  requireLegacy(
    pairingFingerprint(value.offer) === expected.fingerprint &&
      value.count === value.frames.length &&
      value.count === Math.ceil(value.size / TRANSFER_CHUNK),
    'corrupt',
    'Substituted or incomplete transfer'
  )
  const { frames, ...header } = value
  const keys = await suite.kem.deriveKeyPair(binary(session.seed))
  requireLegacy(
    hex(new Uint8Array(await suite.kem.serializePublicKey(keys.publicKey))) ===
      offer.publicKey,
    'permission',
    'Wrong pairing recipient'
  )
  const plain = new Uint8Array(value.size)
  let previous = ''
  for (let index = 0; index < value.count; index++) {
    const frame = frames[index]!
    requireLegacy(
      frame.index === index && frame.previous === previous,
      'corrupt',
      'Replayed, missing or reordered transfer frame'
    )
    const context = pack([header, index, previous])
    let block: Uint8Array
    try {
      block = new Uint8Array(
        await suite.open(
          { recipientKey: keys.privateKey, enc: binary(frame.enc), info: context },
          binary(frame.ciphertext),
          context
        )
      )
    } catch {
      plain.fill(0)
      throw new Error('Legacy transfer authentication failed')
    }
    requireLegacy(
      block.length === Math.min(TRANSFER_CHUNK, value.size - index * TRANSFER_CHUNK),
      'corrupt',
      'Transfer length mismatch'
    )
    plain.set(block, index * TRANSFER_CHUNK)
    block.fill(0)
    previous = digest(pack(frame))
  }
  return { plain, transfer: value.transfer, digest: digest(bytes) }
}
