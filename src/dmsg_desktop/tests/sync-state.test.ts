import assert from 'node:assert/strict'
import { test } from 'node:test'
import { syncShown, syncState } from '../src/renderer/sync-state.ts'
import { failureLabel } from '../src/renderer/failure.ts'

test('a failed incoming pull keeps pending uploads and the last verified success visible', () => {
  const state = syncState({ state: 'unlocked', pending_sync: 7, last_sync: { at: 200, ok_at: 100, error: 'NETWORK_UNAVAILABLE', failures: 4, retry_at: 220 } })
  assert.equal(state.label, 'networkUnavailable')
  assert.equal(state.pending, 7)
  assert.equal(state.okAt, 100)
  assert.equal(state.retryAt, 220)
  assert.equal(state.transient, true)
})

test('a network failure retrying by itself stays quiet until it lasts; other failures speak at once', () => {
  const failure = (error: string, failures: number) => ({ state: 'unlocked', pending_sync: 2, last_sync: { at: 200, ok_at: 100, error, failures, retry_at: 220 } })
  const quiet = syncState(failure('UNAVAILABLE', 3))
  assert.equal(quiet.failed, false)
  assert.equal(quiet.label, 'syncPending')
  assert.equal(quiet.retryAt, undefined)
  assert.equal(syncState(failure('UNAVAILABLE', 4)).failed, true)
  assert.equal(syncState(failure('INTEGRITY_FAILED', 1)).failed, true)
  assert.equal(syncState(failure('SYNC_BUSY', 1)).label, 'syncBusy')
})

test('a background pass is shown when asked for, the first time, or when it takes long', () => {
  const running = { syncing: true, sync_initialized: true }
  assert.equal(syncShown(running, false, 1000, 5999), false)
  assert.equal(syncShown(running, false, 1000, 6000), true)
  assert.equal(syncShown(running, true, undefined, 0), true)
  assert.equal(syncShown({ syncing: true, sync_initialized: false }, false, undefined, 0), true)
  assert.equal(syncShown({ syncing: false }, false, 1000, 60_000), false)
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

test('a new device never claims a verified empty or synced account before its baseline', () => {
  const status = { state: 'unlocked', sync_initialized: false, last_sync: { ok_at: 100 } }
  assert.equal(syncState(status).label, 'syncInitialWaiting')
  const progress = { stage: 'download', entries: 17, total: 240, downloaded_bytes: 262144 }
  const active = syncState({ ...status, syncing: true, sync_progress: progress })
  assert.equal(active.label, 'syncInitial')
  assert.equal(active.stage, 'syncDownload')
  assert.deepEqual(active.progress, progress)
  assert.equal(syncState({ ...status, sync_initialized: true }).label, 'accountSynced')
})

test('server cooldown expires without a new status response and never blocks ordinary owner retries', () => {
  const status = { state: 'unlocked', last_sync: { error: 'SYNC_BUSY', retry_at: 5000 } }
  assert.equal(syncState(status, 4999).coolingDown, true)
  assert.equal(syncState(status, 5000).coolingDown, false)
  assert.equal(syncState({ ...status, last_sync: { error: 'NETWORK_UNAVAILABLE', retry_at: 5000 } }, 4999).coolingDown, false)
})
