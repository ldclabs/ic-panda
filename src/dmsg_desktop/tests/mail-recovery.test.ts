import assert from 'node:assert/strict'
import test from 'node:test'
import { recoverMailboxes } from '../src/shared/mail-recovery.ts'

test('recovers every page from each selected relay without duplicating requests', async () => {
  const requests: unknown[] = []
  assert.equal(await recoverMailboxes(['https://a.test', ' https://a.test ', 'https://b.test'], async (input) => {
    requests.push(input)
    return input.host === 'https://a.test' && !input.cursor ? { next_cursor: 'opaque-next' } : {}
  }), true)
  assert.deepEqual(requests, [
    { host: 'https://a.test', limit: 100 },
    { host: 'https://a.test', limit: 100, cursor: 'opaque-next' },
    { host: 'https://b.test', limit: 100 }
  ])
})
test('stops on repeated cursors and when the selected Agent changes', async () => {
  await assert.rejects(recoverMailboxes(['https://a.test'], async () => ({ next_cursor: 'repeat' })), /did not advance/)
  let active = true, calls = 0
  assert.equal(await recoverMailboxes(['https://a.test'], async () => { calls++; active = false; return { next_cursor: 'next' } }, () => active), false)
  assert.equal(calls, 1)
})
