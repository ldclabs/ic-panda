import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  activationTotal,
  isMemberId,
  memberInputs,
  memberProblem,
  MEMBER_ROLES,
  namedMembers,
  thresholdLimits,
  type MemberDraft
} from '../src/renderer/views/wallet/new-wallet.ts'

const creator = '9m4e2mr0ui3e8a215n4g'
const peer = '0123456789abcdefghig'
const other = '0123456789abcdefghi0'

const draft = (key: number, id = '', role = 'approver', label = ''): MemberDraft => ({ key, id, role, label, touched: false })

test('an account ID or a Principal names a member; other text does not', () => {
  assert.equal(isMemberId(creator), true)
  assert.equal(isMemberId('xkbqi-2qaaa-aaaah-qbpqq-cai'), true)
  assert.equal(isMemberId('2vxsx-fae'), false)
  assert.equal(isMemberId('9m4e2mr0ui3e8a215n4w'), false, 'w is not in the Xid alphabet')
  assert.equal(isMemberId('Panda'), false)
  assert.equal(isMemberId('0123456789abcdefgh'), false)
  assert.equal(isMemberId('not a member'), false)
})

test('a row names its problem: the creator, a repeat, or a shape the hub refuses', () => {
  const rows = [draft(1, creator), draft(2, peer), draft(3, peer), draft(4, 'not a member'), draft(5, '')]
  assert.equal(memberProblem(rows[0], rows, creator), 'twMemberSelf')
  assert.equal(memberProblem(rows[1], rows, creator), 'twMemberDuplicate')
  assert.equal(memberProblem(rows[2], rows, creator), 'twMemberDuplicate')
  assert.equal(memberProblem(rows[3], rows, creator), 'twMemberInvalid')
  assert.equal(memberProblem(rows[4], rows, creator), null)
  // The same ID is fine once the other row goes.
  assert.equal(
    memberProblem(
      rows[1],
      rows.filter((row) => row.key !== 3),
      creator
    ),
    null
  )
})

test('the policy counts the creator and the roles that approve, and no others', () => {
  const rows = [
    draft(1, peer, 'admin'),
    draft(2, other, 'approver'),
    draft(3, 'xkbqi-2qaaa-aaaah-qbpqq-cai', 'proposer'),
    draft(4, '', 'admin')
  ]
  assert.deepEqual(thresholdLimits(rows), { admins: 2, approvers: 3 })
  assert.deepEqual(thresholdLimits([]), { admins: 1, approvers: 1 })
  assert.deepEqual(thresholdLimits([draft(1, peer, 'viewer'), draft(2, other, 'recovery')]), { admins: 1, approvers: 1 })
})

test('only named rows reach the hub, trimmed and with their role and label', () => {
  const rows = [
    draft(1, ` ${peer} `, 'approver', ' Alice '),
    draft(2, '   ', 'admin', 'ignored'),
    draft(3, 'xkbqi-2qaaa-aaaah-qbpqq-cai', 'viewer')
  ]
  assert.deepEqual(memberInputs(rows), [
    { id: peer, role: 'approver', label: 'Alice' },
    { id: 'xkbqi-2qaaa-aaaah-qbpqq-cai', role: 'viewer', label: '' }
  ])
  assert.equal(namedMembers(rows).length, 2)
  assert.deepEqual([...MEMBER_ROLES], ['admin', 'approver', 'proposer', 'viewer', 'recovery'])
})

test('the price and its ledger fee add in base units', () => {
  assert.equal(activationTotal('200000000', '10000'), '200010000')
  assert.equal(activationTotal('0', '10000'), '10000')
})

import {
  actions,
  approvalCount,
  assetIdentity,
  cyclesText,
  cosignAddresses,
  cosignerReferences,
  sameCosign,
  unreachableCosigners,
  decimal,
  knownIntent,
  leaves,
  policyDraft,
  stateAt,
  toAtomic,
  validPrincipal,
  ViewScope,
  type Proposal,
  type Wallet
} from '../src/renderer/views/wallet/model.ts'

