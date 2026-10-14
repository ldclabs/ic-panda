import { test } from 'node:test'
import assert from 'node:assert/strict'
import { freezeInputs, MODULES } from './legacy-freeze-inputs.mjs'
import { freezePlan } from './legacy-freeze-plan.mjs'

const modules = Object.fromEntries(MODULES.map((module, i) => [module, String(i + 1).repeat(64)]))
const inventory = {
  started_at: '2026-10-14T12:01:18.771Z',
  canisters: [
    {
      principal: 'nscli-qiaaa-aaaaj-qa4pa-cai',
      kind: 'message',
      names: ['ic_message'],
      metadata: { module_hash: modules.ic_message }
    },
    {
      principal: 'nvdn4-5qaaa-aaaaj-qa4pq-cai',
      kind: 'channel',
      names: ['ic_message_channel'],
      metadata: { module_hash: '9'.repeat(64) }
    },
    {
      principal: 'n3bau-gaaaa-aaaaj-qa4oq-cai',
      kind: 'cose',
      names: ['ic_cose_canister'],
      metadata: { module_hash: '8'.repeat(64) }
    }
  ]
}

test('pins the release modules and keeps the plan blocked until the review is done', () => {
  const { targets, baseline } = freezeInputs(inventory, 'canisters-v0', modules)
  assert.deepEqual(
    targets.targets.map((t) => [t.kind, t.module_sha256]),
    [
      ['message', modules.ic_message],
      ['channel', modules.ic_message_channel]
    ]
  )
  // The channel still runs another module; each canister needs its upgrade confirmed.
  assert.equal(baseline.unresolved.length, 3)
  assert.match(baseline.unresolved[1], /upgrade it to ic_message_channel\.wasm\.gz/)
  const cutover = '2'.repeat(64)
  assert.throws(() => freezePlan(targets, baseline, cutover))
  assert.throws(() => freezePlan(targets, { ...baseline, unresolved: [] }, cutover))
  const reviewed = {
    ...baseline,
    sources: baseline.sources.map((s) => ({
      ...s,
      evidence_ref: 'SNS proposal'
    })),
    unresolved: []
  }
  const plan = freezePlan(targets, reviewed, cutover)
  assert.deepEqual(
    plan.stages.map((stage) => stage.calls.length),
    [2, 2, 0, 2, 0]
  )
  assert.throws(() =>
    freezeInputs(inventory, 'canisters-v0', {
      ...modules,
      ic_message: undefined
    })
  )
})
