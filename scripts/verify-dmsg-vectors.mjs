// Independent RFC 8949 encoder for the typed fixture subset. No Rust/CBOR
// package is used here, so these vectors catch cross-language encoding drift.
import { readFileSync } from 'node:fs'
import { createHash, createPublicKey, verify } from 'node:crypto'
import assert from 'node:assert/strict'

const input = process.argv[2] ?? new URL('../src/dmsg_types/tests/protocol_vectors.json', import.meta.url)
const vectors = JSON.parse(readFileSync(input, 'utf8'))
function head(major, n) {
  n = BigInt(n)
  if (n < 24n) return Buffer.from([major << 5 | Number(n)])
  const size = n <= 255n ? 1 : n <= 65535n ? 2 : n <= 4294967295n ? 4 : 8
  const b = Buffer.alloc(1 + size)
  b[0] = major << 5 | ({ 1: 24, 2: 25, 4: 26, 8: 27 })[size]
  for (let i = size; i; i--) { b[i] = Number(n & 255n); n >>= 8n }
  assert.equal(n, 0n)
  return b
}
function encode(value) {
  if (value === null) return Buffer.from([0xf6])
  if (typeof value === 'boolean') return Buffer.from([value ? 0xf5 : 0xf4])
  if ('uint' in value) return head(0, value.uint)
  if ('tag' in value) return Buffer.concat([head(6, value.tag), encode(value.value)])
  if ('bytes' in value || 'text' in value) {
    const bytes = 'bytes' in value ? Buffer.from(value.bytes, 'hex') : Buffer.from(value.text, 'utf8')
    return Buffer.concat([head('bytes' in value ? 2 : 3, bytes.length), bytes])
  }
  if ('array' in value) return Buffer.concat([head(4, value.array.length), ...value.array.map(encode)])
  const pairs = value.map.map(([k, v]) => [encode(k), encode(v)])
  pairs.sort((a, b) => Buffer.compare(a[0], b[0]))
  return Buffer.concat([head(5, pairs.length), ...pairs.flat()])
}
for (const vector of vectors) {
  const bytes = encode(vector.value)
  assert.equal(bytes.toString('hex'), vector.cbor_hex, `${vector.name}: CBOR`)
  const hash = createHash('sha256').update(bytes).digest()
  assert.equal(hash.toString('hex'), vector.sha256_hex, `${vector.name}: digest`)
  const publicKey = createPublicKey({
    key: Buffer.from(`302a300506032b6570032100${vector.ed25519_public_hex}`, 'hex'),
    format: 'der', type: 'spki'
  })
  assert(verify(null, hash, publicKey, Buffer.from(vector.signature_over_sha256_hex, 'hex')), `${vector.name}: signature`)
}
console.log(`Verified ${vectors.length} Rust/JavaScript CBOR, SHA-256 and Ed25519 vectors.`)
