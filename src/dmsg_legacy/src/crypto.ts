import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { ECDHKey } from '@ldclabs/cose-ts/ecdh'
import { Encrypt0Message } from '@ldclabs/cose-ts/encrypt0'
import { Header } from '@ldclabs/cose-ts/header'
import { hkdf256 } from '@ldclabs/cose-ts/hkdf'
import { KDFContext, PartyInfo, SuppPubInfo } from '@ldclabs/cose-ts/kdfcontext'
import * as iana from '@ldclabs/cose-ts/iana'
import { argon2idAsync } from '@noble/hashes/argon2.js'
import { DerivedPublicKey, EncryptedVetKey, TransportSecretKey } from '@dfinity/vetkeys'
import { Principal } from '@icp-sdk/core/principal'
import {
  binary,
  digest,
  LegacyError,
  MAX_ARCHIVE,
  pack,
  observation,
  requireLegacy,
  unpack
} from './base'
import { LegacyReader } from './reader'

export interface LegacyRootSource {
  mode: 'Local' | 'ECDH' | 'VetKey'
  principal: string
  keyId: Uint8Array // Preserve original CBOR bytes; never encode the display name.
  myIV: Uint8Array
  canister: string | null
  namespace: string
  userOwned: boolean
  settingVersion: number
  contextVersion: 1 | 2
  keyName: string
}
export interface CachedRoot {
  kind: LegacyRootSource['mode']
  version?: number
  keyId: Uint8Array
  encryptedSecret: Uint8Array
}
export function legacyDerive(secret: Uint8Array, salt: Uint8Array): Uint8Array {
  const context = new KDFContext(
    iana.AlgorithmA256GCM,
    new PartyInfo(),
    new PartyInfo(),
    new SuppPubInfo(
      256,
      new Header(new Map([[iana.HeaderParameterAlg, iana.AlgorithmDirect_HKDF_SHA_256]]))
    )
  )
  return hkdf256(secret, salt, context.toBytes(), 32)
}
export async function legacyDecrypt(
  key: AesGcmKey,
  ciphertext: Uint8Array,
  aad: Uint8Array = new Uint8Array()
): Promise<Uint8Array> {
  requireLegacy(
    ciphertext.length > 0 && ciphertext.length <= MAX_ARCHIVE,
    'limit',
    'Invalid legacy ciphertext size'
  )
  try {
    return (await Encrypt0Message.fromBytes(key, ciphertext, aad)).payload
  } catch {
    throw new LegacyError('corrupt', 'Legacy AEAD authentication failed; key was not replaced')
  }
}
function key(bytes: Uint8Array) {
  try {
    const value = AesGcmKey.fromBytes(bytes)
    requireLegacy(value.getSecret().length === 32, 'corrupt', 'Invalid legacy AES key')
    return value
  } catch {
    throw new LegacyError('corrupt', 'Invalid legacy COSE_Key')
  }
}
/** Pure unlock path: no init, reset, migration, key writes or cached-password use. */
export async function unlockLegacyRoot(
  reader: LegacyReader,
  source: LegacyRootSource,
  input: {
    password?: string | undefined
    salt?: string | undefined
    cached?: CachedRoot | undefined
    expectedPublicKeyDigest?: string
  }
) {
  requireLegacy(
    source.principal === reader.source.principal && source.mode === reader.source.mode,
    'permission',
    'Legacy key identity or mode mismatch'
  )
  const baseline = reader.source.derivation
  requireLegacy(
    baseline
      ? baseline.contextVersion === source.contextVersion &&
          baseline.keyNames[source.mode] === source.keyName
      : source.contextVersion === 1,
    'version',
    'Legacy derivation descriptor is outside the reviewed deployment baseline'
  )
  requireLegacy(
    source.myIV.length > 0 &&
      source.myIV.length <= 64 &&
      source.keyId.length > 0 &&
      source.keyId.length <= 1024,
    'missing_key',
    'Original myIV and keyId are required'
  )
  const aad = Principal.fromText(source.principal).toUint8Array()
  const path = {
    ns: source.namespace,
    key: source.keyId,
    subject: [Principal.fromText(source.principal)],
    version: source.settingVersion,
    user_owned: source.userOwned
  }
  let passwordSecret = new Uint8Array()
  if (source.mode !== 'VetKey') {
    requireLegacy(
      typeof input.password === 'string' &&
        typeof input.salt === 'string' &&
        input.password.length <= 1024 &&
        input.salt.length <= 1024,
      'missing_key',
      'Original password and salt are required'
    )
    passwordSecret = Uint8Array.from(
      await argon2idAsync(input.password, input.salt, { t: 2, m: 19456, p: 1 })
    )
  }
  try {
    if (input.cached) {
      requireLegacy(
        input.cached.kind === source.mode &&
          digest(input.cached.keyId) === digest(source.keyId),
        'permission',
        'Cached root mode/keyId mismatch'
      )
      requireLegacy(
        [1, 2].includes(input.cached.version || 1),
        'version',
        'Unsupported legacy root wrapper'
      )
      return {
        key: key(
          await legacyDecrypt(
            AesGcmKey.fromSecret(legacyDerive(passwordSecret, source.myIV)),
            input.cached.encryptedSecret,
            aad
          )
        ),
        source,
        path,
        publicKeyDigest: null,
        originalWrapper: pack(
          observation({
            kind: input.cached.kind,
            version: input.cached.version || 1,
            keyId: input.cached.keyId,
            encryptedSecret: input.cached.encryptedSecret
          })
        )
      }
    }
    if (source.mode === 'Local') {
      // Deterministic MK alone cannot recover missing random channel KEKs.
      return {
        key: AesGcmKey.fromSecret(legacyDerive(passwordSecret, source.myIV), source.keyId),
        source,
        path,
        publicKeyDigest: null,
        originalWrapper: new Uint8Array()
      }
    }
    requireLegacy(source.canister, 'missing_key', 'Original COSE canister is required')
    if (source.mode === 'ECDH') {
      const ephemeral = ECDHKey.generate(iana.EllipticCurveX25519)
      const nonce = crypto.getRandomValues(new Uint8Array(12))
      const result = await reader.call('cose', source.canister, 'ecdh_cose_encrypted_key', [
        path,
        { public_key: ephemeral.getPublicKey(), nonce }
      ])
      const remote = ECDHKey.fromPublic(iana.EllipticCurveX25519, binary(result.public_key))
      const shared = ephemeral.ecdh(remote)
      try {
        const remoteKey = key(
          await legacyDecrypt(AesGcmKey.fromSecret(shared), binary(result.payload), aad)
        )
        return {
          key: AesGcmKey.fromSecret(
            legacyDerive(passwordSecret, remoteKey.getSecret()),
            source.keyId
          ),
          source,
          path,
          publicKeyDigest: null,
          originalWrapper: new Uint8Array()
        }
      } finally {
        shared.fill(0)
      }
    }
    const setting = await reader.setting(path, source.canister)
    requireLegacy(
      setting.value.dek?.length === 1,
      'missing_key',
      'Existing VetKey master wrapper is missing'
    )
    // Use the actual historical ownership path for both wrapping and derivation.
    const tsk = TransportSecretKey.random()
    const publicBytes = binary(
      await reader.call('cose', source.canister, 'vetkd_public_key', [setting.path])
    )
    if (input.expectedPublicKeyDigest)
      requireLegacy(
        digest(publicBytes) === input.expectedPublicKeyDigest,
        'corrupt',
        'Legacy derived public key changed'
      )
    const encrypted = binary(
      await reader.call('cose', source.canister, 'vetkd_encrypted_key', [
        setting.path,
        tsk.publicKeyBytes()
      ])
    )
    let secret: Uint8Array
    try {
      secret = EncryptedVetKey.deserialize(encrypted)
        .decryptAndVerify(tsk, DerivedPublicKey.deserialize(publicBytes), source.keyId)
        .deriveSymmetricKey('', 32)
    } catch {
      throw new LegacyError('corrupt', 'Legacy VetKey transport verification failed')
    }
    try {
      return {
        key: key(
          await legacyDecrypt(AesGcmKey.fromSecret(secret), binary(setting.value.dek[0]), aad)
        ),
        source,
        path: setting.path,
        publicKeyDigest: digest(publicBytes),
        originalWrapper: pack(observation({ path: setting.path, setting: setting.value }))
      }
    } finally {
      secret.fill(0)
    }
  } finally {
    passwordSecret.fill(0)
  }
}
export async function unlockLegacyChannel(
  master: AesGcmKey,
  wrappedKek: Uint8Array | null,
  wrappedDek: Uint8Array
) {
  requireLegacy(
    wrappedKek?.length,
    'missing_key',
    'Random legacy KEK is missing; a password or new root cannot replace it'
  )
  const kek = key(await legacyDecrypt(master, wrappedKek))
  return { kek, dek: key(await legacyDecrypt(kek, wrappedDek)) }
}
/** Consume an already received exchange locally; never acknowledge or save it
 * back to legacy services. Its exact sender key/ciphertext remain in the archive. */
