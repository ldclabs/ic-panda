import assert from 'node:assert/strict'
import { test } from 'node:test'
import { failureLabel, failureTone, temporaryFailure } from '../src/renderer/failure.ts'
import en from '../src/renderer/i18n/messages/en.ts'
import zh from '../src/renderer/i18n/messages/zh.ts'

test('temporary transport errors and uncertain submissions have safe, actionable wording', () => {
  assert.equal(failureLabel('NETWORK_UNAVAILABLE'), 'networkUnavailable')
  assert.equal(failureLabel('EXECUTION_UNKNOWN'), 'operationUnknown')
  assert.equal(failureLabel('UNAVAILABLE'), 'serviceUnavailable')
  assert.equal(failureLabel('INTEGRITY_FAILED'), undefined)
  for (const catalog of [en, zh]) {
    assert.equal(failureTone(catalog.messages.networkUnavailable), 'warn')
    assert.equal(failureTone(catalog.messages.operationUnknown), 'warn')
    assert.equal(failureTone(catalog.messages.localStorageUnavailable), 'danger')
    assert.equal(temporaryFailure(catalog.messages.networkUnavailable), true)
    assert.equal(failureTone([catalog.messages.networkUnavailable, 'INTEGRITY_FAILED']), 'danger')
  }
})
