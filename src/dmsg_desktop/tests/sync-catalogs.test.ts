import assert from 'node:assert/strict'
import { test } from 'node:test'
import en from '../src/renderer/i18n/messages/en.ts'
import zh from '../src/renderer/i18n/messages/zh.ts'

test('local connectivity is distinct from reconnecting and cloud synchronization', () => {
  assert.match(en.messages.connected, /Local/)
  assert.match(en.messages.daemonDisconnected, /Connecting/)
  assert.match(en.messages.disconnected, /Not connected/)
  assert.match(zh.messages.connected, /本机/)
  assert.notEqual(zh.messages.connected, zh.messages.daemonDisconnected)
  assert.notEqual(zh.messages.connected, zh.messages.accountSynced)
})
