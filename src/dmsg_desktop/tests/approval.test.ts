import assert from 'node:assert/strict'
import { test } from 'node:test'
import { actionValue, holdToApprove, kindOf, monthText, pickLabel, stepsOf, summaryOf, titleOf, trailOf } from '../src/renderer/approval.ts'

const request = (type: string, detail: Record<string, unknown> = {}, source = 'gui', extra: Record<string, unknown> = {}) => ({ type, detail, source, ...extra })

test('the daemon rates the risk; a high-risk request is held to approve', () => {
  assert.equal(holdToApprove(request('checkout', {}, 'gui', { risk: 'high' })), true)
  assert.equal(holdToApprove(request('attest', {}, 'gui', { risk: 'normal' })), false)
  // Without the daemon's word, a click.
  assert.equal(holdToApprove(request('device_revoke')), false)
})

test('the chip names the kind of decision', () => {
  assert.equal(kindOf(request('connect', { method: 'signDocument' })), 'kindWeb')
  assert.equal(kindOf(request('connect', { method: 'checkout' })), 'kindPayment')
  assert.equal(kindOf(request('handle', { total: '1' })), 'kindPayment')
  assert.equal(kindOf(request('handle', {})), 'kindName')
  assert.equal(kindOf(request('device_revoke')), 'kindSecurity')
  assert.equal(kindOf(request('delegation')), 'kindAgent')
  // An agent asking for capabilities is an Agent decision, titled as such.
  const asked = request('grant', { grantee: 'did:agent:z6Mk', capabilities: ['vault.read:work', 'inbox.read'], days: 7 }, 'agent:did:agent:z6Mk')
  assert.equal(kindOf(asked), 'kindAgent')
  assert.deepEqual(titleOf(asked), { label: 'capabilityRequest' })
  assert.equal(summaryOf(asked), 'vault.read:work inbox.read')
  assert.equal(kindOf(request('wallet', { action: 'join' })), 'kindWallet')
  assert.equal(kindOf(request('something')), 'kindOther')
})

test('titles and summaries say what a request asks, the same in the list and the ticket', () => {
  const text = request('attest', { statement: { profile: 'text', content: { text: 'I approve' } } })
  assert.deepEqual(titleOf(text), { label: 'newStatement' })
  assert.equal(summaryOf(text), 'I approve')
  const file = request('attest', { statement: { profile: 'file_statement', content: { text: 'Final', sha256: 'ab' } }, file: { name: 'report.pdf' } })
  assert.deepEqual(titleOf(file), { label: 'signFile' })
  assert.equal(summaryOf(file), 'report.pdf · Final')
  const action = request('connect', { method: 'signAction', app_id: 'token-list', content: { profile: 'app_action', content: { app_id: 'token-list', command: 'Publish' } } })
  assert.deepEqual(titleOf(action), { label: 'appAction' })
  assert.equal(summaryOf(action), 'token-list · Publish')
  assert.deepEqual(titleOf(request('device_revoke', { device_id: 'ab'.repeat(32) })), { label: 'revokeDevice' })
  assert.deepEqual(titleOf(request('account', { change: 'remove_login' })), { label: 'removeLogin' })
  assert.deepEqual(titleOf(request('account', { change: 'something_new' })), { label: 'kindSecurity', detail: 'something_new' })
  assert.deepEqual(titleOf(request('handle', { name: 'alice' })), { label: 'names', detail: 'alice' })
  assert.equal(summaryOf(request('connect', { app_id: 'shop', origin: 'https://shop.example', method: 'checkout' })), 'shop · https://shop.example')
  assert.equal(summaryOf(request('handle', { action: 'Register', name: 'alice' })), '@alice')
  assert.equal(summaryOf(request('handle', { action: 'Transfer', name: 'alice', from: 'a', to: 'b' })), '@alice → b')
})

