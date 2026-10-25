import assert from 'node:assert/strict'
import { test } from 'node:test'
import { RecoveryNotifications, type RecoveryWarning } from '../src/main/recovery-notifications.ts'

test('locked/reconnected observations notify once; new requests and a desktop restart notify again', () => {
  const shown: string[] = []
  const closed: string[] = []
  const notify = (account: string) => { shown.push(account); return () => { closed.push(account) } }
  const notices = new RecoveryNotifications(notify)
  const warning: RecoveryWarning = { account: 'a', pending: true, digest: 'one', observed_at_ms: 1, stale: false }
  notices.update('a', warning, true)
  notices.update('a', { ...warning, observed_at_ms: 2 }, true)
  notices.update('a', null, true)
  notices.update('a', { ...warning, stale: true }, true)
  assert.deepEqual(shown, ['a'])
  notices.update('a', { ...warning, digest: 'two' }, true)
  assert.deepEqual(shown, ['a', 'a'])
  assert.deepEqual(closed, ['a'])
  new RecoveryNotifications(notify).update('a', warning, true)
  assert.deepEqual(shown, ['a', 'a', 'a'])
})

test('cancellation clears a notice, while account switches, stale absence and unsupported systems do not consume one', () => {
  let shown = 0
  let closed = 0
  const notices = new RecoveryNotifications(() => { shown++; return () => { closed++ } })
  const warning: RecoveryWarning = { account: 'a', pending: true, digest: 'one', observed_at_ms: 1, stale: false }
  notices.update('a', warning, false)
  notices.update('b', warning, true)
  assert.equal(shown, 0)
  notices.update('a', warning, true)
  notices.update('b', { ...warning, account: 'b' }, true)
  notices.update('a', warning, true)
  assert.equal(shown, 2)
  notices.update('a', { ...warning, pending: false, digest: null, stale: true }, true)
  assert.equal(closed, 0)
  notices.update('a', { ...warning, pending: false, digest: null }, true)
  assert.equal(closed, 1)
  notices.update('a', warning, true)
  assert.equal(shown, 3)
})
