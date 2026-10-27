// Exercise the real Electron view against an isolated synthetic wallet.
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WebSocketServer } from 'ws'
import { walletFixture } from './wallet-fixture.mjs'

const account = '040g2081040g2081040g', token = 'wallet-upgrade-fixture'
const fixture = walletFixture(account, [])
fixture.state.enabled = true
let version = 2, accepted = true, releaseInstall, installCalls = 0, rejectInstall = true
let holdVersions = false, releaseVersions
const sockets = new Set()
const calls = []
let sequence = 0
const status = () => ({ state: 'unlocked', account, device_id: '55'.repeat(32), approvals_waiting: 0, pending_sync: 0, background_online: false, fast_unlock: false, idle_lock_ms: 900000 })
const emit = type => { for (const ws of sockets) ws.send(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { seq: ++sequence, type, data: {} } })) }
const versions = () => ({ record: { version, locked: false }, versions: [1, 2, 3].map(v => ({ version: v, module_hash: 'ee'.repeat(32), notes: `canisters-v3.1.${v} — Long release description with installation and recovery details`, retired: false })) })
const methods = {
  ...fixture.methods,
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'app/activity': () => null, 'daemon/status': status, 'session/status': status,
  'profile/show': () => null, 'profile/pending': () => null,
  'approval/list': () => [], 'attest/history': () => [], 'channel/list': () => [],
  'agent/ids': () => ({ agents: [] }), 'grants/list': () => [],
  'wallet/list': () => [{ wallet: fixture.a, name: 'Upgrade fixture' }],
  'wallet/principal': () => ({ principal: 'xkbqi-2qaaa-aaaah-qbpqq-cai' }),
  'wallet/deposits': () => [],
  'wallet/show': p => ({ ...fixture.view(p.wallet), code: accepted ? 'Upgrade { version: 3, module_hash: [226, 146] }' : 'None', code_acceptance: accepted ? { Upgrade: { version: 3, module_hash: 'ee'.repeat(32) } } : 'None' }),
  'wallet/versions': async () => {
    const snapshot = versions()
    if (holdVersions) { holdVersions = false; await new Promise(resolve => { releaseVersions = resolve }) }
    return snapshot
  },
  'wallet/upgrade': async () => {
    ++installCalls
    await new Promise(resolve => { releaseInstall = resolve })
    releaseInstall = null
    if (rejectInstall) throw Error('VERSION_CONFLICT')
    version = 3; accepted = false
    return { record: { version, locked: false } }
  }
}
const server = createServer((_, res) => res.writeHead(404).end())
const wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (req, socket, head) => {
  if (req.headers.authorization !== `Bearer ${token}`) return socket.destroy()
  wss.handleUpgrade(req, socket, head, ws => {
    sockets.add(ws); ws.on('close', () => sockets.delete(ws))
    ws.on('message', async bytes => {
      const m = JSON.parse(String(bytes))
      calls.push(m.method)
      try {
        let result = methods[m.method] ? await methods[m.method](m.params ?? {}) : null
        if (m.params?.mode) result = { state: 'present', data: result, observed_at_ms: Date.now(), stale: false, refreshing: false, error: null }
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, result }))
      } catch (error) {
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, error: { code: -32000, message: error.message, data: { code: error.message } } }))
      }
    })
  })
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-wallet-upgrade-ui-')), shots = resolve('test-results/wallet-upgrade')
await mkdir(shots, { recursive: true })
await writeFile(join(profile, 'locale'), 'zh')
await writeFile(join(profile, 'appearance'), 'light')
const app = await electron.launch({ executablePath: electronPath, colorScheme: null, args: [resolve('out/main/index.js')], env: { ...process.env, DMSG_DESKTOP_TEST: '1', DMSG_DESKTOP_USER_DATA: profile, DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`, DMSG_DESKTOP_TOKEN: token, DMSG_HOME: join(profile, 'home') } })
let main
try {
  main = await app.firstWindow()
  main.setDefaultTimeout(15000)
  await main.waitForSelector('[data-testid="sidebar-toggle"]')
  await app.evaluate(({ BrowserWindow }) => { for (const window of BrowserWindow.getAllWindows()) window.webContents.setBackgroundThrottling(false) })
  const open = async () => {
    await main.evaluate(() => { location.hash = '/wallet' })
    await main.getByTestId('wallet-tab-team').click()
    await main.getByTestId('wallet-tab-settings').click()
    await main.getByTestId('wallet-code-current').waitFor()
  }
  const screenshot = async name => {
    const section = main.getByTestId('wallet-code-current').locator('xpath=ancestor::section')
    const receipt = main.getByTestId('wallet-upgrade-error').or(main.getByTestId('wallet-upgrade-success'))
    if (await receipt.count()) await receipt.scrollIntoViewIfNeeded()
    else await section.evaluate(e => e.scrollIntoView({ block: 'start' }))
    assert.ok(await section.evaluate(e => e.scrollWidth <= e.clientWidth + 1), 'version controls fit horizontally')
    await main.screenshot({ path: join(shots, name) })
  }
  const pendingInstall = async () => {
    await main.getByTestId('wallet-upgrade-progress').waitFor()
    for (let i = 0; i < 100 && !releaseInstall; i++) await new Promise(resolve => setTimeout(resolve, 20))
    assert.ok(releaseInstall)
  }
  await open()
  assert.ok((await main.getByTestId('wallet-code-current').innerText()).includes('v2'))
  assert.ok((await main.getByTestId('wallet-code-accepted').innerText()).includes('v3'))
  assert.equal(await main.getByText(/module_hash:/).count(), 0, 'no Rust debug output')
  await screenshot('accepted-zh-light.png')
  const install = main.getByTestId('wallet-upgrade-install')
  await install.focus()
  await main.keyboard.press('Enter')
  await pendingInstall()
  assert.equal(await install.isDisabled(), true)
  await main.keyboard.press('Enter')
  assert.equal(installCalls, 1, 'pending install cannot submit twice')
  releaseInstall()
  await main.getByTestId('wallet-upgrade-error').waitFor()
  assert.equal(await main.getByTestId('wallet-upgrade-success').count(), 0)
  assert.equal(await install.isEnabled(), true, 'failure allows retry')
  for (const [locale, theme] of [['zh', 'light'], ['zh', 'dark'], ['fr', 'light'], ['ar', 'dark']]) {
    await main.evaluate(async ({ locale, theme }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(theme) }, { locale, theme })
    await (await app.browserWindow(main)).evaluate(window => window.setContentSize(760, 540))
    await main.waitForFunction(({ locale, theme }) => document.documentElement.dir === (locale === 'ar' ? 'rtl' : 'ltr') && matchMedia('(prefers-color-scheme: dark)').matches === (theme === 'dark'), { locale, theme })
    await main.waitForSelector(`html[lang|="${locale}"]`)
    // Mount in the selected language, including labels of existing wallet controls.
    await main.evaluate(() => { location.hash = '/channels' })
    await main.getByTestId('wallet-view').waitFor({ state: 'detached' })
    await open()
    await install.click()
    await pendingInstall()
    releaseInstall()
    await main.getByTestId('wallet-upgrade-error').waitFor()
    await screenshot(`failure-${locale}-${theme}-narrow.png`)
  }
  // A pre-install versions read must not undo the returned installation record.
  rejectInstall = false
  holdVersions = true
  emit('wallet/changed')
  for (let i = 0; i < 100 && !releaseVersions; i++) await new Promise(resolve => setTimeout(resolve, 20))
  assert.ok(releaseVersions)
  await install.click()
  await pendingInstall()
  releaseInstall()
  await main.getByTestId('wallet-upgrade-success').waitFor()
  releaseVersions()
  await main.waitForTimeout(200)
  assert.ok((await main.getByTestId('wallet-code-current').innerText()).includes('v3'))
  assert.equal(await install.count(), 0)
  assert.equal(await main.getByTestId('wallet-upgrade-error').count(), 0)
  await screenshot('installed-ar-dark-narrow.png')
  // A result delivered after leaving the view must not appear in the next view.
  version = 2; accepted = true
  await main.evaluate(() => { location.hash = '/channels' })
  await main.getByTestId('wallet-view').waitFor({ state: 'detached' })
  await open()
  await install.click()
  await pendingInstall()
  await main.evaluate(() => { location.hash = '/channels' })
  await main.getByTestId('wallet-view').waitFor({ state: 'detached' })
  releaseInstall()
  await main.waitForTimeout(200)
  assert.equal(await main.getByTestId('wallet-upgrade-success').count(), 0)
  console.log('wallet upgrade Electron smoke passed: keyboard install, pending duplicate prevention, visible failure and retry, authoritative completion, stale reads/navigation, Chinese/French/Arabic RTL and light/dark at 760×540')
} catch (error) {
  console.error(error, calls.slice(-30), await main?.locator('body').innerText())
  await main?.screenshot({ path: join(shots, 'failure-debug.png') })
  throw error
} finally {
  releaseInstall?.(); releaseVersions?.()
  await app.close()
  for (const ws of sockets) ws.terminate()
  wss.close(); server.close()
}