const policy = {
  version: 1,
  approver_threshold: 2,
  admin_threshold: 2,
  spendable_assets: [{ asset: 'ICP', decimals: 8 }],
  tiers: [],
  daily_limits: []
}
const sampleWallet = (role = 'Admin'): Wallet => ({
  wallet: 'rrkah-fqaaa-aaaaa-aaaaq-cai',
  name: 'Team',
  frozen: false,
  policy,
  addresses: [],
  cosigners: [],
  members: [
    { id: creator, role, status: 'Active', agent_ids: [] },
    { id: peer, role: 'Admin', status: 'Active', agent_ids: [] }
  ],
  me: { id: creator, role, status: 'Active', agent_ids: [] }
})
const sampleProposal = (overrides: Partial<Proposal> = {}): Proposal => ({
  id: 1,
  status: 'Open',
  proposer: creator,
  intent: {
    kind: 'transfer',
    from: 0,
    asset: { asset: 'ICP' },
    outputs: [{ to: 'rrkah-fqaaa-aaaaa-aaaaq-cai', amount: '100000000' }],
    max_fee: '10000'
  },
  requirement: { approvals: 2, admin: false, delay_ms: 0 },
  votes: [],
  created_at: 0,
  expires_at: 10000,
  signed: [],
  plans: 0,
  ...overrides
})

test('decimal amounts preserve u128 precision, reject rounding, exponents and overflow', () => {
  assert.equal(toAtomic('1.000000000000000001', 18), '1000000000000000001')
  assert.equal(decimal('1000000000000000001', 18), '1.000000000000000001')
  assert.equal(toAtomic('0.00000001', 8), '1')
  assert.equal(decimal('100000000', 8), '1')
  assert.equal(decimal('100', 0), '100')
  assert.equal(toAtomic(((1n << 128n) - 1n).toString(), 0), ((1n << 128n) - 1n).toString())
  for (const value of ['1e9', '-1', '1.000000001', '1.', ' 1']) assert.throws(() => toAtomic(value, 8))
  assert.throws(() => toAtomic((1n << 128n).toString(), 0))
})

test('Cycles reserves round to two places in T without losing integer precision', () => {
  assert.equal(cyclesText('1987788848105'), '1.99')
  assert.equal(cyclesText('1995000000000'), '2.00')
  assert.equal(cyclesText('0'), '0.00')
  assert.equal(cyclesText('123456789012345678901234'), '123456789012.35')
})

test('wallet actions enforce roles, admin requirements, existing votes and freezing', () => {
  for (const state of ['Ready', 'Executing', 'Signed', 'Broadcast']) {
    const a = actions(sampleProposal({ status: state }), sampleWallet('Viewer'), 100)
    assert.equal(a.execute, false)
    assert.equal(a.rebroadcast, false)
    assert.equal(a.resign, false)
  }
  const adminProposal = sampleProposal({ requirement: { approvals: 2, admin: true, delay_ms: 0 } })
  assert.equal(actions(adminProposal, sampleWallet('Approver'), 100).approve, false)
  assert.equal(actions(adminProposal, sampleWallet('Admin'), 100).approve, true)
  assert.equal(actions(sampleProposal({ votes: [{ member: creator, decision: 'Approve', at: 10 }] }), sampleWallet(), 100).approve, false)
  const frozen = { ...sampleWallet(), frozen: true }
  assert.equal(actions(sampleProposal(), frozen, 100).approve, false)
  assert.equal(actions(sampleProposal({ intent: { kind: 'unfreeze' } }), frozen, 100).approve, true)
  assert.equal(actions(sampleProposal({ status: 'Delayed', ready_at: 1000 }), frozen, 100).veto, true)
  assert.equal(actions(sampleProposal(), { ...sampleWallet(), me: { ...sampleWallet().me!, status: 'Invited' } }, 100).approve, false)
})

test('clock transitions expire and release delayed proposals without another user action', () => {
  const p = sampleProposal({ status: 'Delayed', ready_at: 1000 })
  assert.equal(stateAt(p, 999), 'Delayed')
  assert.equal(stateAt(p, 1000), 'Ready')
  assert.equal(stateAt(p, 10000), 'Expired')
  assert.equal(actions(p, sampleWallet(), 1000).execute, true)
  assert.equal(actions(p, sampleWallet(), 10000).execute, false)
  assert.equal(stateAt({ ...p, status: 'Broadcast' }, 20000), 'Broadcast')
})

