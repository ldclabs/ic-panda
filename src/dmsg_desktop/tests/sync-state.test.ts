import assert from 'node:assert/strict'
import { test } from 'node:test'
import { syncState } from '../src/renderer/sync-state.ts'
import { failureLabel } from '../src/renderer/failure.ts'

test('a failed incoming pull keeps pending uploads and the last verified success visible', () => {
  const state = syncState({ state: 'unlocked', pending_sync: 7, last_sync: { at: 200, ok_at: 100, error: 'NETWORK_UNAVAILABLE', retry_at: 220 } })
  assert.equal(state.label, 'networkUnavailable')
  assert.equal(state.pending, 7)
  assert.equal(state.okAt, 100)
  assert.equal(state.retryAt, 220)
  assert.equal(state.transient, true)
})

test('storage limits, a busy snapshot and protocol size errors have different actions', () => {
  const status = { state: 'unlocked', pending_sync: 2, last_sync: { at: 200, ok_at: 100, error: null, upload_error: 'QUOTA_EXCEEDED' } }
  assert.equal(syncState(status).label, 'syncStorageBlocked')
  assert.equal(syncState(status).okAt, 100)
  assert.equal(syncState({ ...status, last_sync: { error: 'SYNC_BUSY', retry_at: 300 } }).label, 'syncBusy')
  assert.equal(syncState({ ...status, last_sync: { error: 'RESPONSE_TOO_LARGE' } }).label, 'syncSizeError')
  assert.equal(failureLabel('SYNC_BUSY'), 'syncBusy')
  assert.equal(failureLabel('RESPONSE_TOO_LARGE'), 'syncSizeError')
})

test('locking pauses content reads and keeps saved progress; old successful records remain readable', () => {
  const state = syncState({ state: 'locked', pending_sync: 3, last_sync: { at: 200, ok_at: 100, error: 'UNAVAILABLE', retry_at: 300 } })
  assert.equal(state.label, 'locked')
  assert.equal(state.failed, false)
  assert.equal(state.pending, 3)
  assert.equal(state.okAt, 100)
  assert.equal(state.retryAt, undefined)
  assert.equal(syncState({ state: 'unlocked', last_sync: { at: 100, error: null } }).okAt, 100)
  assert.equal(syncState({ state: 'unlocked', last_sync: { at: 100, upload_error: 'QUOTA_EXCEEDED' } }).okAt, undefined)
})
