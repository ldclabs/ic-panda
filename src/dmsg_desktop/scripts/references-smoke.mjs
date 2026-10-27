// Real Electron views with synthetic metadata and an intercepted clipboard.
// No real account, secret value, system clipboard or OS extension is used.
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WebSocketServer } from 'ws'

const account = '040g2081040g2081040g', token = 'reference-ui-fixture'
const fileId = '11'.repeat(32), older = '22'.repeat(32), current = '33'.repeat(32), itemId = '44'.repeat(32)
const calls = [], sockets = new Set()
let sequence = 0, locked = false, conflict = false, holdReference = false, releaseReference, rejectReference = false
const status = () => ({ state: locked ? 'locked' : 'unlocked', account, device_id: '55'.repeat(32), approvals_waiting: 0, pending_sync: 0, background_online: false, fast_unlock: false, idle_lock_ms: 900000 })
const emit = (type, data = {}) => { for (const socket of sockets) socket.send(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { seq: ++sequence, type, data } })) }
const field = (id, type, value) => ({ id, type, value, label: id, section: '' })
const entry = (id, kind, title, tags = []) => ({ id, revision: '66'.repeat(32), synced: true, deleted: false, item: { type: kind, title, tags, notes: '', files: [], favorite: false, createdAt: Date.now(), updatedAt: Date.now(), fields: [field('credential', 'concealed', '********'), field('username', 'text', 'synthetic-user'), field('otp', 'otp', '********')] } })
const entries = [entry(itemId, 'api', 'Reference fixture'), entry('77'.repeat(32), 'wallet', 'Wallet fixture'), entry('88'.repeat(32), 'key', 'Managed fixture', ['dmsg-test'])]
const file = { id: fileId, folder: 'root', name: 'reference-fixture.bin', mime: 'application/octet-stream', size: 42, sha256: '99'.repeat(32), tags: [], note: '', starred: false, created_at: Date.now(), updated_at: Date.now(), current, versions: 2, status: 'local', path: [] }
const versions = [older, current].map((id, index) => ({ id, size: 42, sha256: file.sha256, created_at: Date.now() - (1 - index) * 60000, device: '55'.repeat(32), origin: { type: 'imported' }, local: !!index, current: !!index, status: index ? 'local' : 'cloud_only' }))
const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'app/activity': () => null, 'daemon/status': status, 'session/status': status,
  'profile/show': () => null, 'profile/pending': () => null,
  'approval/list': () => [], 'attest/history': () => [], 'channel/list': () => [],
  'agent/ids': () => ({ agents: [] }), 'grants/list': () => [],
  'vault/templates': () => ({ types: ['api', 'wallet', 'key'], templates: {} }),
  'vault/list': p => p.trash ? [] : entries,
  'vault/show': p => entries.find(e => e.id === p.id),
  'vault/otp': () => ({ code: '123456', period_ms: 30000, remaining_ms: 30000 }),
  'vault/health': () => [], 'vault/conflicts': () => conflict ? [{ copy: entries[0] }] : [],
  'files/tree': () => [], 'files/list': () => ({ folder: null, folders: [], files: [file] }),
  'files/show': () => ({ file, versions }), 'files/versions': () => versions,
  'files/tags': () => [], 'files/share_links': () => [], 'files/backup_status': () => [],
  'files/usage': () => null, 'files/mount_status': () => null,
  'secrets/ref': async p => {
    assert.deepEqual(Object.keys(p).sort(), ['field_id', 'item_id'])
    assert.equal(p.item_id, itemId)
    assert.ok(['credential', 'username'].includes(p.field_id))
    if (holdReference) await new Promise(resolve => { releaseReference = resolve })
    if (rejectReference) throw Error('SECRET_VERSION_STALE')
    return { uri: `dmsg-secret://${account}/${p.item_id}/${p.field_id}` }
  }
}
const server = createServer((_, res) => res.writeHead(404).end())
const wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (req, socket, head) => {
  if (req.headers.authorization !== `Bearer ${token}`) return socket.destroy()
  wss.handleUpgrade(req, socket, head, ws => {
    sockets.add(ws); ws.on('close', () => sockets.delete(ws))
    ws.on('message', async bytes => {
      const m = JSON.parse(String(bytes)); calls.push(m)
      try {
        const handler = methods[m.method]
        ws.send(JSON.stringify(handler ? { jsonrpc: '2.0', id: m.id, result: await handler(m.params ?? {}) } : { jsonrpc: '2.0', id: m.id, error: { code: -32601, message: 'Not found', data: { code: 'NOT_FOUND' } } }))
      } catch (error) {
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, error: { code: -32000, message: String(error), data: { code: 'SECRET_VERSION_STALE' } } }))
      }
    })
  })
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-reference-ui-')), shots = resolve('test-results/references')
await mkdir(shots, { recursive: true })
await writeFile(join(profile, 'locale'), 'en')
await writeFile(join(profile, 'appearance'), 'light')
const app = await electron.launch({ executablePath: electronPath, colorScheme: null, args: [resolve('out/main/index.js')], env: { ...process.env, DMSG_DESKTOP_TEST: '1', DMSG_DESKTOP_USER_DATA: profile, DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`, DMSG_DESKTOP_TOKEN: token, DMSG_HOME: join(profile, 'home') } })
try {
  const main = await app.firstWindow()
  main.setDefaultTimeout(20000)
  await main.waitForSelector('[data-testid="sidebar-toggle"]', { timeout: 20000 })
  await app.evaluate(({ BrowserWindow }) => { for (const window of BrowserWindow.getAllWindows()) window.webContents.setBackgroundThrottling(false) })
  await main.evaluate(() => { Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async text => { window.testClipboard = text } } }) })
  const copy = async (locator, expected) => {
    await main.evaluate(() => { window.testClipboard = 'unchanged' })
    await locator.click()
    await main.waitForFunction(text => window.testClipboard === text, expected)
  }
  const openVault = async title => {
    await main.evaluate(() => { location.hash = '/vault' })
    await main.getByText(title, { exact: true }).first().click()
    await main.getByTestId('vault-inspector').waitFor()
  }
  const fits = async () => assert.ok(await main.evaluate(() => [...document.querySelectorAll('[data-testid="files-detail"], [data-testid="vault-inspector"]')].every(e => e.scrollWidth <= e.clientWidth + 1)), 'detail must fit horizontally')
  const screenshot = async name => {
    // Dismiss only test-generated notifications so the view itself is visible.
    await main.locator('[role="status"] button').evaluateAll(buttons => buttons.forEach(button => button.click()))
    await fits()
    for (const id of ['vault-item-title', 'files-name']) {
      const title = main.getByTestId(id)
      if (await title.count()) assert.ok((await title.boundingBox()).height < 100, 'title must remain readable in a narrow pane')
    }
    await main.screenshot({ path: join(shots, name) })
  }

  await main.evaluate(() => { location.hash = '/files' })
  await main.getByText(file.name, { exact: true }).first().click()
  await copy(main.getByTestId('files-copy-id'), fileId)
  assert.equal(await main.getByTestId('files-copy-id').innerText(), '1111…1111')
  assert.equal(await main.getByTestId('files-copy-current-version').innerText(), '3333…3333')
  assert.equal(await main.getByTestId('files-facts').getByTestId('files-copy-id').count(), 0)
  assert.equal(await main.getByTestId('files-identifiers').evaluate(el => el === el.parentElement.lastElementChild && !!(document.querySelector('[data-testid="files-versions"]').compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING)), true)
  await main.evaluate(() => { window.testClipboard = 'unchanged' })
  await main.getByTestId('files-copy-id').press('Tab')
  await main.getByTestId('files-copy-id').focus()
  await main.locator('[data-tooltip-content]').filter({ hasText: fileId }).waitFor()
  await main.keyboard.press('Enter')
  await main.waitForFunction(id => window.testClipboard === id, fileId)
  await copy(main.getByTestId('files-copy-current-version'), current)
  await copy(main.getByTestId('files-copy-reference'), `dmsg-file://${fileId}/${current}`)
  const historical = main.locator(`[data-testid="files-version-identity"][data-version="${older}"]`)
  await copy(historical.getByTestId('files-copy-version'), older)
  assert.equal(await historical.getByTestId('files-copy-version').innerText(), '2222…2222')
  await copy(historical.getByTestId('files-copy-version-reference'), `dmsg-file://${fileId}/${older}`)
  await main.getByTestId('files-facts').scrollIntoViewIfNeeded()
  await screenshot('files-overview.png')
  await main.getByTestId('files-copy-id').scrollIntoViewIfNeeded()
  await screenshot('files.png')
  await historical.scrollIntoViewIfNeeded()
  await screenshot('file-versions.png')

  await openVault('Reference fixture')
  await copy(main.getByTestId('vault-copy-id'), itemId)
  assert.equal(await main.getByTestId('vault-copy-id').innerText(), '4444…4444')
  assert.equal(await main.getByTestId('vault-identifiers').evaluate(el => el === el.parentElement.lastElementChild && !!(document.querySelector('[data-testid="vault-detail"]').compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING)), true)
  await main.getByTestId('vault-copy-id').press('Tab')
  await main.getByTestId('vault-copy-id').focus()
  await main.locator('[data-tooltip-content]').filter({ hasText: itemId }).waitFor()
  const refButton = field => main.locator(`[data-testid="vault-field"][data-field="${field}"]`).getByTestId('vault-copy-field-reference')
  await copy(refButton('credential'), `dmsg-secret://${account}/${itemId}/credential`)
  await copy(refButton('username'), `dmsg-secret://${account}/${itemId}/username`)
  assert.equal(await refButton('otp').count(), 0)
  await main.getByTestId('vault-copy-id').scrollIntoViewIfNeeded()
  await screenshot('vault.png')

  // A failed authoritative lookup must not manufacture or copy a reference.
  rejectReference = true
  await main.evaluate(() => { window.testClipboard = 'unchanged' })
  await refButton('credential').click()
  await main.waitForFunction(() => !document.querySelector('[data-testid="vault-copy-field-reference"]')?.disabled)
  assert.equal(await main.evaluate(() => window.testClipboard), 'unchanged')
  rejectReference = false

  conflict = true; emit('vault/changed')
  await main.waitForFunction(() => document.querySelector('[data-testid="vault-copy-field-reference"]')?.disabled)
  conflict = false; emit('vault/changed')
  await main.waitForFunction(() => !document.querySelector('[data-testid="vault-copy-field-reference"]')?.disabled)
  for (const title of ['Wallet fixture', 'Managed fixture']) {
    await openVault(title)
    assert.equal(await main.getByTestId('vault-copy-field-reference').count(), 0)
  }

  // A source update, navigation or lock invalidates a pending clipboard action.
  for (const exit of ['revision', 'navigation', 'lock']) {
    await openVault('Reference fixture')
    holdReference = true; releaseReference = undefined
    await main.evaluate(() => { window.testClipboard = 'unchanged' })
    await refButton('credential').click()
    for (let i = 0; i < 100 && !releaseReference; i++) await new Promise(resolve => setTimeout(resolve, 20))
    assert.ok(releaseReference)
    if (exit === 'revision') {
      entries[0].revision = 'aa'.repeat(32); emit('vault/changed')
      await main.waitForFunction(revision => document.querySelector('[data-testid="vault-revision"]')?.title === revision, entries[0].revision)
    } else {
      if (exit === 'navigation') await main.evaluate(() => { location.hash = '/files' })
      else { locked = true; emit('session/changed', status()) }
      await main.getByTestId('vault-inspector').waitFor({ state: 'detached' })
    }
    releaseReference(); holdReference = false
    // Wait for the response and its UI microtasks without changing the clipboard.
    await main.waitForTimeout(250)
    assert.equal(await main.evaluate(() => window.testClipboard), 'unchanged')
  }
  locked = false; emit('session/changed', status())
  await openVault('Reference fixture')
  await main.evaluate(() => window.dmsg.setLocale('zh'))
  await main.getByRole('button', { name: '复制条目 ID', exact: true }).waitFor()
  await (await app.browserWindow(main)).evaluate(window => window.setContentSize(760, 540))
  await main.getByTestId('vault-copy-id').scrollIntoViewIfNeeded()
  await screenshot('vault-zh-narrow.png')
  await main.evaluate(() => window.dmsg.setAppearance('dark'))
  await main.waitForFunction(() => matchMedia('(prefers-color-scheme: dark)').matches)
  await screenshot('vault-zh-dark-narrow.png')
  await main.evaluate(() => window.dmsg.setLocale('ar'))
  await main.waitForFunction(() => document.documentElement.dir === 'rtl')
  await screenshot('vault-ar-narrow.png')
  await main.evaluate(() => { location.hash = '/files' })
  await main.getByText(file.name, { exact: true }).first().click()
  await main.getByTestId('files-copy-id').scrollIntoViewIfNeeded()
  await screenshot('files-ar-dark-narrow.png')
  await main.getByTestId('files-places-open').click()
  await main.getByTestId('files-places-dialog').waitFor()
  await main.keyboard.press('Escape')
  await main.getByTestId('files-places-dialog').waitFor({ state: 'detached' })
  await main.waitForFunction(() => document.activeElement?.getAttribute('data-testid') === 'files-places-open')
  await main.getByTestId('files-places-open').click()
  await main.getByTestId('files-places-dialog').getByTestId('files-scope-root').click()
  await main.getByTestId('files-places-dialog').waitFor({ state: 'detached' })
  await main.getByText(file.name, { exact: true }).first().click()
  await main.getByTestId('files-copy-id').scrollIntoViewIfNeeded()
  await fits()

  assert.equal(calls.filter(c => ['vault/reveal', 'vault/copy', 'files/export', 'files/read', 'files/preview', 'secrets/run/claim'].includes(c.method)).length, 0)
  console.log('references Electron smoke passed: bottom-positioned shortened IDs, full-value tooltips and clipboard, exact current/historical version references, value-free field references, unsupported fields, conflicts, rejected/stale results, lock/navigation and narrow RTL layout')
} catch (error) {
  for (const [index, page] of app.windows().entries()) {
    await page.screenshot({ path: join(shots, `failure-${index}.png`) }).catch(() => {})
    await writeFile(join(shots, `failure-${index}.txt`), await page.locator('body').innerText().catch(() => ''))
  }
  console.error('Last fixture calls', calls.slice(-12).map(c => c.method))
  throw error
} finally {
  releaseReference?.()
  await app.close(); wss.close(); server.close()
}
