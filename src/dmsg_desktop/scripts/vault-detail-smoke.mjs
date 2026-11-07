// The built Electron UI with synthetic vault data and intercepted clipboards.
// No real account, secret, system clipboard, website or OS prompt is touched.
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WebSocketServer } from 'ws'

const account = '040g2081040g2081040g', token = 'vault-detail-fixture', itemId = '44'.repeat(32)
const SECRET = 'SYNTHETIC-KEY-ONLY\n' + 'ABCD0123456789'.repeat(18) + '\nEND-SYNTHETIC-KEY'
const LONG = Array.from({ length: 24 }, (_, i) => `${i + 1}. Synthetic recovery instruction — keep the complete value readable.`).join('\n')
const calls = [], sockets = new Set(), errors = []
let sequence = 0, locked = false, holdReveal = false, releaseReveal, rejectReveal = false, nextOtp
const status = () => ({ state: locked ? 'locked' : 'unlocked', account, device_id: '55'.repeat(32), approvals_waiting: 0, pending_sync: 0, background_online: false, fast_unlock: false, idle_lock_ms: 900000 })
const emit = (type, data = {}) => { for (const socket of sockets) socket.send(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { seq: ++sequence, type, data } })) }
const field = (id, type, value, label = '', section = '') => ({ id, type, value, label, section })
const entry = (id, title, fields, type = 'login', tags = []) => ({ id, revision: '66'.repeat(32), synced: true, deleted: false, item: { type, title, tags, notes: '', files: [], favorite: false, createdAt: Date.now(), updatedAt: Date.now(), fields } })
const entries = [
  entry(itemId, 'Apple · 工作账户', [field('username', 'text', 'person@example.test'), field('password', 'concealed', '********'), field('otp', 'otp', '********'), field('url', 'url', 'https://account.example.test/settings/security'), field('empty', 'text', ''), field('recovery', 'text', LONG, '恢复说明 · Recovery instructions', 'Account recovery')]),
  entry('77'.repeat(32), 'Second fixture', [field('username', 'text', 'second@example.test')]),
  entry('88'.repeat(32), 'Managed fixture', [field('private_key', 'concealed', '********')], 'key', ['dmsg-test'])
]
const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'app/activity': () => null, 'daemon/status': status, 'session/status': status,
  'profile/show': () => null, 'profile/pending': () => null,
  'approval/list': () => [], 'attest/history': () => [], 'channel/list': () => [],
  'agent/ids': () => ({ agents: [] }), 'grants/list': () => [],
  'vault/templates': () => ({ types: ['login', 'key'], templates: {} }),
  'vault/list': p => p.trash ? [] : entries,
  'vault/show': p => entries.find(e => e.id === p.id),
  'vault/health': () => [], 'vault/conflicts': () => [],
  'vault/history': () => [{ revision: '11'.repeat(32), at: Date.now() - 60000, device: '55'.repeat(32), current: false, synced: true }],
  'vault/edit': p => { Object.assign(entries[0].item, p.changes); emit('vault/changed'); return entries[0] },
  'vault/reveal': async p => {
    if (holdReveal) await new Promise(resolve => { releaseReveal = resolve })
    if (rejectReveal) throw Error('LOCKED')
    return { type: p.field === 'otp' ? 'otp' : 'concealed', field: p.field, value: p.field === 'otp' ? 'SYNTHETIC-SEED' : SECRET }
  },
  'vault/otp': async () => {
    const answer = nextOtp ?? { code: '123456', remaining_ms: 30000 }
    nextOtp = undefined
    if (answer.delay) await new Promise(resolve => setTimeout(resolve, answer.delay))
    if (answer.error) throw Error(answer.error)
    return { period_ms: 30000, remaining_ms: answer.remaining_ms, code: answer.code }
  },
  'secrets/ref': p => ({ uri: `dmsg-secret://${account}/${p.item_id}/${p.field_id}` })
}
const waitFor = async predicate => {
  for (let i = 0; i < 300; i++) { if (await predicate()) return; await new Promise(resolve => setTimeout(resolve, 20)) }
  assert.fail('fixture condition timed out')
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
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, error: { code: -32000, message: String(error), data: { code: error.message } } }))
      }
    })
  })
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-vault-detail-')), shots = resolve('test-results/vault-detail')
await mkdir(shots, { recursive: true })
await writeFile(join(profile, 'locale'), 'en')
await writeFile(join(profile, 'appearance'), 'light')
const app = await electron.launch({ executablePath: electronPath, colorScheme: null, args: [resolve('out/main/index.js')], env: { ...process.env, DMSG_DESKTOP_TEST: '1', DMSG_DESKTOP_USER_DATA: profile, DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`, DMSG_DESKTOP_TOKEN: token, DMSG_HOME: join(profile, 'home') } })
try {
  const main = await app.firstWindow()
  main.setDefaultTimeout(15000)
  main.on('pageerror', error => errors.push(error.message))
  await main.getByTestId('sidebar-toggle').waitFor()
  await app.evaluate(({ BrowserWindow, clipboard, shell, Menu }) => {
    Object.assign(globalThis, { testClipboard: '', testOpened: [], testTextMenus: [] })
    for (const window of BrowserWindow.getAllWindows()) window.webContents.setBackgroundThrottling(false)
    clipboard.writeText = async text => { globalThis.testClipboard = text }
    clipboard.write = async items => { globalThis.testClipboardTypes = items[0].types; globalThis.testClipboard = await (await items[0].getType('text/plain')).text() }
    clipboard.readText = async () => globalThis.testClipboard
    clipboard.clear = () => { globalThis.testClipboard = '' }
    shell.openExternal = async url => { globalThis.testOpened.push(url) }
    const build = Menu.buildFromTemplate
    Menu.buildFromTemplate = template => {
      const menu = build(template)
      menu.popup = () => { globalThis.testTextMenus.push(template.map(item => item.role ?? item.label)) }
      return menu
    }
  })
  await main.evaluate(() => {
    Object.assign(window, { testClipboard: '', rejectCopy: false })
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async text => { if (window.rejectCopy) throw Error('copy rejected'); window.testClipboard = text } } })
  })
  const browser = await app.browserWindow(main)
  const row = id => main.locator(`[data-testid="vault-field"][data-field="${id}"]`)
  const dialog = main.getByTestId('vault-field-viewer')
  const select = async (index = 0) => {
    await main.evaluate(() => { location.hash = '/vault' })
    await main.getByTestId('vault-list').getByText(entries[index].item.title, { exact: true }).click()
    await main.waitForFunction(title => document.querySelector('[data-testid="vault-item-title"]')?.textContent === title, entries[index].item.title)
  }
  const openMenu = async id => {
    await main.waitForFunction(() => !document.querySelector('[data-testid="vault-field-menu-content"][data-state="closed"]'))
    await row(id).getByTestId('vault-field-menu').click()
    await main.getByTestId('vault-field-menu-content').waitFor()
  }
  const view = async id => { await openMenu(id); await main.getByTestId('vault-field-view').click(); await dialog.waitFor() }
  const closeViewer = async id => {
    await main.keyboard.press('Escape')
    await dialog.waitFor({ state: 'detached' })
    await main.waitForFunction(id => document.activeElement === document.querySelector(`[data-testid="vault-field"][data-field="${id}"] [data-testid="vault-field-menu"]`), id)
  }
  const fits = async () => {
    assert.deepEqual(await main.evaluate(() => [...document.querySelectorAll('[data-testid="vault-inspector"], [data-testid="vault-field"], [data-testid="vault-field-viewer"], [data-testid="vault-field-menu-content"]')].filter(el => el.scrollWidth > el.clientWidth + 1).map(el => el.getAttribute('data-testid'))), [], 'fields and overlays fit horizontally')
    assert.equal(await main.getByTestId('vault-search').evaluate(el => { const pane = el.closest('section').getBoundingClientRect(), input = el.getBoundingClientRect(); return input.left >= pane.left && input.right <= pane.right }), true, 'long search translations fit the list pane')
    assert.equal(await dialog.evaluateAll(elements => elements.every(el => { const r = el.getBoundingClientRect(); return r.left >= -1 && r.right <= innerWidth + 1 && r.top >= -1 && r.bottom <= innerHeight + 1 })), true, 'the entire dialog stays in the viewport')
  }
  const screenshot = async name => {
    await main.locator('[role="status"] button').evaluateAll(buttons => buttons.forEach(button => button.click()))
    await main.locator('[data-testid="vault-inspector"], [role="dialog"], [role="menu"]').evaluateAll(elements => Promise.all(elements.flatMap(el => el.getAnimations({ subtree: true })).filter(a => a.animationName !== 'drain' && Number.isFinite(a.effect?.getComputedTiming().endTime)).map(a => a.finished.catch(() => {}))))
    await fits()
    await main.screenshot({ path: join(shots, name) })
  }
  await select()
  assert.equal(await row('empty').count(), 0)
  await row('username').getByTestId('vault-field-value').click()
  assert.equal(await main.evaluate(() => window.testClipboard), '', 'desktop values remain selectable without copying on click')
  await row('username').getByTestId('copy-field-value').click()
  await main.waitForFunction(() => window.testClipboard === 'person@example.test')
  assert.equal(await row('username').getByTestId('copy-field-value').getAttribute('data-copied'), 'true')
  await main.getByText('Field value copied', { exact: true }).first().waitFor()
  await openMenu('username')
  await main.getByTestId('vault-copy-field-reference').click()
  await main.waitForFunction(uri => window.testClipboard === uri, `dmsg-secret://${account}/${itemId}/username`)
  await main.getByText('Field reference copied', { exact: true }).first().waitFor()
  await main.evaluate(() => { window.rejectCopy = true; window.testClipboard = 'preserve' })
  await row('url').getByTestId('copy-field-value').click()
  await waitFor(async () => (await row('url').getByTestId('copy-field-value').getAttribute('aria-busy')) === 'false')
  assert.equal(await row('url').getByTestId('copy-field-value').getAttribute('data-copied'), 'false')
  assert.equal(await main.evaluate(() => window.testClipboard), 'preserve')
  await main.evaluate(() => { window.rejectCopy = false })

  // The same actions work via right click; selected text retains the OS copy menu.
  await row('username').getByTestId('vault-field-value').click({ button: 'right' })
  await main.getByTestId('vault-field-context-menu').waitFor()
  assert.equal(await main.getByTestId('vault-field-context-menu').getByTestId('vault-copy-field-reference').count(), 1)
  await main.keyboard.press('Escape')
  await main.getByTestId('vault-field-context-menu').waitFor({ state: 'detached' })
  await row('username').getByTestId('vault-field-value').evaluate(el => { const range = document.createRange(); range.selectNodeContents(el); const selection = window.getSelection(); selection.removeAllRanges(); selection.addRange(range) })
  await row('username').getByTestId('vault-field-value').click({ button: 'right' })
  await waitFor(async () => (await app.evaluate(() => globalThis.testTextMenus.length)) > 0)
  await main.getByTestId('vault-field-context-menu').waitFor({ state: 'detached' })
  await main.evaluate(() => window.getSelection()?.removeAllRanges())

  // Keyboard navigation enters the modal and returns to the exact field trigger.
  await row('username').getByTestId('vault-field-menu').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('vault-field-menu-content').waitFor()
  await main.keyboard.press('ArrowDown')
  await main.keyboard.press('Enter')
  await dialog.waitFor()
  assert.equal(await dialog.getByTestId('vault-large-value').innerText(), 'person@example.test')
  await main.keyboard.press('Tab')
  assert.equal(await main.evaluate(() => !!document.activeElement?.closest('[data-testid="vault-field-viewer"]')), true)
  await closeViewer('username')
  await view('recovery')
  assert.equal(await dialog.getByTestId('vault-large-value').innerText(), LONG)
  assert.equal(await dialog.getByTestId('vault-large-value').evaluate(el => el.parentElement.scrollHeight > el.parentElement.clientHeight), true)
  await dialog.getByTestId('vault-large-copy').click()
  await main.waitForFunction(value => window.testClipboard === value, LONG)
  await closeViewer('recovery')
  await openMenu('url')
  await main.getByTestId('vault-field-website').click()
  await waitFor(async () => (await app.evaluate(() => globalThis.testOpened.length)) === 1)
  assert.deepEqual(await app.evaluate(() => globalThis.testOpened), [entries[0].item.fields.find(f => f.id === 'url').value])

  // Secret copies never reveal plaintext in the renderer; large viewing shares consent and lifetime.
  await row('password').getByTestId('copy-secret').click()
  await waitFor(async () => (await app.evaluate(() => globalThis.testClipboard)) === SECRET)
  assert.equal(await row('password').getByTestId('vault-secret').getAttribute('data-shown'), 'false')
  if (process.platform === 'darwin') assert.ok((await app.evaluate(() => globalThis.testClipboardTypes)).some(type => type.includes('org.nspasteboard.ConcealedType')))
  const revealCount = () => calls.filter(c => c.method === 'vault/reveal').length
  let before = revealCount()
  await view('password')
  assert.equal(await dialog.getByTestId('vault-large-value').getAttribute('data-shown'), 'false')
  assert.equal(revealCount(), before)
  await dialog.getByTestId('reveal-secret').click()
  assert.equal(revealCount(), before)
  await dialog.getByTestId('reveal-secret').click()
  await main.waitForFunction(value => document.querySelector('[data-testid="vault-large-value"]')?.textContent === value, SECRET)
  await closeViewer('password')
  assert.equal(await row('password').getByTestId('vault-secret').getAttribute('data-shown'), 'false')

  await main.clock.install()
  await row('password').getByTestId('reveal-secret').click()
  await row('password').getByTestId('reveal-secret').click()
  await main.waitForSelector('[data-testid="vault-secret"][data-shown="true"]')
  before = revealCount()
  await main.clock.fastForward(55000)
  await view('password')
  assert.equal(await dialog.getByTestId('vault-large-value').innerText(), SECRET)
  assert.equal(revealCount(), before, 'opening an already shown value does not reveal again')
  emit('sync/changed')
  await main.waitForTimeout(100)
  assert.equal(await dialog.getByTestId('vault-large-value').getAttribute('data-shown'), 'true')
  await main.clock.fastForward(5100)
  await main.waitForSelector('[data-testid="vault-large-value"][data-shown="false"]')
  await closeViewer('password')

  rejectReveal = true
  await view('password')
  await dialog.getByTestId('reveal-secret').click()
  await dialog.getByTestId('reveal-secret').click()
  await dialog.locator('[role="alert"]').waitFor()
  rejectReveal = false
  await dialog.getByTestId('reveal-secret').click()
  await dialog.getByTestId('reveal-secret').click()
  await main.waitForSelector('[data-testid="vault-large-value"][data-shown="true"]')
  await closeViewer('password')
  for (const exit of ['close', 'revision', 'navigation', 'lock']) {
    await select()
    await view('password')
    holdReveal = true; releaseReveal = undefined
    await dialog.getByTestId('reveal-secret').click()
    await dialog.getByTestId('reveal-secret').click()
    await waitFor(() => !!releaseReveal)
    if (exit === 'close') await closeViewer('password')
    if (exit === 'revision') { entries[0].revision = 'aa'.repeat(32); emit('vault/changed'); await dialog.waitFor({ state: 'detached' }) }
    if (exit === 'navigation') { await main.evaluate(() => { location.hash = '/channels' }); await dialog.waitFor({ state: 'detached' }) }
    if (exit === 'lock') { locked = true; emit('session/changed', status()); await dialog.waitFor({ state: 'detached' }) }
    releaseReveal(); holdReveal = false
    await main.waitForTimeout(100)
    assert.equal(await main.getByTestId('vault-large-value').count(), 0)
    assert.equal(await main.locator('[data-testid="vault-secret"][data-shown="true"]').count(), 0)
    if (locked) { locked = false; emit('session/changed', status()) }
  }

  // Historical secrets stay read-only; managed fields retain desktop read/copy policy.
  await select()
  await main.getByTestId('vault-revision').click()
  await main.getByTestId('vault-version').click()
  const history = main.getByTestId('vault-version-detail')
  await history.getByTestId('vault-field-menu').click()
  assert.equal(await main.getByTestId('vault-field-copy').count(), 0)
  assert.equal(await main.getByTestId('vault-copy-field-reference').count(), 0)
  await main.getByTestId('vault-field-view').click()
  assert.equal(await dialog.getByTestId('vault-large-copy').count(), 0)
  await main.keyboard.press('Escape')
  await select(2)
  assert.equal(await main.getByTestId('vault-edit-open').count(), 0)
  assert.equal(await row('private_key').getByTestId('copy-secret').count(), 1)
  await openMenu('private_key')
  assert.equal(await main.getByTestId('vault-copy-field-reference').count(), 0)
  await main.keyboard.press('Escape')

  // Main-process OTP copies expire with the code, even when an RPC was slow.
  await select(1) // no background OTP request can consume a fixture response
  nextOtp = { code: '654321', remaining_ms: 900, delay: 150 }
  const copied = await main.evaluate(id => window.dmsg.copySecret(id, 'otp', null), itemId)
  assert.ok(copied.clear_after_ms > 0 && copied.clear_after_ms <= 750, JSON.stringify(copied))
  assert.equal(await app.evaluate(() => globalThis.testClipboard), '654321')
  await waitFor(async () => (await app.evaluate(() => globalThis.testClipboard)) === '')
  nextOtp = { code: '654321', remaining_ms: 700 }
  await main.evaluate(id => window.dmsg.copySecret(id, 'otp', null), itemId)
  await app.evaluate(() => { globalThis.testClipboard = 'a later user copy' })
  await main.waitForTimeout(800)
  assert.equal(await app.evaluate(() => globalThis.testClipboard), 'a later user copy')
  nextOtp = { code: 'expired', remaining_ms: 50, delay: 150 }
  const failed = await main.evaluate(async id => { try { await window.dmsg.copySecret(id, 'otp', null); return '' } catch (error) { return error.message } }, itemId)
  assert.match(failed, /SECRET_VERSION_STALE/)
  assert.equal(await app.evaluate(() => globalThis.testClipboard), 'a later user copy')

  // Live large codes refresh; a failed fetch can be retried in the field.
  nextOtp = { error: 'UNAVAILABLE' }
  await select()
  await row('otp').getByRole('button', { name: 'Retry', exact: true }).click()
  await row('otp').getByTestId('vault-otp-code').waitFor()
  await view('otp')
  assert.equal(await dialog.getByTestId('vault-large-value').innerText(), '123 456')
  nextOtp = { code: '789012', remaining_ms: 30000 }
  await main.clock.fastForward(30100)
  await main.waitForFunction(() => document.querySelector('[data-testid="vault-large-value"]')?.textContent === '789 012')
  await closeViewer('otp')

  await main.getByTestId('vault-more').click()
  await main.getByTestId('vault-star').click()
  await waitFor(() => entries[0].item.favorite)
  assert.equal(await main.getByTestId('vault-inspector').locator('header').getByTestId('vault-star').count(), 0)
  // Screenshot matrix includes Chinese, a longer translation, RTL and the minimum window.
  for (const [lang, theme, width, height] of [['zh', 'light', 1180, 760], ['zh', 'light', 760, 540], ['zh', 'dark', 760, 540], ['de', 'light', 760, 540], ['de', 'dark', 760, 540], ['ar', 'light', 760, 540], ['ar', 'dark', 760, 540]]) {
    await main.evaluate(async ([lang, theme]) => { await window.dmsg.setLocale(lang); await window.dmsg.setAppearance(theme) }, [lang, theme])
    await main.waitForFunction(([lang, theme]) => document.documentElement.lang === (lang === 'zh' ? 'zh-Hans' : lang) && matchMedia('(prefers-color-scheme: dark)').matches === (theme === 'dark'), [lang, theme])
    await browser.evaluate((window, [width, height]) => window.setContentSize(width, height), [width, height])
    await row('username').scrollIntoViewIfNeeded()
    await main.evaluate(() => document.activeElement instanceof HTMLElement && document.activeElement.blur())
    await main.mouse.move(10, 10)
    await screenshot(`vault-${lang}-${theme}-${width}.png`)
    await openMenu('password')
    await screenshot(`menu-${lang}-${theme}-${width}.png`)
    await main.getByTestId('vault-field-view').click()
    await dialog.waitFor()
    await dialog.getByTestId('reveal-secret').click()
    await screenshot(`viewer-${lang}-${theme}-${width}.png`)
    await closeViewer('password')
  }
  await browser.evaluate(window => { window.setContentSize(1180, 760); window.webContents.setZoomFactor(2) })
  await main.waitForFunction(() => innerWidth <= 592 && innerHeight <= 382)
  await view('recovery')
  await dialog.evaluate(el => Promise.all(el.getAnimations({ subtree: true }).map(animation => animation.finished.catch(() => {}))))
  assert.equal(await dialog.getByTestId('vault-large-value').evaluate(el => getComputedStyle(el).direction), 'ltr', 'English field content retains its punctuation order in an RTL interface')
  await fits()
  await main.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
  const zoomCapture = await browser.evaluate(async window => (await window.webContents.capturePage()).toPNG().toString('base64'))
  await writeFile(join(shots, 'viewer-ar-dark-zoom200.png'), Buffer.from(zoomCapture, 'base64'))
  await closeViewer('recovery')
  assert.deepEqual(errors, [])
  console.log('vault detail Electron smoke passed: field menus, text selection, clipboard feedback, keyboard/focus, full values, secret consent/expiry and stale replies, history/managed policy, OTP lifetime/refresh, multilingual narrow and zoomed layouts')
} catch (error) {
  for (const [index, page] of app.windows().entries()) {
    await page.screenshot({ path: join(shots, `failure-${index}.png`) }).catch(() => {})
    await writeFile(join(shots, `failure-${index}.txt`), await page.locator('body').innerText().catch(() => ''))
  }
  console.error('Last fixture calls', calls.slice(-12).map(c => c.method), 'Renderer errors', errors)
  throw error
} finally {
  releaseReveal?.()
  await app.close(); wss.close(); server.close()
}
