import assert from 'node:assert/strict'
import { test } from 'node:test'
import { setImmediate } from 'node:timers/promises'
import { MessageDeliveries, uncertainSend, type Delivery } from '../src/renderer/message-delivery.ts'

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}
const item = (id: string) => ({ message_id: id, draft: { text: id } })

test('all drafts persist immediately while network sends keep channel order across view lifetimes', async () => {
  let rows: Delivery[] = []
  const queue = new MessageDeliveries(next => { rows = next }), first = deferred<{ message: any }>()
  const prepared: string[] = [], sent: string[] = []
  const one = queue.submit('channel', item('one'), async () => { prepared.push('one') }, () => { sent.push('one'); return first.promise })
  const two = queue.submit('channel', item('two'), async () => { prepared.push('two') }, async () => { sent.push('two'); return { message: { seq: 2 } } })
  assert.equal(queue.submit('channel', item('one'), async () => { assert.fail('duplicate preparation') }, async () => { assert.fail('duplicate send') }), one)
  assert.deepEqual(prepared, ['one', 'two'])
  await setImmediate()
  assert.deepEqual(sent, ['one'])
  assert.deepEqual(rows.map(row => row.state), ['sending', 'queued'])
  first.resolve({ message: { seq: 1 } })
  await Promise.all([one, two])
  assert.deepEqual(sent, ['one', 'two'])
  assert.deepEqual(rows.map(row => row.message.seq), [1, 2])
})

test('session reset stops queued publication and ignores in-flight old-account replies', async () => {
  let rows: Delivery[] = []
  const queue = new MessageDeliveries(next => { rows = next }), old = deferred<{ message: any }>()
  const one = queue.submit('old', item('one'), async () => {}, () => old.promise)
  const two = queue.submit('old', item('two'), async () => {}, async () => { assert.fail('old queued send ran') })
  await setImmediate()
  queue.reset()
  assert.equal(rows.length, 0)
  old.resolve({ message: { seq: 1 } })
  await Promise.all([one, two])
  assert.equal(rows.length, 0)
  await queue.submit('new', item('one'), async () => {}, async () => ({ message: { seq: 4 } }))
  assert.equal(rows[0].channel, 'new')
})

test('unconfirmed outcomes keep their original ID and stale snapshots cannot undo an ACK', async () => {
  let rows: Delivery[] = []
  const queue = new MessageDeliveries(next => { rows = next })
  await queue.submit('channel', item('same'), async () => {}, async () => { throw { code: 'EXECUTION_UNKNOWN' } })
  assert.equal(rows[0].message_id, 'same')
  assert.equal(rows[0].draft?.text, 'same')
  assert.equal(rows[0].error, 'EXECUTION_UNKNOWN')
  assert.ok(uncertainSend(rows[0]))
  const old = queue.revision
  await queue.submit('channel', rows[0], async () => {}, async () => ({ message: { seq: 3 } }))
  queue.loaded('channel', [3], old)
  assert.equal(rows[0].state, 'confirmed')
  queue.loaded('channel', [3], queue.revision)
  assert.deepEqual(rows, [])
})

test('disconnection before a queued turn retains its draft without publishing into an unverified session', async () => {
  let rows: Delivery[] = [], ready = true
  const queue = new MessageDeliveries(next => { rows = next }, () => ready), first = deferred<{ message: any }>()
  const one = queue.submit('channel', item('one'), async () => {}, () => first.promise)
  const two = queue.submit('channel', item('two'), async () => {}, async () => { assert.fail('unverified session') })
  await setImmediate()
  ready = false
  first.resolve({ message: { seq: 1 } })
  await Promise.all([one, two])
  assert.equal(rows[1].state, 'failed')
  assert.equal(rows[1].draft?.text, 'two')
  assert.ok(uncertainSend(rows[1]))
  assert.equal(uncertainSend({ ...item('refused'), error: 'FORBIDDEN' }), false)
})

test('a returning connection resends failed sends without a verdict under their own IDs', async () => {
  let rows: Delivery[] = [], online = true
  const queue = new MessageDeliveries(next => { rows = next }, () => online)
  const sent: string[] = []
  let fail: Record<string, string> = { lost: 'NETWORK_UNAVAILABLE', refused: 'DEVICE_NOT_APPROVED' }
  const work = (id: string) => async () => {
    sent.push(id)
    if (fail[id]) throw { code: fail[id] }
    return { message: { seq: sent.length } }
  }
  await queue.submit('channel', item('lost'), async () => {}, work('lost'))
  await queue.submit('channel', item('refused'), async () => {}, work('refused'))
  assert.deepEqual(rows.map(row => row.state), ['failed', 'failed'])
  fail = {}

  const later = Date.now() + 31_000
  queue.resume(Date.now())
  online = false
  queue.resume(later)
  await setImmediate()
  assert.deepEqual(sent, ['lost', 'refused'], 'too soon, or still offline')

  online = true
  queue.resume(later)
  await setImmediate()
  await setImmediate()
  assert.deepEqual(sent, ['lost', 'refused', 'lost'], 'a refusal stays for the person')
  assert.deepEqual(rows.map(row => row.state), ['confirmed', 'failed'])

  queue.resume(later + 25 * 60 * 60 * 1000)
  await setImmediate()
  assert.equal(sent.length, 3)
})

test('a send from an earlier run is adopted once and only while recent and undecided', () => {
  let online = true
  const queue = new MessageDeliveries(() => {}, () => online)
  const now = Date.now(), recent = { ...item('recent'), created_at: now - 1000 }
  assert.equal(queue.adopt('channel', recent, now), true)
  assert.equal(queue.adopt('channel', recent, now), false)
  assert.equal(queue.adopt('channel', { ...item('old'), created_at: now - 25 * 60 * 60 * 1000 }, now), false)
  assert.equal(queue.adopt('channel', { ...item('refused'), created_at: now, error: 'DEVICE_NOT_APPROVED' }, now), false)
  online = false
  const waiting = { ...item('waiting'), created_at: now }
  assert.equal(queue.adopt('channel', waiting, now), false)
  online = true
  assert.equal(queue.adopt('channel', waiting, now), true, 'the daemon was away, so this send is still open')
})
