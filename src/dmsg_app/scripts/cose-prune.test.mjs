import { test } from 'node:test'
import assert from 'node:assert/strict'
import { prunePass } from './cose-prune.mjs'

const cursor = (n) => [Uint8Array.of(n)]

test('follows next_after until a short page ends the pass', async () => {
  const calls = []
  const pages = [
    { next_after: cursor(1), homes_scanned: 64, results_removed: 5 },
    { next_after: cursor(2), homes_scanned: 64, results_removed: 0 },
    { next_after: [], homes_scanned: 3, results_removed: 2 }
  ]
  const cose = { prune_executions: async (after) => (calls.push(after), pages.shift()) }
  assert.deepEqual(await prunePass(cose), { pages: 3, removed: 7 })
  assert.deepEqual(calls, [[], cursor(1), cursor(2)])
})

test('names the cursor to resume from when a page fails', async () => {
  let first = true
  const cose = {
    prune_executions: async () => {
      if (first) {
        first = false
        return { next_after: cursor(0xab), homes_scanned: 64, results_removed: 0 }
      }
      throw new Error('boundary node timeout')
    }
  }
  await assert.rejects(prunePass(cose), /\(resume with --after ab\)$/)
})
