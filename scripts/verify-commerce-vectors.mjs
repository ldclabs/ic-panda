// Independent RFC 8949 encoder for the typed fixture subset. No Rust/CBOR
// package is used here, so these vectors catch cross-language encoding drift.
import { readFileSync } from 'node:fs'
import { createHash, createPublicKey, verify } from 'node:crypto'
import assert from 'node:assert/strict'

const input = process.argv[2] ?? new URL('../src/dmsg_types/tests/commerce_vectors.json', import.meta.url)
const vectors = JSON.parse(readFileSync(input, 'utf8'))
const fixture = (name) => {
  const vector = vectors.find((v) => v.name === name)
  assert(vector, `missing ${name}`)
  return vector.value
}
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
  if ('int' in value) return head(1, -1n - BigInt(value.int))
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


const uint = v => v.tag === 2 ? BigInt('0x' + v.value.bytes) : BigInt(v.uint)
const field = (v, name) => v.map.find(([k]) => k.text === name)[1]
for (const v of vectors) {
  if (/^panda_[0-9]/.test(v.name)) {
    const [price, num, den, decimals, result] = v.value.array.map(uint)
    const n = price * num * 10n ** decimals, d = 100n * den
    assert.equal((n + d - 1n) / d, result)
  } else if (v.name.startsWith('delivery_fee_')) {
    const [net, policy, result] = v.value.array
    const fee = (uint(net) * uint(field(policy, 'rate_bps')) + 9999n) / 10000n
    const minimum = uint(field(policy, 'minimum_atomic'))
    assert.equal(uint(result), fee > minimum ? fee : minimum)
  } else if (v.name.startsWith('month_')) {
    const [, start, end, segments, result] = v.value.array
    let weighted = 0n
    for (const segment of segments.array) weighted += (uint(field(segment, 'end_ms')) - uint(field(segment, 'start_ms'))) * uint(field(segment, 'monthly_units'))
    assert.equal(weighted / (uint(end) - uint(start)), uint(result))
  } else if (v.name.startsWith('year_')) {
    const [start, expected] = v.value.array.map(v => Number(uint(v)))
    const d = new Date(start), month = d.getUTCMonth()
    d.setUTCFullYear(d.getUTCFullYear() + 1)
    if (d.getUTCMonth() !== month) d.setUTCDate(0)
    assert.equal(d.getTime(), expected)
  }
}
console.log('Verified commerce thresholds, fee rounding, calendar terms, and monthly proration with independent BigInt arithmetic.')

const quote = fixture('delivery_quote_v2').array[2]
const receipt = fixture('delivery_receipt_v2').array[2]
assert.equal(uint(field(quote, 'amount')), uint(field(quote, 'recipient_net')) + uint(field(quote, 'service_fee')) + uint(field(quote, 'fee_reserve')))
assert.equal(uint(field(receipt, 'protocol')), 2n)
assert.equal(field(receipt, 'quote_digest').bytes, createHash('sha256').update(encode(fixture('delivery_quote_v2'))).digest('hex'))
const grant = fixture('execution_grant_v3').array[2]
assert.deepEqual(field(field(grant, 'commerce'), 'reservation_id'), field(grant, 'request_id'))
console.log('Verified delivery v2 quote/receipt binding and execution grant v3 reservation identity.')
