import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { canonical, decodeCanonical, hex, hash, unb64, unhex } from '../src/lib/protocol/codec'
import { ed25519 } from '../src/lib/crypto/primitives'
import { canonicalTarget } from '../src/lib/services/relay'
import { Tagged } from 'cborg'

type Value = {
  uint?: string
  int?: string
  text?: string
  bytes?: string
  array?: Value[]
  map?: [Value, Value][]
  bool?: boolean
  null?: boolean
  tag?: number
  value?: Value
}
function value(input: Value): unknown {
  if (input === null || typeof input === 'boolean') return input
  if (input.uint !== undefined)
    return BigInt(input.uint) <= BigInt(Number.MAX_SAFE_INTEGER)
      ? Number(input.uint)
      : BigInt(input.uint)
  if (input.int !== undefined) return Number(input.int)
  if (input.text !== undefined) return input.text
  if (input.bytes !== undefined)
    return input.bytes === '' ? new Uint8Array() : unhex(input.bytes)
  if (input.tag !== undefined) return new Tagged(input.tag, value(input.value!))
  if (input.array !== undefined) return input.array.map(value)
  if (input.map !== undefined) return new Map(input.map.map(([k, v]) => [value(k), value(v)]))
  if (input.bool !== undefined) return input.bool
  if (input.null !== undefined) return null
  throw new Error(JSON.stringify(input))
}
const vectors = JSON.parse(
  readFileSync(
    new URL('../../dmsg_types/tests/protocol_vectors.json', import.meta.url),
    'utf8'
  )
) as {
  name: string
  value: Value
  cbor_hex: string
  sha256_hex: string
  ed25519_public_hex: string
  signature_over_sha256_hex: string
}[]
describe('public Rust protocol vectors', () => {
  for (const vector of vectors)
    it(vector.name, () => {
      const data = value(vector.value)
      // COSE's registered tag is outside the content codec's allowed tag set.
      // Encode its validated array body and the RFC 9052 tag independently.
      const encoded =
        data instanceof Tagged && data.tag === 18
          ? Uint8Array.from([0xd2, ...canonical(data.value)])
          : canonical(data)
      expect(hex(encoded)).toBe(vector.cbor_hex)
      expect(hash(encoded)).toBe(vector.sha256_hex)
      expect(
        ed25519.verify(
          unhex(vector.signature_over_sha256_hex),
          unhex(vector.sha256_hex),
          unhex(vector.ed25519_public_hex),
          { zip215: false }
        )
      ).toBe(true)
    })
})
describe('bounded canonical decoding', () => {
  it.each(['1801', '0101', 'a2616101616102', '9fff', 'f7', 'fa3f800000', 'c001'])(
    'rejects malformed or ambiguous %s',
    (input) => {
      expect(() => decodeCanonical(unhex(input))).toThrow()
    }
  )
  it('rejects deeply nested input before decoding', () => {
    expect(() => decodeCanonical(new Uint8Array([...Array(100).fill(0x81), 1]))).toThrow()
  })
  it('keeps canonical base64url only', () => {
    expect(() => unb64('Zh')).toThrow()
    expect(() => unb64('Zg==')).toThrow()
    expect([...unb64('Zg')]).toEqual([102])
  })
  it('canonicalizes HTTP targets and rejects duplicate/encoded keys', () => {
    expect(canonicalTarget(new URL('https://relay.example/v1/sync?z=3&a=1'))).toBe(
      '/v1/sync?a=1&z=3'
    )
    expect(() => canonicalTarget(new URL('https://relay.example/v1/sync?a=1&%61=2'))).toThrow()
    expect(() => canonicalTarget(new URL('https://relay.example/v1/a%2fb'))).toThrow()
  })
})
