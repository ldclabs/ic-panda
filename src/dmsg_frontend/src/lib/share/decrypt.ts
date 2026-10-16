// A share link's file in the browser, by the rules of
// tests/vectors/client/content-v1.json: the key comes from the link's
// fragment or from the password wrap; the manifest and every chunk are
// untagged COSE_Encrypt0 under AES-256-GCM, each with its own AAD, and the
// whole is checked against the manifest's SHA-256 before it is handed over.
// Nothing here talks to a server: the page fetches, this module opens.

import { argon2id } from '@noble/hashes/argon2.js'
import { decode, encode } from 'cborg'

/** Plaintext bytes per chunk. */
export const CHUNK = 1024 * 1024
/** The plaintext format of a share manifest. */
export const SHARE_FORMAT = 'dmsg-share-file/1'
/** Argon2id of the password key: 19 MiB, two passes, one lane, the share ID as the salt. */
export const PASSWORD = { m: 19_456, t: 2, p: 1 } as const
/** The sealed placeholder of an empty file: `{"format": "dmsg-cloud-empty-file/1"}` as CBOR. */
const EMPTY_FILE = hexBytes(
  'a166666f726d617477646d73672d636c6f75642d656d7074792d66696c652f31'
)
/** The canonical protected header `{1: 3}`: alg A256GCM. */
const PROTECTED = new Uint8Array([0xa1, 0x01, 0x03])

export type ShareChunk = { id: string; digest: string; size: number }
export type ShareFile = {
  id: string
  version: string
  name: string
  mime: string
  size: number
  sha256: string
  chunks: ShareChunk[]
  key: string
}
export type ShareManifest = {
  format: string
  share_id: string
  file: ShareFile
}

export class ShareError extends Error {
  constructor(
    readonly code: 'INTEGRITY_FAILED' | 'WRONG_PASSWORD' | 'UNSUPPORTED',
    message?: string
  ) {
    super(message ?? code)
    this.name = 'ShareError'
  }
}

export function hexBytes(hex: string): Uint8Array {
  if (hex.length % 2 || /[^0-9a-f]/.test(hex))
    throw new ShareError('INTEGRITY_FAILED', 'hex')
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++)
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16)
  return out
}
export function bytesHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}
/** Canonical base64url: no padding. */
export function unbase64url(text: string): Uint8Array {
  if (/[^A-Za-z0-9_-]/.test(text))
    throw new ShareError('INTEGRITY_FAILED', 'base64url')
  const binary = atob(text.replace(/-/g, '+').replace(/_/g, '/'))
  return Uint8Array.from(binary, (c) => c.charCodeAt(0))
}
export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  return bytesHex(
    new Uint8Array(await crypto.subtle.digest('SHA-256', bytes as BufferSource))
  )
}

