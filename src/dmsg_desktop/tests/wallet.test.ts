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
const other = '0123456789abcdefghih'

const draft = (key: number, id = '', role = 'approver', label = ''): MemberDraft => ({ key, id, role, label, touched: false })

test('an account ID or a Principal names a member; other text does not', () => {
  assert.equal(isMemberId(creator), true)
  assert.equal(isMemberId('xkbqi-2qaaa-aaaah-qbpqq-cai'), true)
  assert.equal(isMemberId('2vxsx-fae'), true)
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
  assert.equal(memberProblem(rows[1], rows.filter((row) => row.key !== 3), creator), null)
})

test('the policy counts the creator and the roles that approve, and no others', () => {
  const rows = [draft(1, peer, 'admin'), draft(2, other, 'approver'), draft(3, 'xkbqi-2qaaa-aaaah-qbpqq-cai', 'proposer'), draft(4, '', 'admin')]
  assert.deepEqual(thresholdLimits(rows), { admins: 2, approvers: 3 })
  assert.deepEqual(thresholdLimits([]), { admins: 1, approvers: 1 })
  assert.deepEqual(thresholdLimits([draft(1, peer, 'viewer'), draft(2, other, 'recovery')]), { admins: 1, approvers: 1 })
})

test('only named rows reach the hub, trimmed and with their role and label', () => {
  const rows = [draft(1, ` ${peer} `, 'approver', ' Alice '), draft(2, '   ', 'admin', 'ignored'), draft(3, 'xkbqi-2qaaa-aaaah-qbpqq-cai', 'viewer')]
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
