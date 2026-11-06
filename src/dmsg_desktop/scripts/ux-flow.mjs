import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'

/**
 * What the interaction review added, against the smoke runner's fixture: every
 * view loading on its own, coming back to where a view was left, the shortcuts,
 * what locks the device, list widths, localized menus, the first-run page and
 * the approval window closing itself.
 */
export async function uxFlow({ app, main, shots, methods, requests, request, emit, waitFor, calls, profile, formMatrix, setSession }) {
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
  const count = (method) => calls.filter((call) => call.method === method).length
  const go = (route) => main.evaluate((route) => (location.hash = route), route)
  const hash = (route) => main.waitForFunction((route) => location.hash === `#${route}`, route)
  const settled = () => new Promise((resolve) => setTimeout(resolve, 300))

  // Every view loads from its own file, with nothing the page's policy refuses.
  await main.evaluate(() => {
    window.__refused = []
    window.addEventListener('securitypolicyviolation', (event) => window.__refused.push(`${event.violatedDirective} ${event.blockedURI}`))
  })
  for (const [route, testid] of [
    ['/files', 'files-list'],
    ['/vault', 'vault-list'],
    ['/inbox', 'letter-search'],
    ['/agents', 'agents-tab-capabilities'],
    ['/wallet', 'wallet-view'],
    ['/reminders', 'reminders-page'],
    ['/settings/session', 'session-details']
  ]) {
    await go(route)
    await main.getByTestId(testid).first().waitFor()
  }
  assert.deepEqual(await main.evaluate(() => window.__refused), [])

  // A tab is part of the route, and a view's shortcut comes back to it.
  await go('/vault/policies')
  await main.getByTestId('workflow-tab-policies').waitFor()
  await main.keyboard.press(`${mod}+4`)
  await hash('/inbox')
  await main.getByTestId('letter-search').waitFor()
  await main.keyboard.press(`${mod}+3`)
  await hash('/vault/policies')
  await go('/vault')
  await main.getByTestId('vault-list').waitFor()

  // The shortcuts answer on request, and Esc goes back to where it was.
  await main.keyboard.press(`${mod}+/`)
  await main.getByTestId('shortcuts').waitFor()
  await formMatrix(app, main, shots, 'shortcuts', '[data-testid="shortcuts"]')
  await main.keyboard.press('Escape')
  await main.getByTestId('shortcuts').waitFor({ state: 'detached' })

  // The screen locking or the Mac sleeping closes the account unless turned off.
  methods['session/lock'] = () => null
  const signal = (event) => app.evaluate(({ powerMonitor }, event) => powerMonitor.emit(event), event)
  await go('/settings/security')
  await main.getByTestId('lock-settings').waitFor()
  await main.waitForSelector('[data-testid="lock-on-screen-lock"][aria-checked="true"]')
  await formMatrix(app, main, shots, 'lock-settings', '[data-testid="lock-settings"]')
  const locks = count('session/lock')
  await signal('lock-screen')
  await waitFor(() => count('session/lock') === locks + 1)
  await signal('unlock-screen')
  await signal('suspend')
  await waitFor(() => count('session/lock') === locks + 2)
  await signal('resume')
  await main.getByTestId('lock-on-screen-lock').click()
  await main.waitForSelector('[data-testid="lock-on-screen-lock"][aria-checked="false"]')
  await signal('lock-screen')
  await signal('suspend')
  await settled()
  assert.equal(count('session/lock'), locks + 2)
  await signal('unlock-screen')
  await signal('resume')
  await main.getByTestId('lock-on-screen-lock').click()
  await main.waitForSelector('[data-testid="lock-on-screen-lock"][aria-checked="true"]')

  // The idle time goes to the daemon through the home's preferences.
  const reloads = count('files/preferences_reload')
  await main.click('[data-testid="idle-lock"]')
  await main.click('[data-testid="idle-lock-menu"] [data-value="30"]')
  await waitFor(() => count('files/preferences_reload') === reloads + 1)
  assert.equal(JSON.parse(await readFile(join(profile, 'home', 'preferences.json'), 'utf8')).idle_lock_minutes, 30)

  // A list's width follows its handle, by hand or by key, and a double click resets it.
  for (const [route, name] of [['/files', 'files'], ['/vault', 'vault'], ['/channels', 'channels']]) {
    await go(route)
    const handle = main.getByTestId(`pane-handle-${name}`)
    await handle.waitFor()
    const width = () => handle.evaluate((el) => el.previousElementSibling.getBoundingClientRect().width)
    const start = await width()
    await handle.focus()
    await main.keyboard.press('ArrowRight')
    await waitFor(async () => (await width()) > start)
    await main.keyboard.press('End')
    await waitFor(async () => (await width()) === 520)
    await main.keyboard.press('Home')
    await waitFor(async () => (await width()) === 200)
    await handle.dblclick()
    await waitFor(async () => (await width()) === start)
  }

  // A pasted screenshot has no path: the main process reads it from the clipboard.
  await app.evaluate(({ clipboard }) => {
    clipboard.read = async () => [{ types: ['image/png'], getType: async () => new Blob([Buffer.from('89504e470d0a1a0a', 'hex')], { type: 'image/png' }) }]
  })
  await main.click('[data-testid="channel-list"] button >> nth=0')
  await main.getByTestId('channel-text').waitFor()
  methods['files/mkdir'] = () => ({ id: 'root' })
  const imports = count('files/import')
  await main.getByTestId('channel-text').evaluate((el) => {
    const data = new DataTransfer()
    data.items.add(new File([new Uint8Array([137, 80, 78, 71])], 'image.png', { type: 'image/png' }))
    el.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }))
  })
  await waitFor(() => count('files/import') === imports + 1)
  const pasted = calls.filter((call) => call.method === 'files/import').at(-1).params
  assert.match(pasted.paths[0], /Pasted-[\d-]+\.png$/)
  assert.equal(pasted.keep_both, true)
  await main.getByTestId('channel-attached').first().waitFor()

  // The menus follow the language.
  const menu = () => app.evaluate(({ Menu }) => Menu.getApplicationMenu().items.map((item) => item.label))
  const english = await menu()
  await main.evaluate(() => window.dmsg.setLocale('zh'))
  await main.waitForSelector('html[lang|="zh"]')
  await waitFor(async () => JSON.stringify(await menu()) !== JSON.stringify(english))
  await main.evaluate(() => window.dmsg.setLocale('en'))
  await main.waitForSelector('html[lang="en"]')
  await waitFor(async () => JSON.stringify(await menu()) === JSON.stringify(english))

  // A device with no account opens on the first-run page, without the shell.
  const accounts = methods['session/accounts']
  methods['session/accounts'] = () => []
  setSession('logged_out')
  emit('session/changed', { state: 'logged_out' })
  await main.getByTestId('welcome-login').waitFor()
  assert.equal(await main.getByTestId('sidebar').count(), 0)
  await formMatrix(app, main, shots, 'welcome', '[data-testid="welcome"]')
  // The first login goes on to what makes the device quicker, then to the app.
  methods['session/accounts'] = accounts
  setSession('unlocked')
  emit('session/changed', { state: 'unlocked' })
  await main.getByTestId('welcome-finish').waitFor()
  await main.getByTestId('welcome-done').click()
  await main.getByTestId('sidebar').waitFor()

  // The approval window closes itself after its last decision.
  for (const other of requests.values()) other.state = 'done'
  const open = async () => {
    request.state = 'awaiting_user'
    const opened = app.waitForEvent('window')
    await main.evaluate((id) => window.dmsg.openApproval(id), request.id)
    const approval = await opened
    await approval.getByTestId('approval-origin').waitFor()
    await approval.click('[data-testid="approve"]')
    await approval.waitForSelector('[data-testid="approval-outcome"]')
    return approval
  }
  const closing = await open()
  await closing.waitForEvent('close', { timeout: 5_000 })
  // A window the person goes on using stays open.
  const staying = await open()
  await staying.keyboard.press('Shift')
  await new Promise((resolve) => setTimeout(resolve, 2_500))
  assert.equal(staying.isClosed(), false)
}
