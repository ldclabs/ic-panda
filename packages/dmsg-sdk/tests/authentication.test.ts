import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { canonical, decodeCanonical } from '../src/encoding.ts'
import { verifyAuthenticationCertificate, type CertifiedBatch } from '../src/ic.ts'
import type { AuthenticationRequest } from '../src/contracts.ts'

const vector = JSON.parse(readFileSync(new URL('../../../src/dmsg_types/tests/authentication_vector.json', import.meta.url), 'utf8'))
const bytes = (s: string) => Uint8Array.from(Buffer.from(s, 'hex'))
const batch = decodeCanonical(bytes(vector.batch_cbor_hex)) as unknown as CertifiedBatch
const request = decodeCanonical(bytes(vector.request_cbor_hex)) as unknown as AuthenticationRequest
const root = bytes(vector.root_der_hex), now = BigInt(vector.now_ms)

test('independent SDK validates the same real BLS-signed certificate as Rust', async () => {
  const result = await verifyAuthenticationCertificate(batch, request, batch.canister, root, now)
  assert.deepEqual(result.request, request)
  assert.equal(result.security_epoch, 2n)
})

test('wrong root, home, path, leaf, session and purpose cannot authenticate', async () => {
  const wrongRoot = root.slice(); wrongRoot[wrongRoot.length - 1] ^= 1
  await assert.rejects(verifyAuthenticationCertificate(batch, request, batch.canister, wrongRoot, now))
  await assert.rejects(verifyAuthenticationCertificate(batch, request, Uint8Array.of(99, 1), root, now))
  await assert.rejects(verifyAuthenticationCertificate(batch, { ...request, purpose: 'Link' }, batch.canister, root, now))
  await assert.rejects(verifyAuthenticationCertificate(batch, { ...request, session_key_hash: new Uint8Array(32).fill(99) }, batch.canister, root, now))
  let b = structuredClone(batch); b.entries[0].key[0] ^= 1
  await assert.rejects(verifyAuthenticationCertificate(b, request, batch.canister, root, now))
  b = structuredClone(batch); b.entries[0].value = canonical({})
  await assert.rejects(verifyAuthenticationCertificate(b, request, batch.canister, root, now))
  b = structuredClone(batch); b.entries.push(b.entries[0])
  await assert.rejects(verifyAuthenticationCertificate(b, request, batch.canister, root, now))
  await assert.rejects(verifyAuthenticationCertificate(batch, request, batch.canister, root, now - 1n))
  await assert.rejects(verifyAuthenticationCertificate(batch, request, batch.canister, root, now + 60_001n))
  await assert.rejects(verifyAuthenticationCertificate(batch, request, batch.canister, root, request.expires_at_ms))
})

test('noncanonical and duplicate CBOR encodings never reach proof validation', () => {
  for (const hex of ['1800', 'a2616101616102', '9fff', 'c24100', 'a0a0', '20', 'f97c00', '61ff']) assert.throws(() => decodeCanonical(bytes(hex)), hex)
  assert.deepEqual(decodeCanonical(canonical({ amount: (1n << 128n) - 1n })), { amount: (1n << 128n) - 1n })
})


test('SDK verifies the proof issued by the real PocketIC user Wasm', { skip: !process.env.DMSG_EXTERNAL_FIXTURE }, async () => {
  const sample = decodeCanonical(new Uint8Array(readFileSync(process.env.DMSG_EXTERNAL_FIXTURE!))) as unknown as [bigint, string, AuthenticationRequest, CertifiedBatch, Uint8Array, bigint]
  assert.equal(sample[0], 1n); assert.equal(sample[1], 'dmsg-authentication/1')
  const result = await verifyAuthenticationCertificate(sample[3], sample[2], sample[3].canister, sample[4], sample[5])
  assert.deepEqual(result.request, sample[2])
})
