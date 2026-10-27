import assert from 'node:assert/strict'
import { test } from 'node:test'
import { httpsUrl, profileChanged, profileText } from '../src/renderer/views/profile-form.ts'
import type { PrincipalDocument } from 'agent-protocols/delegation'

const original = { id: 'https://dmsg.test/account', name: 'Person', links: [], extra: { one: 1, two: 2 } } as unknown as PrincipalDocument

test('profile dirty comparison ignores authority metadata and object key order, but includes editable data', () => {
  assert.equal(profileChanged(original, { ...original, updated_at: 99, name: ' Person ', extra: { two: 2, one: 1 } }), false)
  assert.equal(profileChanged(original, { ...original, extra: { ...original.extra, 'dmsg.link_images': {} } }), false)
  assert.equal(profileChanged(original, { ...original, name: 'Changed' }), true)
  assert.equal(profileChanged(original, { ...original, links: [{ name: 'Home', rel: 'related', url: 'https://example.test' }] }), true)
  assert.equal(profileChanged(original, { ...original, avatar_url: 'https://example.test/image.png' }), true)
})

test('profile URL checks mirror the public HTTPS contract', () => {
  assert.equal(httpsUrl('https://example.test/路径?q=a#b'), true)
  for (const value of ['', 'http://example.test', 'https://user:pass@example.test', 'https://example.test/\n', 'not a url', `https://example.test/${'x'.repeat(2048)}`]) assert.equal(httpsUrl(value), false, value.slice(0, 50))
})

test('profile length validation counts Unicode characters and allows only biography line breaks', () => {
  assert.equal(profileText('🙂'.repeat(80), 80), true)
  assert.equal(profileText('🙂'.repeat(81), 80), false)
  assert.equal(profileText('one\ntwo', 2000, true), true)
  assert.equal(profileText('one\ntwo', 80), false)
  assert.equal(profileText('one\ttwo', 2000, true), false)
})
