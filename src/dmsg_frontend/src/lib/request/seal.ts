// A file request's page in the browser, by the rules of
// tests/vectors/client/request-v1.json. The link's fragment gives the power to
// encrypt, never to decrypt: it opens the descriptor, which holds the link's
// HPKE public key, and derives the upload token and the key of this browser's
// own records. Each file has a random key; its chunks follow the file chunk
// rules, its manifest is sealed to the public key. Nothing here talks to a
// server.

import { Aes256Gcm, CipherSuite, HkdfSha256 } from '@hpke/core'
import { DhkemX25519HkdfSha256 } from '@hpke/dhkem-x25519'
import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { decode, encode } from 'cborg'
import {
  bytesHex,
  chunkAad,
  chunkNonce,
  CHUNK,
  open,
  ShareError,
  unbase64url
} from '$lib/share/decrypt'

export { CHUNK }

/** The plaintext format of a descriptor. */
export const REQUEST_FORMAT = 'dmsg-request/1'
/** The plaintext format of a received file's manifest. */
export const REQUEST_FILE_FORMAT = 'dmsg-request-file/1'
/** The upload kind of a received file. */
export const REQUEST_FILE = 'request_file'
/** Largest name and note a sender may write, together, in bytes. */
export const MAX_SENDER = 1024
/** Largest sealed receipt the relay keeps, in bytes. */
export const MAX_RECEIPT = 2048

const SALT = new Uint8Array(32)
/** The fixed head of a COSE_Encrypt0: array of 3, `{1: 3}`, `{5: nonce}`. */
const HEAD = new Uint8Array([0x83, 0x43, 0xa1, 0x01, 0x03, 0xa1, 0x05, 0x4c])
const PROTECTED = new Uint8Array([0xa1, 0x01, 0x03])

const suite = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm()
})

export type RequestDescriptor = {
  format: string
  account: string
  request_id: string
  hpke_pub: string
  title: string
  instructions: string
}
export type RequestSender = { name: string; note: string }
export type FileEntry = {
  id: string
  version: string
  name: string
  mime: string
  size: number
  sha256: string
  chunks: { id: string; digest: string; size: number }[]
  key: string
}
export type RequestFile = {
  format: string
  request_id: string
  upload_id: string
  file: FileEntry
  sender: RequestSender | null
  replaces: string | null
  sent_at: number
}
/** What this browser keeps of an upload, sealed so that only the link opens it. */
export type Receipt = {
  name: string
  size: number
  mime: string
  sha256: string
  sent_at: number
  replaces: string | null
}

/** Canonical base64url: no padding. */
export function base64url(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i += 0x8000)
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

/** `D(K, C)`: HKDF-SHA-256 with a zero salt and `info = CBOR(C)`. */
function derive(key: Uint8Array, context: unknown[]): Uint8Array {
  return hkdf(sha256, key, SALT, encode(context), 32)
}

/** The token that shows the relay this browser has the whole link. */
export function uploadToken(
  pageKey: Uint8Array,
  account: string,
  requestId: string
): string {
  return base64url(
    derive(pageKey, ['dmsg/request-upload-token/1', account, requestId])
  )
}
/** The key of this browser's records of this link. */
export function receiptKey(
  pageKey: Uint8Array,
  account: string,
  requestId: string
): Uint8Array {
  return derive(pageKey, ['dmsg/request-receipt/1', account, requestId])
}

export function descriptorAad(account: string, requestId: string): Uint8Array {
  return encode(['dmsg/request-descriptor/1', account, requestId])
}
export function receiptAad(requestId: string, uploadId: string): Uint8Array {
  return encode(['dmsg/request-receipt/1', requestId, uploadId])
}
export function fileInfo(
  account: string,
  requestId: string,
  uploadId: string,
  objectId: string,
  versionId: string
): Uint8Array {
  return encode([
    'dmsg/request-file/1',
    account,
    requestId,
    uploadId,
    objectId,
    versionId
  ])
}

