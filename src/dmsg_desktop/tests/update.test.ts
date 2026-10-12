import assert from 'node:assert/strict'
import { test } from 'node:test'
import { describe, stopForUpdate } from '../src/main/update.ts'

test('an update waits for work in flight, then gives up and says what holds it', async () => {
  let clock = 0
  const calls: unknown[] = []
  const busy = { stopped: false, in_flight: { approvals: 1, root_rotation: true } }
  const result = await stopForUpdate(
    async (method, params) => {
      calls.push([method, params])
      return busy
    },
    5_000,
    async (ms) => void (clock += ms),
    () => clock
  )
  assert.deepEqual(result, {
    stopped: false,
    busy: '1 approved request(s) whose outcome is not settled, a root rotation'
  })
  // Asked at 0, 2 and 4 seconds, and once more past the deadline.
  assert.equal(calls.length, 4)
  assert.deepEqual(calls[0], ['daemon/stop', { if_idle: true }])
})

test('an update proceeds once the daemon stopped, or when none answers', async () => {
  const replies = [{ stopped: false, in_flight: { account_update: true } }, { stopped: true }]
  const stopped = await stopForUpdate(async () => replies.shift(), 60_000, async () => {})
  assert.deepEqual(stopped, { stopped: true })
  const gone = await stopForUpdate(async () => {
    throw new Error('UNAVAILABLE: Not connected to the dmsg daemon')
  })
  assert.deepEqual(gone, { stopped: true })
  assert.equal(describe({ account_update: true }), 'an account update whose outcome is not known')
})
