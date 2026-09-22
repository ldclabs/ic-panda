import { DerivedPublicKey, EncryptedVetKey, TransportSecretKey } from '@dfinity/vetkeys'
import { Principal } from '@icp-sdk/core/principal'
import {
  b64,
  canonical,
  decodeCanonical,
  digest,
  equal,
  hash,
  random,
  unb64,
  unhex
} from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { ed25519, hpkeSeal, open, recoveryContext, seal } from './primitives'
import { ensure } from '../errors'
import { z } from 'zod'

const fixedId = z.string().regex(/^[0-9a-f]{64}$/)
const encoded = (max: number) =>
  z
    .string()
    .max(max)
    .regex(/^[A-Za-z0-9_-]+$/)
const generation = z.number().int().positive().max(Number.MAX_SAFE_INTEGER)
const contextSchema = z.strictObject({
  account: z.string().regex(/^[0-9a-v]{19}[0g]$/),
  environment: z.enum(['local', 'staging', 'production']),
  homeCose: z.string().max(63),
  generation,
  opId: fixedId,
  recoveryGeneration: generation,
  recoveryPublic: encoded(43),
  recoverySigningPublic: encoded(43)
})
const bundleSchema = z.strictObject({
  payload: z.strictObject({
    format: z.literal('dmsg-root-bundle/1'),
    context: contextSchema,
    key: z.strictObject({
      publicKey: encoded(128),
      fingerprint: fixedId,
      keyId: fixedId,
      keyName: z.string().min(1).max(64)
    }),
    online: encoded(256),
    recovery: z.strictObject({ enc: encoded(43), ciphertext: encoded(64) }),
    previous: z
      .strictObject({ digest: fixedId, uploadId: fixedId, generation, envelope: encoded(256) })
      .nullable(),
    device: fixedId,
    signingPublic: encoded(43)
  }),
  signature: z.instanceof(Uint8Array).refine((v) => v.length === 64)
})

export interface RootContext {
  account: string
  environment: string
  homeCose: string
  generation: number
  opId: string
  recoveryGeneration: number
  recoveryPublic: string
  recoverySigningPublic: string
}
export interface RootKey {
  publicKey: string
  fingerprint: string
  keyId: string
  keyName: string
}
export interface RootMaterial {
  context: RootContext
  root: string
  transport: string
  bytes?: string
  roots?: Record<string, string>
  bundles?: string[]
}
export function rootMaterial(context: RootContext): RootMaterial {
  contextSchema.parse(context)
  xidBytes(context.account)
  Principal.fromText(context.homeCose)
  ensure(
    ['local', 'staging', 'production'].includes(context.environment) &&
      Number.isSafeInteger(context.generation) &&
      context.generation > 0 &&
      Number.isSafeInteger(context.recoveryGeneration) &&
      context.recoveryGeneration > 0 &&
      unb64(context.recoveryPublic).length === 32 &&
      unb64(context.recoverySigningPublic).length === 32 &&
      unhex(context.opId).length === 32,
    'INVALID_INPUT'
  )
  return {
    context: { ...context },
    root: b64(random()),
    transport: b64(TransportSecretKey.random().serialize())
  }
}
export const rootTransport = (material: RootMaterial) =>
  TransportSecretKey.deserialize(unb64(material.transport)).publicKeyBytes()
export const onlineContext = (context: RootContext) => [
  'dmsg/online-root/1',
  context.environment,
  xidBytes(context.account),
  Principal.fromText(context.homeCose).toUint8Array(),
  context.generation,
  2
]

export function onlineKey(material: RootMaterial, key: RootKey, encryptedKey: Uint8Array) {
  const c = material.context
  ensure(
    key.fingerprint === hash(unb64(key.publicKey)) &&
      (c.environment !== 'production' || key.keyName === 'key_1'),
    'INTEGRITY_FAILED'
  )
  const expectedId = digest('dmsg/key-id/v3', [
    c.environment[0].toUpperCase() + c.environment.slice(1),
    Principal.fromText(c.homeCose).toUint8Array(),
    2,
    xidBytes(c.account),
    { purpose: 'ContentRoot', algorithm: 'VetKdBls12381', generation: c.generation }
  ])
  ensure(equal(unhex(key.keyId), expectedId), 'INTEGRITY_FAILED')
  const vetkey = EncryptedVetKey.deserialize(encryptedKey).decryptAndVerify(
    TransportSecretKey.deserialize(unb64(material.transport)),
    DerivedPublicKey.deserialize(unb64(key.publicKey)),
    canonical([xidBytes(c.account), c.generation])
  )
  return vetkey.deriveSymmetricKey(canonical(onlineContext(c)), 32)
}

/** Fixed-size root record. Previous roots are enveloped one link at a time;
 * exporters must traverse every referenced bundle before claiming completeness. */
