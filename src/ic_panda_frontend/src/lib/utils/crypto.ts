import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { Encrypt0Message } from '@ldclabs/cose-ts/encrypt0'
import { Header } from '@ldclabs/cose-ts/header'
import { hkdf256 } from '@ldclabs/cose-ts/hkdf'
import * as iana from '@ldclabs/cose-ts/iana'
import { KDFContext, PartyInfo, SuppPubInfo } from '@ldclabs/cose-ts/kdfcontext'
import { argon2id } from '@noble/hashes/argon2'

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
  nonce: Uint8Array, // 12 bytes
  key_id?: Uint8Array
): Promise<Uint8Array> {
  const protect = new Header().setParam(
    iana.HeaderParameterAlg,
    iana.AlgorithmA256GCM
  )
  const unprotected = new Header().setParam(iana.HeaderParameterIV, nonce)
  if (key_id) {
    unprotected.setParam(iana.HeaderParameterKid, key_id)
  }
  const msg = new Encrypt0Message(payload, protect, unprotected)
  return await msg.toBytes(key, aad)
}

export async function coseA256GCMDecrypt0(
  key: AesGcmKey,
  data: Uint8Array,
  aad: Uint8Array
): Promise<Uint8Array> {
  const msg = await Encrypt0Message.fromBytes(key, data, aad)
  return msg.payload
}