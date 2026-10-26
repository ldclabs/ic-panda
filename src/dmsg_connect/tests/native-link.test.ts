import { test } from 'node:test'
import assert from 'node:assert/strict'
import { nativeLink } from '../src/native-link.ts'

test('native hints contain only a validated account and original request ID', () => {
  const account = '040g2081040g2081040g', request = 'a'.repeat(64)
  assert.equal(nativeLink('request', account, request), `https://connect.dmsg.net/app/request?account=${account}&request=${request}`)
  assert.equal(nativeLink('pair', undefined, undefined, false), 'dmsg://connect/pair')
  assert.throws(() => nativeLink('pair', account + '&callback=https://evil.test'))
  assert.throws(() => nativeLink('request', account, '../payload'))
  assert.throws(() => nativeLink('pair', account, request))
})
