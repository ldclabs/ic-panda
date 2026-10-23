import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  acceptLink,
  canonicalName,
  cashTotal,
  covers,
  DAY,
  daysLeft,
  isTab,
  moving,
  namePrice,
  nameProblem,
  nameState,
  neuronUnlockAt,
  orderActions,
  orderState,
  refundOutcome,
  reusableQuote,
  skuOf,
  tokenOf
} from '../src/renderer/views/plan/plan.ts'

const now = 1_800_000_000_000
const cash = (stage: string, extra: Record<string, unknown> = {}) => ({ id: 'a1', stage, starts_at_ms: now - DAY, terms: { method: 'Cash', funding_deadline_ms: now + 60_000 }, ...extra })
const panda = (stage: string) => ({ id: 'b2', stage, starts_at_ms: now, terms: { method: 'Panda', application_deadline_ms: now + 60_000 } })

test('an order reads as the service reports it, or as its journal stands before that', () => {
  assert.deepEqual(orderState(cash('review')), { label: 'orderReview', tone: 'neutral' })
  assert.deepEqual(orderState(cash('accepted'), { status: 'AwaitingFunding' }), { label: 'orderAwaitingFunding', tone: 'warn' })
  assert.deepEqual(orderState(cash('accepted'), { status: 'Applied' }), { label: 'orderApplied', tone: 'ok' })
  assert.deepEqual(orderState(panda('accepted'), { status: 'Active' }), { label: 'orderApplied', tone: 'ok' })
  assert.equal(orderState(cash('accepted'), { status: 'RefundCommitted' }).label, 'orderRefundAvailable')
  assert.equal(orderState(cash('accepted'), { status: 'Closing' }).label, 'sourceClosing')
  assert.equal(orderState(cash('accepted'), { status: 'Cancelled' }).label, 'orderCancelled')
  assert.equal(orderState(cash('strange')).label, 'orderUnknown')
})

test('only what the service still moves is read again', () => {
  assert.equal(moving(cash('accepted'), { status: 'AwaitingFunding' }, now), true)
  assert.equal(moving(cash('accepted'), { status: 'Applied' }, now), false)
  assert.equal(moving(panda('accepted'), { status: 'Checking' }, now), true)
  assert.equal(moving(cash('authorized'), undefined, now), true)
  assert.equal(moving(cash('authorized'), undefined, now + 120_000), false)
  assert.equal(moving(cash('review'), undefined, now), false)
})

test('each order offers what its state allows', () => {
  assert.deepEqual(orderActions(cash('review'), undefined, now), ['pay', 'discard'])
  // An approval that never reached the service is removed after its deadline only.
  assert.deepEqual(orderActions(cash('authorized'), undefined, now), [])
  assert.deepEqual(orderActions(cash('authorized'), undefined, now + 60_000), ['discard'])
  assert.deepEqual(orderActions(cash('unknown'), undefined, now), ['reconcile'])
  assert.deepEqual(orderActions(cash('accepted'), { status: 'AwaitingFunding' }, now), ['cancel'])
  assert.deepEqual(orderActions(cash('accepted'), { status: 'Applying' }, now), ['reconcile'])
  // A started term is final, unless it is the first purchase within its refund window.
  assert.deepEqual(orderActions(cash('accepted'), { status: 'Applied' }, now), [])
  assert.deepEqual(orderActions(cash('accepted'), { status: 'Applied' }, now, 'a1'), ['refund'])
  assert.deepEqual(orderActions(cash('accepted'), { status: 'Applied' }, now, 'other'), [])
  // A renewal that has not started may be cancelled.
  assert.deepEqual(orderActions(cash('accepted', { starts_at_ms: now + DAY }), { status: 'Applied' }, now), ['cancel'])
  // A cash order is checked by the service; a PANDA application can be asked
  // to qualify again when another SNS check caught it in flight.
  assert.deepEqual(orderActions(cash('accepted'), { status: 'Checking' }, now), ['cancel'])
  assert.deepEqual(orderActions(panda('accepted'), { status: 'Checking', eligibility: 'Unverifiable' }, now), ['reconcile', 'cancel'])
  assert.deepEqual(orderActions(panda('accepted'), { status: 'Checking', eligibility: 'Eligible' }, now), ['advance', 'cancel'])
  assert.deepEqual(orderActions(panda('accepted'), { status: 'Active' }, now), [])
})

test('an order names its plan, its upgrade or its storage pack', () => {
  const catalog = { plans: [{ plan_id: 'Pro' }], storage_products: [{ product_id: 'ab'.repeat(32), storage_bytes: 100 }] }
  assert.deepEqual(skuOf('pro', catalog), { kind: 'plan', plan: 'Pro' })
  assert.deepEqual(skuOf('plus'), { kind: 'plan', plan: 'Plus' })
  assert.deepEqual(skuOf('upgrade-pro', catalog), { kind: 'upgrade', plan: 'Pro' })
  assert.deepEqual(skuOf('ab'.repeat(32), catalog), { kind: 'storage', bytes: 100 })
  assert.deepEqual(skuOf('cd'.repeat(32), catalog), { kind: 'storage', bytes: undefined })
})

