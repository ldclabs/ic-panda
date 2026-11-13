import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'

/** Real Electron registrations, isolated profile; invoke captured native callbacks. */
export async function shortcutsFlow({ app, main, methods, calls, emit, waitFor, formMatrix, shots, profile, setSession }) {
  const native = await app.browserWindow(main)
  await app.evaluate(({ app, globalShortcut }) => {
    app.focus({ steal: true })
    globalThis.__shortcutCallbacks = new Map()
    globalThis.__shortcutBlocked = new Set()
    const register = globalShortcut.register.bind(globalShortcut)
    globalShortcut.register = (key, callback) => {
      if (globalThis.__shortcutBlocked.has(key)) return false
      const accepted = register(key, callback)
      if (accepted) globalThis.__shortcutCallbacks.set(key, callback)
      return accepted
    }
  })
  await native.evaluate(window => { window.show(); window.focus(); window.webContents.focus() })
  await waitFor(() => main.evaluate(() => document.hasFocus()))
  const status = () => main.evaluate(() => window.dmsg.shortcuts())
  const suspended = () => app.evaluate(({ globalShortcut }) => globalShortcut.isSuspended())
  const defaults = (await status()).bindings
  await main.evaluate(() => location.hash = '/settings/shortcuts')
  await main.getByTestId('shortcut-open').waitFor()

  const record = async (field, keys) => {
    await main.getByTestId(`shortcut-${field}`).focus()
    await waitFor(suspended)
    await waitFor(async () => (await main.getByTestId(`shortcut-${field}`).inputValue()) === 'Press a shortcut…')
    await main.keyboard.press(keys)
    await waitFor(async () => !await suspended())
  }
  await main.getByTestId('shortcut-open').focus()
  await waitFor(suspended)
  await waitFor(async () => (await main.getByTestId('shortcut-open').inputValue()) === 'Press a shortcut…')
  await main.keyboard.press('a')
  await main.getByText('Include ⌘/Ctrl or Alt and another key.', { exact: true }).waitFor()
  await main.keyboard.press('Escape')
  await waitFor(async () => !await suspended())
  assert.deepEqual((await status()).bindings, defaults)

  await record('open', 'Alt+Shift+F11')
  await record('lock', 'Alt+Shift+F11')
  assert.equal(await main.getByTestId('shortcuts-save').isEnabled(), false)
  await main.getByText('Choose a different shortcut for each action.', { exact: true }).first().waitFor()
  await record('lock', 'Alt+Shift+F12')
  await app.evaluate(() => globalThis.__shortcutBlocked.add('Alt+Shift+F12'))
  await main.getByTestId('shortcuts-save').click()
  await main.getByRole('alert').filter({ hasText: 'This shortcut could not be registered.' }).waitFor()
  assert.deepEqual((await status()).bindings, defaults)
  assert.equal(await app.evaluate(({ globalShortcut }) => globalShortcut.isRegistered('Alt+Shift+F11')), false)
  assert.match(await main.getByTestId('shortcut-open').inputValue(), /F11/)
  await app.evaluate(() => globalThis.__shortcutBlocked.clear())

  // Hold the real profile write: saving disables both recorders and keeps its draft.
  await app.evaluate(() => {
    const fs = process.getBuiltinModule('fs/promises')
    const write = fs.writeFile
    fs.writeFile = async (...args) => {
      if (String(args[0]).endsWith('shortcuts.json.tmp')) await new Promise((_resolve, reject) => { globalThis.__finishShortcutWrite = () => reject(new Error('Fixture disk write failure')) })
      return write(...args)
    }
    globalThis.__restoreShortcutWrite = () => { fs.writeFile = write }
  })
  await main.getByTestId('shortcuts-save').focus()
  await main.keyboard.press('Enter')
  await waitFor(() => app.evaluate(() => !!globalThis.__finishShortcutWrite))
  assert.equal(await main.getByTestId('shortcut-open').isDisabled(), true)
  assert.equal(await main.getByTestId('shortcut-lock').isDisabled(), true)
  await app.evaluate(() => { globalThis.__restoreShortcutWrite(); globalThis.__finishShortcutWrite() })
  await main.getByText('Local data could not be read or saved. Check disk space and access permissions.', { exact: true }).waitFor()
  assert.deepEqual((await status()).bindings, defaults)
  assert.match(await main.getByTestId('shortcut-open').inputValue(), /F11/)
  await main.getByTestId('shortcuts-save').click()
  await waitFor(async () => (await status()).bindings.open === 'Alt+Shift+F11')
  assert.deepEqual(JSON.parse(await readFile(join(profile, 'shortcuts.json'), 'utf8')), { open: 'Alt+Shift+F11', lock: 'Alt+Shift+F12' })

  await record('lock', 'Backspace')
  await main.getByTestId('shortcuts-save').click()
  await waitFor(async () => (await status()).bindings.lock === null)
  assert.equal(await app.evaluate(({ globalShortcut }) => globalShortcut.isRegistered('Alt+Shift+F12')), false)
  await main.getByTestId('shortcuts-defaults').click()
  await main.getByTestId('shortcuts-save').click()
  await waitFor(async () => (await status()).bindings.open === defaults.open)
  assert.deepEqual((await status()).registered, { open: true, lock: true })

  // Navigation and focus loss must never strand the system shortcuts in capture mode.
  await main.getByTestId('shortcut-open').focus()
  await waitFor(suspended)
  await main.evaluate(() => location.hash = '/settings/appearance')
  await waitFor(async () => !await suspended())
  await main.evaluate(() => location.hash = '/settings/shortcuts')
  await main.getByTestId('shortcut-open').focus()
  await waitFor(suspended)
  await native.evaluate(window => window.hide())
  await waitFor(async () => !await suspended())
  const unlocks = calls.filter(call => call.method === 'session/unlock').length
  await app.evaluate((_electron, key) => globalThis.__shortcutCallbacks.get(key)(), defaults.open)
  await waitFor(() => native.evaluate(window => window.isVisible() && window.isFocused()))
  assert.equal(await main.evaluate(() => location.hash), '#/settings/shortcuts')
  assert.equal(calls.filter(call => call.method === 'session/unlock').length, unlocks)

  const locks = calls.filter(call => call.method === 'session/lock').length
  methods['session/lock'] = () => { setSession('locked'); emit('session/changed'); return null }
  await native.evaluate(window => window.hide())
  await app.evaluate((_electron, key) => globalThis.__shortcutCallbacks.get(key)(), defaults.lock)
  await waitFor(() => calls.filter(call => call.method === 'session/lock').length === locks + 1)
  assert.equal(await native.evaluate(window => window.isVisible()), false)
  await app.evaluate((_electron, key) => globalThis.__shortcutCallbacks.get(key)(), defaults.open)
  await waitFor(() => main.evaluate(() => document.hasFocus()))
  await formMatrix(app, main, shots, 'shortcuts', '[data-testid="shortcut-settings"]', [
    ['zh', 'light', 1180, 760], ['zh', 'dark', 760, 540], ['de', 'light', 760, 540], ['ar', 'dark', 760, 540]
  ])
  // A rejected lock is visible and never reported as success.
  methods['session/lock'] = () => { throw { code: 'UNAVAILABLE', message: 'Temporary lock failure' } }
  await app.evaluate((_electron, key) => globalThis.__shortcutCallbacks.get(key)(), defaults.lock)
  await main.getByText('Could not lock dMsg. Check the background service and try again.', { exact: true }).waitFor()
  await main.keyboard.press(process.platform === 'darwin' ? 'Meta+/' : 'Control+/')
  await main.getByTestId('shortcuts').getByText('Global shortcuts', { exact: true }).waitFor()
  await main.keyboard.press('Escape')
}
