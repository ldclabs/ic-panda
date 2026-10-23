import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  bySubject,
  covers,
  grantState,
  matrix,
  mcpSetup,
  outcomeTone,
  profileHint,
  serviceOrigins,
  services,
  sensitive,
  shellWord,
  unknownCapabilities
} from '../src/renderer/views/agents/agents.ts'

test('live grants form a matrix of services and the origins they reach', () => {
  const now = 1_000
  const reach = matrix(
    [
      { grantee: 'a', capabilities: ['mail.read:https://x.example', 'mail.read:*', 'vault.read:work'] },
      { grantee: 'a', capabilities: ['discourse.write:https://x.example', 'agent.profile.write'] },
      { grantee: 'a', capabilities: ['knowledge.read:*'], revoked_at: 5 },
      { grantee: 'b', capabilities: ['knowledge.read:*'], expires_at: 999 }
    ],
    now
  )
  assert.deepEqual(reach.get('a'), {
    services: { 'mail.read': ['https://x.example', '*'], 'discourse.write': ['https://x.example'], 'agent.profile.write': ['*'] },
    other: ['vault.read:work']
  })
  assert.equal(reach.has('b'), false)
})

test('an audit outcome reads as done, refused or in between', () => {
  assert.equal(outcomeTone('ok'), 'ok')
  assert.equal(outcomeTone('denied'), 'danger')
  assert.equal(outcomeTone('pending'), 'warn')
})

/** The daemon's vocabulary (`dmsg_client::agents::vocabulary`), in part. */
const vocabulary = ['account.read', 'vault.read:<tag>', 'vault.secret:<item>', 'files.read:<folder>', 'wallet.read', 'wallet.propose:<wallet>', 'mail.read:<origin>']

test('only what the vocabulary names may be typed', () => {
  assert.deepEqual(
    unknownCapabilities(
      ['account.read', 'vault.read:work', 'vault.read:*', 'files.read:f1', 'wallet.propose:aaaaa-aa', 'mail.read:https://mail.dmsg.net', 'mail.read:*'],
      vocabulary
    ),
    []
  )
  assert.deepEqual(
    unknownCapabilities(
      ['file.read:work', 'vault.read:', 'vault.read:<tag>', 'files.read:<folder>', 'vault.read:a b', 'vault.secret:dmsg-agent', 'account.read:x', 'mail.read:mail.dmsg.net', 'mail.read:https://mail.dmsg.net/v1'],
      vocabulary
    ),
    ['file.read:work', 'vault.read:', 'vault.read:<tag>', 'files.read:<folder>', 'vault.read:a b', 'vault.secret:dmsg-agent', 'account.read:x', 'mail.read:mail.dmsg.net', 'mail.read:https://mail.dmsg.net/v1']
  )
})

test('grants reaching everything, a secret or a wallet’s execution ask again', () => {
  assert.equal(sensitive('vault.read:*'), true)
  assert.equal(sensitive('vault.secret:i1'), true)
  assert.equal(sensitive('wallet.execute:aaaaa-aa'), true)
  assert.equal(sensitive('vault.read:work'), false)
  assert.equal(grantState({ revoked_at: 1 }, 10), 'revoked')
  assert.equal(grantState({ expires_at: 5 }, 10), 'expired')
  assert.equal(grantState({ expires_at: 50 }, 10), 'live')
  assert.equal(grantState({}, 10), 'live')
})

test('an MCP client runs dmsg mcp with the credential and the home', () => {
  const setup = mcpSetup({ binary: '/Users/me/Library/Application Support/dMsg/bin/dmsg', home: '/Users/me/.dmsg' }, 'claude', 'dmsg-agent-ab')
  assert.equal(
    setup.command,
    "claude mcp add --scope user dmsg-claude --env DMSG_AGENT_TOKEN=dmsg-agent-ab -- '/Users/me/Library/Application Support/dMsg/bin/dmsg' --home /Users/me/.dmsg mcp"
  )
  assert.deepEqual(JSON.parse(setup.config), {
    mcpServers: {
      'dmsg-claude': { command: '/Users/me/Library/Application Support/dMsg/bin/dmsg', args: ['--home', '/Users/me/.dmsg', 'mcp'], env: { DMSG_AGENT_TOKEN: 'dmsg-agent-ab' } }
    }
  })
  assert.equal(shellWord("it's"), "'it'\\''s'")
})

test('credentials group by agent and give a profile hint', () => {
  const groups = bySubject([
    { id: 'a.1', subject: 's1', accepted_at: 1 },
    { id: 'a.2', subject: 's2', accepted_at: 2 },
    { id: 'a.3', subject: 's1', accepted_at: 3 }
  ])
  assert.deepEqual(
    groups.map(([subject, list]) => [subject, list.map((c) => c.id)]),
    [
      ['s1', ['a.3', 'a.1']],
      ['s2', ['a.2']]
    ]
  )
  assert.deepEqual(profileHint({ principal_id: 'https://id.dmsg.net/a', type: 'Person' }, { id: 'a.1', scopes: ['x'], relationship: 'assistant' }), {
    id: 'a.1',
    principal: { id: 'https://id.dmsg.net/a', type: 'person' },
    relationship: 'assistant',
    scopes: ['x']
  })
  const restricted = { delegation: { scopes: ['x'], audiences: ['https://a.example'] } }
  assert.equal(covers(restricted, ['x'], ['https://a.example']), true)
  assert.equal(covers(restricted, ['y'], ['https://a.example']), false)
  assert.equal(covers({ delegation: '*' }, ['y'], ['https://b.example']), true)
})


test('Agent Profile has separate read/write grants and includes configured profile origins', () => {
  assert.ok(services.some((service) => service.read === 'agent.profile.read' && service.write === 'agent.profile.write'))
  assert.deepEqual(serviceOrigins([
    { host: 'https://rooms.example', label: 'Rooms', profile_service: 'https://profiles.example' },
    { host: 'https://mail.example', profile_service: 'https://profiles.example' }
  ]).map((host) => host.host), ['https://rooms.example', 'https://mail.example', 'https://profiles.example'])
  const reach = matrix([{ grantee: 'a', capabilities: ['agent.profile.read:https://profiles.example'] }]).get('a')!
  assert.deepEqual(reach.services['agent.profile.read'], ['https://profiles.example'])
  assert.equal(reach.services['agent.profile.write'], undefined)
  assert.deepEqual(reach.other, [])
})
