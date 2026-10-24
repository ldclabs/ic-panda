// @vitest-environment node
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { attachmentName, parseSecretPackage } from './package'
import { unbase64url } from './decrypt'
const vector = JSON.parse(readFileSync(resolve('../../tests/vectors/client/secret-package-v1.json'), 'utf8'))
const bytes = unbase64url(vector.bytes_base64url)
const encode = (manifest: unknown) => { const json = new TextEncoder().encode(JSON.stringify(manifest)); const out = new Uint8Array(4 + json.length); new DataView(out.buffer).setUint32(0, json.length); out.set(json, 4); return out }
describe('secret package', () => {
  it('checks the same Rust vector and preserves untrusted text literally', async () => {
    const pack = await parseSecretPackage(bytes)
    expect(pack.item).toEqual(vector.manifest.item)
    expect(new TextDecoder().decode(pack.attachments[0]!.bytes)).toBe('fixture')
    expect(attachmentName(pack.attachments[0]!.name)).toBe('fixture.txt')
  })
  it('rejects truncation, trailing bytes, corrupt attachments and unknown format', async () => {
    for (const n of [0, 3, 4, bytes.length - 1]) await expect(parseSecretPackage(bytes.slice(0, n))).rejects.toThrow()
    const trailing = new Uint8Array(bytes.length + 1); trailing.set(bytes); await expect(parseSecretPackage(trailing)).rejects.toThrow()
    const bad = bytes.slice(); bad[bad.length - 1] = bad[bad.length - 1]! ^ 1; await expect(parseSecretPackage(bad)).rejects.toThrow()
    await expect(parseSecretPackage(encode({ ...vector.manifest, format: 'dmsg-secret-package/2' }))).rejects.toThrow()
  })
  it('rejects duplicate JSON keys, unsafe lengths and unexpected source identifiers', async () => {
    const json = new TextEncoder().encode('{"format":"dmsg-secret-package/1","format":"dmsg-secret-package/1"}')
    const duplicate = new Uint8Array(4 + json.length); new DataView(duplicate.buffer).setUint32(0, json.length); duplicate.set(json, 4)
    await expect(parseSecretPackage(duplicate)).rejects.toThrow()
    await expect(parseSecretPackage(encode({ ...vector.manifest, source_account: 'not-allowed' }))).rejects.toThrow()
    await expect(parseSecretPackage(encode({ ...vector.manifest, attachments: [{ ...vector.manifest.attachments[0], size: Number.MAX_SAFE_INTEGER }] }))).rejects.toThrow()
  })
})
