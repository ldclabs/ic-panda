// @vitest-environment node
// WebCrypto needs buffers of its own realm; jsdom's would be refused.
//
// The share page's download against a live relay, as the page runs it: the
// desktop end-to-end test (`tests/dmsg_integration`, desktop.rs) names a JSON
// file in DMSG_SHARE_LIVE with the links to open and the SHA-256 each must
// give. Without it there is nothing to open and the test is skipped.
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import {
  assemble,
  isEmptyPlaceholder,
  keyFromFragment,
  openChunk,
  openManifest,
  sha256Hex,
  unwrapKey,
  type Descriptor
} from './decrypt'

type Link = { url: string; relay: string; password?: string; sha256: string }

const input = process.env['DMSG_SHARE_LIVE']

describe.skipIf(!input)('live share links', () => {
  it('open to the bytes that were shared', { timeout: 120_000 }, async () => {
    const links = JSON.parse(readFileSync(input!, 'utf8')) as Link[]
    expect(links.length).toBeGreaterThan(0)
    for (const { url, relay, password, sha256 } of links) {
      const link = new URL(url)
      const shareId = link.pathname.split('/').pop()!
      const get = async (path: string) => {
        const response = await fetch(`${relay}/public/shares/${shareId}${path}`)
        expect(response.status, path).toBe(200)
        return response
      }
      const bytes = async (path: string) =>
        new Uint8Array(await (await get(path)).arrayBuffer())
      const descriptor = (
        (await (await get('')).json()) as { data: Descriptor }
      ).data
      expect(descriptor.status).toBe('available')
      expect(descriptor.password).toBe(!!password)
      const key = password
        ? await unwrapKey(descriptor.key_wrap!, password, shareId)
        : keyFromFragment(link.hash)
      const file = await openManifest(
        key,
        descriptor,
        descriptor.upload_id,
        await bytes('/manifest')
      )
      const chunks: Uint8Array[] = []
      for (let i = 0; i < file.chunks.length; i++)
        chunks.push(await openChunk(key, file, i, await bytes(`/chunks/${i}`)))
      if (file.size === 0)
        expect(isEmptyPlaceholder(await bytes('/chunks/0'))).toBe(true)
      expect(await sha256Hex(await assemble(file, chunks))).toBe(sha256)
    }
  })
})
