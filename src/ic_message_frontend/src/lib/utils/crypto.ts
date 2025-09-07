import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { ECDHKey } from '@ldclabs/cose-ts/ecdh'
import { Encrypt0Message } from '@ldclabs/cose-ts/encrypt0'
import { Header } from '@ldclabs/cose-ts/header'
import { hkdf256 } from '@ldclabs/cose-ts/hkdf'
import * as iana from '@ldclabs/cose-ts/iana'
import { KDFContext, PartyInfo, SuppPubInfo } from '@ldclabs/cose-ts/kdfcontext'
import { randomBytes } from '@ldclabs/cose-ts/utils'
import { argon2id } from '@noble/hashes/argon2'
import { hmac } from '@noble/hashes/hmac'
import { sha3_256 } from '@noble/hashes/sha3'

export {
  assertEqual,
  base64ToBytes,
  bytesToBase64Url,
  bytesToHex,
  compareBytes,
  concatBytes,
  decodeCBOR,
  encodeCBOR,
  hexToBytes,
  randomBytes,
  toBytes,
  utf8ToBytes
} from '@ldclabs/cose-ts/utils'

export { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
export { ECDHKey } from '@ldclabs/cose-ts/ecdh'
export * as iana from '@ldclabs/cose-ts/iana'

export function toArrayBuffer(
  data: Uint8Array | number[],
  property?: string
): ArrayBuffer {
  if (!(data instanceof Uint8Array)) {
    data = new Uint8Array(data)
  }
  const buf = data.buffer.slice(
    data.byteOffset,
    data.byteOffset + data.byteLength
  ) as ArrayBuffer

  if (property) {
    Object.defineProperty(buf, 'property', {
      enumerable: false,
      value: undefined
    })
  }
  return buf
}

export function toUint8Array(
  data: Uint8Array | number[],
  property?: string
): Uint8Array {
  if (!(data instanceof Uint8Array)) {
    data = new Uint8Array(data)
  }

  if (property) {
    Object.defineProperty(data, 'property', {
      enumerable: false,
      value: undefined
    })
  }
  return data
}

export function generateECDHKey(): ECDHKey {
  return ECDHKey.generate(iana.EllipticCurveX25519)
}

export function hashPassword(password: string, salt: string): Uint8Array {
  // default params from https://docs.rs/argon2/latest/argon2/struct.Params.html
  return argon2id(password, salt, { t: 2, m: 19456, p: 1 })
}

// HKDF-SHA-256 with Context Information Structure
// https://datatracker.ietf.org/doc/html/rfc9053#name-context-information-structu
export function deriveA256GCMSecret(
  secret: Uint8Array,
  salt: Uint8Array
): Uint8Array {
  const ctx = new KDFContext(
    iana.AlgorithmA256GCM,
    new PartyInfo(),
    new PartyInfo(),
    new SuppPubInfo(
      256,
      new Header(
        new Map([[iana.HeaderParameterAlg, iana.AlgorithmDirect_HKDF_SHA_256]])
      )
    )
  )

  return hkdf256(secret, salt, ctx.toBytes(), 32)
}

export async function coseA256GCMEncrypt0(
  key: AesGcmKey,
  payload: Uint8Array,
  aad: Uint8Array,
  keyId?: Uint8Array
): Promise<Uint8Array> {
  const protect = new Header().setParam(
    iana.HeaderParameterAlg,
    iana.AlgorithmA256GCM
  )
  const unprotected = new Header().setParam(
    iana.HeaderParameterIV,
    randomBytes(12)
  )

  if (keyId) {
    unprotected.setParam(iana.HeaderParameterKid, keyId)
  }
  const msg = new Encrypt0Message(payload, protect, unprotected)
  return await msg.toBytes(key, aad)
}

export async function coseA256GCMDecrypt0(
  key: AesGcmKey,
  data: Uint8Array,
  aad: Uint8Array
): Promise<Uint8Array> {
  try {
    const msg = await Encrypt0Message.fromBytes(key, data, aad)
    return msg.payload
  } catch (err) {
    throw new Error(`Failed to decrypt: ${err}`)
  }
}

export function hmac3_256(key: Uint8Array, message: Uint8Array): Uint8Array {
  return hmac(sha3_256, key, message)
}
