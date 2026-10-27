// Real Electron against the Rust daemon test harness, with only the transport
// proxy controlled here. No RPC response is mocked. All data is synthetic.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import WebSocket, { WebSocketServer } from 'ws'

const [origin, token] = process.argv.slice(2)
assert.ok(origin && token)
const server = createServer()
const wss = new WebSocketServer({ server })
let reads = 0
let holdStatus = false
let heldStatus = []
wss.on('connection', (socket) => {
  const statusIds = new Set()
  const upstream = new WebSocket(origin.replace(/^http/, 'ws') + '/ws/app/v1', { headers: { authorization: `Bearer ${token}` } })
  const waiting = []
  upstream.on('open', () => { for (const data of waiting) upstream.send(data, { binary: false }); waiting.length = 0 })
  socket.on('message', (data) => {
    const request = JSON.parse(String(data))
    if (request.method === 'profile/display') reads++
    if (request.method === 'daemon/status') statusIds.add(request.id)
    if (upstream.readyState === WebSocket.OPEN) upstream.send(data, { binary: false })
    else waiting.push(data)
  })
  upstream.on('message', (data) => {
    const reply = JSON.parse(String(data))
    const send = () => { if (socket.readyState === WebSocket.OPEN) socket.send(data, { binary: false }) }
    if (statusIds.delete(reply.id) && holdStatus) heldStatus.push(send)
    else send()
  })
  upstream.on('error', () => socket.close())
  upstream.on('close', () => socket.close())
  socket.on('close', () => upstream.close())
})
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-cache-real-'))
await writeFile(join(profile, 'locale'), 'en')
let app
try {
  app = await electron.launch({ executablePath: electronPath, args: [resolve('out/main/index.js')], env: {
    ...process.env, DMSG_DESKTOP_TEST: '1', DMSG_DESKTOP_USER_DATA: profile,
    DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`, DMSG_DESKTOP_TOKEN: token, DMSG_HOME: join(profile, 'home')
  } })
  const page = await app.firstWindow()
  await page.waitForSelector('[data-testid="account-switcher"]', { timeout: 30_000 })
  await page.waitForSelector('[data-testid="daemon-disconnected"]', { state: 'detached' })
  const times = []
  for (let n = 0; n < 20; n++) {
    await page.evaluate(() => window.dmsg.call('session/lock'))
    await page.waitForFunction(() => !document.querySelector('[data-testid="account-switcher"]')?.textContent.includes('Cached Person'))
    const elapsed = await page.evaluate(async () => {
      await window.dmsg.call('session/unlock', { fast: true })
      const start = performance.now()
      while (performance.now() - start < 3000) {
        const switcher = document.querySelector('[data-testid="account-switcher"]')
        const image = switcher?.querySelector('img')
        if (switcher?.textContent.includes('Cached Person') && image?.complete && image.naturalWidth > 0) return performance.now() - start
        await new Promise(requestAnimationFrame)
      }
      throw new Error('cached profile did not render')
    })
    times.push(elapsed)
  }
  // A broken local connection masks the view, but a same-session reconnect
  // keeps the edited form. A new unlock generation resets it.
  await page.evaluate(() => { location.hash = '#/settings/security' })
  await page.waitForSelector('[data-testid="security"]')
  await page.getByTestId('security-policy-edit').click()
  const field = page.locator('[data-testid="daily-executions"]')
  await field.fill('77')
  for (const socket of wss.clients) socket.terminate()
  await page.waitForSelector('[data-testid="daemon-disconnected"]')
  assert.equal(await field.inputValue(), '77')
  await page.waitForSelector('[data-testid="daemon-disconnected"]', { state: 'detached', timeout: 15_000 })
  assert.equal(await field.inputValue(), '77')
  holdStatus = true
  await page.evaluate(() => window.dmsg.call('session/lock'))
  await page.waitForFunction(() => !document.querySelector('[data-testid="daily-executions"]'), { timeout: 3000 })
  assert.equal(await page.locator('body').getAttribute('inert'), '')
  holdStatus = false
  for (const send of heldStatus) send()
  heldStatus = []
  await page.evaluate(() => window.dmsg.call('session/unlock', { fast: true }))
  await page.waitForSelector('[data-testid="security"]')
  await page.getByTestId('security-policy-edit').click()
  assert.equal(await field.inputValue(), '5')
  assert.equal(await page.evaluate(() => Object.keys(localStorage).some((key) => /cache|profile|names/.test(key))), false)
  const sorted = [...times].sort((a,b) => a-b)
  const p95 = sorted[Math.ceil(sorted.length * .95) - 1]
  const warm = times.slice(1).sort((a,b) => a-b)
  console.log(JSON.stringify({ fixture: 'real Electron + Rust daemon, synthetic memory keystore, unreachable upstream', samples: times.length, first_ms: times[0], warm_p95_ms: warm[Math.ceil(warm.length * .95) - 1], p50_ms: sorted[Math.floor(sorted.length / 2)], p95_ms: p95, max_ms: sorted.at(-1), samples_ms: times, profile_reads: reads, same_session_form_preserved: true }))
  assert.ok(p95 <= 100, `cached profile P95 ${p95.toFixed(1)}ms > 100ms`)
} finally {
  if (app) await app.close()
  for (const socket of wss.clients) socket.terminate()
  await new Promise((resolve) => wss.close(resolve))
  await new Promise((resolve) => server.close(resolve))
  await rm(profile, { recursive: true, force: true })
}
