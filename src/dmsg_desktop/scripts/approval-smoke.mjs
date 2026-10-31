// Real Electron windows and IPC with synthetic SSH metadata. No private key,
// real SSH connection, system clipboard or OS identity confirmation is used.
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WebSocketServer } from 'ws'

const account = '040g2081040g2081040g', device = '33'.repeat(32), token = 'approval-ui-fixture'
const reference = { schema_version: 1, account_id: account, item_id: '11'.repeat(32), field_id: 'private_key' }
const key = {
  reference, title: 'Development SSH key', supported: true,
  fingerprint: `SHA256:${'AbCdEf0123/'.repeat(4)}xyz`,
  public_key: `ssh-ed25519 ${'A'.repeat(68)}`
}
const resolved = { reference, item_revision: '44'.repeat(32), field_kind: 'concealed', item_title: key.title, field_label: 'Private key' }
const requests = new Map(), calls = [], sockets = new Set(), pageErrors = []
let sequence = 0, locked = false
const status = () => ({
  state: locked ? 'locked' : 'unlocked', session: locked ? 'locked-session' : 'fixture-session',
  account, device_id: device, fast_unlock: false, fast_presence: null,
  approvals_waiting: [...requests.values()].filter(r => r.state === 'awaiting_user').length,
  pending_sync: 0, background_online: false, idle_lock_ms: 900000
})
const emit = type => {
  const event = { jsonrpc: '2.0', method: 'event', params: { seq: ++sequence, type, data: null } }
  for (const socket of sockets) socket.send(JSON.stringify(event))
}
const makeRequest = (type, detail) => {
  const id = (++sequence).toString(16).padStart(64, '0')
  const request = { id, digest: id, type, detail, source: 'gui', kind: 'secret', risk: 'high',
    state: 'awaiting_user', created_at: Date.now(), updated_at: Date.now(), expires_at: Date.now() + 300000 }
  requests.set(id, request)
  return request
}
const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'app/activity': () => null, 'daemon/status': status, 'session/status': status,
  'profile/show': () => null, 'profile/pending': () => null,
  'channel/list': () => [], 'legacy/pairs': () => [], 'agent/ids': () => ({ agents: [] }),
  'secrets/policies/list': () => ({ policies: [] }), 'secrets/uses/list': () => ({ uses: [] }),
  'secrets/ssh/keys': () => ({ keys: [key], bindings: [] }),
  'secrets/ssh/status': () => ({ available: true, supported: true, socket: '/synthetic/agent.sock', identities: 0 }),
  'secrets/ssh/binding_request': params => {
    assert.deepEqual(params.reference, reference)
    assert.ok(['ssh_auth', 'git_sign'].includes(params.purposes[0]))
    return makeRequest('secret_ssh_binding', { id: '55'.repeat(32), resolved,
      public_key: key.public_key, fingerprint: key.fingerprint, purposes: params.purposes, enabled: true, device_id: device })
  },
  'approval/list': params => params.ids_only
    ? [...requests.values()].filter(r => r.state === 'awaiting_user').map(r => r.id)
    : [...requests.values()],
  'approval/show': params => requests.get(params.id),
  'approval/respond': params => {
    const request = requests.get(params.id)
    assert.equal(params.digest, request.digest)
    assert.equal(params.source, 'gui')
    assert.equal(params.human, true)
    request.state = params.approve ? 'done' : 'rejected'
    emit('approval/changed')
    return request
  }
}
const server = createServer((_, response) => response.writeHead(404).end())
const wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (request, socket, head) => {
  if (request.headers.authorization !== `Bearer ${token}`) return socket.destroy()
  wss.handleUpgrade(request, socket, head, ws => {
    sockets.add(ws)
    ws.on('close', () => sockets.delete(ws))
    ws.on('message', async bytes => {
      const message = JSON.parse(String(bytes))
      calls.push(message)
      try {
        const handler = methods[message.method]
        ws.send(JSON.stringify(handler
          ? { jsonrpc: '2.0', id: message.id, result: await handler(message.params ?? {}) }
          : { jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'Not found', data: { code: 'NOT_FOUND' } } }))
      } catch (error) {
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: message.id,
          error: { code: -32000, message: String(error), data: { code: 'INVALID_INPUT' } } }))
      }
    })
  })
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-approval-ui-'))
const shots = resolve('test-results/approvals')
await mkdir(shots, { recursive: true })
await writeFile(join(profile, 'locale'), 'en')
await writeFile(join(profile, 'appearance'), 'light')
const app = await electron.launch({ executablePath: electronPath, colorScheme: null,
  args: [resolve('out/main/index.js')], env: { ...process.env, DMSG_DESKTOP_TEST: '1',
    DMSG_DESKTOP_USER_DATA: profile, DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`,
    DMSG_DESKTOP_TOKEN: token, DMSG_HOME: join(profile, 'home') } })
const watch = page => {
  page.setDefaultTimeout(10000)
  page.on('pageerror', error => { pageErrors.push(error.message); console.error('Renderer error:', error.message) })
}
app.on('window', watch)
for (const page of app.windows()) watch(page)
try {
  const main = await app.firstWindow()
  await main.getByTestId('sidebar-toggle').waitFor()
  await main.evaluate(() => { location.hash = '/approvals' })
  await main.getByTestId('approvals-tab-secrets').click()
  await main.getByTestId('workflow-tab-ssh').click()
  const [approval] = await Promise.all([
    app.waitForEvent('window'),
    main.getByRole('button', { name: 'Request activation · SSH authentication', exact: true }).click()
  ])
  const summary = approval.getByTestId('secret-review-summary')
  await summary.waitFor()
  assert.equal(await approval.getByTestId('approval-title').innerText(), 'Enable an SSH key on this device')
  assert.match(await summary.innerText(), /Development SSH key[\s\S]*Private key[\s\S]*SSH authentication/)
  await approval.waitForFunction(() => document.activeElement?.getAttribute('data-testid') === 'reject')
  assert.equal(await approval.getByTestId('approve').isDisabled(), true)
  assert.equal(calls.filter(c => c.method === 'approval/respond').length, 0)

  // The full fingerprint is inspectable and keyboard-copyable in this window.
  await approval.evaluate(() => Object.defineProperty(navigator, 'clipboard', {
    configurable: true, value: { writeText: async text => { window.testClipboard = text } }
  }))
  const fingerprint = summary.getByRole('button', { name: key.fingerprint, exact: true })
  await fingerprint.focus()
  await approval.locator('[data-tooltip-content]').filter({ hasText: key.fingerprint }).waitFor()
  await approval.keyboard.press('Enter')
  await approval.waitForFunction(text => window.testClipboard === text, key.fingerprint)
  await approval.keyboard.press('Escape')
  await approval.getByRole('button', { name: 'Dismiss', exact: true }).click()
  await approval.getByTestId('reject').focus()
  await approval.screenshot({ path: join(shots, 'ssh-enable-en-light.png') })

  // Both themes, long translations and RTL at the minimum supported size.
  await (await app.browserWindow(approval)).evaluate(window => window.setSize(400, 560))
  for (const language of ['zh', 'de', 'ar']) {
    await main.evaluate(language => window.dmsg.setLocale(language), language)
    await approval.waitForFunction(language => document.documentElement.lang === language, language === 'zh' ? 'zh-Hans' : language)
    for (const appearance of ['light', 'dark']) {
      await main.evaluate(appearance => window.dmsg.setAppearance(appearance), appearance)
      await approval.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
      for (const id of ['approve', 'reject', 'approval-risk']) {
        assert.ok(await approval.getByTestId(id).evaluate(element =>
          element.scrollWidth <= element.clientWidth + 1 && element.scrollHeight <= element.clientHeight + 1), `${language} ${id} must show its whole label`)
      }
      await approval.getByTestId('approval-title').scrollIntoViewIfNeeded()
      await approval.screenshot({ path: join(shots, `ssh-enable-${language}-${appearance}-minimum.png`) })
      await summary.scrollIntoViewIfNeeded()
      assert.ok(await summary.evaluate(element => element.scrollWidth <= element.clientWidth + 1), 'summary must fit horizontally')
      assert.ok(await approval.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'window must fit horizontally')
      await approval.screenshot({ path: join(shots, `ssh-enable-${language}-${appearance}-summary.png`) })
    }
  }
  assert.equal(await approval.locator('html').getAttribute('dir'), 'rtl')
  await main.evaluate(() => window.dmsg.setLocale('en'))
  await main.evaluate(() => window.dmsg.setAppearance('light'))

  // A normal click cannot approve a high-risk request; holding Space can.
  await app.evaluate(({ BrowserWindow }) => {
    for (const window of BrowserWindow.getAllWindows()) window.webContents.setBackgroundThrottling(false)
  })
  await approval.waitForFunction(() => {
    const button = document.querySelector('[data-testid="approve"]')
    return button && !button.disabled
  })
  await approval.getByTestId('approve').click()
  assert.equal(calls.filter(c => c.method === 'approval/respond').length, 0)
  await approval.getByTestId('approve').focus()
  await approval.keyboard.down('Space')
  try { await approval.getByTestId('approval-outcome').waitFor() }
  finally { await approval.keyboard.up('Space') }
  assert.equal(calls.filter(c => c.method === 'approval/respond').length, 1)
  assert.equal(calls.find(c => c.method === 'approval/respond').params.approve, true)

  // Reusing the window for Git signing still renders, focuses Reject and can reject.
  await main.getByRole('button', { name: 'Request activation · Git signing', exact: true }).click()
  await summary.getByText('Git signing', { exact: true }).waitFor()
  await approval.waitForFunction(() => document.activeElement?.getAttribute('data-testid') === 'reject')
  await approval.keyboard.press('Enter')
  await approval.getByTestId('approval-outcome').waitFor()
  assert.equal(calls.filter(c => c.method === 'approval/respond').at(-1).params.approve, false)

  // Real detail shapes from approval_view: mount/share reviews also use Id;
  // package and discovery imports have no top-level account/device fields.
  const binding = [...requests.values()][0].detail
  const source = { type: 'library_file', file_id: '66'.repeat(32), version_id: '77'.repeat(32) }
  const config = { id: '88'.repeat(32), version: 1, account_id: account, device_id: device,
    input: { project_label: 'Fixture project', project_directory: '/synthetic/project', mount_point: '/synthetic/mount',
      entries: [{ relative_path: 'credential', source }], aliases: [] }, mode: { type: 'linux_handle', owner_uid: 1000 } }
  const examples = [
    ['secret_ssh', { binding, purpose: { kind: 'ssh_auth', username: 'developer', host_fingerprint: `SHA256:${'B'.repeat(43)}` } }, key.fingerprint],
    ['secret_mount_config', config, source.version_id],
    ['secret_mount_read', { config, entry: { id: '99'.repeat(32), relative_path: 'credential', generation: 1, size: 42,
      source: { type: 'raw', source: { type: 'library', file_id: source.file_id, version_id: source.version_id, size: 42, sha256: 'aa'.repeat(32) } } },
      scope: { type: 'linux_handle', uid: 1000, handle: 1 }, grant_lifetime_ms: 30000 }, source.version_id],
    ['secret_share', { account_id: account, device_id: device, title: 'Fixture snapshot', fields: [] }, account],
    ['secret_package_import', { preview: { title: 'Fixture import', fields: [] }, selection: { fields: [] } }, 'Fixture import'],
    ['secret_discovery_import', {
      scan: { root: '/synthetic/project', report: { candidates: [{ id: 'candidate', relative_path: '.env', variables: ['API_KEY'] }] } }, candidate_ids: ['candidate'] }, '.env']
  ]
  for (const [type, detail, expected] of examples) {
    const request = makeRequest(type, detail)
    await main.evaluate(id => window.dmsg.openApproval(id), request.id)
    await approval.waitForFunction(id => location.hash.endsWith(id), request.id)
    await summary.waitFor()
    assert.ok((await summary.innerText()).includes(expected), `${type} must show the reviewed source`)
    await approval.getByTestId('reject').click()
    await approval.getByTestId('approval-outcome').waitFor()
    assert.equal(calls.filter(c => c.method === 'approval/respond').at(-1).params.id, request.id)
  }

  locked = true
  emit('session/changed')
  await approval.getByTestId('approval-locked').waitFor()
  assert.equal(await summary.count(), 0)
  assert.equal(await approval.getByTestId('approve').count(), 0)
  assert.deepEqual(pageErrors, [])
  console.log('approval Electron smoke passed: SSH/Git enable, identifier tooltips, keyboard copy, hold/reject, shared secret reviews, lock masking, light/dark minimum-size zh/de/ar')
} catch (error) {
  for (const [index, page] of app.windows().entries()) {
    await page.screenshot({ path: join(shots, `failure-${index}.png`) }).catch(() => {})
    console.error('Window:', page.url(), await page.locator('body').innerText().catch(() => ''))
  }
  throw error
} finally {
  await app.close()
  wss.close()
  server.close()
}
