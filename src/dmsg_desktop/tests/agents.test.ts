import assert from 'node:assert/strict'
import { test } from 'node:test'
import { matrix, outcomeTone } from '../src/renderer/views/agents/agents.ts'

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
