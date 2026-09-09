import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { encode } from 'cborg'
import { ed25519 } from '@noble/curves/ed25519.js'
import { secp256k1 } from '@noble/curves/secp256k1.js'
import { Principal } from '@icp-sdk/core/principal'
import { canonical, decodeBounded, hash, hex, unhex, utf8 } from '../src/lib/protocol/codec'
import {
  accountIssuer,
  assertUri,
  principalIssuer,
  xidBytes,
  xidText
} from '../src/lib/protocol/identity'
import {
  DIGEST_PROFILE,
  statementBytes,
  verifyDocumentArtifact,
  type DocumentStatement
} from '../src/lib/protocol/statements'
const vectors = JSON.parse(
  readFileSync(
    new URL('../../dmsg_types/tests/protocol_vectors.json', import.meta.url),
    'utf8'
  )
) as { name: string; cbor_hex: string }[]
const vector = (name: string) => unhex(vectors.find((v) => v.name === name)!.cbor_hex)
const artifact = (name = 'cose_digest_v3') => ({
  cose_sign1: vector(name),
  cose_key: vector('cose_key_v3')
})
const seed = new Uint8Array(32).fill(7)
const signingInput: DocumentStatement = {
  issuer: 'urn:example:author',
  content: { kind: 'text', text: '  原文\nunchanged  ' }
}
function resigned(change: (headers: Map<number, unknown>) => void) {
  const [raw, unsigned, payload] = decodeBounded(artifact().cose_sign1.subarray(1)) as [
    Uint8Array,
    Map<number, unknown>,
    Uint8Array
  ]
  const headers = decodeBounded(raw) as Map<number, unknown>
  change(headers)
  const protectedBytes = canonical(headers)
  const tbs = canonical(['Signature1', protectedBytes, new Uint8Array(), payload])
  return {
    ...artifact(),
    cose_sign1: Uint8Array.from([
      0xd2,
      ...canonical([protectedBytes, unsigned, payload, ed25519.sign(tbs, seed)])
    ])
  }
}
describe('portable document profiles', () => {
  it('verifies Rust text, hash and appended TSA vectors without inventing trust', () => {
    const text = verifyDocumentArtifact(artifact('cose_text_v3'))
    expect(text.statement.content).toEqual({ kind: 'text', text: 'Approved release v1' })
    expect(text.statement.issuedAt).toBe(1800000000n)
    expect(text.checks.issuerBinding).toBe('not_checked')
    const digest = verifyDocumentArtifact(artifact())
    expect(digest.checks.content).toBe('not_provided')
    expect(verifyDocumentArtifact(artifact(), utf8('document')).checks.content).toBe(
      'verified'
    )
    expect(() => verifyDocumentArtifact(artifact(), utf8('other'))).toThrow()
    const timestamp = verifyDocumentArtifact(artifact('timestamped_digest_v3'))
    expect(timestamp.toBeSigned).toEqual(digest.toBeSigned)
    expect(timestamp.signature).toEqual(digest.signature)
    expect(timestamp.checks.timestamp).toBe('not_checked')
    expect(timestamp.checks.authorization).toBe('not_checked')
  })
  it('rejects unknown profiles, claims, algorithms and hash-envelope content type even when correctly signed', () => {
    for (const change of [
      (h: Map<number, unknown>) => h.set(16, 'application/unknown+cose'),
      (h: Map<number, unknown>) => h.set(1, 'dmsg:bip340-sha256'),
      (h: Map<number, unknown>) => h.set(3, 'application/pdf'),
      (h: Map<number, unknown>) => h.set(2, [15, 16]),
      (h: Map<number, unknown>) => h.set(258, -44),
      (h: Map<number, unknown>) => h.set(1000, true),
      (h: Map<number, unknown>) => (h.get(15) as Map<number, unknown>).set(4, 1800000000),
      (h: Map<number, unknown>) =>
        (h.get(15) as Map<number, unknown>).set(6, 0x8000000000000000n)
    ])
      expect(() => verifyDocumentArtifact(resigned(change))).toThrow()
    expect(() => statementBytes(signingInput, 'Bip340' as never, utf8('kid'))).toThrow()
    expect(() =>
      statementBytes(
        {
          issuer: signingInput.issuer,
          content: {
            kind: 'digest',
            sha256: new Uint8Array(32),
            contentType: 'application/pdf;foo'
          }
        },
        'Ed25519',
        utf8('kid')
      )
    ).toThrow()
  })
  it('rejects substituted keys and private material', () => {
    const key = decodeBounded(artifact().cose_key) as Map<number, unknown>
    key.set(-2, ed25519.getPublicKey(new Uint8Array(32).fill(8)))
    expect(() => verifyDocumentArtifact({ ...artifact(), cose_key: canonical(key) })).toThrow()
    key.set(-2, ed25519.getPublicKey(seed))
    key.set(-4, seed)
    expect(() => verifyDocumentArtifact({ ...artifact(), cose_key: canonical(key) })).toThrow()
    expect(() =>
      verifyDocumentArtifact({ ...artifact(), cose_sign1: artifact().cose_sign1.subarray(1) })
    ).toThrow()
  })
  it('rejects invalid Unicode instead of silently replacing signed characters', () => {
    for (const text of ['\ud800', '\udc00']) {
      expect(() =>
        statementBytes(
          { ...signingInput, content: { kind: 'text', text } },
          'Ed25519',
          utf8('kid')
        )
      ).toThrow()
      expect(() =>
        statementBytes({ ...signingInput, subject: text }, 'Ed25519', utf8('kid'))
      ).toThrow()
    }
    expect(() =>
      statementBytes(
        { ...signingInput, content: { kind: 'text', text: '🦀' } },
        'Ed25519',
        utf8('kid')
      )
    ).not.toThrow()
  })
  it('preserves text and accepts variable kid lengths', () => {
    for (const length of [1, 12, 29, 32, 256]) {
      const kid = new Uint8Array(length).fill(8)
      const { protectedBytes, payload, toBeSigned } = statementBytes(
        signingInput,
        'Ed25519',
        kid
      )
      expect(payload).toEqual(utf8('  原文\nunchanged  '))
      const key = new Map<number, unknown>([
        [1, 1],
        [3, -19],
        [-1, 6],
        [-2, ed25519.getPublicKey(seed)]
      ])
      const signed = {
        cose_sign1: Uint8Array.from([
          0xd2,
          ...canonical([protectedBytes, new Map(), payload, ed25519.sign(toBeSigned, seed)])
        ]),
        cose_key: canonical(key)
      }
      expect(verifyDocumentArtifact(signed).statement.content).toEqual(signingInput.content)
    }
  })
  it('preserves external protected bytes instead of re-encoding before verification', () => {
    const [raw, unsigned, payload] = decodeBounded(artifact().cose_sign1.subarray(1)) as [
      Uint8Array,
      Map<number, unknown>,
      Uint8Array
    ]
    const headers = decodeBounded(raw) as Map<number, unknown>
    const reversed = new Map([...headers].reverse())
    // cborg's sorter is disabled to produce a valid but non-deterministic map.
    const protectedBytes = encode(reversed, { mapSorter: () => 0 })
    expect(hex(protectedBytes)).not.toBe(hex(raw))
    const tbs = canonical(['Signature1', protectedBytes, new Uint8Array(), payload])
    const result = verifyDocumentArtifact({
      ...artifact(),
      cose_sign1: Uint8Array.from([
        0xd2,
        ...canonical([protectedBytes, unsigned, payload, ed25519.sign(tbs, seed)])
      ])
    })
    expect(result.toBeSigned).toEqual(tbs)
  })
  it('verifies ES256K with identical fingerprints for compressed and full coordinates', () => {
    const kid = utf8('ec-key'),
      publicKey = secp256k1.getPublicKey(seed, false)
    const { protectedBytes, payload, toBeSigned } = statementBytes(
      signingInput,
      'EcdsaSecp256k1',
      kid
    )
    const signature = secp256k1.sign(toBeSigned, seed, { prehash: true })
    const key = new Map<number, unknown>([
      [1, 2],
      [3, -47],
      [-1, 8],
      [-2, publicKey.subarray(1, 33)],
      [-3, publicKey.subarray(33)]
    ])
    const cose_sign1 = Uint8Array.from([
      0xd2,
      ...canonical([protectedBytes, new Map(), payload, signature])
    ])
    const full = verifyDocumentArtifact({ cose_sign1, cose_key: canonical(key) })
    const required = new Map([
      [1, key.get(1)],
      [-1, key.get(-1)],
      [-2, key.get(-2)],
      [-3, key.get(-3)]
    ])
    expect(hex(full.keyFingerprint)).toBe(hash(canonical(required)))
    key.set(-3, (publicKey[64] & 1) === 1)
    expect(
      verifyDocumentArtifact({ cose_sign1, cose_key: canonical(key) }).keyFingerprint
    ).toEqual(full.keyFingerprint)
  })
})
describe('identity adapters', () => {
  it('round trips Xid bytes and both Principal bounds without padding or hashing', () => {
    const account = new Uint8Array(12).fill(1),
      text = xidText(account)
    expect(xidBytes(text)).toEqual(account)
    expect(accountIssuer('https://dmsg.test/u/', account)).toBe(
      decodeBounded(vector('account_issuer'))
    )
    expect(
      principalIssuer('https://id.test/ic/mainnet/principals/', Principal.managementCanister())
    ).toBe(decodeBounded(vector('management_principal_issuer')))
    const long = Principal.fromUint8Array(new Uint8Array(29).fill(3))
    expect(principalIssuer('urn:ic:principal:', long)).toBe(
      'urn:ic:principal:' + long.toText()
    )
    for (const bad of [text.toUpperCase(), text.slice(0, 19) + '1', '11'.repeat(32)])
      expect(() => xidBytes(bad)).toThrow()
  })
  it('rejects implicit URI rewrites, wrong escapes, controls and namespace changes', () => {
    for (const bad of [
      'relative',
      'https://USER:pass@example.com/',
      'https://EXAMPLE.com/',
      'https://example.com/%xx',
      'https://example.com/%'
    ])
      expect(() => assertUri(bad)).toThrow()
    expect(() =>
      statementBytes({ ...signingInput, subject: 'bad\u0085subject' }, 'Ed25519', utf8('kid'))
    ).toThrow()
    expect(() => accountIssuer('https://dmsg.test/u', new Uint8Array(12))).toThrow()
    expect(DIGEST_PROFILE).toBe('application/vnd.dmsg.digest-statement+cose;v=1')
  })
})
