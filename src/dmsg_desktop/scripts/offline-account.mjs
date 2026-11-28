/** Run against a built desktop and an actual daemon. All account state and input are synthetic. */
import assert from 'node:assert/strict'
import { execFile } from 'node:child_process'
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { promisify } from 'node:util'
import electronPath from 'electron'
import { _electron as electron } from 'playwright'

const binary = process.env.DMSG_BIN || 'dmsg'
const profile = await mkdtemp(join(tmpdir(), 'dmsg-local-'))
const home = join(profile, 'home')
const shots = process.env.DMSG_OFFLINE_SCREENSHOTS
const password = 'synthetic offline workspace password'
const destination = resolve('.')
const testTray = process.platform === 'darwin'
const cli = (...args) => promisify(execFile)(binary, ['--home', home, '--json', ...args])
const requests = []
const relay = createServer((req, res) => { requests.push(req.url); res.writeHead(503).end() })
await new Promise(resolve => relay.listen(0, '127.0.0.1', resolve))
const endpoint = 'http://127.0.0.1:' + relay.address().port
let app
let page
async function open() {
  app = await electron.launch({ executablePath: electronPath, args: [destination], colorScheme: null,
    env: { ...process.env, DMSG_BIN: binary, DMSG_HOME: home, DMSG_DESKTOP_TEST: '1', DMSG_DESKTOP_TEST_TRAY: testTray ? '1' : '0', DMSG_DESKTOP_USER_DATA: join(profile, 'user') } })
  page = await app.firstWindow()
  page.setDefaultTimeout(20000)
  if (testTray) {
    await app.evaluate(({ Tray }) => {
      const setContextMenu = Tray.prototype.setContextMenu
      Tray.prototype.setContextMenu = function (menu) {
        globalThis.localTrayLockEnabled = menu.items.find(item => item.label === 'Lock')?.enabled
        setContextMenu.call(this, menu)
      }
    })
  }
}
async function expectTrayLock(enabled) {
  if (!testTray) return
  const actual = await app.evaluate(async (_electron, expected) => {
    const deadline = Date.now() + 5000
    while (globalThis.localTrayLockEnabled !== expected && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 50))
    }
    return globalThis.localTrayLockEnabled
  }, enabled)
  assert.equal(actual, enabled, 'Standalone lock state in the tray menu')
}
const rpc = (method, params = {}) => page.evaluate(([m, p]) => window.dmsg.call(m, p), [method, params])
async function stop() {
  if (app) { await app.close(); app = null }
  await cli('stop').catch(() => {})
}
try {
  await mkdir(home); await mkdir(join(profile, 'user'))
  await writeFile(join(profile, 'user', 'locale'), 'en')
  await writeFile(join(home, 'config.toml'), [
    'environment = "local"', 'ic_host = "' + endpoint + '"', 'relay = "' + endpoint + '"',
    'handle = "wfxrt-2iaaa-aaaaj-qsjwa-cai"', 'secure_storage = "file"', 'sync_interval_minutes = 0'
  ].join('\n'))
  await open()
  await page.getByTestId('local-create').click()
  await page.getByTestId('local-new-password').fill(password)
  await page.getByTestId('local-confirm-password').fill(password)
  await page.getByTestId('local-submit').click()
  await page.getByTestId('vault').waitFor()
  const created = await rpc('daemon/status')
  assert.equal(created.account, '00000000000000000000')
  assert.equal(created.account_kind, 'local')
  await expectTrayLock(true)
  const item = await rpc('vault/create', { type: 'note', changes: { title: 'Offline test note', notes: 'Local fixture only' } })
  assert.equal(item.save_state, 'local_only'); assert.equal(item.synced, false)
  await page.locator(`[data-id="${item.id}"]`).first().click()
  await page.getByTestId('vault-inspector').waitFor()
  assert.equal(await page.getByTestId('vault-share').count(), 1)
  await page.getByTestId('vault-more').click()
  await page.getByTestId('vault-more-menu').waitFor()
  assert.equal(await page.getByTestId('vault-share-links').count(), 0)
  await page.keyboard.press('Escape')
  await page.evaluate(() => { location.hash = '/vault/sharing' })
  await page.getByTestId('snapshot-review').waitFor()
  assert.equal(await page.getByTestId('snapshot-mode').count(), 0)
  assert.equal(await page.getByTestId('snapshot-links').count(), 0)
  await assert.rejects(rpc('secrets/shares/prepare', { input: {
    item_id: item.id, revision: item.revision, field_ids: [], include_notes: true, attachments: []
  } }), /LOCAL_ACCOUNT_ONLY/)
  const contents = 'Offline file fixture\n'
  const backup = (action, input) => page.evaluate(([a, p]) => window.dmsg.vaultBackup(a, p), [action, input])
  const exportId = await backup('begin', { import: false })
  await backup('inventory', { id: exportId })
  const approval = await backup('request', { id: exportId, items: [item.id], attachments: false, password: 'sample archive password', confirmation: 'sample archive password' })
  await assert.rejects(backup('export', { id: exportId, approval: approval.id }))
  const opened = app.windows().find(w => w !== page) ?? app.waitForEvent('window')
  await page.evaluate(id => window.dmsg.openApproval(id), approval.id)
  const review = await opened
  await review.getByLabel('Local password', { exact: true }).waitFor()
  const approve = secret => review.evaluate(([id, digest, value]) => window.dmsg.decide(id, digest, true, value), [approval.id, approval.digest, secret])
  await assert.rejects(approve('wrong password'), /LOCAL_UNLOCK_FAILED/)
  await approve(password)
  await backup('export', { id: exportId, approval: approval.id })
  await backup('cancel', { id: exportId })
  const input = join(profile, 'local.txt')
  await writeFile(input, contents)
  await cli('files', 'add', input)
  const listing = await rpc('files/list', { folder: 'root' })
  const file = listing.files.find(f => f.name === 'local.txt')
  assert.ok(file)
  await assert.rejects(rpc('files/cloud_usage'), /LOCAL_ACCOUNT_ONLY/)
  await assert.rejects(rpc('sync/run'), /LOCAL_ACCOUNT_ONLY/)
  assert.equal((await rpc('daemon/status')).pending_sync, 0)
  await rpc('session/lock')
  await page.getByTestId('local-unlock').waitFor()
  await expectTrayLock(false)
  await stop(); await open()
  await page.getByTestId('local-unlock').click()
  await page.getByTestId('local-current-password').fill('wrong synthetic password')
  await page.getByTestId('local-submit').click()
  await page.getByTestId('local-account-dialog').getByText(/LOCAL_UNLOCK_FAILED/i).waitFor()
  await page.waitForFunction(() => {
    const input = document.querySelector('[data-testid="local-current-password"]')
    return input === document.activeElement && input.selectionStart === 0 && input.selectionEnd === input.value.length
  })
  await page.getByTestId('local-current-password').fill(password)
  await page.getByTestId('local-submit').click()
  await page.getByTestId('vault').waitFor()
  assert.equal((await rpc('vault/show', { id: item.id })).item.title, 'Offline test note')
  const output = join(profile, 'copy.txt')
  await cli('files', 'get', file.id, '--out', output)
  assert.equal(await readFile(output, 'utf8'), contents)
  await page.evaluate(() => { location.hash = '/files' })
  await page.getByTestId('files').waitFor()
  assert.equal(await page.getByTestId('files-scope-cloud').count(), 0)
  await page.getByTestId('files-nearby-receive').click()
  await page.getByTestId('nearby-receive-qr').waitFor()
  const discovery = page.getByTestId('nearby-discoverable-switch')
  await discovery.focus()
  await page.keyboard.press('Space')
  await page.waitForFunction(async () => (await window.dmsg.call('nearby/status', {})).receive?.discoverable)
  const { receive } = await rpc('nearby/status')
  await rpc('nearby/send_start', { host: '127.0.0.1', port: receive.port, files: [{ file_id: file.id }] })
  const dialog = page.getByTestId('nearby-share-dialog')
  const verification = dialog.getByRole('status').filter({ hasText: /\d{3} \d{3}/ })
  await verification.waitFor()
  const { send } = await rpc('nearby/status')
  assert.ok((await verification.innerText()).includes(send.sas.slice(0, 3) + ' ' + send.sas.slice(3)))
  assert.equal(await dialog.getByText(file.name, { exact: true }).count(), 0)
  if (shots) {
    await mkdir(shots, { recursive: true })
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setContentSize(760, 540))
    for (const [locale, appearance] of [['zh', 'light'], ['de', 'dark'], ['ar', 'dark'], ['en', 'light']]) {
      await page.evaluate(async ([l, a]) => { await window.dmsg.setLocale(l); await window.dmsg.setAppearance(a) }, [locale, appearance])
      await verification.scrollIntoViewIfNeeded()
      await page.screenshot({ path: join(shots, 'nearby-' + locale + '-' + appearance + '.png'), animations: 'disabled' })
    }
  }
  await rpc('nearby/send_confirm_sas')
  await dialog.getByText(file.name, { exact: true }).waitFor()
  await page.getByTestId('nearby-peer-reject').click()
  await page.getByTestId('nearby-pending-peer').waitFor({ state: 'hidden' })
  await page.keyboard.press('Escape')
  await dialog.waitFor({ state: 'hidden' })
  await assert.rejects(cli('nearby', 'send', '--wait', '--host', '127.0.0.1', '--port', String(receive.port), file.id),
    error => error.code > 0 && (error.stdout + error.stderr).includes('UNAVAILABLE'))
  await page.keyboard.press('Tab')
  if (shots) {
    await mkdir(shots, { recursive: true })
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setContentSize(760, 540))
    for (const [locale, appearance] of [['zh', 'light'], ['de', 'dark'], ['ar', 'dark'], ['en', 'light']]) {
      await page.evaluate(async ([l, a]) => { await window.dmsg.setLocale(l); await window.dmsg.setAppearance(a) }, [locale, appearance])
      await page.waitForFunction(l => document.documentElement.lang === l || (l === 'zh' && document.documentElement.lang.startsWith('zh')), locale)
      await page.screenshot({ path: join(shots, locale + '-' + appearance + '.png'), animations: 'disabled' })
    }
  }
  await page.evaluate(() => { location.hash = '/settings' })
  await page.getByTestId('local-password').click()
  const changedPassword = 'another independent test password'
  await page.getByTestId('local-current-password').fill(password)
  await page.getByTestId('local-new-password').fill(changedPassword)
  await page.getByTestId('local-confirm-password').fill(changedPassword)
  await page.getByTestId('local-submit').click()
  await page.getByTestId('local-account-dialog').waitFor({ state: 'hidden' })
  assert.deepEqual(await rpc('session/local_counts'), { vault: 1, files: 1 })
  await page.getByTestId('local-delete').click()
  await page.getByTestId('local-current-password').fill(password)
  await page.getByTestId('local-submit').click()
  await page.getByTestId('local-account-dialog').getByText(/LOCAL_UNLOCK_FAILED/i).waitFor()
  await page.getByTestId('local-current-password').fill(changedPassword)
  await page.getByTestId('local-submit').click()
  await page.getByTestId('local-create').waitFor()
  assert.deepEqual(await rpc('session/accounts'), [])
  assert.deepEqual(requests, [], 'IC and relay requests from a standalone workspace')
  console.log('Standalone desktop creation, restart, vault and file round trip, cloud refusal and zero requests passed.')
} finally {
  await stop()
  await new Promise(resolve => relay.close(resolve))
  await rm(profile, { recursive: true, force: true })
}
