import assert from 'node:assert/strict'
import { test } from 'node:test'
import { PreparedOperation } from '../src/main/prepared-operation.ts'

test('only the current native preview can supply a submission, exactly once', () => {
  const imports = new PreparedOperation<{ path: string; digest: string }>()
  const old = imports.begin()
  assert.equal(imports.take(old), null)
  const current = imports.begin()
  assert.equal(imports.ready(old, { path: '/old', digest: 'old' }), false)
  assert.equal(imports.ready(current, { path: '/chosen', digest: 'checked' }), true)
  imports.finish(current) // Completing preparation retains the preview for review.
  assert.equal(imports.take(old), null)
  assert.deepEqual(imports.take(current), { path: '/chosen', digest: 'checked' })
  assert.equal(imports.take(current), null)
})

test('session cancellation rejects late preparation and an old completion leaves the new preview alone', () => {
  const mounts = new PreparedOperation<{ directory: string }>()
  const cancelled = mounts.begin()
  mounts.finish(cancelled) // A cancelled dialog or failed preparation releases its turn.
  assert.equal(mounts.ready(cancelled, { directory: '/old-account' }), false)
  const submitted = mounts.begin()
  mounts.ready(submitted, { directory: '/first' })
  mounts.take(submitted)
  const current = mounts.begin()
  mounts.ready(current, { directory: '/second' })
  mounts.finish(submitted)
  assert.deepEqual(mounts.take(current), { directory: '/second' })
  mounts.finish(current)
  assert.equal(mounts.take(current), null)
})
