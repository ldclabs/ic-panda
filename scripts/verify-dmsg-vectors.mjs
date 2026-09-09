// Independent RFC 8949 encoder for the typed fixture subset. No Rust/CBOR
// package is used here, so these vectors catch cross-language encoding drift.
import { readFileSync } from 'node:fs'
import { createHash, createPublicKey, verify } from 'node:crypto'
import assert from 'node:assert/strict'

const input = process.argv[2] ?? new URL('../src/dmsg_types/tests/protocol_vectors.json', import.meta.url)
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

// Assemble the profile from registered headers independently of the Rust encoder.
const B = bytes => ({bytes}), T = text => ({text}), U = n => n < 0 ? {int:String(n)} : {uint:String(n)}
const M = pairs => ({map:pairs.map(([k,v]) => [U(k),v])})
const A = (...array) => ({array})
const hash = bytes => createHash('sha256').update(bytes).digest()
const key = fixture('cose_key_v3')
const keyField = n => key.map.find(([k]) => BigInt(k.int ?? k.uint) === BigInt(n))[1]
assert.deepEqual(keyField(1),U(1)); assert.deepEqual(keyField(3),U(-19)); assert.deepEqual(keyField(-1),U(6))
const thumbprint = M([[1,U(1)],[-1,U(6)],[-2,keyField(-2)]])
assert.deepEqual(encode(thumbprint),encode(fixture('key_thumbprint_input')))
const kid = hash(encode(thumbprint)).toString('hex')
assert.deepEqual(keyField(2),B(kid))
const publicKey = createPublicKey({key:Buffer.from(`302a300506032b6570032100${keyField(-2).bytes}`,'hex'),format:'der',type:'spki'})
const claims = M([[1,fixture('account_issuer')],[2,T('release/spec')],[6,U(1800000000)]])
for (const [name,digest,timestamped] of [['cose_text_v3',false,false],['cose_digest_v3',true,false],['timestamped_digest_v3',true,true]]) {
  const cose = fixture(name)
  assert.equal(cose.tag,18)
  const [protectedHeaders,unprotected,payload,signature] = cose.value.array
  const headers = [[1,U(-19)],[2,A(...(digest?[15,16,258]:[15,16]).map(U))],[4,B(kid)],[15,claims],
    [16,T(`application/vnd.dmsg.${digest?'digest':'text'}-statement+cose;v=1`)]]
  if (digest) headers.push([258,U(-16)],[259,T('application/pdf')])
  else headers.push([3,T('text/plain;charset=utf-8')])
  assert.deepEqual(protectedHeaders,B(encode(M(headers)).toString('hex')),`${name}: protected profile`)
  assert.deepEqual(payload,B(digest?hash(Buffer.from('document')).toString('hex'):Buffer.from('Approved release v1').toString('hex')))
  assert.deepEqual(unprotected,timestamped?M([[270,B('3000')]]):M([]))
  const tbs = encode(A(T('Signature1'),protectedHeaders,B(''),payload))
  assert.deepEqual(tbs,encode(fixture(digest?'digest_tbs_v3':'text_tbs_v3')))
  assert(verify(null,tbs,publicKey,Buffer.from(signature.bytes,'hex')),`${name}: standard Ed25519 signature`)
  if (digest) assert.deepEqual(encode(fixture('ctt_imprint_input')),encode(signature),'RFC 9921 hashes the bstr header too')
}
// Xid text encodes exactly 96 bits, with four zero padding bits.
const account = Buffer.from(fixture('account_id').bytes,'hex')
assert.equal(account.length,12)
let bits = BigInt('0x'+account.toString('hex')) << 4n, xid = ''
for(let i=0;i<20;i++){xid='0123456789abcdefghijklmnopqrstuv'[Number(bits&31n)]+xid;bits >>= 5n}
assert.equal(xid,fixture('account_xid_text').text)
assert.equal(fixture('account_issuer').text,`https://dmsg.test/u/${xid}`)
assert.equal(fixture('management_principal_issuer').text,'https://id.test/ic/mainnet/principals/aaaaa-aa')
assert.equal(fixture('root_input_v2').array[0].bytes,account.toString('hex'))
const ns = hash(encode(fixture('allocator_namespace_v1')))
const allocated = Buffer.concat([Buffer.from('00000064','hex'),ns.subarray(0,5),Buffer.from('000000','hex')])
assert.equal(fixture('allocated_account_v1').bytes,allocated.toString('hex'))
const preimage = fixture('execution_request_id_v2'), approval = fixture('sign_approval_v3')
assert.equal(preimage.array[2].array[0].bytes,account.toString('hex'))
assert.equal(preimage.array[2].array[2].bytes.length,64)
assert.deepEqual(approval.array[2].array[6],B(hash(encode(preimage)).toString('hex')))
assert.equal(approval.array[2].array[7].uint,'1800000000000') // approval milliseconds; CWT seconds above
console.log('Verified independent COSE text/digest profiles, key thumbprint, timestamp imprint and Xid allocation.')