test('collected replacement signatures execute only an unsigned plan with current member permission', () => {
  const wallet = sampleWallet()
  for (const status of ['Signed', 'Broadcast']) {
    const replacement = sampleProposal({ status, plans: 2, signed: [{ plan: 1, txid: 'old' }] })
    assert.equal(actions(replacement, wallet, 100).execute, false)
    assert.equal(actions(replacement, wallet, 100).executeCollected, true)
    assert.equal(actions(replacement, { ...wallet, frozen: true }, 100).executeCollected, false)
    assert.equal(actions(replacement, sampleWallet('Viewer'), 100).executeCollected, false)
    assert.equal(actions(replacement, { ...wallet, me: { ...wallet.me!, status: 'Invited' } }, 100).executeCollected, false)
    assert.equal(actions({ ...replacement, plans: 1 }, wallet, 100).executeCollected, false)
  }
  assert.equal(actions(sampleProposal({ status: 'Ready' }), wallet, 100).executeCollected, true)
  for (const status of ['Open', 'Delayed', 'Executing', 'Settled', 'Failed', 'Expired']) {
    assert.equal(actions(sampleProposal({ status, ready_at: 1000 }), wallet, 100).executeCollected, false, status)
  }
  assert.equal(actions(sampleProposal({ status: 'Ready' }), wallet, 10000).executeCollected, false)
})

test('approval counts exclude inactive and ineligible voters', () => {
  const p = sampleProposal({
    votes: [
      { member: creator, decision: 'Approve', at: 1 },
      { member: peer, decision: 'Approve', at: 1 }
    ]
  })
  const w = sampleWallet('Approver')
  assert.equal(approvalCount(p, w.members), 2)
  assert.equal(approvalCount({ ...p, requirement: { ...p.requirement, admin: true } }, w.members), 1)
  assert.equal(
    approvalCount(
      p,
      w.members.map((m) => ({ ...m, status: 'Invited' }))
    ),
    0
  )
})

test('responses from an earlier wallet or account cannot overwrite the current view', async () => {
  const scope = new ViewScope()
  const a = scope.next()
  const b = scope.next()
  let rendered = ''
  if (scope.current(b)) rendered = 'B'
  await Promise.resolve()
  if (scope.current(a)) rendered = 'A'
  assert.equal(rendered, 'B')
  scope.clear()
  assert.equal(scope.current(b), false)
})

test('principals need canonical encoding and checksum; forbidden members are refused', () => {
  assert.equal(validPrincipal('rrkah-fqaaa-aaaaa-aaaaq-cai'), true)
  assert.equal(validPrincipal('ryjl3-tyaaa-aaaaa-aaaba-cai'), true)
  for (const id of ['a', 'panda', 'rrkah-fqaaa-aaaaa-aaaaq-caa', 'rrkahfqaaaaaaaaaaaaaqcai']) assert.equal(validPrincipal(id), false, id)
  for (const id of ['2vxsx-fae', 'aaaaa-aa', '0123456789abcdefghih']) assert.equal(isMemberId(id), false, id)
})

test('approval rendering refuses unknown/incomplete intents and includes false and empty changes', () => {
  assert.equal(knownIntent({ kind: 'future_transfer' }), false)
  assert.equal(knownIntent({ kind: 'transfer', asset: { asset: 'ICP' } }), false)
  assert.equal(knownIntent(sampleProposal().intent), true)
  assert.equal(knownIntent({ kind: 'set_policy', policy }), true)
  const fields = leaves({ public: false, agent_ids: [], cosign: null })
  assert.deepEqual(
    fields.map((x) => x.value),
    ['false', '—', '—']
  )
  const draft = policyDraft(policy)
  draft.spendable_assets.push('btc')
  assert.equal(policy.spendable_assets.length, 1)
  assert.equal(draft.version, 2)
  assert.match(assetIdentity({ asset: 'ICRC', symbol: 'PANDA', ledger: 'ledger-A' }), /ledger-A/)
})

