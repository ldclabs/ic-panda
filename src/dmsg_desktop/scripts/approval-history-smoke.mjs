// Real Electron history/details fed by the client's old-wallet regression test.
// From the repo root, first generate the synthetic fixture:
// DMSG_WALLET_APPROVAL_FIXTURE=/tmp/dmsg-wallet-approvals.json cargo test -p dmsg_client approval_history_reads_pre_1_2
// Then, from this desktop directory, after pnpm build:
// node scripts/approval-history-smoke.mjs /tmp/dmsg-wallet-approvals.json
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WebSocketServer } from 'ws'

assert.ok(process.argv[2], 'pass the synthetic client test fixture')
const history = JSON.parse(await readFile(process.argv[2], 'utf8'))
assert.equal(history.length, 2)
const token = 'approval-history-fixture', pageErrors = [], calls = []
const status = { state: 'unlocked', session: 'history-fixture', account: '040g2081040g2081040g',
  device_id: '33'.repeat(32), fast_unlock: false, approvals_waiting: 0,
  pending_sync: 0, background_online: false, idle_lock_ms: 900000 }
const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status }),
  'app/activity': () => null, 'daemon/status': () => status, 'session/status': () => status,
  'profile/show': () => null, 'profile/pending': () => null,
  'channel/list': () => [], 'legacy/pairs': () => [], 'agent/ids': () => ({ agents: [] }),
  'approval/list': p => p.ids_only ? [] : history,
  'approval/show': p => history.find(r => r.id === p.id)
}
const server = createServer((_, response) => response.writeHead(404).end())
const wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (request, socket, head) => {
  if (request.headers.authorization !== `Bearer ${token}`) return socket.destroy()
  wss.handleUpgrade(request, socket, head, ws => {
    ws.on('message', bytes => {
      const message = JSON.parse(String(bytes))
      calls.push(message.method)
      const handler = methods[message.method]
      ws.send(JSON.stringify(handler
        ? { jsonrpc: '2.0', id: message.id, result: handler(message.params ?? {}) }
        : { jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'Not found', data: { code: 'NOT_FOUND' } } }))
    })
  })
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-approval-history-'))
const shots = resolve('test-results/approval-history')
await mkdir(shots, { recursive: true })
await writeFile(join(profile, 'locale'), 'en')
await writeFile(join(profile, 'appearance'), 'light')
const app = await electron.launch({ executablePath: electronPath, colorScheme: null,
  args: [resolve('out/main/index.js')], env: { ...process.env, DMSG_DESKTOP_TEST: '1',
    DMSG_DESKTOP_USER_DATA: profile, DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`,
    DMSG_DESKTOP_TOKEN: token, DMSG_HOME: join(profile, 'home') } })
const watch = page => {
  page.setDefaultTimeout(10000)
  page.on('pageerror', error => pageErrors.push(error.message))
}
app.on('window', watch)
for (const page of app.windows()) watch(page)
try {
  const main = await app.firstWindow()
  await main.getByTestId('sidebar-toggle').waitFor()
  await main.evaluate(() => { location.hash = '/approvals' })
  await main.waitForFunction(() => document.querySelectorAll('[data-testid="history-open"]').length === 2)
  assert.equal(await main.getByTestId('approvals-error').count(), 0)
  await main.getByTestId('history-open').first().focus()
  const [approval] = await Promise.all([app.waitForEvent('window'), main.keyboard.press('Enter')])
  await approval.getByTestId('approval-outcome').waitFor()
  assert.equal(await approval.getByTestId('approval-error').count(), 0)
  assert.equal(await approval.getByTestId('approve').count(), 0)
  assert.equal(await approval.getByTestId('reject').count(), 0)
  await (await app.browserWindow(main)).evaluate(window => window.setSize(760, 540))
  await (await app.browserWindow(approval)).evaluate(window => window.setSize(400, 560))
  for (const language of ['zh', 'de', 'ar']) {
    await main.evaluate(language => window.dmsg.setLocale(language), language)
    for (const page of [main, approval]) {
      await page.waitForFunction(language => document.documentElement.lang === language, language === 'zh' ? 'zh-Hans' : language)
    }
    for (const appearance of ['light', 'dark']) {
      await main.evaluate(appearance => window.dmsg.setAppearance(appearance), appearance)
      for (const [name, page] of [['history', main], ['detail', approval]]) {
        await page.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${name} must fit`)
        await page.screenshot({ path: join(shots, `${name}-${language}-${appearance}.png`) })
      }
    }
  }
  assert.equal(await main.locator('html').getAttribute('dir'), 'rtl')
  for (const request of history) {
    await main.evaluate(id => window.dmsg.openApproval(id), request.id)
    await approval.waitForFunction(id => location.hash.endsWith(id), request.id)
    await approval.getByTestId('approval-outcome').waitFor()
    assert.equal(await approval.getByTestId('approval-detail-unavailable').count(), 0)
    assert.equal(await approval.getByTestId('approval-error').count(), 0)
  }
  assert.ok(!calls.includes('approval/respond'), 'history never submits a decision')
  assert.deepEqual(pageErrors, [])
  console.log('approval history Electron smoke passed: decoded pre-1.2 decide/cosign records, keyboard open, read-only detail, minimum sizes, zh/de/ar, light/dark')
} finally {
  await app.close()
  wss.close()
  await new Promise(resolve => server.close(resolve))
}