/** Open the descriptor and check it names this link. */
export async function openDescriptor(
  pageKey: Uint8Array,
  account: string,
  requestId: string,
  sealed: string
): Promise<RequestDescriptor> {
  const plain = await open(pageKey, sealed, descriptorAad(account, requestId))
  let descriptor: RequestDescriptor
  try {
    descriptor = decode(plain) as RequestDescriptor
  } catch {
    throw new ShareError('INTEGRITY_FAILED', 'descriptor')
  }
  if (
    descriptor.format !== REQUEST_FORMAT ||
    descriptor.account !== account ||
    descriptor.request_id !== requestId ||
    unbase64url(descriptor.hpke_pub).length !== 32
  )
    throw new ShareError('INTEGRITY_FAILED', 'descriptor')
  return descriptor
}

function bstrHeader(length: number): Uint8Array {
  if (length < 24) return new Uint8Array([0x40 | length])
  if (length < 0x100) return new Uint8Array([0x58, length])
  if (length < 0x10000)
    return new Uint8Array([0x59, length >> 8, length & 0xff])
  return new Uint8Array([
    0x5a,
    (length >>> 24) & 0xff,
    (length >>> 16) & 0xff,
    (length >>> 8) & 0xff,
    length & 0xff
  ])
}

/** Untagged COSE_Encrypt0 under `key` and `nonce`, as the Rust client writes it. */
export async function sealWithNonce(
  key: Uint8Array,
  plain: Uint8Array,
  aad: Uint8Array,
  nonce: Uint8Array
): Promise<Uint8Array> {
  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    key as BufferSource,
    'AES-GCM',
    false,
    ['encrypt']
  )
  const ciphertext = new Uint8Array(
    await crypto.subtle.encrypt(
      {
        name: 'AES-GCM',
        iv: nonce as BufferSource,
        additionalData: encode(['Encrypt0', PROTECTED, aad]) as BufferSource
      },
      cryptoKey,
      plain as BufferSource
    )
  )
  const length = bstrHeader(ciphertext.length)
  const out = new Uint8Array(
    HEAD.length + nonce.length + length.length + ciphertext.length
  )
  let at = 0
  for (const part of [HEAD, nonce, length, ciphertext]) {
    out.set(part, at)
    at += part.length
  }
  return out
}

/** Seal chunk `index` of `count`; the nonce is fixed, so the same input seals the same bytes. */
export function sealChunk(
  key: Uint8Array,
  fileId: string,
  version: string,
  index: number,
  count: number,
  plain: Uint8Array
): Promise<Uint8Array> {
  return sealWithNonce(
    key,
    plain,
    chunkAad(fileId, version, index, count, plain.length),
    chunkNonce(index)
  )
}

/** A manifest as the relay stores it: `enc || ciphertext` to the link's public key. */
export async function sealManifest(
  hpkePub: Uint8Array,
  account: string,
  file: RequestFile
): Promise<Uint8Array> {
  const info = fileInfo(
    account,
    file.request_id,
    file.upload_id,
    file.file.id,
    file.file.version
  )
  const recipientPublicKey = await suite.kem.deserializePublicKey(
    hpkePub.slice().buffer
  )
  const sealed = await suite.seal(
    { recipientPublicKey, info: info.slice().buffer },
    encode(file).slice().buffer,
    info.slice().buffer
  )
  const out = new Uint8Array(sealed.enc.byteLength + sealed.ct.byteLength)
  out.set(new Uint8Array(sealed.enc), 0)
  out.set(new Uint8Array(sealed.ct), sealed.enc.byteLength)
  return out
}

/** What this browser keeps of an upload, sealed under the link's receipt key, base64url. */
export async function sealReceipt(
  key: Uint8Array,
  requestId: string,
  uploadId: string,
  receipt: Receipt
): Promise<string> {
  const nonce = crypto.getRandomValues(new Uint8Array(12))
  return base64url(
    await sealWithNonce(
      key,
      encode(receipt),
      receiptAad(requestId, uploadId),
      nonce
    )
  )
}

export async function openReceipt(
  key: Uint8Array,
  requestId: string,
  uploadId: string,
  sealed: string
): Promise<Receipt> {
  const plain = await open(key, sealed, receiptAad(requestId, uploadId))
  try {
    return decode(plain) as Receipt
  } catch {
    throw new ShareError('INTEGRITY_FAILED', 'receipt')
  }
}

/** A random 32-byte ID, lowercase hex, never all zero. */
export function randomId(): string {
  for (;;) {
    const id = crypto.getRandomValues(new Uint8Array(32))
    if (id.some((b) => b !== 0)) return bytesHex(id)
  }
}
