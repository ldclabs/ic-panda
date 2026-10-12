import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { xidText } from '../src/bridge.ts'
import { canonical, decodeCanonical, hex, unb64, unhex } from '../src/codec.ts'
import { aesOpen, open1, suite } from '../src/crypto.ts'
import {
  REQUEST_TYP,
  POP_TYP,
  operationDigest,
  pairingId,
  pairingMac,
  parseCommand,
  parseInvite,
  parseResponse,
  proofMessage,
  requestAad,
  responseInfo,
  wrapInfo,
  type Command
} from '../src/protocol.ts'

const v = JSON.parse(readFileSync(new URL('../../../tests/vectors/client/connect-v1.json', import.meta.url), 'utf8'))
const buf = (b: Uint8Array) => new Uint8Array(b).buffer

test('reproduces digests, MACs and encodings', async () => {
  const command = parseCommand(v.command) as Command
  assert.equal(hex(canonical(command)), v.command_cbor_hex)
  assert.equal(xidText(unhex(v.account_hex)), v.account)
  const message = proofMessage(unhex(v.proof.nonce_hex), v.proof.origin, unhex(v.proof.connection_id_hex), command)
  assert.equal(hex(message), v.proof.message_hex)
  assert.equal(hex(operationDigest(v.proof.origin, command)), v.operation_digest_hex)
  const pub = unhex(v.keys.pairing_spki_hex)
  assert.equal(hex(pairingId(pub)), v.pairing_id_hex)
  const invite = parseInvite(v.invite.fragment)
  assert.equal(hex(canonical(invite)), v.invite.cbor_hex)
  const mac = await pairingMac(invite.secret, invite.environment, invite.account_id, invite.invite_id, pub, v.invite.label)
  assert.equal(hex(mac), v.invite.mac_hex)
  assert.notEqual(hex(await pairingMac(invite.secret, invite.environment, invite.account_id, invite.invite_id, pub, 'Evil')), v.invite.mac_hex)
})

test('verifies the request, the proof of possession and the page signature', async () => {
  const pub = unhex(v.keys.pairing_spki_hex)
  const pairing = unhex(v.pairing_id_hex)
  const payload = await open1(unhex(v.request.cose_hex), pairing, REQUEST_TYP, pub)
  assert.equal(hex(payload), v.request.cbor_hex)
  await assert.rejects(open1(unhex(v.request.cose_hex), pairing, POP_TYP, pub))
  await assert.rejects(open1(unhex(v.request.cose_hex), pairing, REQUEST_TYP, unhex(v.keys.page_spki_hex)))
  assert.equal(hex(await open1(unhex(v.pop.cose_hex), pairing, POP_TYP, pub)), v.pop.cbor_hex)
  const request = decodeCanonical<Record<string, any>>(payload)
  const key = await crypto.subtle.importKey('spki', buf(unhex(v.keys.page_spki_hex)), { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify'])
  assert.ok(
    await crypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, key, buf(request.page_signature), buf(unhex(v.proof.message_hex)))
  )
})

test('a device opens the envelope; the page opens only the certified device reply', async () => {
  const account = unhex(v.account_hex)
  const pairing = unhex(v.pairing_id_hex)
  const request = decodeCanonical<Record<string, any>>(unhex(v.request.cbor_hex))
  const device = unhex(v.keys.device_id_hex)
  const envelope = decodeCanonical<Record<string, any>>(unhex(v.envelope.cbor_hex))
  assert.equal(hex(requestAad(account, pairing, request.request_id)), v.envelope.aad_hex)
  assert.equal(hex(wrapInfo(account, pairing, request.request_id, device)), v.envelope.wrap_info_hex)
  const deviceKey = await suite.kem.deriveKeyPair(buf(unhex(v.keys.device_hpke_seed_hex)))
  const recipient = envelope.recipients[0]
  const cek = new Uint8Array(
    await suite.open(
      { recipientKey: deviceKey, enc: buf(recipient.enc), info: buf(unhex(v.envelope.wrap_info_hex)) },
      buf(recipient.wrapped),
      buf(unhex(v.envelope.wrap_info_hex))
    )
  )
  assert.equal(hex(cek), v.envelope.cek_hex)
  const cose = await aesOpen(cek, envelope.nonce, envelope.ciphertext, unhex(v.envelope.aad_hex))
  assert.equal(hex(cose), v.request.cose_hex)

  const info = responseInfo(account, pairing, request.request_id, 1, device)
  assert.equal(hex(info), v.response.info_hex)
  const sealed = decodeCanonical<{ enc: Uint8Array; ciphertext: Uint8Array }>(unhex(v.response.sealed_cbor_hex))
  const resultKey = await suite.kem.deriveKeyPair(buf(unhex(v.keys.result_seed_hex)))
  const devicePublic = await suite.kem.deserializePublicKey(buf(unhex(v.keys.device_hpke_public_hex)))
  const plain = new Uint8Array(
    await suite.open({ recipientKey: resultKey, enc: buf(sealed.enc), senderPublicKey: devicePublic, info: buf(info) }, buf(sealed.ciphertext), buf(info))
  )
  assert.equal(hex(plain), v.response.cbor_hex)
  const response = parseResponse(plain, request.request_id, 1)
  assert.deepEqual(JSON.parse(new TextDecoder().decode(response.value)), v.response.value)
  // DT32: a reply that does not come from the certified device's key does not open.
  const other = await suite.kem.generateKeyPair()
  await assert.rejects(
    suite.open({ recipientKey: resultKey, enc: buf(sealed.enc), senderPublicKey: other.publicKey, info: buf(info) }, buf(sealed.ciphertext), buf(info))
  )
  await assert.rejects(
    suite.open(
      { recipientKey: resultKey, enc: buf(sealed.enc), senderPublicKey: devicePublic, info: buf(responseInfo(account, pairing, request.request_id, 2, device)) },
      buf(sealed.ciphertext),
      buf(info)
    )
  )
  assert.throws(() => parseResponse(plain, request.request_id, 2))
  assert.equal(unb64(v.pop.header).length > 0, true)
})

test('seals content keys the device opens', async () => {
  const { hpkeSeal } = await import('../src/crypto.ts')
  const info = unhex(v.envelope.wrap_info_hex)
  const sealed = await hpkeSeal(unhex(v.keys.device_hpke_public_hex), unhex(v.envelope.cek_hex), info)
  assert.equal(sealed.enc.length, 32)
  assert.equal(sealed.ciphertext.length, 48)
  const deviceKey = await suite.kem.deriveKeyPair(buf(unhex(v.keys.device_hpke_seed_hex)))
  const cek = new Uint8Array(
    await suite.open({ recipientKey: deviceKey, enc: buf(sealed.enc), info: buf(info) }, buf(sealed.ciphertext), buf(info))
  )
  assert.equal(hex(cek), v.envelope.cek_hex)
})