test('a decision shows each real step of its outcome', () => {
  const names = (steps: ReturnType<typeof stepsOf>) => steps.map((s) => [s.label ?? s.state, s.tone])
  assert.deepEqual(stepsOf(request('attest', {}, 'gui', { state: 'awaiting_user' })), [])
  // The window's own approval shows at once, before the daemon answers.
  assert.deepEqual(names(stepsOf(request('attest', {}, 'gui', { state: 'awaiting_user' }), true)), [
    ['stepApproved', 'ok'],
    ['stepRunning', 'pending']
  ])
  assert.deepEqual(names(stepsOf(request('attest', {}, 'gui', { state: 'done' }))), [
    ['stepApproved', 'ok'],
    ['done', 'ok']
  ])
  assert.deepEqual(names(stepsOf(request('attest', {}, 'gui', { state: 'lost' }))), [
    ['stepApproved', 'ok'],
    ['lost', 'warn']
  ])
  assert.deepEqual(names(stepsOf(request('attest', {}, 'gui', { state: 'rejected' }))), [['rejected', 'muted']])
  // A web request: the relay's authorization, the result, the page taking it.
  const web = (op: string, state: string) => names(stepsOf(request('connect', { state: op }, 'connect:https://app.example', { state })))
  assert.deepEqual(web('authorizing', 'executing'), [
    ['stepApproved', 'ok'],
    ['stepAuthorized', 'pending']
  ])
  assert.deepEqual(web('signed', 'done'), [
    ['stepApproved', 'ok'],
    ['stepAuthorized', 'ok'],
    ['stepResult', 'ok'],
    ['stepReturned', 'pending']
  ])
  assert.deepEqual(web('returned', 'done'), [
    ['stepApproved', 'ok'],
    ['stepAuthorized', 'ok'],
    ['stepResult', 'ok'],
    ['stepReturned', 'ok']
  ])
  assert.deepEqual(web('execution_unknown', 'unknown'), [
    ['stepApproved', 'ok'],
    ['stepAuthorized', 'ok'],
    ['unknown', 'warn']
  ])
})

test("an application's labels follow the interface language", () => {
  const labels = { en: 'Revision', zh: '修订', 'fr-CA': 'Révision' }
  assert.equal(pickLabel(labels, 'zh'), '修订')
  assert.equal(pickLabel(labels, 'fr'), 'Révision')
  assert.equal(pickLabel(labels, 'ar'), 'Revision')
  assert.equal(pickLabel({ de: 'Fassung' }, 'ar'), 'Fassung')
  assert.equal(pickLabel(null, 'en'), '')
  assert.equal(actionValue({ type: 'bool', label: { en: 'Yes, notify', zh: '通知' } }, 'zh'), '通知')
  assert.equal(actionValue({ type: 'choice', text: 'beta', label: { en: 'Beta' } }, 'en'), 'Beta')
  assert.equal(actionValue({ type: 'nat', text: '7' }, 'en'), '7')
  assert.equal(
    actionValue({ type: 'record', fields: [{ label: { en: 'Name' }, value: { type: 'text', text: 'x' } }] }, 'en'),
    'Name: x'
  )
  assert.equal(monthText(202610, 'en'), 'Oct 2026')
  assert.equal(monthText(7, 'en'), '7')
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
  // A hosted Agent ID shows by its name.
  assert.equal(trailOf(request('attest', {}, 'agent:did:agent:z6Mk'), { 'did:agent:z6Mk': 'Research' })[0].name, 'Research')
  assert.equal(trailOf(request('attest', {}, 'cli'))[0].role, 'sourceTerminal')
  assert.equal(trailOf(request('attest', {}, 'gui'))[0].role, 'sourceWindow')
  assert.deepEqual(trailOf(request('attest', {}, 'odd'))[0], { icon: 'window', name: 'odd', role: 'sourceOther' })
})


test('credential uses identify their exact operation and keep unknown outcomes unresolved', () => {
  const r = request('secret_use', {target:{owner:'example',repo:'test'},arguments:{title:'<script>literal</script>'}}, 'agent:did:agent:test', {state:'lost',risk:'high'})
  assert.equal(kindOf(r), 'secretUses')
  assert.equal(titleOf(r).label, 'secretUse')
  assert.equal(summaryOf(r), 'example/test · <script>literal</script>')
  assert.equal(holdToApprove(r), true)
  assert.equal(stepsOf(r).at(-1)?.state, 'lost')
  assert.equal(titleOf(request('secret_policy')).label, 'secretPolicy')
})


test('all P1 secret approvals remain high risk and are classified in the secret center',()=>{
  for(const type of ['secret_process','secret_ssh','secret_ssh_binding','secret_mount_config','secret_mount_read','secret_share','secret_package_import','secret_discovery_import']){
    const r=request(type,{},'gui',{risk:'high'})
    assert.equal(kindOf(r),'secretUses',type)
    assert.equal(holdToApprove(r),true,type)
  }
})
