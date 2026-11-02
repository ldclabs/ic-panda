// Real Electron renderer with synthetic channel data; never sends live messages.
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile, readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import assert from 'node:assert/strict'
import { WebSocketServer } from 'ws'

const account = '040g2081040g2081040g', peer = '0123456789abcdefghi0', id = '33'.repeat(32), token = 'channel-ui-fixture'
const calls = [], sockets = new Set()
let seq = 0, releaseSend
const status = () => ({ state: 'unlocked', session: 'fixture', account, device_id: '55'.repeat(32), approvals_waiting: 0, pending_sync: 0, background_online: false, fast_unlock: false, idle_lock_ms: 900000 })
const channel = { id, name: '频道消息 · Conversation', genesis: '44'.repeat(32), ledger: { channel_id: id, type: 'collaboration', owner: account, epoch: 1, status: 'active', members: Object.fromEntries([account, peer].map((account, index) => [account, { account, role: index ? 'member' : 'owner', joined_epoch: 1, can_rotate: !index, history: [] }])) }, message_cursor: 2, readable_epochs: [1] }
const row = (seq, account, text) => ({ seq, account, text, message_id: String(seq).padStart(64, '0'), digest: 'dd'.repeat(32), epoch: 1, created_at: Date.now(), stored_at: Date.now(), file: null })
const messages = [row(1, peer, '收到 👋 你好，欢迎加入频道。'), row(2, account, '测试消息 · My message')]
const png = `data:image/png;base64,${(await readFile('resources/icon.png')).toString('base64')}`
const emit = (type, data = {}) => { for (const socket of sockets) socket.send(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { seq: ++seq, type, data } })) }
const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }), 'app/activity': () => null,
  'session/status': status, 'daemon/status': status, 'session/accounts': () => [],
  'profile/display': () => ({ name: 'Me', avatar: null }), 'profile/show': () => null,
  'approval/list': () => [], 'legacy/channels': () => ({ channels: [], errors: [] }), 'legacy/list': () => [], 'legacy/pairs': () => [],
  'channel/list': () => [channel], 'channel/show': () => channel,
  'channel/message_page': () => ({ items: messages, has_more: false, next_before: null }),
  'channel/messages': p => messages.filter(m => m.seq > (p.after ?? 0)),
  'channel/sync': () => ({ through: messages.length, received: 0 }), 'channel/outbox': () => [],
  'channel/receipts': () => ({ latest_seq: messages.length, cursors: {} }),
  'channel/mark': () => channel, 'channel/draft': () => null,
  'channel/names': p => Object.fromEntries(p.accounts.map(a => [a, { name: a === account ? 'Me' : 'ICPanda', avatar_url: `https://avatar.example/${a}.png`, stale: false }])),
  'channel/avatar': () => ({ url: png }),
  'channel/queue': p => ({ message_id: p.message_id }),
  'channel/send': async p => { await new Promise(resolve => { releaseSend = resolve }); const message = { ...row(messages.length + 1, account, p.text), message_id: p.message_id }; messages.push(message); return { message, receipt: { seq: message.seq } } },
  'channel/hide': p => { const message = messages.find(m => m.seq === p.seq); message.hidden = p.hidden; return message },
  'notifications/context': () => null, 'notifications/observed': () => null,
  'notifications/next': () => ({ plans: [], remove: [] }),
  'notifications/status': () => ({ counts: { action: 0, message: 0, security: 0, service: 0 }, badge: 0 })
}
const server = createServer((_, res) => res.writeHead(404).end()), wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (req, socket, head) => {
  if (req.headers.authorization !== `Bearer ${token}`) return socket.destroy()
  wss.handleUpgrade(req, socket, head, ws => {
    sockets.add(ws); ws.on('close', () => sockets.delete(ws))
    ws.on('message', async bytes => {
      const m = JSON.parse(String(bytes)); calls.push(m)
      try { ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, result: await methods[m.method]?.(m.params ?? {}) ?? null })) }
      catch { ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, error: { code: -32000, message: 'Unavailable', data: { code: 'UNAVAILABLE' } } })) }
    })
  })
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-channel-ui-')), shots = resolve('test-results/channel-messages')
await mkdir(shots, { recursive: true }); await writeFile(join(profile, 'locale'), 'en'); await writeFile(join(profile, 'appearance'), 'light')
const app = await electron.launch({ executablePath: electronPath, colorScheme: null, args: [resolve('out/main/index.js')], env: { ...process.env, DMSG_DESKTOP_TEST: '1', DMSG_DESKTOP_USER_DATA: profile, DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`, DMSG_DESKTOP_TOKEN: token, DMSG_HOME: join(profile, 'home') } })
try {
  const main = await app.firstWindow(); main.setDefaultTimeout(15000)
  const failures = []; main.on('pageerror', error => failures.push(`${main.url()} ${error.stack}`))
  await main.getByTestId('channel-item').first().click()
  await main.locator('[data-seq="2"] img').waitFor()
  const own = main.locator('article[data-seq="2"]'), incoming = main.locator('article[data-seq="1"]')
  const right = async locator => assert.ok(await locator.evaluate(el => { const avatar = (el.querySelector('img') ?? el.firstElementChild).getBoundingClientRect(), row = el.getBoundingClientRect(); return avatar.x > row.x + row.width / 2 }), 'own avatar or continuation belongs on the right')
  await right(own)
  assert.ok(await incoming.evaluate(el => el.querySelector('img').getBoundingClientRect().x < el.getBoundingClientRect().x + 50))
  const avatarReads = calls.filter(c => c.method === 'channel/avatar').length
  await main.evaluate(() => { location.hash = '/settings/appearance' }); await main.getByTestId('channel-messages').waitFor({ state: 'detached' })
  await main.evaluate(() => { location.hash = '/channels' }); await main.getByTestId('channel-item').first().click(); await main.locator('[data-seq="2"] img').waitFor()
  assert.equal(calls.filter(c => c.method === 'channel/avatar').length, avatarReads, 'reopening the channel reuses in-memory images')
  const draft = main.getByTestId('channel-text')
  await draft.fill('Pending message 发送中'); await draft.press('Enter')
  const pending = main.getByTestId('message-outgoing'); await pending.waitFor(); await right(pending)
  while (!releaseSend) await new Promise(resolve => setTimeout(resolve, 20))
  await main.screenshot({ path: join(shots, 'pending-light.png') })
  releaseSend(); await pending.waitFor({ state: 'detached' }); await right(main.locator('article[data-seq="3"]'))
  await main.evaluate(() => { Object.defineProperty(document, 'hasFocus', { configurable: true, value: () => true }) })
  const plan = { system_id: 'a'.repeat(64), delivery_id: 'b'.repeat(64), account, category: 'message', target: { kind: 'channel', id }, covered: [{ id: 'c'.repeat(64), version: '1', stage: 1 }], count: null, sound: false, in_app: true, update: false, attempted_at: Date.now() }
  emit('notifications/present', plan)
  await main.waitForTimeout(150); assert.equal(await main.getByTestId('notification-toast').count(), 0)
  emit('notifications/present', { ...plan, count: 1 }); await main.getByTestId('notification-toast').waitFor()
  await main.getByTestId('notification-toast').getByRole('button').last().click()
  const menu = async () => { await incoming.getByTestId('message-more').focus(); await main.keyboard.press('Enter') }
  await menu()
  for (const label of ['Reply', 'Copy', 'Hide on this device']) await main.getByRole('menuitem', { name: label, exact: true }).waitFor()
  assert.equal(await main.getByTestId('message-report').count(), 0)
  assert.equal(await main.getByTestId('message-block').count(), 0)
  await main.getByTestId('message-hide').click()
  await main.getByTestId('message-hidden').getByRole('button', { name: 'Show', exact: true }).click()
  await incoming.waitFor()
  messages[1].text = '很长的消息，验证自动换行与窄窗口。 A long message that should wrap naturally. '.repeat(9); emit('channel/changed')
  for (const locale of ['zh', 'de', 'ar']) {
    await main.evaluate(locale => window.dmsg.setLocale(locale), locale)
    await main.waitForFunction(locale => document.documentElement.lang === (locale === 'zh' ? 'zh-Hans' : locale), locale)
    for (const theme of ['light', 'dark']) {
      await main.evaluate(theme => window.dmsg.setAppearance(theme), theme)
      await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(760, 540))
      await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, theme === 'dark')
      const members = main.getByTestId('channel-members'); if (await members.isVisible()) await members.getByRole('button').first().click()
      await right(own)
      assert.ok(await main.getByTestId('channel-messages').evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'timeline fits the minimum window')
      await main.screenshot({ path: join(shots, `${locale}-${theme}-narrow.png`) })
      await menu()
      assert.equal(await main.getByTestId('message-report').count(), 0)
      assert.equal(await main.getByTestId('message-block').count(), 0)
      await main.screenshot({ path: join(shots, `${locale}-${theme}-menu.png`) })
      await main.keyboard.press('Escape')
    }
  }
  const beforeNames = calls.filter(call => call.method === 'channel/names').length
  await main.getByTestId('channel-messages').evaluate(el => { el.scrollTop = el.scrollHeight })
  messages.push(...Array.from({ length: 60 }, (_, i) => row(4 + i, `${(i + 1).toString(16).padStart(19, '0')}0`, `Participant ${i + 1}`)))
  emit('channel/changed')
  await main.locator('article[data-seq="63"]').waitFor()
  await main.waitForTimeout(300)
  const requested = new Set(calls.filter(call => call.method === 'channel/names').slice(beforeNames).flatMap(call => call.params.accounts))
  assert.ok(requested.size > 0 && requested.size < 20, `only visible authors should be fetched, got ${requested.size}/60`)
  assert.deepEqual(failures, [])
  console.log(`Channel Electron checks passed; screenshots: ${shots}`)
} catch (error) {
  const page = await app.firstWindow()
  await page.screenshot({ path: join(shots, 'failure.png') })
  console.log('Recent calls:', calls.slice(-15).map(call => call.method))
  throw error
} finally { await app.close(); for (const socket of sockets) socket.terminate(); wss.close(); await new Promise(resolve => server.close(resolve)) }
