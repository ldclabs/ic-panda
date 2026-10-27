import assert from 'node:assert/strict'
import { test } from 'node:test'
import { dailyLimit, policyChanges, sameSelection } from '../src/renderer/views/security.ts'

const policy = { frozen: true, daily_executions: 20, allowed_purposes: ['FileAttestation', 'Statement', 'AppAction'] }

test('signing limits match the account contract, including zero and its upper boundary', () => {
  for (const [value, expected] of [['0', 0], ['64', 64], ['020', 20]] as const) assert.equal(dailyLimit(value), expected)
  for (const value of ['', ' ', '-1', '1.5', '1e1', '65', '9007199254740993']) assert.equal(dailyLimit(value), null, value)
})

test('reselecting purposes in a different order is not a policy change', () => {
  assert.equal(policyChanges(policy, '020', ['AppAction', 'FileAttestation', 'Statement']), null)
  assert.equal(sameSelection(['payment', 'attestation'], ['attestation', 'payment']), true)
  assert.equal(sameSelection(['payment'], ['attestation']), false)
})

test('a single approval includes changed fields and preserves unrelated settings', () => {
  assert.deepEqual(policyChanges(policy, '7', policy.allowed_purposes), { daily_executions: 7 })
  assert.deepEqual(policyChanges(policy, '20', ['Statement']), { allowed_purposes: ['Statement'] })
  assert.deepEqual(policyChanges(policy, '0', []), { daily_executions: 0, allowed_purposes: [] })
  assert.equal(policyChanges(policy, '65', ['Statement']), null)
  assert.equal(policy.frozen, true)
})
