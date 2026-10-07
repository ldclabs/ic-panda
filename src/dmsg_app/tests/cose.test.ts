import { readFileSync } from 'node:fs'
import { describe, expect, it, vi } from 'vitest'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { ed25519 } from '../src/lib/crypto/primitives'
import { equal, hex, unhex, hash, utf8 } from '../src/lib/protocol/codec'
import { accountIssuer } from '../src/lib/protocol/identity'
import { idlFactory } from '../src/lib/canisters/generated/user/index.js'
import type { _SERVICE, SignedArtifact } from '../src/lib/canisters/generated/user'
import {
  deviceKeyThumbprint,
  prepareAttest,
  prepareRootDerivation,
  recordedAttestation,
  recordedExecution
} from '../src/lib/services/cose'

const fill = (n: number) => new Uint8Array(32).fill(n)
const now = 1799999940000
const deviceSeed = fill(7),
  devicePublicKey = ed25519.getPublicKey(deviceSeed)
const context = () => ({
  homeUser: Principal.fromUint8Array(new Uint8Array([0, 1, 1])),
  accountId: new Uint8Array(12).fill(1),
  issuer: accountIssuer('https://dmsg.test/u/', new Uint8Array(12).fill(1)),
  deviceId: fill(2),
  securityEpoch: 0n,
  sequence: 0n,
  expiresAt: 1800000000000n
})
const content = () => ({
  origin: 'https://example.com',
  devicePublicKey,
  statement: {
    issuer: context().issuer,
    subject: 'release/spec',
    issuedAt: 1800000000n,
    content: {
      kind: 'digest' as const,
      sha256: unhex(hash(utf8('document'))),
      contentType: 'application/pdf'
    }
  }
})
const transport = unhex(
  '97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb'
)
const vectors = JSON.parse(
  readFileSync(
    new URL('../../dmsg_types/tests/protocol_vectors.json', import.meta.url),
    'utf8'
  )
) as {
  name: string
  cbor_hex: string
  sha256_hex: string
}[]
const vector = (name: string) => {
  const result = vectors.find((v) => v.name === name)
  expect(result, name).toBeDefined()
  return result!
}
const sign = async (message: Uint8Array) => ed25519.sign(message, deviceSeed)

