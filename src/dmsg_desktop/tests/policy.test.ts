import assert from 'node:assert/strict'
import { test } from 'node:test'
import { WALLET_CREATION_PAUSED } from '../src/shared/wallet-availability.ts'
import { APPROVAL_MAIN_ROUTES, DESKTOP_TOOLS, externalAllowed, legacyAllowed, parseLink, reminderDue, windowParams, WINDOW_METHODS } from '../src/main/policy.ts'

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
    'files/preferences_reload',
    'vault/import_preview',
    'vault/import',
    'vault/otp_scan',
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
  // A window names a library version, never a local path.
  assert.deepEqual(windowParams('attest/request', { statement: {}, file: '/etc/hosts', library: { file: 'f', version: 'v' } }), {
    source: 'gui',
    statement: {},
    library: { file: 'f', version: 'v' }
  })
  assert.equal(WINDOW_METHODS.has('approval/check_file'), false)
  for (const method of ['wallet/create', 'wallet/quote', 'wallet/quote_preview']) {
    assert.equal(WINDOW_METHODS.has(method), !WALLET_CREATION_PAUSED, method)
    if (WALLET_CREATION_PAUSED) assert.throws(() => windowParams(method, {}), { code: 'FORBIDDEN' })
  }
  for (const method of ['wallet/show', 'wallet/join', 'wallet/activation_continue', 'wallet/refund']) {
    assert.equal(WINDOW_METHODS.has(method), true, method)
  }
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
    'agent/compromise',
    'agent/rename',
    'agent/grant',
    'agent/revoke',
    'agent/credential_revoke',
    'agent/pause',
    'agent/remove',
    'device/capabilities',
    'device/formal_approve',
    'device/revoke',
    ...(!WALLET_CREATION_PAUSED ? ['wallet/create'] : []),
    'wallet/join',
    'wallet/approve',
    'wallet/veto',
    'wallet/execute',
    'plan/checkout',
    'plan/advance'
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
  // Reading what a connector holds changes nothing, from the desktop's own session.
  assert.deepEqual(windowParams('ap/read', { agent: 'claude', uri: 'agent-protocols://rooms' }), {
    agent: 'claude',
    uri: 'agent-protocols://rooms',
    source: 'gui'
  })
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
  // An invitation link only opens the join dialog with it.
  const invitation = 'eyJmb3JtYXQiOiJkbXNnLWNoYW5uZWwtaW52aXRhdGlvbi8xIn0'
  assert.deepEqual(parseLink(`dmsg://channel/join#${invitation}`), { route: 'join', invitation })
  // A name transfer's link only opens the accept dialog with its token.
  assert.deepEqual(parseLink(`dmsg://handle/accept#${invitation}`), { route: 'accept', token: invitation })
  for (const bad of [
    'dmsg://channel/join#short',
    'dmsg://channel/join#not+base64url!',
    `dmsg://channel/accept#${invitation}`,
    'dmsg://handle/accept#not+base64url!',
    `dmsg://handle/transfer#${invitation}`,
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

test('a term ending reminds 30, 7 and 1 days before, each once', () => {
  const day = 86_400_000
  assert.equal(reminderDue(31 * day), null)
  assert.equal(reminderDue(30 * day), 30)
  assert.equal(reminderDue(20 * day, 30), null)
  assert.equal(reminderDue(5 * day, 30), 7)
  // Away for weeks, the nearest threshold reminds alone.
  assert.equal(reminderDue(day / 2), 1)
  assert.equal(reminderDue(day / 2, 1), null)
  assert.equal(reminderDue(0), null)
})

test('approval recovery opens payments without permitting arbitrary routes', () => {
  assert.equal(APPROVAL_MAIN_ROUTES.has('/plan?tab=payments'), true)
  for (const route of ['/plan?tab=unknown', '/plan?tab=payments&other=1', 'https://example.com', '/settings/reset'])
    assert.equal(APPROVAL_MAIN_ROUTES.has(route), false)
})

test('wallet invitations navigate only, and structured actions cannot forge human approval', () => {
  assert.deepEqual(parseLink('dmsg://wallet/rrkah-fqaaa-aaaaa-aaaaq-cai/12'),{route:'wallet',wallet:'rrkah-fqaaa-aaaaa-aaaaq-cai',proposal:12})
  assert.equal(parseLink('dmsg://wallet/panda'),null)
  assert.equal(parseLink('dmsg://wallet/rrkah-fqaaa-aaaaa-aaaaq-cai?execute=12'),null)
  const params=windowParams('wallet/cosign',{source:'agent:untrusted',human:true,wallet:'rrkah-fqaaa-aaaaa-aaaaq-cai',proposal:12,plan:1})
  assert.equal(params.human,undefined)
  assert.equal(params.source,'gui')
  assert.throws(()=>windowParams('wallet/propose',{intent:{Unfreeze:null}}),(e:any)=>e.code==='FORBIDDEN')
})
