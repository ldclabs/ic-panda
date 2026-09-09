import { readFileSync } from 'node:fs'
import { describe, expect, it, vi } from 'vitest'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { ed25519 } from '../src/lib/crypto/primitives'
import { equal, hex, unhex, hash, utf8 } from '../src/lib/protocol/codec'
import { accountIssuer } from '../src/lib/protocol/identity'
import { idlFactory } from '../src/lib/canisters/generated/user/index.js'
import type { _SERVICE, ExecutionResult } from '../src/lib/canisters/generated/user'
import { coseClient, prepareRootDerivation, prepareSign } from '../src/lib/services/cose'

const fill = (n: number) => new Uint8Array(32).fill(n)
const now = 1799999940000
const context = () => ({
  homeUser: Principal.fromUint8Array(new Uint8Array([0, 1, 1])),
  accountId: new Uint8Array(12).fill(1),
  issuer: accountIssuer('https://dmsg.test/u/', new Uint8Array(12).fill(1)),
  deviceId: fill(2),
  securityEpoch: 0n,
  sequence: 0n,
  expiresAt: 1800000000000n,
  maxCycles: 100000000000n
})
const content = () => ({
  origin: 'https://example.com',
  key: {
    algorithm: 'Ed25519' as const,
    kid: unhex(vector('key_thumbprint_input').sha256_hex),
    publicKeyFingerprint: unhex(vector('key_thumbprint_input').sha256_hex)
  },
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

describe('typed COSE client and independent Rust approval vectors', () => {
  it('encodes exactly the Rust formal payload and sign approval', () => {
    const prepared = prepareSign(context(), content(), now)
    expect(hex(prepared.toBeSigned!)).toBe(vector('digest_tbs_v3').cbor_hex)
    expect(hex(prepared.approvalMessage)).toBe(vector('sign_approval_v3').sha256_hex)
  })
  it('binds origin, deadline and sequence to approval without changing the portable statement', () => {
    const first = prepareSign(context(), content(), now)
    const next = prepareSign(
      { ...context(), sequence: 1n, expiresAt: context().expiresAt + 1n },
      { ...content(), origin: 'https://other.example' },
      now
    )
    expect(next.toBeSigned).toEqual(first.toBeSigned)
    expect(next.approvalMessage).not.toEqual(first.approvalMessage)
    expect(next.requestId).not.toEqual(first.requestId)
    expect(() =>
      prepareSign(
        context(),
        { ...content(), statement: { ...content().statement, issuer: 'urn:other:account' } },
        now
      )
    ).toThrow()
  })
  it('binds root target and transport to the Rust approval', () => {
    const candidate = prepareRootDerivation(
      context(),
      { kind: 'candidate', generation: 2n, opId: fill(9) },
      transport,
      now
    )
    expect(hex(candidate.approvalMessage)).toBe(vector('derive_root_approval_v3').sha256_hex)
    const current = prepareRootDerivation(
      context(),
      { kind: 'current', generation: 2n },
      transport,
      now
    )
    expect(equal(current.approvalMessage, candidate.approvalMessage)).toBe(false)
    const otherTransport = Uint8Array.from(transport)
    otherTransport[10] ^= 1
    expect(
      hex(
        prepareRootDerivation(
          context(),
          { kind: 'candidate', generation: 2n, opId: fill(9) },
          otherTransport,
          now
        ).approvalMessage
      )
    ).not.toBe(hex(candidate.approvalMessage))
  })
  it('freezes the reviewed request, persists approval before sending, and submits only once', async () => {
    const input = content(),
      ctx = context(),
      prepared = prepareSign(ctx, input, now)
    const digest = prepared.approvalMessage
    input.statement.subject = 'tampered'
    input.statement.content.sha256.fill(99)
    ctx.accountId.fill(9)
    const review = prepared.review
    review.request.max_cycles = 1n
    prepared.approvalMessage.fill(0)
    expect(prepared.approvalMessage).toEqual(digest)
    const order: string[] = []
    const signer = vi.fn(async (message: Uint8Array) => {
      order.push('sign')
      expect(message).toEqual(digest)
      return ed25519.sign(message, fill(7))
    })
    const sign = vi.fn(async (request) => {
      order.push('send')
      expect(request.statement.subject).toEqual(['release/spec'])
      expect(request.account_id).toEqual(new Uint8Array(12).fill(1))
      expect(
        ed25519.verify(request.approval.signature, digest, ed25519.getPublicKey(fill(7)))
      ).toBe(true)
      const method = idlFactory({ IDL })._fields.find(([name]) => name === 'sign')![1]
      const encoded = IDL.encode(method.argTypes, [request])
      expect(IDL.decode(method.argTypes, encoded)).toHaveLength(1)
      return {
        Ok: {
          request_id: request.approval.request_id,
          outcome: { Executing: null },
          charged_cycles: 0n
        }
      }
    })
    const user = { sign, derive_root: vi.fn() } as unknown as _SERVICE
    const persist = vi.fn(async (signed) => {
      order.push('persist')
      expect(signed.request.approval.signature).toHaveLength(64)
      // A persistence adapter cannot change the request being sent.
      signed.request.max_cycles = 1n
    })
    const [a, b] = await Promise.all([
      prepared.approveAndExecute(user, signer, persist),
      prepared.approveAndExecute(user, signer, persist)
    ])
    expect(a).toEqual(b)
    expect(order).toEqual(['sign', 'persist', 'send'])
    expect(sign).toHaveBeenCalledOnce()
    expect(sign.mock.calls[0][0].max_cycles).toBe(100000000000n)
  })
  it('preserves the request ID after transport failure and reconciles explicitly', async () => {
    const prepared = prepareSign(context(), content(), now)
    const result: ExecutionResult = {
      request_id: unhex(prepared.requestId),
      outcome: { Unknown: { ExecutionUnknown: null } },
      charged_cycles: 0n
    }
    const user = {
      sign: vi.fn().mockRejectedValue(new Error('connection lost')),
      derive_root: vi.fn(),
      get_execution: vi.fn().mockResolvedValue({ Ok: result }),
      reconcile_execution: vi.fn().mockResolvedValue({ Ok: result })
    } as unknown as _SERVICE
    const signer = vi.fn(async () => ed25519.sign(prepared.approvalMessage, fill(7)))
    await expect(prepared.approveAndExecute(user, signer)).rejects.toMatchObject({
      code: 'EXECUTION_UNKNOWN'
    })
    await expect(prepared.approveAndExecute(user, signer)).rejects.toMatchObject({
      code: 'EXECUTION_UNKNOWN'
    })
    expect(user.sign).toHaveBeenCalledOnce()
    expect(signer).toHaveBeenCalledOnce()
    const client = coseClient(user, { public_key: vi.fn() } as never)
    expect(
      await client.reconcileExecution(context().accountId, unhex(prepared.requestId))
    ).toEqual(result)
    expect(user.reconcile_execution).toHaveBeenCalledWith(
      context().accountId,
      unhex(prepared.requestId)
    )
  })
  it('keeps a server refusal distinct from an unknown execution', async () => {
    const prepared = prepareSign(context(), content(), now)
    const user = {
      sign: vi.fn().mockResolvedValue({ Err: { Forbidden: null } }),
      derive_root: vi.fn()
    } as unknown as _SERVICE
    await expect(
      prepared.approveAndExecute(user, async (d) => ed25519.sign(d, fill(7)))
    ).rejects.toMatchObject({ code: 'Forbidden' })
  })
  it('rejects expired approval, invalid origin, oversized body and transport length before signing', () => {
    expect(() => prepareSign(context(), content(), Number(context().expiresAt))).toThrow()
    expect(() =>
      prepareSign(context(), { ...content(), origin: 'https://example.com/path' }, now)
    ).toThrow()
    expect(() =>
      prepareSign(
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
      prepareRootDerivation(
        context(),
        { kind: 'current', generation: 1n },
        transport.slice(0, 47),
        now
      )
    ).toThrow()
  })
})
