import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { Principal } from '@icp-sdk/core/principal'
import { accountBytes, parseAttestationBundle, verifyAttestationBundle, type AttestationBundle } from '../src/bundle.ts'
import { b64, unb64 } from '../src/encoding.ts'

// The same bundle the Rust verifier checks (`dmsg_protocol` example `attestation_vector`).
const vector = JSON.parse(readFileSync(new URL('../../../src/dmsg_types/tests/attestation_vector.json', import.meta.url), 'utf8'))
const bytes = (s: string) => Uint8Array.from(Buffer.from(s, 'hex'))
const bundle: AttestationBundle = vector.bundle
const rootKey = bytes(vector.root_der_hex)
const file = bytes(vector.file_hex)
const homes = [Principal.fromText(bundle.home).toUint8Array()]

test('account IDs read back to their twelve bytes', () => {
  assert.deepEqual(accountBytes(bundle.account), new Uint8Array(12).fill(1))
  assert.throws(() => accountBytes('040g2081040g2081040h'))
  assert.throws(() => accountBytes('short'))
})

test('the Rust bundle verifies check by check, with its file', async () => {
  const report = await verifyAttestationBundle(bundle, { homes, rootKey, file })
  assert.equal(report.valid, true, JSON.stringify(report.failed))
  assert.equal(report.content, 'matches_file')
  assert.deepEqual([report.signature, report.evidence, report.binding], [true, true, true])
  assert.equal(report.origin, 'https://dmsg.net')
  assert.equal(report.securityEpoch, 3n)
  assert.equal(report.certifiedAt, BigInt(vector.certified_at_ms))
  assert.equal(report.statement?.content.kind, 'file_statement')
  assert.equal((await verifyAttestationBundle(bundle, { homes, rootKey })).content, 'digest_only')
})

test('each failed check is reported on its own', async () => {
  // Another file: the content fails, the signature stands.
  const changed = await verifyAttestationBundle(bundle, { homes, rootKey, file: new TextEncoder().encode('another file') })
  assert.deepEqual([changed.valid, changed.content, changed.signature, changed.failed], [false, 'mismatch', true, ['content']])
  // A home outside the deployment or another root: no evidence.
  const foreign = await verifyAttestationBundle(bundle, { homes: [Uint8Array.of(2, 2)], rootKey })
  assert.deepEqual([foreign.evidence, foreign.binding], [false, false])
  const root = rootKey.slice()
  root[root.length - 1] ^= 1
  assert.equal((await verifyAttestationBundle(bundle, { homes, rootKey: root })).evidence, false)
  // Another request's receipt does not bind this artifact.
  const rebound = await verifyAttestationBundle({ ...bundle, request_id: '09'.repeat(32) }, { homes, rootKey })
  assert.deepEqual([rebound.signature, rebound.evidence, rebound.valid], [true, false, false])
  // A changed signature: nothing about the content can be said.
  const sign1 = unb64(bundle.cose_sign1)
  sign1[sign1.length - 1] ^= 1
  const tampered = await verifyAttestationBundle({ ...bundle, cose_sign1: b64(sign1) }, { homes, rootKey, file })
  assert.deepEqual([tampered.signature, tampered.binding, tampered.content], [false, false, 'unknown'])
})

test('a malformed bundle is refused, not reported', async () => {
  assert.throws(() => parseAttestationBundle({ ...bundle, format: 'other/1' }), { code: 'UNSUPPORTED_PROTOCOL' })
  assert.throws(() => parseAttestationBundle({ ...bundle, extra: 'x' }))
  assert.throws(() => parseAttestationBundle([]))
  await assert.rejects(verifyAttestationBundle({ ...bundle, request_id: 'xyz' }, { homes, rootKey }))
})
