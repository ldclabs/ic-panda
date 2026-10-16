// @vitest-environment node
// WebCrypto needs buffers of its own realm; jsdom's would be refused.
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  assemble,
  bytesHex,
  chunkAad,
  chunkNonce,
  hexBytes,
  keyFromFragment,
  keyWrapAad,
  manifestAad,
  open,
  openChunk,
  openManifest,
  sha256Hex,
  unbase64url,
  unwrapKey,
  type Descriptor
} from './decrypt'

// The language-independent vectors the Rust client checks too.
const vectors = JSON.parse(
  readFileSync(resolve('../../tests/vectors/client/content-v1.json'), 'utf8')
)

describe('share links follow the content vectors', () => {
  it('opens the chunk rules with the same AAD and nonce', async () => {
    for (const rule of vectors.chunk_rules) {
      const aad = chunkAad(
        rule.file_id,
        rule.version,
        rule.index,
        rule.count,
        rule.plaintext_length
      )
      expect(bytesHex(aad)).toBe(rule.aad.hex)
      expect(bytesHex(chunkNonce(rule.index))).toBe(rule.nonce_hex)
      const plain = await open(
        hexBytes(rule.key_hex),
        rule.sealed,
        aad,
        chunkNonce(rule.index)
      )
      expect(bytesHex(plain)).toBe(rule.plaintext_hex)
    }
  })

  it('opens a shared file: the password wrap, the manifest and the chunks', async () => {
    for (const share of vectors.shares) {
      const key = hexBytes(share.key_hex)
      expect(bytesHex(keyWrapAad(share.share_id))).toBe(share.key_wrap_aad)
      expect(
        bytesHex(
          await unwrapKey(share.key_wrap, share.password, share.share_id)
        )
      ).toBe(share.key_hex)
      await expect(
        unwrapKey(share.key_wrap, 'wrong', share.share_id)
      ).rejects.toMatchObject({
        code: 'WRONG_PASSWORD'
      })
      expect(
        bytesHex(
          manifestAad(
            share.share_id,
            share.upload_id,
            share.share_id,
            share.version_id
          )
        )
      ).toBe(share.manifest_aad)
      const sealed = unbase64url(share.manifest)
      const descriptor: Descriptor = {
        status: 'available',
        share_id: share.share_id,
        upload_id: share.upload_id,
        object_id: share.share_id,
        version_id: share.version_id,
        chunks: share.chunks.map((c: any) => ({ digest: c.digest, size: 0 })),
        manifest: { digest: share.manifest_digest, size: sealed.length },
        expires_at: 0,
        max_downloads: 0,
        downloads: 0,
        password: true,
        key_wrap: share.key_wrap,
        created_at: 0
      }
      const file = await openManifest(key, descriptor, share.upload_id, sealed)
      expect(file.name).toBe('small.txt')
      expect(file.chunks).toHaveLength(share.chunks.length)
      const chunks = []
      for (const chunk of share.chunks) {
        const plain = await openChunk(
          key,
          file,
          chunk.index,
          unbase64url(chunk.ciphertext)
        )
        expect(bytesHex(plain)).toBe(chunk.plaintext_hex)
        chunks.push(plain)
      }
      const whole = await assemble(file, chunks)
      expect(await sha256Hex(whole)).toBe(file.sha256)
      // A manifest for another share, a changed chunk and a wrong key fail.
      await expect(
        openManifest(
          key,
          { ...descriptor, object_id: share.version_id },
          share.upload_id,
          sealed
        )
      ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
      const damaged = unbase64url(share.chunks[0]!.ciphertext)
      damaged[damaged.length - 1] = damaged[damaged.length - 1]! ^ 1
      await expect(openChunk(key, file, 0, damaged)).rejects.toMatchObject({
        code: 'INTEGRITY_FAILED'
      })
      await expect(
        openChunk(
          hexBytes('00'.repeat(32)),
          file,
          0,
          unbase64url(share.chunks[0]!.ciphertext)
        )
      ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
      const fragment = `#${btoa(String.fromCharCode(...key))
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=+$/, '')}`
      expect(bytesHex(keyFromFragment(fragment))).toBe(share.key_hex)
    }
  })
})
