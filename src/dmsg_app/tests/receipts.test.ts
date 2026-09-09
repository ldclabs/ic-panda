import { readFileSync } from 'node:fs'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import { config } from '../src/lib/config'
import { decodeCanonical, hex } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'
import { verifyExecutionReceipt } from '../src/lib/services/ic'
import type { CertifiedBatch } from '../src/lib/canisters/generated/user'
import type { Artifact } from '../src/lib/protocol/statements'
vi.mock('../src/lib/config', () => ({ config: { canisters: { user: '' } } }))
type RawBatch = Omit<CertifiedBatch, 'canister' | 'entries'> & {
  canister: Uint8Array
  entries: { key: Uint8Array; value: Uint8Array | null; witness: Uint8Array }[]
}
const [root, at, raw, artifact, account, request, issuer] = decodeCanonical<
  [Uint8Array, number, RawBatch, Artifact, Uint8Array, Uint8Array, string]
>(new Uint8Array(readFileSync(new URL('./fixtures/execution-receipt.cbor', import.meta.url))))
const batch = (): CertifiedBatch => ({
  ...raw,
  canister: Principal.fromUint8Array(raw.canister),
  entries: raw.entries.map((e) => ({ ...e, value: e.value ? [e.value] : [] }))
})
// Only rootKey is used; the real Certificate verifier performs BLS/delegation checks.
const agent = { rootKey: root } as HttpAgent
const verify = (b = batch(), a = artifact) =>
  verifyExecutionReceipt(b, agent, xidText(account), hex(request), issuer, a)
beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(at)
  config.canisters.user = Principal.fromUint8Array(raw.canister).toText()
})
afterEach(() => vi.useRealTimers())
describe('authenticated execution evidence from PocketIC', () => {
  it('authenticates the certificate and promotes only the linked execution claims', async () => {
    const result = await verify()
    expect(result.verification.checks).toMatchObject({
      signature: 'verified',
      issuerBinding: 'verified',
      authorization: 'verified',
      timestamp: 'not_provided',
      currentStatus: 'not_checked'
    })
    expect(result.receipt.request_id).toEqual(request)
  })
  it('rejects fabricated values, proofs, request paths and identity namespaces', async () => {
    const changed = batch()
    changed.entries[0].value = [Uint8Array.from([0xa0])]
    await expect(verify(changed)).rejects.toThrow()
    const forged = batch()
    const certificate = Uint8Array.from(forged.certificate)
    certificate[certificate.length - 1] ^= 1
    forged.certificate = certificate
    await expect(verify(forged)).rejects.toThrow()
    await expect(
      verifyExecutionReceipt(
        batch(),
        agent,
        xidText(account),
        'ff'.repeat(32),
        issuer,
        artifact
      )
    ).rejects.toThrow()
    await expect(
      verifyExecutionReceipt(
        batch(),
        agent,
        xidText(account),
        hex(request),
        'urn:other:author',
        artifact
      )
    ).rejects.toThrow()
    config.canisters.user = Principal.fromUint8Array(new Uint8Array([1])).toText()
    await expect(verify()).rejects.toThrow()
  })
  it('rejects another valid artifact even with the original certified receipt', async () => {
    const values = JSON.parse(
      readFileSync(
        new URL('../../dmsg_types/tests/protocol_vectors.json', import.meta.url),
        'utf8'
      )
    ) as { name: string; cbor_hex: string }[]
    const bytes = (name: string) =>
      Uint8Array.from(Buffer.from(values.find((v) => v.name === name)!.cbor_hex, 'hex'))
    await expect(
      verify(batch(), { cose_sign1: bytes('cose_digest_v3'), cose_key: bytes('cose_key_v3') })
    ).rejects.toThrow()
  })
  it('accepts archived receipts and rejects an unrelated trust root', async () => {
    vi.setSystemTime(at + 60001)
    await expect(verify()).resolves.toMatchObject({ certifiedAt: at })
    const wrong = Uint8Array.from(root)
    wrong[wrong.length - 1] ^= 1
    await expect(
      verifyExecutionReceipt(
        batch(),
        { rootKey: wrong } as HttpAgent,
        xidText(account),
        hex(request),
        issuer,
        artifact
      )
    ).rejects.toThrow()
  })
})