export async function unlockLegacyExchange(
  staticKey: Uint8Array,
  expectedPublic: Uint8Array,
  remote: [Uint8Array, Uint8Array],
  wrappedDek: Uint8Array
) {
  const local = ECDHKey.fromBytes(staticKey)
  requireLegacy(
    digest(local.getPublicKey()) === digest(expectedPublic),
    'missing_key',
    'Static ECDH key does not match the frozen recipient'
  )
  const secret = local.ecdh(ECDHKey.fromPublic(iana.EllipticCurveX25519, remote[0]))
  try {
    const kek = key(await legacyDecrypt(AesGcmKey.fromSecret(secret), remote[1]))
    return { kek, dek: key(await legacyDecrypt(kek, wrappedDek)) }
  } finally {
    secret.fill(0)
  }
}
export async function decodeLegacyMessage(
  dek: AesGcmKey | null,
  input: { kind: number; payload: Uint8Array }
) {
  if (!input.payload.length) return { deleted: true, text: '', file: null }
  requireLegacy(input.kind === 0 || input.kind === 1, 'version', 'Unknown legacy message kind')
  requireLegacy(input.kind === 1 || dek, 'missing_key', 'Channel DEK is missing')
  const plain = input.kind === 1 ? input.payload : await legacyDecrypt(dek!, input.payload)
  const value = unpack(plain, 32768)
  if (typeof value === 'string') return { deleted: false, text: value, file: null }
  requireLegacy(
    Array.isArray(value) &&
      value.length === 3 &&
      typeof value[0] === 'string' &&
      value[1] === 'File' &&
      value[2] instanceof Uint8Array,
    'version',
    'Unsupported legacy message payload'
  )
  const file = unpack(value[2], 8192) as {
    canister: Uint8Array
    id: number
    name: string
    size: number
    type: string
  }
  requireLegacy(
    file &&
      file.canister instanceof Uint8Array &&
      Number.isInteger(file.id) &&
      file.id >= 0 &&
      file.id <= 0xffffffff &&
      Number.isSafeInteger(file.size) &&
      file.size >= 0 &&
      typeof file.name === 'string' &&
      typeof file.type === 'string',
    'corrupt',
    'Invalid legacy attachment reference'
  )
  return { deleted: false, text: value[0], file }
}
export const channelSettingKey = (canister: string, id: number) =>
  pack([Principal.fromText(canister).toUint8Array(), id])
