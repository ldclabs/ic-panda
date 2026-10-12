import assert from 'node:assert/strict'
import { test } from 'node:test'
import { externalAllowed, parseLink, windowParams, WINDOW_METHODS } from '../src/main/policy.ts'

test('windows never decide or grant, and never send human', () => {
  for (const method of ['approval/respond', 'connect/grant', 'daemon/stop', 'workspace/reset', 'session/login'])
    assert.equal(WINDOW_METHODS.has(method), false, method)
  assert.throws(() => windowParams('approval/respond', { id: 'x', human: true }), /FORBIDDEN/)
  assert.deepEqual(windowParams('vault/list', { trash: true, human: true }), { trash: true })
  assert.deepEqual(windowParams('attest/request', { source: 'cli', statement: {} }), { source: 'gui', statement: {} })
})

test('dmsg links only navigate', () => {
  const account = '0123456789abcdefghig'
  assert.deepEqual(parseLink(`dmsg://connect/pair?account=${account}`), { route: 'browsers', account })
  assert.deepEqual(parseLink('dmsg://connect/pair'), { route: 'browsers', account: null })
  const request = 'ab'.repeat(32)
  assert.deepEqual(parseLink(`dmsg://connect/request?account=${account}&request=${request}`), {
    route: 'request',
    account,
    request
  })
  for (const bad of [
    'dmsg://connect/approve?account=x',
    'dmsg://connect/pair?account=x',
    `dmsg://connect/request?account=${account}&request=zz`,
    `https://connect/pair?account=${account}`,
    'dmsg://other/pair',
    'not a url'
  ])
    assert.equal(parseLink(bad), null, bad)
})

test('only the login and pairing pages open in the browser', () => {
  const login = ['https://dmsg.net/_/login']
  const pairing = ['https://connect.dmsg.net']
  assert.ok(externalAllowed('https://dmsg.net/_/login#abc', login, pairing))
  assert.ok(externalAllowed('https://connect.dmsg.net/v1/pair#abc', login, pairing))
  assert.equal(externalAllowed('https://evil.example/_/login', login, pairing), false)
  assert.equal(externalAllowed('https://connect.dmsg.net/v1/other', login, pairing), false)
  assert.equal(externalAllowed('file:///etc/passwd', login, pairing), false)
})
