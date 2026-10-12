import { test } from 'node:test'
import assert from 'node:assert/strict'
import { freezePlan } from './legacy-freeze-plan.mjs'
const target = {
  kind: 'oss',
  canister: 'aaaaa-aa',
  module_sha256: '1'.repeat(64),
  scopes: [1, 3]
}
const baseline = {
  format: 'dmsg-legacy-baseline-review/1',
  sources: [{ ...target, evidence_ref: 'local-fixture-report' }],
  unresolved: []
}
test('requires an explicit pinned scope and separates review, drain and seal payloads', () => {
  const plan = freezePlan({ targets: [target] }, baseline, '2'.repeat(64))
  assert.equal(plan.executes, false)
  assert.deepEqual(
    plan.stages[1].calls.map((c) => c.method),
    ['admin_legacy_drain_folder', 'admin_legacy_drain_folder']
  )
  assert.equal(plan.stages[3].calls.length, 2)
  for (const input of [
    { ...target, scopes: [0] },
    { ...target, scopes: [] },
    { ...target, scopes: [1, 1] },
    { ...target, module_sha256: '3'.repeat(64) }
  ])
    assert.throws(() => freezePlan({ targets: [input] }, baseline, '2'.repeat(64)))
  assert.throws(() =>
    freezePlan(
      { targets: [target] },
      { ...baseline, unresolved: ['unreconciled token payment'] },
      '2'.repeat(64)
    )
  )
})