test('names follow the registry: 1 to 19 letters, digits and underscores, not first', () => {
  assert.equal(nameProblem(''), null)
  assert.equal(nameProblem('@Panda_1'), null)
  assert.equal(nameProblem('a'.repeat(20)), 'nameTooLong')
  assert.equal(nameProblem('_panda'), 'nameUnderscore')
  assert.equal(nameProblem('pan-da'), 'nameCharacters')
  assert.equal(nameProblem('熊猫'), 'nameCharacters')
  assert.equal(canonicalName(' @Panda '), 'panda')
  assert.deepEqual([1, 2, 3, 4, 5, 6, 7, 19].map(namePrice), [1_000_000, 200_000, 50_000, 50_000, 20_000, 20_000, 100, 100])
})

test('a payer covers a registration only with a known balance', () => {
  const payer = { balances: [{ ledger: 'panda', balance: '10000000000' }, { ledger: 'icp', error: 'UNAVAILABLE' }] }
  assert.equal(covers(payer, 'panda', '10000000000'), true)
  assert.equal(covers(payer, 'panda', '10000000001'), false)
  assert.equal(covers(payer, 'icp', '1'), null)
  assert.equal(covers(null, 'panda', '1'), null)
})

test('a cash checkout needs its amount, fee reserve and network fee', () => {
  assert.equal(cashTotal({ amount: '1000', fee_reserve: '200', network_fee: '3' }), 1203n)
})

test('a transfer waits for the target, then for its owner while the acceptance lasts', () => {
  const token = 'abcdefghijklmnop'
  assert.equal(nameState({ phase: 'prepared', token }, now), 'offered')
  assert.equal(nameState({ phase: 'prepared', token, accepted_until_ms: now + 1 }, now), 'committable')
  assert.equal(nameState({ phase: 'prepared', token, accepted_until_ms: now }, now), 'offered')
  assert.equal(nameState({ phase: 'accepted', until_ms: now + 1 }, now), 'waiting')
  assert.equal(nameState({ phase: 'accepted', until_ms: now }, now), 'expired')
  assert.equal(nameState({ phase: 'accepted', expired: true }, now), 'expired')
  assert.equal(nameState({ phase: 'prepared' }, now), 'prepared')
  assert.equal(nameState({ phase: 'done' }, now), 'done')
  assert.equal(tokenOf(` ${acceptLink(token)} `), token)
  assert.equal(tokenOf(token), token)
})

test('time left rounds up to whole days; the page parts are known', () => {
  assert.equal(daysLeft(now + DAY, now), 1)
  assert.equal(daysLeft(now + DAY + 1, now), 2)
  assert.equal(daysLeft(now - 1, now), 0)
  assert.equal(isTab('payments'), true)
  assert.equal(isTab('wallet'), false)
})

test('a fixed dissolve delay moves with the quote time; a dissolving deadline does not', () => {
  const listed = Date.parse('2026-10-21T08:00:00Z')
  const quoted = listed + 2000
  const end = Date.parse('2027-10-21T08:00:02Z')
  const neuron = { dissolve_delay_seconds: 365 * 86400, unlock_at_ms: listed + 365 * DAY }
  assert.ok(neuron.unlock_at_ms < end)
  assert.equal(neuronUnlockAt(neuron, quoted), end)
  assert.ok(neuronUnlockAt({ ...neuron, dissolve_delay_seconds: null }, quoted) < end)
  assert.ok(neuronUnlockAt({ ...neuron, dissolve_delay_seconds: 364 * 86400 }, quoted) < end)
})

test('refund notices distinguish successful, refused and unknown transfers', () => {
  assert.equal(refundOutcome([{ status: 'Succeeded' }]).label, 'refundSent')
  assert.equal(refundOutcome([{ status: 'Rejected' }]).error, 'Rejected')
  assert.equal(refundOutcome([{ error: 'FEE_BLOCKED' }]).error, 'FEE_BLOCKED')
  for (const refunds of [[], [{ status: 'Unknown' }], [{ status: 'EXECUTION_UNKNOWN' }], [{ status: 'Succeeded' }, { status: 'Unknown' }]])
    assert.equal(refundOutcome(refunds).label, 'refundPending')
})


test('quote reuse respects both acceptance and funding deadlines and approval state', () => {
  const quote = { ...cash('review'), accept_by_ms: now + 30_000 }
  assert.equal(reusableQuote(quote, now), true)
  assert.equal(reusableQuote(quote, now + 30_000), false)
  assert.equal(reusableQuote({ ...quote, accept_by_ms: now + 90_000 }, now + 60_000), false)
  assert.equal(reusableQuote({ ...quote, stage: 'accepted' }, now), false)
  assert.equal(reusableQuote(cash('review'), now), false)
})
