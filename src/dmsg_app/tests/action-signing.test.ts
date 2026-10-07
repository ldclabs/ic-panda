import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import { decodeCanonical } from 'dmsg-sdk'
import { actionFromCandid, actionToCandid } from '../src/lib/protocol/app-action'
import { prepareAttest } from '../src/lib/services/cose'
import { ed25519 } from '../src/lib/crypto/primitives'
import { unhex } from '../src/lib/protocol/codec'
const vectors = JSON.parse(
  readFileSync(
    new URL('../../../src/dmsg_types/tests/integration_vectors.json', import.meta.url),
    'utf8'
  )
)
const fixture = (name: string): any =>
  decodeCanonical(unhex(vectors.find((v: any) => v.name === name).cbor_hex))
describe('typed application attestation', () => {
  it('Candid, portable COSE and device approval match the Rust protocol', async () => {
    const request = fixture('app_action_request'),
      body = request.action,
      seed = new Uint8Array(32).fill(7)
    expect(actionFromCandid(actionToCandid(body))).toEqual(body)
    const prepared = prepareAttest(
      {
        homeUser: Principal.fromUint8Array(new Uint8Array([1, 1])),
        accountId: body.signing_account,
        issuer: request.issuer,
        deviceId: request.approval.device_id,
        securityEpoch: 1n,
        sequence: 0n,
        expiresAt: body.expires_at_ms
      },
      {
        origin: body.origin,
        devicePublicKey: ed25519.getPublicKey(seed),
        statement: { issuer: request.issuer, content: { kind: 'app_action', action: body } }
      },
      Number(body.issued_at_ms)
    )
    expect(prepared.review.kind).toBe('attest_app_action')
    expect(prepared.toBeSigned).toEqual(
      unhex(vectors.find((v: any) => v.name === 'app_action_signing_bytes').cbor_hex)
    )
    const signature = ed25519.sign(prepared.toBeSigned, seed)
    expect(signature).toEqual(request.signature)
    expect(prepared.approvalMessage(signature)).toEqual(fixture('app_action_approval'))
  })
})
