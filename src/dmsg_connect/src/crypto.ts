/** WebCrypto P-256, HMAC and AES-GCM, and HPKE (X25519, HKDF-SHA256, AES-256-GCM). */
import { Aes256Gcm, CipherSuite, DhkemX25519HkdfSha256, HkdfSha256 } from '@hpke/core'
import { canonical, concat, decodeMaps, equal, hex } from './codec.ts'
import { ensure, fail } from './errors.ts'

const subtle = globalThis.crypto.subtle
const buffer = (bytes: Uint8Array) => new Uint8Array(bytes)

/** DER prefix of a P-256 SubjectPublicKeyInfo; the uncompressed point follows. */
export const P256_SPKI_PREFIX = '3059301306072a8648ce3d020106082a8648ce3d030107034200'

export function isP256Spki(der: Uint8Array): boolean {
  return der.length === 91 && hex(der.subarray(0, 26)) === P256_SPKI_PREFIX && der[26] === 4
}

/** A P-256 key pair whose private key never leaves WebCrypto. */
export function p256Pair(extractable = false): Promise<CryptoKeyPair> {
  return subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, extractable, [
    'sign',
    'verify'
  ]) as Promise<CryptoKeyPair>
}

export async function spki(key: CryptoKey): Promise<Uint8Array> {
  return new Uint8Array(await subtle.exportKey('spki', key))
}

/** ECDSA P-256 / SHA-256 in IEEE P1363 form. */
export async function p256Sign(key: CryptoKey, message: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, key, buffer(message)))
}

export async function p256Verify(der: Uint8Array, message: Uint8Array, signature: Uint8Array): Promise<boolean> {
  if (!isP256Spki(der) || signature.length !== 64) return false
  try {
    const key = await subtle.importKey('spki', buffer(der), { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify'])
    return await subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, key, buffer(signature), buffer(message))
  } catch {
    return false
  }
}

/** `{1: -7, 3: "application/cbor", 4: kid, 16: typ}`. */
export function es256Header(kid: Uint8Array, typ: string): Uint8Array {
  return canonical(
    new Map<number, unknown>([
      [1, -7],
      [3, 'application/cbor'],
      [4, kid],
      [16, typ]
    ])
  )
}

const sigStructure = (protectedBytes: Uint8Array, payload: Uint8Array) =>
  canonical(['Signature1', protectedBytes, new Uint8Array(), payload])

/** A tagged COSE_Sign1 signed with a P-256 key. */
export async function sign1(key: CryptoKey, kid: Uint8Array, typ: string, payload: Uint8Array): Promise<Uint8Array> {
  const protectedBytes = es256Header(kid, typ)
  const signature = await p256Sign(key, sigStructure(protectedBytes, payload))
  return concat(Uint8Array.of(0xd2), canonical([protectedBytes, new Map(), payload, signature]))
}

/** Verify a P-256 COSE_Sign1 with exactly the header of `kid` and `typ`; returns the payload. */
export async function open1(cose: Uint8Array, kid: Uint8Array, typ: string, der: Uint8Array): Promise<Uint8Array> {
  ensure(cose[0] === 0xd2, 'INTEGRITY_FAILED')
  const value = decodeMaps(cose.subarray(1))
  ensure(Array.isArray(value) && value.length === 4, 'INTEGRITY_FAILED')
  const [protectedBytes, unprotected, payload, signature] = value as [unknown, unknown, unknown, unknown]
  ensure(
    protectedBytes instanceof Uint8Array &&
      unprotected instanceof Map &&
      unprotected.size === 0 &&
      payload instanceof Uint8Array &&
      signature instanceof Uint8Array &&
      equal(protectedBytes, es256Header(kid, typ)),
    'INTEGRITY_FAILED'
  )
  ensure(await p256Verify(der, sigStructure(protectedBytes, payload), signature), 'INTEGRITY_FAILED')
  return payload
}

export async function hmacSha256(secret: Uint8Array, message: Uint8Array): Promise<Uint8Array> {
  const key = await subtle.importKey('raw', buffer(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign'])
  return new Uint8Array(await subtle.sign('HMAC', key, buffer(message)))
}

export async function aesSeal(key: Uint8Array, nonce: Uint8Array, plaintext: Uint8Array, aad: Uint8Array) {
  const k = await subtle.importKey('raw', buffer(key), 'AES-GCM', false, ['encrypt'])
  return new Uint8Array(
    await subtle.encrypt({ name: 'AES-GCM', iv: buffer(nonce), additionalData: buffer(aad) }, k, buffer(plaintext))
  )
}

export async function aesOpen(key: Uint8Array, nonce: Uint8Array, ciphertext: Uint8Array, aad: Uint8Array) {
  const k = await subtle.importKey('raw', buffer(key), 'AES-GCM', false, ['decrypt'])
  try {
    return new Uint8Array(
      await subtle.decrypt({ name: 'AES-GCM', iv: buffer(nonce), additionalData: buffer(aad) }, k, buffer(ciphertext))
    )
  } catch {
    fail('INTEGRITY_FAILED')
  }
}

export const suite = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm()
})

/** HPKE base mode to a raw X25519 public key; `info` is also the AAD. */
export async function hpkeSeal(recipient: Uint8Array, plaintext: Uint8Array, info: Uint8Array) {
  const recipientPublicKey = await suite.kem.deserializePublicKey(buffer(recipient).buffer)
  const sealed = await suite.seal({ recipientPublicKey, info: buffer(info).buffer }, buffer(plaintext).buffer, buffer(info).buffer)
  return { enc: new Uint8Array(sealed.enc), ciphertext: new Uint8Array(sealed.ct) }
}

/** The bridge page's result key pair: X25519 whose private key WebCrypto never exports. */
export function resultPair(): Promise<CryptoKeyPair> {
  return subtle.generateKey({ name: 'X25519' }, false, ['deriveBits']) as Promise<CryptoKeyPair>
}

export async function rawPublic(key: CryptoKey): Promise<Uint8Array> {
  return new Uint8Array(await suite.kem.serializePublicKey(key))
}

/** HPKE auth mode: open a reply only `sender` (raw X25519) could have written. */
export async function hpkeOpenAuth(
  recipient: CryptoKeyPair,
  sender: Uint8Array,
  enc: Uint8Array,
  ciphertext: Uint8Array,
  info: Uint8Array
): Promise<Uint8Array> {
  try {
    const senderPublicKey = await suite.kem.deserializePublicKey(buffer(sender).buffer)
    return new Uint8Array(
      await suite.open(
        { recipientKey: recipient, enc: buffer(enc).buffer, senderPublicKey, info: buffer(info).buffer },
        buffer(ciphertext).buffer,
        buffer(info).buffer
      )
    )
  } catch {
    fail('INTEGRITY_FAILED')
  }
}
