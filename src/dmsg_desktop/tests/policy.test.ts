import assert from 'node:assert/strict'
import { test } from 'node:test'
import { DESKTOP_TOOLS, externalAllowed, legacyAllowed, parseLink, windowParams, WINDOW_METHODS } from '../src/main/policy.ts'

test('windows never decide or grant, and never send human', () => {
  for (const method of [
    'approval/respond',
    'connect/grant',
    'app/initialize',
    'app/activity',
    'daemon/stop',
    'workspace/reset',
    'session/login',
    'files/import',
    'files/export',
    'files/open',
    'profile/avatar',
    'legacy/import',
    'legacy/file',
    'legacy/frozen',
    'payer/use',
    'channel/download',
    'attest/verify',
    'attest/action',
    'plan/refund',
    'mcp-events/poll',
    'mcp-events/ack',
    'mcp-events/bind',
    'mcp/call',
    'mcp/tools'
  ])
    assert.equal(WINDOW_METHODS.has(method), false, method)
  assert.throws(() => windowParams('approval/respond', { id: 'x', human: true }), { code: 'FORBIDDEN' })
  assert.deepEqual(windowParams('vault/list', { trash: true, human: true }), { trash: true })
  assert.deepEqual(windowParams('attest/request', { source: 'cli', statement: {} }), { source: 'gui', statement: {} })
  for (const method of [
    'inbox/pay',
    'plan/checkout',
    'handle/claim',
    'handle/register',
    'handle/accept',
    'handle/transfer_commit',
    'account/bind_approve',
    'account/unbind',
    'account/policy',
    'account/recovery_delay',
    'recovery/dispute',
    'agent/enable',
    'agent/register',
    'agent/retire',
    'agent/grant',
    'agent/revoke',
    'device/capabilities',
    'device/formal_approve',
    'device/revoke',
    'wallet/create',
    'wallet/join',
    'wallet/approve',
    'wallet/veto',
    'wallet/execute'
  ])
    assert.equal(windowParams(method, { source: 'cli' }).source, 'gui', method)
})

test('windows run only the Mail and Knowledge tools as an Agent ID', () => {
  // The audit record names the desktop as the way in.
  assert.deepEqual(windowParams('ap/call', { agent: 'claude', tool: 'agent_protocols_mail_sync', input: {} }), {
    agent: 'claude',
    tool: 'agent_protocols_mail_sync',
    input: {},
    source: 'gui'
  })
  assert.equal(DESKTOP_TOOLS.size, 10)
  for (const tool of [
    'agent_protocols_room_send_message',
    'agent_protocols_delegation_grant',
    'agent_protocols_identity_current',
    'dmsg_vault_read',
    'dmsg_events_next',
    undefined
  ])
    assert.throws(() => windowParams('ap/call', { tool, input: {} }), { code: 'FORBIDDEN' }, String(tool))
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

test('only old-site migration links open from the legacy view', () => {
  assert.equal(legacyAllowed('https://dmsg.net/legacy#pair=pmVub25jZQ'), true)
  assert.equal(legacyAllowed('https://panda.fans/legacy#approve=eyJmb3JtYXQiOjF9'), true)
  for (const url of [
    'https://dmsg.net/legacy',
    'https://dmsg.net/legacy?x=1#pair=abc',
    'https://dmsg.net/_/login#pair=abc',
    'https://evil.example/legacy#pair=abc',
    'http://dmsg.net/legacy#pair=abc',
    'https://dmsg.net/legacy#pair=a+b',
    'javascript:alert(1)'
  ])
    assert.equal(legacyAllowed(url), false, url)
})