import { explorerUrl } from '../src/renderer/views/wallet/model.ts'
test('transaction links name a known production explorer only', () => {
  const hash = 'ab'.repeat(32)
  assert.equal(explorerUrl({ asset: 'ETH', chain_id: 1 }, `0x${hash}`, 'production'), `https://etherscan.io/tx/0x${hash}`)
  assert.equal(explorerUrl({ asset: 'ETH', chain_id: 1 }, `0x${hash}`, 'local'), null)
  assert.equal(explorerUrl({ asset: 'ETH', chain_id: 31337 }, `0x${hash}`, 'production'), null)
  assert.equal(explorerUrl({ asset: 'BTC' }, '../../javascript:alert(1)', 'production'), null)
})

import { intentFields, utf8Length } from '../src/renderer/views/wallet/model.ts'
test('every supported management intent exposes all committed fields for review', () => {
  const member = { id: creator, role: 'Admin', label: 'Operator', agent_ids: ['did:agent:example'] }
  const intents = [
    { kind: 'canister_call', canister: 'rrkah-fqaaa-aaaaa-aaaaq-cai', method: 'transfer', arg: '00', cycles: '1', consent: null },
    { kind: 'add_address', chain: 'Bitcoin', control: 'ChainKey', label: 'Income', cosign: null },
    { kind: 'set_cosign', address: 1, cosign: { threshold: 2, cosigners: [1, 2] } },
    {
      kind: 'add_cosigner',
      member: creator,
      curve: 'P256',
      public_key: '02' + '11'.repeat(32),
      envelope: 'Raw',
      hardware: false,
      label: 'Key'
    },
    { kind: 'remove_cosigner', cosigner: 2 },
    { kind: 'add_member', member },
    { kind: 'update_member', member },
    { kind: 'remove_member', member: creator },
    { kind: 'set_policy', policy },
    { kind: 'book_add', destination: 'destination', label: 'Vendor' },
    { kind: 'book_remove', destination: 'destination' },
    { kind: 'unfreeze' },
    { kind: 'refuel', from: 0, icp_e8s: '100000000' },
    { kind: 'accept_upgrade', version: 2, module_hash: '11'.repeat(32) },
    { kind: 'lock_code' },
    { kind: 'cancel_evm_nonce', address: 2, chain_id: 1, max_fee: '1000000000000000' },
    { kind: 'release_evm_nonce', address: 2, chain_id: 1 },
    { kind: 'init_sol_nonce', address: 3, max_fee: '10000' }
  ]
  for (const intent of intents) {
    assert.equal(knownIntent(intent), true, intent.kind)
    for (const key of Object.keys(intent).filter((k) => k !== 'kind'))
      assert.ok(intentFields(intent).includes(key), `${intent.kind}.${key}`)
  }
  assert.equal(utf8Length('中'.repeat(21)), 63)
  assert.equal(utf8Length('中'.repeat(22)), 66)
})

test('cosigning applies to spending sources and all ICP addresses of a general call, not management proposals', () => {
  const wallet = sampleWallet('Admin')
  wallet.addresses = [
    { id: 0, chain: 'Icp', cosign: { threshold: 1, cosigners: [1] } },
    { id: 1, chain: 'Bitcoin', cosign: { threshold: 2, cosigners: [1, 2] } },
    { id: 2, chain: 'Icp', cosign: { threshold: 1, cosigners: [2] } }
  ]
  wallet.cosigners = [{ id: 1, hardware: false }, { id: 2, hardware: true }]
  assert.deepEqual(cosignAddresses({ kind: 'set_cosign', address: 1 }, wallet), [])
  assert.deepEqual(cosignAddresses({ kind: 'canister_call' }, wallet).map((a) => a.id), [0, 2])
  assert.deepEqual(cosignAddresses({ kind: 'transfer', from: 1 }, wallet).map((a) => a.id), [1])
  assert.deepEqual(cosignerReferences(wallet, [1]).map((a) => a.id), [0, 1])
  assert.deepEqual(unreachableCosigners(wallet, true).map((a) => a.id), [0, 1])
  assert.deepEqual(unreachableCosigners(wallet, false), [])
  assert.equal(sameCosign({threshold:2,cosigners:[1,2]}, {threshold:2,cosigners:[2,1]}), true)
  assert.equal(sameCosign(wallet.addresses[0].cosign, null), false)
})
