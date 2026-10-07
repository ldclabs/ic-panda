import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { ed25519 } from '@noble/curves/ed25519.js'
import { Aes256Gcm, CipherSuite, HkdfSha256 } from '@hpke/core'
import { DhkemX25519HkdfSha256 } from '@hpke/dhkem-x25519'
import { b64, unb64, bytes, canonical, decodeCanonical, random } from '../protocol/codec'
import { ensure } from '../errors'

export { ed25519 }
export const hpke = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm()
})
export const derive = (key: Uint8Array, domain: unknown) =>
  bytes(hkdf(sha256, key, new Uint8Array(32), canonical(domain), 32))
/** Local unlock key from the user home's login-gated unlock secret. */
export function unlockKey(secret: Uint8Array, workspace: string) {
  ensure(secret.length === 32, 'INVALID_INPUT')
  return derive(secret, ['dmsg/local-unlock/2', workspace])
}
/** Local unlock key from a platform authenticator's WebAuthn PRF output. */
export function prfKey(output: Uint8Array, workspace: string) {
  ensure(output.length === 32, 'INVALID_INPUT')
  return derive(output, ['dmsg/local-unlock-prf/1', workspace])
}
/** The salt a workspace evaluates its PRF credential with. */
export const prfSalt = (workspace: string) => derive(new Uint8Array(32), ['dmsg/prf-salt/1', workspace])

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
/** HPKE info/AAD of a device's content-root envelope. */
export const deviceRootContext = (
  environment: string,
  account: string,
  generation: number,
  deviceId: string
) => ['dmsg/device-root/1', environment, account, generation, deviceId]
