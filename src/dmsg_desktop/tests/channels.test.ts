import assert from 'node:assert/strict'
import { test } from 'node:test'
import { days, initials, ordered, powers, tone } from '../src/renderer/views/channels/channels.ts'

const ledger = {
  owner: 'o',
  status: 'active',
  members: {
    m: { account: 'm', role: 'member' },
    o: { account: 'o', role: 'owner' },
    a: { account: 'a', role: 'admin' },
    p: { account: 'p', role: 'publisher' }
  }
}

test('the owner and admins manage; members write while the channel is active', () => {
  assert.deepEqual(powers(ledger, 'o'), { member: true, owner: true, manages: true, writes: true })
  assert.deepEqual(powers(ledger, 'a'), { member: true, owner: false, manages: true, writes: true })
  assert.deepEqual(powers(ledger, 'm'), { member: true, owner: false, manages: false, writes: true })
  assert.deepEqual(powers({ ...ledger, status: 'archived' }, 'm').writes, false)
  assert.deepEqual(powers(ledger, 'x'), { member: false, owner: false, manages: false, writes: false })
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
