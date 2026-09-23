import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { canonical, digest, sha256 } from '../src/encoding.ts'
import { validateShape, validateApp, validateProduct, validateBillingOffer, validateAuthentication, validateRatePolicy, validateOrigin, requiredPandaStake, billingOfferHash, authenticationRequestHash, applicationApprovalHash, quotePanda, validateApplicationApproval, validateCashQuote, matchProductReceipt } from '../src/validation.ts'
import type { AppRegistration, ProductRegistration, BillingOffer, AuthenticationRequest, ApplicationApproval } from '../src/contracts.ts'

const vectors = JSON.parse(readFileSync(new URL('../../../src/dmsg_types/tests/integration_vectors.json', import.meta.url), 'utf8'))
function tree(v: any): any {
  if (v === null || typeof v === 'boolean') return v
  if ('uint' in v) return BigInt(v.uint)
  if ('text' in v) return v.text
  if ('bytes' in v) return Uint8Array.from(Buffer.from(v.bytes, 'hex'))
  if ('array' in v) return v.array.map(tree)
  if ('tag' in v) {
    assert.equal(v.tag, 2)
    return BigInt('0x' + v.value.bytes)
  }
  return Object.fromEntries(v.map.map(([k, value]: any[]) => [tree(k), tree(value)]))
}
const fixture = (name: string) => tree(vectors.find((v: any) => v.name === name).value)
const app = fixture('app') as AppRegistration
const product = fixture('product') as ProductRegistration
const offer = fixture('project_offer')[2] as BillingOffer
const auth = fixture('authentication')[2] as AuthenticationRequest
const approval = fixture('approval')[2] as ApplicationApproval
const now = 1_800_000_000_000n

test('independent TypeScript encoder matches every Rust CBOR and digest byte', async () => {
  for (const vector of vectors) {
    const value = tree(vector.value), bytes = canonical(value)
    assert.equal(Buffer.from(bytes).toString('hex'), vector.cbor_hex, vector.name)
    assert.equal(Buffer.from(await sha256(bytes)).toString('hex'), vector.sha256_hex, vector.name)
  }
  assert.deepEqual(await billingOfferHash(offer), await digest('dmsg/commerce/offer/v2', offer))
  assert.deepEqual(await authenticationRequestHash(auth), await digest('dmsg/authentication/request/v1', auth))
  assert.deepEqual(await applicationApprovalHash(approval), await digest('dmsg/application/approval/v1', approval))
})

test('typed fixtures use the same valid shapes and registration boundaries', () => {
  validateApp(app); validateProduct(product); validateBillingOffer(offer, app, product, now); validateAuthentication(auth, app, now)
  validateRatePolicy(fixture('panda_quote')[2].policy)
  for (const [name, type] of [['approval', 'ApplicationApproval'], ['panda_quote', 'PandaQuote'], ['cash_a', 'CashQuote'], ['cash_b', 'CashQuote'], ['decision', 'ProductDecision']]) validateShape(type, fixture(name)[2])
  const account = fixture('account_offer')[2]
  validateBillingOffer(account, app, { ...product, product_id: 'sample', subject_schema: 'sample-account-v1', subject_size: 12n }, now)
  assert.throws(() => validateBillingOffer(account, app, product, now))
})

test('closed fields, versions, bytes and integer units reject malformed input', () => {
  assert.throws(() => validateShape('BillingOffer', { ...offer, discount_bps: 100n }))
  assert.throws(() => validateShape('BillingOffer', { ...offer, amount_usd_micros: 1_000_001 }))
  assert.throws(() => validateShape('BillingOffer', { ...offer, amount_usd_micros: 1n << 128n }))
  assert.throws(() => validateShape('BillingOffer', { ...offer, operation_id: new Uint8Array(31) }))
  assert.throws(() => validateBillingOffer({ ...offer, version: 1n }, app, product, now))
  assert.throws(() => validateBillingOffer({ ...offer, accept_by_ms: offer.accept_by_ms * 1_000_000n }, app, product, now))
  assert.throws(() => validateBillingOffer({ ...offer, expires_at_ms: offer.expires_at_ms / 1000n }, app, product, now))
  assert.throws(() => validateBillingOffer(offer, app, product, offer.accept_by_ms))
  assert.throws(() => validateAuthentication({ ...auth, receiver: new Uint8Array([99, 1]) }, app, now))
  assert.throws(() => validateAuthentication(auth, app, auth.expires_at_ms))
  assert.throws(() => canonical(Number.MAX_SAFE_INTEGER + 1))
  assert.throws(() => canonical('\ud800'))
})

test('exact origins and app pause reject unsafe acceptance', () => {
  for (const value of ['https://example.test', 'http://localhost:5188', 'http://127.0.0.1:5188', 'http://[::1]:5188']) validateOrigin(value, 'Local')
  for (const value of ['https://example.test/', 'https://user@example.test', 'https://example.test/a', 'https://EXAMPLE.test', 'https://example.test:443', 'https://example.test?x=1', 'http://example.test', 'null']) assert.throws(() => validateOrigin(value, 'Local'))
  assert.throws(() => validateOrigin('http://localhost:5188', 'Production'))
  assert.throws(() => validateBillingOffer(offer, { ...app, paused: true }, product, now))
})

test('PANDA exact ceiling and wide intermediates match Rust', () => {
  for (const [amount, n, d, result] of [[1_000_000n, 2n, 1n, 200_000_000n], [1n, 1n, 3n, 34n], [1_000_001n, 7n, 3n, 233_333_567n]]) assert.equal(requiredPandaStake(amount, n, d), result)
  const max = (1n << 128n) - 1n
  assert.equal(requiredPandaStake(max, 1n, 100n), max)
  assert.throws(() => requiredPandaStake(max, 1n, 1n))
  assert.throws(() => requiredPandaStake(0n, 1n, 1n))
  assert.throws(() => requiredPandaStake(1n, 0n, 1n))
  assert.throws(() => requiredPandaStake(1n, 1n, 0n))
})


test('quotations, approval and receipt bindings match the Rust admission rules', async () => {
  const expected = fixture('panda_quote')[2]
  assert.deepEqual(await quotePanda(offer, app, product, expected.policy, now), expected)
  await assert.rejects(quotePanda(offer, app, product, { ...expected.policy, effective_at_ms: now + 1n }, now))
  await assert.rejects(quotePanda({ ...offer, amount_usd_micros: 0n }, app, product, expected.policy, now))
  validateApplicationApproval(approval, app, product, Uint8Array.of(8, 1), now)
  assert.throws(() => validateApplicationApproval(approval, app, product, Uint8Array.of(99, 1), now))
  const cash = fixture('cash_a')[2]
  await validateCashQuote(cash, offer, product, now)
  await validateCashQuote(fixture('cash_b')[2], offer, product, now)
  await assert.rejects(validateCashQuote({ ...cash, version: 1n }, offer, product, now))
  await assert.rejects(validateCashQuote({ ...cash, fee_reserve_atomic: 0n }, offer, product, now))
  const d = fixture('decision')[2]
  const receipt = { version: 2n, decision_id: d.decision_id, decision_hash: await digest('dmsg/commerce/decision/v2', d), adapter: d.offer.adapter, outcome: { Applied: { business_revision: 10n, contract_id: new Uint8Array(32).fill(27), committed_until_ms: d.offer.expires_at_ms } }, applied_at_ms: now }
  await matchProductReceipt(receipt, d)
  receipt.outcome.Applied.committed_until_ms = now
  await assert.rejects(matchProductReceipt(receipt, d))
})
