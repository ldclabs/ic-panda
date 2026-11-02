import assert from 'node:assert/strict'
import { test } from 'node:test'
import { textFor } from '../src/main/text.ts'
import { describe, stopForUpdate } from '../src/main/update.ts'

test('an update waits for work in flight, then defers without interrupting when declined', async () => {
  let clock = 0
  const calls: unknown[] = []
  const busy = { stopped: false, in_flight: { approvals: 1, root_rotation: true } }
  const prompts: unknown[] = []
  const result = await stopForUpdate(
    async (method, params) => {
      calls.push([method, params])
      return busy
    },
    async (inFlight) => { prompts.push(inFlight); return false },
    5_000,
    async (ms) => void (clock += ms),
    () => clock
  )
  assert.equal(result, false)
  assert.deepEqual(prompts, [busy.in_flight])
  assert.equal(
    describe(busy.in_flight, textFor('en-US')),
    '1 approved request(s) whose outcome is not settled, a root rotation'
  )
  assert.equal(describe(busy.in_flight, textFor('zh-CN')), '1 个已批准、结果尚未确定的请求、一次换根')
  // Asked at 0, 2 and 4 seconds, and once more past the deadline.
  assert.equal(calls.length, 4)
  assert.deepEqual(calls[0], ['daemon/stop', { if_idle: true }])
  assert.ok(calls.every((call) => (call as [string, { if_idle: boolean }])[1].if_idle))
})

test('an update proceeds without prompting once the daemon confirms it stopped', async () => {
  const replies = [{ stopped: false, in_flight: { account_update: true } }, { stopped: true }]
  const stopped = await stopForUpdate(async () => replies.shift(), async () => assert.fail('must not prompt'), 60_000, async () => {})
  assert.equal(stopped, true)
  assert.equal(describe({ account_update: true }, textFor('en')), 'an account update whose outcome is not known')
})

test('a stuck root job can be interrupted only after an explicit choice, with a confirmed stop', async () => {
  const calls: unknown[] = []
  let confirm!: (interrupt: boolean) => void
  const choice = new Promise<boolean>((resolve) => { confirm = resolve })
  const stopped = stopForUpdate(async (method, params) => {
    calls.push([method, params])
    return (params as { if_idle: boolean }).if_idle
      ? { stopped: false, in_flight: { root_rotation: true } }
      : { stopped: true }
  }, async (inFlight) => {
    assert.deepEqual(inFlight, { root_rotation: true })
    return choice
  }, 0)
  await new Promise((resolve) => setImmediate(resolve))
  assert.deepEqual(calls, [['daemon/stop', { if_idle: true }]])
  confirm(true)
  assert.equal(await stopped, true)
  assert.deepEqual(calls, [['daemon/stop', { if_idle: true }], ['daemon/stop', { if_idle: false }]])
})

test('RPC failures and malformed replies never authorize installation', async () => {
  for (const code of ['UNAVAILABLE', 'EXECUTION_UNKNOWN', 'FORBIDDEN']) {
    const failure = new Error(code)
    await assert.rejects(stopForUpdate(async () => { throw failure }, async () => assert.fail('must not prompt')), failure)
  }
  for (const reply of [undefined, null, {}, { stopped: 'true' }]) {
    await assert.rejects(stopForUpdate(async () => reply, async () => assert.fail('must not prompt')), /did not confirm/)
  }
  for (const reply of [undefined, { stopped: false }]) {
    const replies = [{ stopped: false, in_flight: { root_rotation: true } }, reply]
    await assert.rejects(stopForUpdate(async () => replies.shift(), async () => true, 0), /did not confirm/)
  }
  let calls = 0
  await assert.rejects(stopForUpdate(async () => {
    if (++calls === 1) return { stopped: false, in_flight: { root_rotation: true } }
    throw new Error('EXECUTION_UNKNOWN')
  }, async () => true, 0), /EXECUTION_UNKNOWN/)
})
