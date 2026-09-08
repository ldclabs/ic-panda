import { argon2idAsync } from '@noble/hashes/argon2.js'
import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { ed25519 } from '@noble/curves/ed25519.js'
import { Aes256Gcm, CipherSuite, HkdfSha256 } from '@hpke/core'
import { DhkemX25519HkdfSha256 } from '@hpke/dhkem-x25519'
import { b64, unb64, bytes, canonical, decodeCanonical, random, utf8 } from '../protocol/codec'
import type { KdfParams } from '../models'
import { ensure } from '../errors'

export { ed25519 }
export const hpke = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm()
})
export const derive = (key: Uint8Array, domain: unknown) =>
  bytes(hkdf(sha256, key, new Uint8Array(32), canonical(domain), 32))
export const defaultKdf = (): KdfParams => ({
  algorithm: 'argon2id',
  version: 1,
  memory: 65536,
  iterations: 3,
  parallelism: 1,
  salt: b64(random(16))
})
export async function passwordKey(
  password: string,
  params: KdfParams
): Promise<Uint8Array<ArrayBuffer>> {
  ensure(
    params.algorithm === 'argon2id' &&
      params.version === 1 &&
      params.memory === 65536 &&
      params.iterations === 3 &&
      params.parallelism === 1 &&
      unb64(params.salt).length === 16,
    'UNSUPPORTED_PROTOCOL',
    '不支持的口令参数。'
  )
  ensure(utf8(password).length <= 1024, 'INVALID_INPUT', '口令过长。')
  const key = await argon2idAsync(utf8(password), unb64(params.salt), {
    m: params.memory,
    t: params.iterations,
    p: params.parallelism,
    dkLen: 32
  })
  try {
    return derive(key, ['dmsg/local-unlock/1'])
  } finally {
    key.fill(0)
  }
}

async function aesKey(key: Uint8Array, usage: KeyUsage[]) {
  ensure(key.length === 32, 'INTEGRITY_FAILED')
  return crypto.subtle.importKey('raw', bytes(key), 'AES-GCM', false, usage)
}

// Untagged COSE_Encrypt0. The protected algorithm and caller's domain are both
// authenticated through Enc_structure; no plaintext or ad-hoc cipher fallback.
export async function seal(
  key: Uint8Array,
  plaintext: Uint8Array,
  aad: unknown,
  iv = random(12)
): Promise<string> {
  ensure(iv.length === 12, 'INVALID_INPUT')
  const protectedHeaders = canonical(new Map([[1, 3]]))
  const encrypted = await crypto.subtle.encrypt(
    {
      name: 'AES-GCM',
      iv: bytes(iv),
      additionalData: canonical(['Encrypt0', protectedHeaders, canonical(aad)]),
      tagLength: 128
    },
    await aesKey(key, ['encrypt']),
    bytes(plaintext)
  )
  return b64(canonical([protectedHeaders, new Map([[5, iv]]), new Uint8Array(encrypted)]))
}
export async function open(
  key: Uint8Array,
  encoded: string,
  aad: unknown
): Promise<Uint8Array<ArrayBuffer>> {
  const value = decodeCanonical<unknown[]>(unb64(encoded), 2 * 1024 * 1024)
  ensure(
    Array.isArray(value) &&
      value.length === 3 &&
      value[0] instanceof Uint8Array &&
      value[1] instanceof Map &&
      value[2] instanceof Uint8Array,
    'INTEGRITY_FAILED'
  )
  const [protectedHeaders, unprotected, ciphertext] = value as [
    Uint8Array,
    Map<number, Uint8Array>,
    Uint8Array
  ]
  const header = decodeCanonical<Map<number, number>>(protectedHeaders)
  ensure(
    header instanceof Map &&
      header.size === 1 &&
      header.get(1) === 3 &&
      unprotected.size === 1 &&
      unprotected.get(5)?.length === 12 &&
      ciphertext.length >= 16,
    'UNSUPPORTED_PROTOCOL'
  )
  try {
    return new Uint8Array(
      await crypto.subtle.decrypt(
        {
          name: 'AES-GCM',
          iv: bytes(unprotected.get(5)!),
          additionalData: canonical(['Encrypt0', protectedHeaders, canonical(aad)]),
          tagLength: 128
        },
        await aesKey(key, ['decrypt']),
        bytes(ciphertext)
      )
    )
  } catch {
    throw new Error('INTEGRITY_FAILED：密文校验失败，内容未打开。')
  }
}
export async function hpkeKeys(seed: Uint8Array) {
  return hpke.kem.deriveKeyPair(bytes(seed))
}
export async function hpkePublic(seed: Uint8Array) {
  return b64(
    new Uint8Array(await hpke.kem.serializePublicKey((await hpkeKeys(seed)).publicKey))
  )
}
export async function hpkeSeal(publicKey: string, data: Uint8Array, context: unknown) {
  const recipientPublicKey = await hpke.kem.deserializePublicKey(unb64(publicKey))
  const result = await hpke.seal(
    { recipientPublicKey, info: canonical(context) },
    bytes(data),
    canonical(context)
  )
  return { enc: b64(new Uint8Array(result.enc)), ciphertext: b64(new Uint8Array(result.ct)) }
}
export async function hpkeOpen(
  seed: Uint8Array,
  envelope: { enc: string; ciphertext: string },
  context: unknown
) {
  return new Uint8Array(
    await hpke.open(
      {
        recipientKey: (await hpkeKeys(seed)).privateKey,
        enc: unb64(envelope.enc),
        info: canonical(context)
      },
      unb64(envelope.ciphertext),
      canonical(context)
    )
  )
}
export function recoverySeeds(
  code: Uint8Array,
  environment: string,
  subject: string,
  generation: number
) {
  ensure(code.length === 32, 'INVALID_INPUT', '恢复码应为 256 位。')
  return {
    signing: derive(code, ['dmsg/recovery/1', environment, subject, generation, 'sign']),
    hpke: derive(code, ['dmsg/recovery/1', environment, subject, generation, 'hpke'])
  }
}
export const recoveryContext = (environment: string, subject: string, generation: number) => [
  'dmsg/recovery-root/1',
  environment,
  subject,
  generation
]