export async function wrapRoot(
  material: RootMaterial,
  key: RootKey,
  encryptedKey: Uint8Array,
  signer: { deviceId: string; seed: Uint8Array },
  previous: { digest: string; uploadId: string; generation: number; root: Uint8Array } | null
) {
  if (material.bytes) return unb64(material.bytes)
  const context = material.context,
    root = unb64(material.root),
    online = onlineKey(material, key, encryptedKey)
  try {
    const payload = {
      format: 'dmsg-root-bundle/1',
      context,
      key,
      online: await seal(online, root, onlineContext(context)),
      recovery: await hpkeSeal(
        context.recoveryPublic,
        root,
        recoveryContext(context.environment, context.account, context.generation)
      ),
      previous: previous
        ? {
            digest: previous.digest,
            uploadId: previous.uploadId,
            generation: previous.generation,
            envelope: await seal(root, previous.root, [
              'dmsg/previous-root/1',
              context.account,
              context.generation,
              previous.generation,
              previous.digest
            ])
          }
        : null,
      device: signer.deviceId,
      signingPublic: b64(ed25519.getPublicKey(signer.seed))
    }
    const result = canonical({
      payload,
      signature: ed25519.sign(digest('dmsg/root-bundle/1', payload), signer.seed)
    })
    ensure(result.length <= 64000, 'QUOTA_EXCEEDED')
    return result
  } finally {
    root.fill(0)
    online.fill(0)
  }
}

export interface RootBundle {
  payload: {
    format: 'dmsg-root-bundle/1'
    context: RootContext
    key: RootKey
    online: string
    recovery: { enc: string; ciphertext: string }
    previous: { digest: string; uploadId: string; generation: number; envelope: string } | null
    device: string
    signingPublic: string
  }
  signature: Uint8Array
}
export function readRootBundle(bytes: Uint8Array, expectedDigest: string): RootBundle {
  ensure(hash(bytes) === expectedDigest, 'INTEGRITY_FAILED')
  const result = bundleSchema.parse(decodeCanonical(bytes, 64000))
  ensure(
    result.payload?.format === 'dmsg-root-bundle/1' &&
      result.signature instanceof Uint8Array &&
      ed25519.verify(
        result.signature,
        digest('dmsg/root-bundle/1', result.payload),
        unb64(result.payload.signingPublic),
        { zip215: false }
      ),
    'INTEGRITY_FAILED'
  )
  rootMaterialValidation(result.payload.context)
  return result
}
function rootMaterialValidation(context: RootContext) {
  xidBytes(context.account)
  ensure(
    context.generation > 0 &&
      Number.isSafeInteger(context.generation) &&
      ['local', 'staging', 'production'].includes(context.environment),
    'INTEGRITY_FAILED'
  )
}
export async function openRoot(
  material: RootMaterial,
  key: RootKey,
  encryptedKey: Uint8Array,
  bundles: Uint8Array[],
  expectedDigest: string
) {
  ensure(bundles.length > 0 && bundles.length <= 256, 'QUOTA_EXCEEDED')
  const current = readRootBundle(bundles[0], expectedDigest)
  ensure(
    equal(canonical(material.context), canonical(current.payload.context)) &&
      equal(canonical(key), canonical(current.payload.key)),
    'INTEGRITY_FAILED'
  )
  const online = onlineKey(material, key, encryptedKey)
  let root: Uint8Array
  try {
    root = await open(online, current.payload.online, onlineContext(material.context))
  } finally {
    online.fill(0)
  }
  ensure(root.length === 32, 'INTEGRITY_FAILED')
  const roots: Record<string, string> = {}
  let cursor = current,
    keyBytes = root
  try {
    for (let i = 1; i < bundles.length; i++) {
      const link = cursor.payload.previous
      ensure(link && link.generation < cursor.payload.context.generation, 'INTEGRITY_FAILED')
      const previous = readRootBundle(bundles[i], link.digest)
      ensure(
        previous.payload.context.generation === link.generation &&
          previous.payload.context.account === material.context.account &&
          previous.payload.context.environment === material.context.environment &&
          previous.payload.context.homeCose === material.context.homeCose,
        'INTEGRITY_FAILED'
      )
      const prior = await open(keyBytes, link.envelope, [
        'dmsg/previous-root/1',
        material.context.account,
        cursor.payload.context.generation,
        link.generation,
        link.digest
      ])
      if (keyBytes !== root) keyBytes.fill(0)
      ensure(prior.length === 32, 'INTEGRITY_FAILED')
      keyBytes = prior
      roots[String(link.generation)] = b64(prior)
      cursor = previous
    }
    ensure(cursor.payload.previous === null, 'RECOVERY_INCOMPLETE')
    return {
      ...material,
      root: b64(root),
      roots,
      bytes: b64(bundles[0]),
      bundles: bundles.map(b64)
    }
  } finally {
    root.fill(0)
    keyBytes.fill(0)
  }
}
