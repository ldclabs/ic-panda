import { test } from 'node:test'
import assert from 'node:assert/strict'
import { trayVariant } from '../src/main/tray-state.ts'

test('locking and unlocking preserve the independent approval badge', () => {
  assert.equal(trayVariant('unlocked', 0), 'idle')
  assert.equal(trayVariant('locked', 0), 'locked')
  assert.equal(trayVariant('locked', 2), 'lockedWaiting')
  assert.equal(trayVariant('unlocked', 2), 'waiting')
  assert.equal(trayVariant('unlocked', 0), 'idle')
})

test('unknown, disconnected and setup states do not claim a confirmed lock', () => {
  for (const state of ['connecting', 'unknown', 'logged_out', 'pairing_required', 'no_account', 'root_required']) {
    assert.equal(trayVariant(state, 0), 'idle')
    assert.equal(trayVariant(state, 1), 'waiting')
  }
})
