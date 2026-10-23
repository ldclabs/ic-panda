import assert from 'node:assert/strict'
import { test } from 'node:test'
import { controllerErrors, controllerParams } from '../src/renderer/views/agents/controllers.ts'

test('a blank controller form never grants unrestricted authority', () => {
  assert.deepEqual(Object.keys(controllerErrors('', '', '')), ['scopes', 'audiences'])
  assert.deepEqual(controllerParams('', '', ''), { name: null, scopes: [''], audiences: [''] })
  assert.deepEqual(controllerErrors('dMsg default signer', '', '', true), {})
  assert.deepEqual(controllerParams('dMsg default signer', '', '', true), { name: 'dMsg default signer', unrestricted: true })
})

test('controller audiences use the protocol validator and exact origin syntax', () => {
  for (const audience of ['hello', 'dmsg.net', 'http://example.com', 'https://example.com/path', 'https://example.com/', 'https://example.com?x=1', '*', 'did:agent:garbage', 'https://example.com,https://example.com', 'https://example.com,']) {
    assert.equal(controllerErrors('', 'app.read', audience).audiences, 'controllerAudiencesInvalid', audience)
  }
  for (const audience of ['https://dmsg.net', 'did:agent:' + 'A'.repeat(43)]) {
    assert.deepEqual(controllerErrors('', 'app.read', audience), {})
  }
})

test('restricted ceilings reject duplicates, wildcards, empty entries and byte/count overflow', () => {
  for (const scope of ['', '*', 'app.*', 'app read', 'app.\u0000read', 'app.read,', 'app.read,app.read', '界'.repeat(22), Array.from({ length: 9 }, (_, i) => `app.${i}`).join(',')]) {
    assert.equal(controllerErrors('', scope, 'https://example.com').scopes, 'controllerScopesInvalid', scope)
  }
  assert.ok(controllerErrors('', 'read', Array.from({ length: 5 }, (_, i) => `https://${i}.example.com`).join(',')).audiences)
  assert.ok(controllerErrors('界'.repeat(22), '', '', true).name)
  assert.ok(controllerErrors('bad\nname', '', '', true).name)
  assert.deepEqual(controllerErrors('界'.repeat(21), '界'.repeat(21), 'https://example.com'), {})
  assert.deepEqual(controllerParams(' My key ', ' app.read, app.write ', ' https://example.com '), {
    name: 'My key', scopes: ['app.read', 'app.write'], audiences: ['https://example.com']
  })
})
