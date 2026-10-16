import assert from 'node:assert/strict'
import { test } from 'node:test'
import { holdToApprove, kindOf, trailOf } from '../src/renderer/approval.ts'

const request = (type: string, detail: Record<string, unknown> = {}, source = 'gui') => ({ type, detail, source })

test('payments, device revocation and control of the account are approved by holding', () => {
  for (const held of [
    request('checkout'),
    request('letter'),
    request('device_revoke'),
    request('handle', { total: '100' }),
    request('connect', { method: 'checkout' }, 'connect:https://app.example'),
    request('account', { change: 'bind_login' }),
    request('account', { change: 'remove_login' }),
    request('account', { change: 'policy' }),
    request('account', { change: 'recovery_delay' })
  ])
    assert.equal(holdToApprove(held), true, `${held.type} ${JSON.stringify(held.detail)}`)
  for (const clicked of [
    request('attest'),
    request('connect', { method: 'signDocument' }, 'connect:https://app.example'),
    request('delegation', { kind: { type: 'grant' } }),
    request('principal'),
    request('device_capabilities'),
    request('handle', { action: 'transfer' }),
    request('account', { change: 'dispute_recovery' }),
    request('unknown')
  ])
    assert.equal(holdToApprove(clicked), false, `${clicked.type} ${JSON.stringify(clicked.detail)}`)
})

test('the chip names the kind of decision', () => {
  assert.equal(kindOf(request('connect', { method: 'signDocument' })), 'kindWeb')
  assert.equal(kindOf(request('connect', { method: 'checkout' })), 'kindPayment')
  assert.equal(kindOf(request('handle', { total: '1' })), 'kindPayment')
  assert.equal(kindOf(request('handle', {})), 'kindName')
  assert.equal(kindOf(request('device_revoke')), 'kindSecurity')
  assert.equal(kindOf(request('delegation')), 'kindAgent')
  assert.equal(kindOf(request('something')), 'kindOther')
})

test('a web request arrives relayed through the browser and connect.dmsg.net', () => {
  const trail = trailOf(request('connect', { origin: 'https://app.example', pairing_label: 'Chrome · Test', relayed_by: 'connect.dmsg.net' }, 'connect:https://app.example'))
  assert.deepEqual(
    trail.map((hop) => [hop.role, hop.name, !!hop.relayed]),
    [
      ['sourceWebsite', 'app.example', false],
      ['sourceBrowser', 'Chrome · Test', false],
      ['sourceRelay', 'connect.dmsg.net', true],
      ['sourceDevice', '', true]
    ]
  )
})

test('agents, the terminal and the main window reach this device directly', () => {
  assert.deepEqual(trailOf(request('attest', {}, 'agent:claude')).map((hop) => [hop.role, hop.name]), [
    ['sourceAgent', 'claude'],
    ['sourceDevice', '']
  ])
  assert.equal(trailOf(request('attest', {}, 'cli'))[0].role, 'sourceTerminal')
  assert.equal(trailOf(request('attest', {}, 'gui'))[0].role, 'sourceWindow')
  assert.deepEqual(trailOf(request('attest', {}, 'odd'))[0], { icon: 'window', name: 'odd', role: 'sourceOther' })
})
