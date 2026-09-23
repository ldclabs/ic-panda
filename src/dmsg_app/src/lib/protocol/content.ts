import { z } from 'zod'
import { b64, canonical, decodeCanonical, equal, hash, unb64 } from './codec'
import { MAX_CIPHER_CHUNK, MAX_CIPHER_UPLOAD } from '../config'
import { ensure } from '../errors'
import type { EncryptedObject, FileManifest } from '../models'
import { open, seal } from '../crypto/primitives'

const identifier = z.string().regex(/^[0-9a-f]{64}$/)
const uint = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER)
export const uploadPlanSchema = z
  .strictObject({
    upload_id: identifier,
    object_id: identifier,
    version_id: identifier,
    root_generation: uint.min(1),
    epoch: z.literal(0),
    control_head: z.null(),
    kind: z.enum(['vault', 'file']),
    chunks: z
      .array(z.strictObject({ digest: identifier, size: uint.min(1).max(MAX_CIPHER_CHUNK) }))
      .min(1)
      .max(128),
    manifest_digest: identifier,
    manifest_size: uint.min(1).max(65536),
    expires_at: uint
  })
  .refine(
    (p) => p.manifest_size + p.chunks.reduce((n, c) => n + c.size, 0) <= MAX_CIPHER_UPLOAD
  )
export type ContentUploadPlan = z.infer<typeof uploadPlanSchema>
export const storedUploadPlanSchema = z
  .strictObject({
    upload_id: identifier,
    object_id: identifier,
    version_id: identifier,
    root_generation: uint,
    epoch: z.literal(0),
    control_head: z.null(),
    kind: z.enum(['vault', 'file', 'root', 'avatar', 'envelopes']),
    chunks: z
      .array(z.strictObject({ digest: identifier, size: uint.min(1).max(MAX_CIPHER_CHUNK) }))
      .min(1)
      .max(128),
    manifest_digest: identifier,
    manifest_size: uint.min(1).max(65536),
    expires_at: uint
  })
  .refine(
    (p) => p.manifest_size + p.chunks.reduce((n, c) => n + c.size, 0) <= MAX_CIPHER_UPLOAD
  )
export type StoredUploadPlan = z.infer<typeof storedUploadPlanSchema>
export interface ContentUpload {
  plan: ContentUploadPlan
  manifest: string
  // Internal encrypted journal references. These never contain FileKey.
  chunkIds: string[]
  object?: string
}
export interface ContentJob {
  format: 'dmsg-content-job/1'
  account: string
  recordKey: string
  recordDigest: string
  uploads: ContentUpload[]
  revision: {
    requestId: string
    signed?: string
    deadline?: number
    result?: { revision_id: string; head: string; conflict: boolean; tombstone: boolean }
  }
}
export const emptyFile = () => canonical({ format: 'dmsg-cloud-empty-file/1' })
export const objectBytes = (record: EncryptedObject) =>
  canonical({ format: 'dmsg-cloud-object/1', record })
// Build local routing indexes only from authenticated plaintext. Index hints
// are not part of the uploaded record or its wire metadata.
export function objectChannel(record: Pick<EncryptedObject, 'id' | 'kind'>, payload: unknown) {
  const value =
    record.kind === 'formal_channel' ? record.id : (payload as { channel?: unknown })?.channel
  return record.kind.startsWith('formal_') &&
    typeof value === 'string' &&
    /^[0-9a-f]{64}$/.test(value)
    ? value
    : ''
}
export function readObject(bytes: Uint8Array): EncryptedObject {
  const value = decodeCanonical<{ format: string; record: EncryptedObject }>(
    bytes,
    MAX_CIPHER_CHUNK
  )
  ensure(
    value.format === 'dmsg-cloud-object/1' && value.record && Object.keys(value).length === 2,
    'UNSUPPORTED_PROTOCOL'
  )
  return value.record
}
const context = (account: string, p: ContentUploadPlan) => [
  'dmsg/cloud-manifest/1',
  account,
  p.upload_id,
  p.object_id,
  p.version_id,
  p.kind,
  p.root_generation
]
export async function sealContentManifest(
  root: Uint8Array,
  account: string,
  plan: Omit<ContentUploadPlan, 'manifest_digest' | 'manifest_size'>,
  content: FileManifest | null
) {
  const value = {
    format: 'dmsg-cloud-manifest/1',
    account_id: account,
    upload_id: plan.upload_id,
    object_id: plan.object_id,
    version_id: plan.version_id,
    kind: plan.kind,
    root_generation: plan.root_generation,
    chunks: plan.chunks,
    content
  }
  const manifest = await seal(
    root,
    canonical(value),
    context(account, plan as ContentUploadPlan)
  )
  const bytes = unb64(manifest)
  return {
    manifest,
    plan: uploadPlanSchema.parse({
      ...plan,
      manifest_digest: hash(bytes),
      manifest_size: bytes.length
    })
  }
}
export async function openContentManifest(
  root: Uint8Array,
  account: string,
  plan: ContentUploadPlan,
  encrypted: Uint8Array
) {
  uploadPlanSchema.parse(plan)
  ensure(
    encrypted.length === plan.manifest_size && hash(encrypted) === plan.manifest_digest,
    'INTEGRITY_FAILED'
  )
  const value = decodeCanonical<{
    format: string
    account_id: string
    upload_id: string
    object_id: string
    version_id: string
    kind: string
    root_generation: number
    chunks: ContentUploadPlan['chunks']
    content: FileManifest | null
  }>(await open(root, b64(encrypted), context(account, plan)))
  ensure(
    value.format === 'dmsg-cloud-manifest/1' &&
      value.account_id === account &&
      value.upload_id === plan.upload_id &&
      value.object_id === plan.object_id &&
      value.version_id === plan.version_id &&
      value.kind === plan.kind &&
      value.root_generation === plan.root_generation &&
      equal(canonical(value.chunks), canonical(plan.chunks)),
    'INTEGRITY_FAILED'
  )
  ensure(
    plan.kind === 'vault'
      ? value.content === null
      : value.content &&
          value.content.id === plan.object_id &&
          value.content.version === plan.version_id,
    'INTEGRITY_FAILED'
  )
  return value.content
}
