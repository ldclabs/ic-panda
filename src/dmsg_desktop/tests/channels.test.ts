import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  arrange,
  days,
  initials,
  invitationLink,
  mayRotate,
  ordered,
  packetStep,
  powers,
  readInvitation,
  receiptOf,
  tone
} from '../src/renderer/views/channels/channels.ts'

const ledger = {
  owner: 'o',
  status: 'active',
  epoch: 2,
  members: {
    m: { account: 'm', role: 'member', joined_epoch: 1, can_rotate: false },
    o: { account: 'o', role: 'owner', joined_epoch: 1, can_rotate: true },
    a: { account: 'a', role: 'admin', joined_epoch: 2, can_rotate: false },
    p: { account: 'p', role: 'publisher', joined_epoch: 1, can_rotate: true }
  }
}

test('the owner and admins manage; members write while the channel is active', () => {
  assert.deepEqual(powers(ledger, 'o'), { member: true, owner: true, manages: true, rotates: true, writes: true })
  assert.deepEqual(powers(ledger, 'a'), { member: true, owner: false, manages: true, rotates: true, writes: true })
  assert.deepEqual(powers(ledger, 'm'), { member: true, owner: false, manages: false, rotates: false, writes: true })
  assert.deepEqual(powers({ ...ledger, status: 'archived' }, 'm').writes, false)
  assert.deepEqual(powers(ledger, 'x'), { member: false, owner: false, manages: false, rotates: false, writes: false })
})

test('the owner, admins and allowed members holding the epoch rotate', () => {
  assert.ok(mayRotate(ledger, 'p'))
  // A member waiting for the next epoch does not coordinate it.
  assert.ok(!mayRotate({ ...ledger, epoch: 1 }, 'a'))
  assert.ok(!mayRotate(ledger, undefined))
})

test('members list the owner first, then by role', () => {
  assert.deepEqual(
    ordered(ledger).map((m) => m.account),
    ['o', 'a', 'p', 'm']
  )
})

test('a timeline splits into local days, oldest first', () => {
  const day = (d: number, h: number) => new Date(2026, 9, d, h).getTime()
  const groups = days([{ stored_at: day(16, 9) }, { stored_at: day(15, 23) }, { stored_at: day(16, 1) }])
  assert.deepEqual(
    groups.map((g) => g.messages.length),
    [1, 2]
  )
  assert.ok(groups[1].messages[0].stored_at < groups[1].messages[1].stored_at)
})

test('avatars show a name or an ID and keep their tone', () => {
  assert.equal(initials('release team'), 'R')
  assert.equal(initials('0123456789abcdefghi0'), '01')
  assert.equal(initials(' '), '?')
  assert.equal(tone('abc'), tone('abc'))
  assert.ok(tone('abc') < 4)
})

test('a distribution channel lets its members read and its publishers write', () => {
  const distribution = { ...ledger, type: 'distribution' }
  assert.equal(powers(distribution, 'm').writes, false)
  assert.equal(powers(distribution, 'p').writes, true)
  assert.equal(powers(distribution, 'o').writes, true)
})

test('the list puts joined channels by latest activity, then the others', () => {
  const joined = { owner: 'o', status: 'active', members: { me: {} } }
  const list = arrange(
    [
      { id: 'a', name: 'b', ledger: joined, last: { stored_at: 1 } },
      { id: 'b', name: 'a', ledger: joined, last: { stored_at: 5 } },
      { id: 'c', name: 'c', ledger: { ...joined, status: 'archived' } },
      { id: 'd', name: 'd', ledger: { ...joined, members: {} } },
      { id: 'e', name: 'e', ledger: joined, hidden: true }
    ],
    'me'
  )
  assert.deepEqual(
    list.live.map((c) => c.id),
    ['b', 'a']
  )
  assert.deepEqual(
    list.other.map((c) => c.id),
    ['c', 'd', 'e']
  )
})

test('a message of this account counts the members that took and read it', () => {
  const receipts = { cursors: { m: { delivered: 5, read: 4 }, a: { delivered: 3, read: null } } }
  assert.deepEqual(receiptOf(4, receipts, ledger, 'o'), { delivered: 1, read: 1, others: 3 })
  assert.deepEqual(receiptOf(3, receipts, ledger, 'o'), { delivered: 2, read: 1, others: 3 })
})

test('an invitation goes through its link and back, and its JSON reads too', () => {
  const invitation = { format: 'dmsg-channel-invitation/1', channel: 'c', name: '团队 ✓' }
  const link = invitationLink(invitation)
  assert.ok(link.startsWith('dmsg://channel/join#') && !/[+/=]/.test(link.slice(20)))
  assert.deepEqual(readInvitation(link), invitation)
  assert.deepEqual(readInvitation(JSON.stringify(invitation)), invitation)
  assert.equal(readInvitation('dmsg://channel/join#bm90IGFuIGludml0YXRpb24'), null)
})

test('a transfer packet asks its target to accept and its proposer to commit', () => {
  const proposal = { format: 'dmsg-channel-owner-transfer/1', payload: { action: { account: 'm' } }, context: { account_id: 'o' }, acceptance: null }
  assert.equal(packetStep(proposal, ledger, null, 'm')?.step, 'accept')
  assert.equal(packetStep(proposal, ledger, null, 'a')?.step, 'wait')
  const accepted = { ...proposal, acceptance: 'sig' }
  assert.equal(packetStep(accepted, ledger, null, 'o')?.step, 'commit')
  assert.equal(packetStep(accepted, { ...ledger, owner: 'm' }, null, 'o')?.step, 'done')
  const sponsor = { format: 'dmsg-channel-sponsor/1', payload: { to_payer: 'm' }, context: { account_id: 'o' }, acceptance: 'sig' }
  assert.equal(packetStep(sponsor, ledger, { billing_account: 'm' }, 'o')?.step, 'done')
  assert.equal(packetStep({ format: 'other' }, ledger, null, 'o'), null)
})
