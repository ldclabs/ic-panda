import assert from 'node:assert/strict'
import { test } from 'node:test'
import { single } from '../src/shared/single.ts'

test('calls while a run is under way ask for one more run, however many there are', async () => {
  const started: number[] = []
  let release: () => void = () => {}
  const run = single(async () => {
    started.push(started.length)
    if (started.length === 1) await new Promise<void>((resolve) => (release = resolve))
  })
  const first = run()
  const second = run()
  const third = run()
  assert.equal(started.length, 1)
  release()
  await Promise.all([first, second, third])
  assert.equal(started.length, 2)
  await run()
  assert.equal(started.length, 3)
})

test('a failed run lets the next call start afresh', async () => {
  let fail = true
  let runs = 0
  const run = single(async () => {
    runs++
    if (fail) throw new Error('down')
  })
  await assert.rejects(run(), /down/)
  fail = false
  await run()
  assert.equal(runs, 2)
})
