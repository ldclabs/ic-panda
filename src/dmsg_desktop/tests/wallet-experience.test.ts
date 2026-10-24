import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  batchLimit,
  durationText,
  networkName,
  resultState,
  queueKind,
  teamReadiness,
  policyExample,
  validPolicyDraft,
  toAtomic,
  type Wallet,
  type Proposal
} from '../src/renderer/views/wallet/model.ts'
import { walletHistoryCsv } from '../src/shared/wallet-report.ts'
import { experienceRows } from '../src/renderer/views/wallet/copy.ts'

const policy = {
  version: 1,
  approver_threshold: 2,
  admin_threshold: 1,
  daily_signatures: 50,
  proposal_ttl_ms: 604800000,
  address_book_cooldown_ms: 86400000,
  unfreeze_delay_ms: 86400000,
  tiers: [],
  daily_limits: [],
  spendable_assets: ['icp'],
  refuel: null
}
const wallet: Wallet = {
  wallet: 'w',
  name: 'Team',
  policy,
  frozen: false,
  addresses: [],
  cosigners: [],
  members: [
    { id: 'a', role: 'Admin', status: 'Active', agent_ids: [] },
    { id: 'b', role: 'Approver', status: 'Invited', agent_ids: [] }
  ],
  me: { id: 'a', role: 'Admin', status: 'Active', agent_ids: [] }
}
const proposal = (change: Partial<Proposal> = {}): Proposal => ({
  id: 1,
  status: 'Open',
  proposer: 'a',
  intent: { kind: 'transfer', from: 0, asset: { asset: 'ICP', decimals: 8 }, outputs: [{ to: 'b', amount: '100000000' }] },
  requirement: { approvals: 2, admin: false, delay_ms: 0 },
  votes: [],
  expires_at: 10000,
  created_at: 0,
  signed: [],
  plans: 0,
  ...change
})

test('networks retain their numeric identity, including unknown EVM networks', () => {
  assert.equal(networkName({ asset: 'ETH', chain_id: 8453 }), 'Base (8453)')
  assert.equal(networkName({ asset: 'ERC20', chain_id: 1 }), 'Ethereum (1)')
  assert.equal(networkName({ asset: 'ETH', chain_id: 1234567 }), 'EVM (1234567)')
  assert.match(durationText(86400000, 'en'), /1 day/)
  assert.match(durationText(604800000, 'zh'), /7.*天/)
})

test('completion never turns an unknown call or partial payment into success', () => {
  assert.equal(resultState(proposal({ status: 'Settled', outcome: { kind: 'call_unknown' } }), 0), 'resultUnknown')
  assert.equal(resultState(proposal({ status: 'Failed', outcome: { kind: 'ledger', blocks: [9] } }), 0), 'partialPayment')
  assert.equal(resultState(proposal({ status: 'Settled', outcome: { kind: 'ledger', blocks: [9] } }), 0), 'ledgerConfirmed')
  assert.equal(resultState(proposal({ status: 'Settled', signed: [{ plan: 1, txid: 'tx' }] }), 0), 'chainReported')
  assert.equal(queueKind(proposal({ status: 'Settled', outcome: { kind: 'call_unknown' } }), wallet, 0), 'attention')
})

test('queues distinguish the next action and exclude expired records', () => {
  assert.equal(queueKind(proposal(), wallet, 0), 'waiting')
  assert.equal(queueKind(proposal(), wallet, 10000), 'history')
  assert.equal(queueKind(proposal({ status: 'Ready' }), wallet, 0), 'executable')
  const guarded = { ...wallet, addresses: [{ id: 0, chain: 'Icp', cosign: { threshold: 1, cosigners: [1] } }] }
  assert.equal(queueKind(proposal({ status: 'Ready' }), guarded, 0), 'cosigning')
  assert.equal(queueKind(proposal({ status: 'Broadcast' }), wallet, 0), 'confirmation')
})

test('policy examples include pending reservations and escalate above the all-voter threshold', () => {
  const p = {
    ...policy,
    tiers: [{ asset: 'icp', up_to: '1000000000', approvals: 1, delay_ms: 0, book_only: true }],
    daily_limits: [{ asset: 'icp', amount: '1000000000' }]
  }
  const equal = policyExample(p, 'icp', '100000000', '600000000', '300000000', 3)
  assert.equal(equal.total, '1000000000')
  assert.equal(equal.approvals, 1)
  assert.equal(equal.book, true)
  const above = policyExample(p, 'icp', '100000001', '600000000', '300000000', 3)
  assert.equal(above.approvals, 3)
  assert.equal(above.all, true)
})

test('policy inputs reject fractional votes, invalid timing and amounts without rounding', () => {
  assert.equal(validPolicyDraft(policy, wallet.members), true)
  assert.equal(validPolicyDraft({ ...policy, approver_threshold: 1.5 }, wallet.members), false)
  assert.equal(validPolicyDraft({ ...policy, proposal_ttl_ms: 3600 }, wallet.members), false)
  assert.equal(validPolicyDraft({ ...policy, daily_limits: [{ asset: 'icp', amount: '' }] }, wallet.members), false)
  assert.equal(toAtomic('1.00000001', 8), '100000001')
  assert.throws(() => toAtomic('1.000000001', 8))
  assert.equal(batchLimit('Icp'), 32)
  assert.equal(batchLimit('Evm'), 1)
})

test('readiness counts people who joined, never device bindings as extra votes', () => {
  const rows = teamReadiness({ ...wallet, members: [{ ...wallet.members[0], principals: ['p1', 'p2'] }, wallet.members[1]] })
  assert.deepEqual(
    rows.find((r) => r.key === 'readyVoters'),
    { key: 'readyVoters', count: '1 / 2', warn: true }
  )
  assert.equal(rows.find((r) => r.key === 'invitedMembers')?.count, '1')
  assert.ok(rows.some((r) => r.key === 'recoveryMissing'))
})

test('history export escapes notes and spreadsheet formulas, and rejects arbitrary row shapes', () => {
  const row = ['w', '1', 'confirmed', 'transfer', 'Base (8453)', 'ETH', '1 ETH', '1', 'a', 'date', 'tx', '9', '=HYPERLINK("bad")', 'scope']
  const csv = walletHistoryCsv({ format: 'dmsg-wallet-history/1', rows: [row] })
  assert.match(csv, /"'=HYPERLINK\(""bad""\)"/)
  assert.throws(() => walletHistoryCsv({ format: 'dmsg-wallet-history/1', rows: [['x']] }))
  assert.throws(() => walletHistoryCsv({ format: 'other', rows: [] }))
})

test('workflow instructions have all six translations and no duplicate keys', () => {
  const keys = new Set<string>()
  for (const row of experienceRows.trim().split('\n')) {
    const [key, ...words] = row.split('|')
    assert.ok(!keys.has(key), key)
    keys.add(key)
    assert.equal(words.length, 6, key)
    assert.ok(
      words.every((s) => s.trim()),
      key
    )
  }
})
