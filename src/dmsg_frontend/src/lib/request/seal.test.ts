// @vitest-environment node
// WebCrypto needs buffers of its own realm; jsdom's would be refused.
import { Aes256Gcm, CipherSuite, HkdfSha256 } from '@hpke/core'
import { DhkemX25519HkdfSha256 } from '@hpke/dhkem-x25519'
import { decode, encode } from 'cborg'
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { bytesHex, hexBytes, open, sha256Hex, unbase64url } from '$lib/share/decrypt'
import {
  base64url,
  descriptorAad,
  fileInfo,
  openDescriptor,
  openReceipt,
  receiptAad,
  receiptKey,
  REQUEST_FILE,
  REQUEST_FILE_FORMAT,
  sealChunk,
  sealManifest,
  sealReceipt,
  sealWithNonce,
  uploadToken,
  type RequestFile
} from './seal'

const path = resolve('../../tests/vectors/client/request-v1.json')
const vectors = JSON.parse(readFileSync(path, 'utf8'))
const pageKey = hexBytes(vectors.page_key_hex)
const { account, request_id: requestId } = vectors
const suite = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm()
})

describe('file requests follow the request vector', () => {
  it('derives the upload token and the receipt key', async () => {
    const token = uploadToken(pageKey, account, requestId)
    expect(token).toBe(vectors.upload_token.token)
    expect(await sha256Hex(unbase64url(token))).toBe(vectors.upload_token.hash)
    expect(bytesHex(receiptKey(pageKey, account, requestId))).toBe(
      vectors.receipt.key_hex
    )
    expect(base64url(pageKey)).toBe(vectors.page_key_fragment)
  })

  it('opens the descriptor and refuses one for another link', async () => {
    expect(bytesHex(descriptorAad(account, requestId))).toBe(
      vectors.descriptor.aad_hex
    )
    const descriptor = await openDescriptor(
      pageKey,
      account,
      requestId,
      vectors.descriptor.sealed
    )
    expect(descriptor.hpke_pub).toBe(vectors.hpke_pub)
    expect(bytesHex(encode(descriptor))).toBe(vectors.descriptor.plaintext_hex)
    await expect(
      openDescriptor(pageKey, account, '7b'.repeat(32), vectors.descriptor.sealed)
    ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
  })

  it('seals a descriptor, a receipt and a chunk byte for byte', async () => {
    const sealed = await sealWithNonce(
      pageKey,
      hexBytes(vectors.descriptor.plaintext_hex),
      descriptorAad(account, requestId),
      hexBytes(vectors.descriptor.nonce_hex)
    )
    expect(base64url(sealed)).toBe(vectors.descriptor.sealed)
    const r = vectors.receipt
    const sealedReceipt = await sealWithNonce(
      hexBytes(r.key_hex),
      hexBytes(r.plaintext_hex),
      receiptAad(requestId, r.upload_id),
      hexBytes(r.nonce_hex)
    )
    expect(base64url(sealedReceipt)).toBe(r.sealed)
    const file = vectors.file
    const chunk = file.chunks[0]
    const sealedChunk = await sealChunk(
      hexBytes(file.file_key_hex),
      file.object_id,
      file.version_id,
      0,
      1,
      hexBytes(chunk.plaintext_hex)
    )
    expect(bytesHex(sealedChunk)).toBe(chunk.sealed_hex)
    expect(await sha256Hex(sealedChunk)).toBe(chunk.digest)
  })

  it('opens the receipt it seals, and never under another upload', async () => {
    const key = hexBytes(vectors.receipt.key_hex)
    const opened = await openReceipt(
      key,
      requestId,
      vectors.receipt.upload_id,
      vectors.receipt.sealed
    )
    expect(opened.name).toBe('contract.pdf')
    expect(bytesHex(encode(opened))).toBe(vectors.receipt.plaintext_hex)
    const sealed = await sealReceipt(key, requestId, 'ab'.repeat(32), opened)
    expect(await openReceipt(key, requestId, 'ab'.repeat(32), sealed)).toEqual(
      opened
    )
    await expect(
      openReceipt(key, requestId, 'cd'.repeat(32), sealed)
    ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
  })

  it('opens a manifest the Rust client sealed, with the same encoding', async () => {
    const file = vectors.file
    const pair = await suite.kem.deriveKeyPair(
      hexBytes(vectors.hpke_seed_hex).slice().buffer
    )
    expect(
      base64url(new Uint8Array(await suite.kem.serializePublicKey(pair.publicKey)))
    ).toBe(vectors.hpke_pub)
    const info = fileInfo(
      account,
      requestId,
      file.upload_id,
      file.object_id,
      file.version_id
    )
    expect(bytesHex(info)).toBe(file.info_hex)
    const manifest = hexBytes(file.manifest_hex)
    const plain = await suite.open(
      {
        recipientKey: pair.privateKey,
        enc: manifest.slice(0, 32).buffer,
        info: info.slice().buffer
      },
      manifest.slice(32).buffer,
      info.slice().buffer
    )
    expect(bytesHex(new Uint8Array(plain))).toBe(file.plaintext_hex)
    const opened = decode(new Uint8Array(plain)) as RequestFile
    expect(bytesHex(encode(opened))).toBe(file.plaintext_hex)
    expect(opened.sender?.name).toBe('Ada')
    expect(
      await open(
        hexBytes(file.file_key_hex),
        hexBytes(file.chunks[0].sealed_hex),
        hexBytes(file.chunks[0].aad_hex)
      )
    ).toEqual(hexBytes(file.chunks[0].plaintext_hex))
  })

  // The Rust client opens this: `opens_a_manifest_the_browser_sealed`.
  it('seals a manifest the Rust client opens', async () => {
    const plaintext = new TextEncoder().encode('Sealed in a browser. 浏览器加密。')
    const key = crypto.getRandomValues(new Uint8Array(32))
    const [id, version, upload] = ['81', '82', '83'].map((b) => b.repeat(32))
    const chunk = await sealChunk(key, id!, version!, 0, 1, plaintext)
    const digest = await sha256Hex(chunk)
    const file: RequestFile = {
      format: REQUEST_FILE_FORMAT,
      request_id: requestId,
      upload_id: upload!,
      file: {
        id: id!,
        version: version!,
        name: '报告.txt',
        mime: 'text/plain',
        size: plaintext.length,
        sha256: await sha256Hex(plaintext),
        chunks: [
          { id: `${id}:${version}:0`, digest, size: plaintext.length }
        ],
        key: base64url(key)
      },
      sender: { name: 'Lin', note: '' },
      replaces: null,
      sent_at: 1_760_000_000_000
    }
    const manifest = await sealManifest(
      unbase64url(vectors.hpke_pub),
      account,
      file
    )
    const pair = await suite.kem.deriveKeyPair(
      hexBytes(vectors.hpke_seed_hex).slice().buffer
    )
    const info = fileInfo(account, requestId, upload!, id!, version!)
    const plain = await suite.open(
      {
        recipientKey: pair.privateKey,
        enc: manifest.slice(0, 32).buffer,
        info: info.slice().buffer
      },
      manifest.slice(32).buffer,
      info.slice().buffer
    )
    expect(decode(new Uint8Array(plain))).toEqual(file)
    if (process.env.DMSG_WRITE_CLIENT_VECTORS) {
      vectors.browser = {
        generator: 'src/dmsg_frontend/src/lib/request/seal.test.ts',
        name: file.file.name,
        sender: file.sender?.name,
        plaintext_hex: bytesHex(plaintext),
        chunk_hex: bytesHex(chunk),
        manifest_hex: bytesHex(manifest),
        plan: {
          upload_id: upload,
          object_id: id,
          version_id: version,
          root_generation: 0,
          epoch: 0,
          control_head: null,
          kind: REQUEST_FILE,
          chunks: [{ digest, size: chunk.length }],
          manifest_digest: await sha256Hex(manifest),
          manifest_size: manifest.length,
          expires_at: 1_760_000_300_000
        }
      }
      writeFileSync(path, JSON.stringify(vectors, null, 2) + '\n')
    }
  })
})
