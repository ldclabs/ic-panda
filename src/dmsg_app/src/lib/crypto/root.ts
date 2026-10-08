import {
  DerivedPublicKey,
  EncryptedVetKey,
  IbeCiphertext,
  IbeIdentity,
  IbeSeed,
  TransportSecretKey
} from '@icp-sdk/vetkeys'
import { Principal } from '@icp-sdk/core/principal'
import { sha256 } from '@noble/hashes/sha2.js'
import {
  b64,
  canonical,
  decodeCanonical,
  digest,
  equal,
  hash,
  hex,
  random,
  unb64,
  unhex
} from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { rootBundleDigest, rootRecipientsDigest } from '../protocol/account'
import {
  derive,
  deviceRootContext,
  ed25519,
  hpkeOpen,
  hpkeSeal,
  open,
  seal
} from './primitives'
import { ensure } from '../errors'
import { z } from 'zod'

export const ROOT_SUITE = 'dmsg-root-v2'
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
  generation,
  opId: fixedId,
  securityEpoch: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER)
})
const recoveryKeySchema = z.strictObject({
  homeCose: z.string().max(63),
  keyName: z.string().min(1).max(64),
  publicKey: encoded(128)
})
const bundleSchema = z.strictObject({
  body: z.strictObject({
    format: z.literal('dmsg-root-bundle/2'),
    context: contextSchema,
    commitment: fixedId,
    envelopes: z
      .array(z.strictObject({ device: fixedId, enc: encoded(43), ciphertext: encoded(64) }))
      .min(1)
      .max(16),
    recoveryKey: recoveryKeySchema,
    recovery: encoded(512),
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
  generation: number
  opId: string
  securityEpoch: number
}
/** The COSE content-root vetKD public key every recovery envelope is encrypted to. */
export interface RecoveryKey {
  homeCose: string
  keyName: string
  publicKey: string
}
export interface RootMaterial {
  context: RootContext
  root: string
  transport: string
  bytes?: string
  roots?: Record<string, string>
  bundles?: string[]
}
export type RootBundle = z.infer<typeof bundleSchema>
export function rootMaterial(context: RootContext): RootMaterial {
  contextSchema.parse(context)
  xidBytes(context.account)
  return {
    context: { ...context },
    root: b64(random()),
    transport: b64(TransportSecretKey.random().serialize())
  }
}
export const rootTransport = (material: RootMaterial) =>
  TransportSecretKey.deserialize(unb64(material.transport)).publicKeyBytes()
/** vetKD IBE identity of one account generation: `canonical((account_id, generation))`. */
export const recoveryIdentity = (account: string, generation: number) =>
  canonical([xidBytes(account), generation])
const commitment = (root: Uint8Array) => hash(derive(root, ['dmsg/root-commitment/1']))
export function recoveryKeyOf(key: RecoveryKey): DerivedPublicKey {
  recoveryKeySchema.parse(key)
  Principal.fromText(key.homeCose)
  const bytes = unb64(key.publicKey)
  ensure(bytes.length === 96, 'INTEGRITY_FAILED')
  return DerivedPublicKey.deserialize(bytes)
}

/** Build the generation's bundle: one HPKE envelope per active device, one
 * IBE envelope to the vetKD recovery identity, and the previous root sealed
 * under the new one. Exporters must traverse every referenced bundle. */
export async function wrapRoot(
  material: RootMaterial,
  recipients: { deviceId: string; hpkePublic: string }[],
  recoveryKey: RecoveryKey,
  signer: { deviceId: string; seed: Uint8Array },
  previous: { digest: string; uploadId: string; generation: number; root: Uint8Array } | null
) {
  if (material.bytes) return unb64(material.bytes)
  const context = material.context,
    root = unb64(material.root)
  ensure(
    recipients.length > 0 &&
      recipients.length <= 16 &&
      new Set(recipients.map((r) => r.deviceId)).size === recipients.length &&
      recipients.some((r) => r.deviceId === signer.deviceId),
    'INVALID_INPUT'
  )
  const publicKey = recoveryKeyOf(recoveryKey)
  try {
    const envelopes = []
    for (const recipient of [...recipients].sort((a, b) =>
      a.deviceId.localeCompare(b.deviceId)
    )) {
      ensure(unhex(recipient.deviceId).length === 32, 'INVALID_INPUT')
      const sealed = await hpkeSeal(
        recipient.hpkePublic,
        root,
        deviceRootContext(
          context.environment,
          context.account,
          context.generation,
          recipient.deviceId
        )
      )
      envelopes.push({ device: recipient.deviceId, ...sealed })
    }
    const body = {
      format: 'dmsg-root-bundle/2' as const,
      context,
      commitment: commitment(root),
      envelopes,
      recoveryKey,
      recovery: b64(
        IbeCiphertext.encrypt(
          publicKey,
          IbeIdentity.fromBytes(recoveryIdentity(context.account, context.generation)),
          root,
          IbeSeed.random()
        ).serialize()
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
      body,
      signature: ed25519.sign(digest('dmsg/root-bundle/2', body), signer.seed)
    })
    ensure(result.length <= 64000, 'QUOTA_EXCEEDED')
    return result
  } finally {
    root.fill(0)
  }
}

/** The digests the user home binds at `CommitRoot`. */
export function bundleDigests(bundle: RootBundle) {
  const recipients = rootRecipientsDigest(
    bundle.body.envelopes.map((e) => unhex(e.device)),
    bundle.body.context.generation
  )
  const body = sha256(canonical(bundle.body))
  return {
    recipientsDigest: recipients,
    bodyDigest: body,
    bundleDigest: rootBundleDigest(recipients, body)
  }
}
/** Parse a bundle and check its device signature, without the on-chain commitment. */
export function parseRootBundle(bytes: Uint8Array): RootBundle {
  const result = bundleSchema.parse(decodeCanonical(bytes, 64000))
  ensure(
    ed25519.verify(
      result.signature,
      digest('dmsg/root-bundle/2', result.body),
      unb64(result.body.signingPublic),
      { zip215: false }
    ) &&
      result.body.envelopes.every(
        (e, i) => i === 0 || e.device > result.body.envelopes[i - 1].device
      ) &&
      result.body.envelopes.some((e) => e.device === result.body.device),
    'INTEGRITY_FAILED'
  )
  xidBytes(result.body.context.account)
  return result
}
/** Parse and authenticate a bundle against its certified on-chain commitment. */
export function readRootBundle(bytes: Uint8Array, expectedBundleDigest: string): RootBundle {
  const result = parseRootBundle(bytes)
  ensure(hex(bundleDigests(result).bundleDigest) === expectedBundleDigest, 'INTEGRITY_FAILED')
  return result
}
async function walkPrevious(
  material: RootMaterial,
  current: RootBundle,
  bundles: Uint8Array[],
  root: Uint8Array
) {
  ensure(
    root.length === 32 && commitment(root) === current.body.commitment,
    'INTEGRITY_FAILED'
  )
  const roots: Record<string, string> = {}
  let cursor = current,
    keyBytes = root
  try {
    for (let i = 1; i < bundles.length; i++) {
      const link = cursor.body.previous
      ensure(link && link.generation < cursor.body.context.generation, 'INTEGRITY_FAILED')
      // Earlier bundles are authenticated by the envelope chain: the link's
      // digest names the cloud bytes, and the sealed root must open them.
      ensure(hash(bundles[i]) === link.digest, 'INTEGRITY_FAILED')
      const previous = bundleSchema.parse(decodeCanonical(bundles[i], 64000))
      ensure(
        previous.body.context.generation === link.generation &&
          previous.body.context.account === material.context.account &&
          previous.body.context.environment === material.context.environment,
        'INTEGRITY_FAILED'
      )
      const prior = await open(keyBytes, link.envelope, [
        'dmsg/previous-root/1',
        material.context.account,
        cursor.body.context.generation,
        link.generation,
        link.digest
      ])
      if (keyBytes !== root) keyBytes.fill(0)
      ensure(
        prior.length === 32 && commitment(prior) === previous.body.commitment,
        'INTEGRITY_FAILED'
      )
      keyBytes = prior
      roots[String(link.generation)] = b64(prior)
      cursor = previous
    }
    ensure(cursor.body.previous === null, 'RECOVERY_INCOMPLETE')
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
/** Open the current bundle with this device's own HPKE envelope. */
export async function openRoot(
  material: RootMaterial,
  device: { deviceId: string; hpkeSeed: Uint8Array },
  bundles: Uint8Array[],
  expectedBundleDigest: string
) {
  ensure(bundles.length > 0 && bundles.length <= 256, 'QUOTA_EXCEEDED')
  const current = readRootBundle(bundles[0], expectedBundleDigest)
  ensure(
    equal(canonical(material.context), canonical(current.body.context)),
    'INTEGRITY_FAILED'
  )
  const envelope = current.body.envelopes.find((e) => e.device === device.deviceId)
  ensure(envelope, 'DeviceNotApproved', '当前根包没有封装给这台设备。')
  const root = await hpkeOpen(
    device.hpkeSeed,
    envelope,
    deviceRootContext(
      material.context.environment,
      material.context.account,
      material.context.generation,
      device.deviceId
    )
  )
  return walkPrevious(material, current, bundles, root)
}
/** Open the current bundle's recovery envelope with a vetKD key derived to
 * this material's transport key. `key` is the executor's descriptor. */
export async function recoverRoot(
  material: RootMaterial,
  key: RecoveryKey,
  encryptedKey: Uint8Array,
  bundles: Uint8Array[],
  expectedBundleDigest: string
) {
  ensure(bundles.length > 0 && bundles.length <= 256, 'QUOTA_EXCEEDED')
  const current = readRootBundle(bundles[0], expectedBundleDigest)
  ensure(
    equal(canonical(material.context), canonical(current.body.context)) &&
      equal(canonical(key), canonical(current.body.recoveryKey)),
    'INTEGRITY_FAILED',
    '根包的恢复公钥与密钥服务的描述不一致。'
  )
  const vetkey = EncryptedVetKey.deserialize(encryptedKey).decryptAndVerify(
    TransportSecretKey.deserialize(unb64(material.transport)),
    recoveryKeyOf(key),
    recoveryIdentity(material.context.account, material.context.generation)
  )
  const root = IbeCiphertext.deserialize(unb64(current.body.recovery)).decrypt(vetkey)
  return walkPrevious(material, current, bundles, root)
}