describe('device attestation client and independent Rust approval vectors', () => {
  it('derives the artifact kid as the RFC 9679 thumbprint of the device key', () => {
    expect(hex(deviceKeyThumbprint(devicePublicKey))).toBe(vector('key_thumbprint_input').sha256_hex)
  })
  it('encodes exactly the Rust Sig_structure and attest approval', async () => {
    const prepared = prepareAttest(context(), content(), now)
    expect(hex(prepared.toBeSigned)).toBe(vector('digest_tbs_v3').cbor_hex)
    const signature = await sign(prepared.toBeSigned)
    expect(hex(prepared.approvalMessage(signature))).toBe(vector('attest_approval_v1').sha256_hex)
  })
  it('binds a file statement to the Rust approval bytes and Candid variant', async () => {
    const input = {
      ...content(),
      statement: {
        ...content().statement,
        content: {
          kind: 'file_statement' as const,
          text: '  第三章需要补充实验数据。\n',
          sha256: unhex(hash(utf8('document'))),
          contentType: 'application/pdf',
          location: 'urn:example:report'
        }
      }
    }
    const prepared = prepareAttest(context(), input, now)
    expect(hex(prepared.toBeSigned)).toBe(vector('file_statement_tbs_v1').cbor_hex)
    expect(hex(prepared.approvalMessage(await sign(prepared.toBeSigned)))).toBe(
      vector('file_statement_approval_v1').sha256_hex
    )
    const method = idlFactory({ IDL })._fields.find(([name]) => name === 'attest')![1]
    const [decoded] = IDL.decode(
      method.argTypes,
      IDL.encode(method.argTypes, [prepared.review.request])
    ) as any[]
    expect(decoded.statement.content.FileStatement.text).toBe(input.statement.content.text)
    expect(Uint8Array.from(decoded.statement.content.FileStatement.sha256)).toEqual(
      input.statement.content.sha256
    )
    const original = prepared.toBeSigned
    input.statement.content.text = 'I approve this file.'
    expect(prepareAttest(context(), input, now).toBeSigned).not.toEqual(original)
    expect(prepared.toBeSigned).toEqual(original)
  })
  it('binds origin, deadline, sequence and signature to approval without changing the portable statement', async () => {
    const first = prepareAttest(context(), content(), now)
    const next = prepareAttest(
      { ...context(), sequence: 1n, expiresAt: context().expiresAt + 1n },
      { ...content(), origin: 'https://other.example' },
      now
    )
    expect(next.toBeSigned).toEqual(first.toBeSigned)
    const signature = await sign(first.toBeSigned)
    expect(next.approvalMessage(signature)).not.toEqual(first.approvalMessage(signature))
    expect(first.approvalMessage(new Uint8Array(64))).not.toEqual(first.approvalMessage(signature))
    expect(next.requestId).not.toEqual(first.requestId)
    expect(() =>
      prepareAttest(
        context(),
        { ...content(), statement: { ...content().statement, issuer: 'urn:other:account' } },
        now
      )
    ).toThrow()
  })
  it('binds generation, transport and the cycles ceiling to the Rust derive approval', () => {
    const prepared = prepareRootDerivation(context(), 2n, transport, 70000000000n, now)
    expect(hex(prepared.approvalMessage)).toBe(vector('derive_root_approval_v1').sha256_hex)
    for (const other of [
      prepareRootDerivation(context(), 3n, transport, 70000000000n, now),
      prepareRootDerivation(context(), 2n, transport, 70000000001n, now)
    ])
      expect(equal(other.approvalMessage, prepared.approvalMessage)).toBe(false)
    const otherTransport = Uint8Array.from(transport)
    otherTransport[10] ^= 1
    expect(
      hex(prepareRootDerivation(context(), 2n, otherTransport, 70000000000n, now).approvalMessage)
    ).not.toBe(hex(prepared.approvalMessage))
  })
  it('freezes the reviewed request, persists approval before sending, and submits only once', async () => {
    const input = content(),
      ctx = context(),
      prepared = prepareAttest(ctx, input, now)
    const toBeSigned = Uint8Array.from(prepared.toBeSigned)
    input.statement.subject = 'tampered'
    input.statement.content.sha256.fill(99)
    ctx.accountId.fill(9)
    const review = prepared.review
    if ('origin' in review.request) review.request.origin = 'https://tampered.example'
    prepared.toBeSigned.fill(0)
    expect(prepared.toBeSigned).toEqual(toBeSigned)
    const order: string[] = []
    const signer = vi.fn(async (message: Uint8Array) => {
      order.push('sign')
      return ed25519.sign(message, deviceSeed)
    })
    const artifact = async (request: any): Promise<SignedArtifact> => {
      const { statementBytes } = await import('../src/lib/protocol/statements')
      const { canonical } = await import('../src/lib/protocol/codec')
      const { protectedBytes, payload } = statementBytes(
        {
          issuer: request.statement.issuer,
          subject: request.statement.subject[0],
          issuedAt: request.statement.issued_at[0],
          content: {
            kind: 'digest',
            sha256: Uint8Array.from(request.statement.content.Digest.sha256),
            contentType: request.statement.content.Digest.content_type[0]
          }
        },
        'Ed25519',
        prepared.kid
      )
      return {
        cose_sign1: Uint8Array.from([
          0xd2,
          ...canonical([protectedBytes, new Map(), payload, Uint8Array.from(request.signature)])
        ]),
        cose_key: canonical(
          new Map<number, unknown>([
            [1, 1],
            [2, prepared.kid],
            [3, -19],
            [-1, 6],
            [-2, devicePublicKey]
          ])
        )
      }
    }
    const attest = vi.fn(async (request: any) => {
      order.push('send')
      expect(request.statement.subject).toEqual(['release/spec'])
      expect(request.origin).toBe('https://example.com')
      expect(request.account_id).toEqual(new Uint8Array(12).fill(1))
      expect(ed25519.verify(request.signature, toBeSigned, devicePublicKey)).toBe(true)
      expect(
        ed25519.verify(
          request.approval.signature,
          prepared.approvalMessage(Uint8Array.from(request.signature)),
          devicePublicKey
        )
      ).toBe(true)
      const method = idlFactory({ IDL })._fields.find(([name]) => name === 'attest')![1]
      expect(IDL.decode(method.argTypes, IDL.encode(method.argTypes, [request]))).toHaveLength(1)
      return { Ok: await artifact(request) }
    })
    const user = { attest, attest_app_action: vi.fn() } as unknown as _SERVICE
    const persist = vi.fn(async (signed) => {
      order.push('persist')
      expect(signed.request.approval.signature).toHaveLength(64)
      // A persistence adapter cannot change the request being sent.
      signed.request.origin = 'https://tampered.example'
    })
    const [a, b] = await Promise.all([
      prepared.approveAndExecute(user, signer, persist),
      prepared.approveAndExecute(user, signer, persist)
    ])
    expect(a).toEqual(b)
    expect(order).toEqual(['sign', 'sign', 'persist', 'send'])
    expect(attest).toHaveBeenCalledOnce()
    // A substituted artifact signed by another key is rejected.
    const forged = prepareAttest(context(), content(), now)
    const other = { attest: vi.fn(async () => ({ Ok: await artifact({ ...forged.review.request, signature: ed25519.sign(forged.toBeSigned, fill(8)) }) })) } as unknown as _SERVICE
    await expect(forged.approveAndExecute(other, signer)).rejects.toThrow('INTEGRITY_FAILED')
  })
  it('keeps a lost reply distinct from a refusal and reads stored artifacts and derivations', async () => {
    const prepared = prepareAttest(context(), content(), now)
    const user = {
      attest: vi.fn().mockRejectedValue(new Error('connection lost')),
      get_attestation: vi.fn().mockResolvedValue({ Err: { ResultExpired: null } })
    } as unknown as _SERVICE
    const signer = vi.fn(async (d: Uint8Array) => ed25519.sign(d, deviceSeed))
    await expect(prepared.approveAndExecute(user, signer)).rejects.toMatchObject({
      code: 'EXECUTION_UNKNOWN'
    })
    await expect(prepared.approveAndExecute(user, signer)).rejects.toMatchObject({
      code: 'EXECUTION_UNKNOWN'
    })
    expect(user.attest).toHaveBeenCalledOnce()
    expect(await recordedAttestation(user, context().accountId, unhex(prepared.requestId))).toBeNull()
    const refused = prepareAttest(context(), content(), now)
    await expect(
      refused.approveAndExecute(
        { attest: vi.fn().mockResolvedValue({ Err: { Forbidden: null } }) } as unknown as _SERVICE,
        signer
      )
    ).rejects.toMatchObject({ code: 'Forbidden' })
    const derivation = prepareRootDerivation(context(), 2n, transport, 100000000000n, now)
    const result = {
      request_id: unhex(derivation.requestId),
      outcome: { Unknown: { ExecutionUnknown: null } },
      cycles_cost_upper_bound: 0n,
      cycles_charged: 0n
    }
    const home = {
      derive_root: vi.fn().mockRejectedValue(new Error('connection lost')),
      get_execution: vi.fn().mockResolvedValue({ Ok: result }),
      reconcile_execution: vi.fn().mockResolvedValue({ Ok: result })
    } as unknown as _SERVICE
    await expect(derivation.approveAndExecute(home, signer)).rejects.toMatchObject({
      code: 'EXECUTION_UNKNOWN'
    })
    expect(await recordedExecution(home, context().accountId, unhex(derivation.requestId))).toEqual(result)
    expect(home.reconcile_execution).toHaveBeenCalledWith(context().accountId, unhex(derivation.requestId))
  })
  it('rejects expired approval, invalid origin, oversized body and transport length before signing', () => {
    expect(() => prepareAttest(context(), content(), Number(context().expiresAt))).toThrow()
    expect(() =>
      prepareAttest(context(), { ...content(), origin: 'https://example.com/path' }, now)
    ).toThrow()
    expect(() =>
      prepareAttest(
        context(),
        {
          ...content(),
          statement: {
            ...content().statement,
            content: { kind: 'text', text: 'a'.repeat(4097) }
          }
        },
        now
      )
    ).toThrow()
    expect(() =>
      prepareRootDerivation(context(), 1n, transport.slice(0, 47), 100000000000n, now)
    ).toThrow()
  })
})