/** The 32-byte key the link's fragment carries. */
export function keyFromFragment(fragment: string): Uint8Array {
  const key = unbase64url(fragment.replace(/^#/, ''))
  if (key.length !== 32) throw new ShareError('INTEGRITY_FAILED', 'key')
  return key
}

/** The AAD of the share manifest. */
export function manifestAad(
  shareId: string,
  uploadId: string,
  objectId: string,
  versionId: string
): Uint8Array {
  return encode([
    'dmsg/share-manifest/1',
    shareId,
    uploadId,
    objectId,
    versionId
  ])
}
/** The AAD of chunk `index` of `count`, `length` plaintext bytes long. */
export function chunkAad(
  fileId: string,
  version: string,
  index: number,
  count: number,
  length: number
): Uint8Array {
  return encode(['dmsg/file-chunk/1', fileId, version, index, count, length])
}
/** The AAD of the password wrap of the share key. */
export function keyWrapAad(shareId: string): Uint8Array {
  return encode(['dmsg/share-key/1', shareId])
}
/** `"dMsg" || u64_be(index)`. */
export function chunkNonce(index: number): Uint8Array {
  const nonce = new Uint8Array(12)
  nonce.set([0x64, 0x4d, 0x73, 0x67])
  new DataView(nonce.buffer).setBigUint64(4, BigInt(index))
  return nonce
}

/** `Enc_structure = ["Encrypt0", protected, external_aad]`. */
function encStructure(aad: Uint8Array): Uint8Array {
  return encode(['Encrypt0', PROTECTED, aad])
}

/** Open an untagged COSE_Encrypt0 (base64url or bytes) under `key` and `aad`. */
export async function open(
  key: Uint8Array,
  sealed: string | Uint8Array,
  aad: Uint8Array,
  nonce?: Uint8Array
): Promise<Uint8Array> {
  const bytes = typeof sealed === 'string' ? unbase64url(sealed) : sealed
  let items: unknown
  try {
    items = decode(bytes, { useMaps: true })
  } catch {
    throw new ShareError('INTEGRITY_FAILED', 'cbor')
  }
  if (!Array.isArray(items) || items.length !== 3)
    throw new ShareError('INTEGRITY_FAILED', 'cose')
  const [header, unprotected, ciphertext] = items as [
    Uint8Array,
    Map<number, Uint8Array>,
    Uint8Array
  ]
  if (
    !(header instanceof Uint8Array) ||
    bytesHex(header) !== bytesHex(PROTECTED) ||
    !(unprotected instanceof Map) ||
    unprotected.size !== 1 ||
    !(unprotected.get(5) instanceof Uint8Array) ||
    unprotected.get(5)!.length !== 12 ||
    !(ciphertext instanceof Uint8Array) ||
    ciphertext.length < 16
  )
    throw new ShareError('UNSUPPORTED', 'cose header')
  const iv = unprotected.get(5)!
  if (nonce && bytesHex(iv) !== bytesHex(nonce))
    throw new ShareError('INTEGRITY_FAILED', 'nonce')
  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    key as BufferSource,
    'AES-GCM',
    false,
    ['decrypt']
  )
  try {
    return new Uint8Array(
      await crypto.subtle.decrypt(
        {
          name: 'AES-GCM',
          iv: iv as BufferSource,
          additionalData: encStructure(aad) as BufferSource
        },
        cryptoKey,
        ciphertext as BufferSource
      )
    )
  } catch {
    throw new ShareError('INTEGRITY_FAILED', 'aead')
  }
}

/** The share key out of its password wrap; a wrong password fails the AEAD. */
export async function unwrapKey(
  keyWrap: string,
  password: string,
  shareId: string
): Promise<Uint8Array> {
  if (!password) throw new ShareError('WRONG_PASSWORD')
  const derived = argon2id(
    new TextEncoder().encode(password),
    hexBytes(shareId),
    {
      ...PASSWORD,
      dkLen: 32
    }
  )
  try {
    const key = await open(derived, keyWrap, keyWrapAad(shareId))
    if (key.length !== 32) throw new ShareError('INTEGRITY_FAILED', 'key')
    return key
  } catch (error) {
    if (error instanceof ShareError && error.code === 'INTEGRITY_FAILED')
      throw new ShareError('WRONG_PASSWORD')
    throw error
  } finally {
    derived.fill(0)
  }
}

/** What the relay says of a share, as `GET /public/shares/:share` returns it. */
export type Descriptor = {
  status: 'available' | 'expired' | 'revoked' | 'exhausted'
  share_id: string
  upload_id: string
  object_id: string
  version_id: string
  chunks: { digest: string; size: number }[]
  manifest: { digest: string; size: number }
  expires_at: number
  max_downloads: number
  downloads: number
  password: boolean
  key_wrap: string | null
  created_at: number
}

/** Open the manifest and check it names the share the descriptor describes. */
export async function openManifest(
  key: Uint8Array,
  descriptor: Descriptor,
  uploadId: string,
  sealed: Uint8Array
): Promise<ShareFile> {
  if (
    sealed.length !== descriptor.manifest.size ||
    (await sha256Hex(sealed)) !== descriptor.manifest.digest
  )
    throw new ShareError('INTEGRITY_FAILED', 'manifest digest')
  const plain = await open(
    key,
    sealed,
    manifestAad(
      descriptor.share_id,
      uploadId,
      descriptor.object_id,
      descriptor.version_id
    )
  )
  let manifest: ShareManifest
  try {
    manifest = decode(plain) as ShareManifest
  } catch {
    throw new ShareError('INTEGRITY_FAILED', 'manifest')
  }
  const file = manifest.file
  const expectedChunks = file.size === 0 ? 0 : Math.ceil(file.size / CHUNK)
  if (
    manifest.format !== SHARE_FORMAT ||
    manifest.share_id !== descriptor.share_id ||
    file.id !== descriptor.object_id ||
    file.version !== descriptor.version_id ||
    file.chunks.length !== expectedChunks ||
    file.chunks.some(
      (c, i) =>
        c.id !== `${file.id}:${file.version}:${i}` ||
        c.size !== Math.min(file.size - i * CHUNK, CHUNK) ||
        c.digest !== descriptor.chunks[i]?.digest
    ) ||
    (file.size === 0 && descriptor.chunks.length !== 1)
  )
    throw new ShareError('INTEGRITY_FAILED', 'manifest')
  return file
}

/** Open chunk `index` as fetched, checking its digest, size and nonce. */
export async function openChunk(
  key: Uint8Array,
  file: ShareFile,
  index: number,
  sealed: Uint8Array
): Promise<Uint8Array> {
  const chunk = file.chunks[index]
  if (!chunk || (await sha256Hex(sealed)) !== chunk.digest)
    throw new ShareError('INTEGRITY_FAILED', 'chunk digest')
  const plain = await open(
    key,
    sealed,
    chunkAad(file.id, file.version, index, file.chunks.length, chunk.size),
    chunkNonce(index)
  )
  if (plain.length !== chunk.size)
    throw new ShareError('INTEGRITY_FAILED', 'chunk size')
  return plain
}

/** The whole file: every chunk in order, checked against the manifest's SHA-256. */
export async function assemble(
  file: ShareFile,
  chunks: Uint8Array[]
): Promise<Uint8Array> {
  const whole = new Uint8Array(file.size)
  let at = 0
  for (const chunk of chunks) {
    whole.set(chunk, at)
    at += chunk.length
  }
  if (at !== file.size || (await sha256Hex(whole)) !== file.sha256)
    throw new ShareError('INTEGRITY_FAILED', 'file digest')
  return whole
}

/** Whether a fetched slot of an empty file is the placeholder. */
export function isEmptyPlaceholder(bytes: Uint8Array): boolean {
  return bytesHex(bytes) === bytesHex(EMPTY_FILE)
}
