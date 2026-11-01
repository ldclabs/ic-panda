import assert from 'node:assert/strict'
import { mkdir, readFile, rm } from 'node:fs/promises'
import { join } from 'node:path'

/** Real channel persistence first; then isolated IPC replies exercise update UI without installing a release. */
export async function updatesFlow({ app, main, profile, shots, waitFor }) {
  const go = part => main.evaluate(part => location.hash = `/settings/${part}`, part)
  await go('about')
  const checkbox = main.getByTestId('beta-updates')
  await checkbox.waitFor()
  await waitFor(() => checkbox.isEnabled())
  assert.equal(await checkbox.getAttribute('aria-checked'), 'false')
  assert.equal(await checkbox.getAttribute('aria-describedby'), 'beta-updates-hint')
  assert.equal(await main.getByTestId('check-updates').count(), 0)
  await checkbox.focus()
  await main.keyboard.press('Space')
  await waitFor(async () => (await readFile(join(profile, 'update-channel'), 'utf8')) === 'beta')
  await waitFor(async () => (await main.getByTestId('update-channel').innerText()).trim() === 'Beta')
  await go('appearance')
  await go('about')
  await waitFor(async () => (await checkbox.getAttribute('aria-checked')) === 'true')

  // A failed atomic save leaves the requested choice visible and retryable, without changing the effective channel.
  await mkdir(join(profile, 'update-channel.tmp'))
  await checkbox.click()
  await main.getByTestId('update-channel-error').waitFor()
  assert.equal(await checkbox.getAttribute('aria-checked'), 'false')
  assert.equal((await main.getByTestId('update-channel').innerText()).trim(), 'Beta')
  await rm(join(profile, 'update-channel.tmp'), { recursive: true })
  await main.getByTestId('retry-update-channel').click()
  await waitFor(async () => (await readFile(join(profile, 'update-channel'), 'utf8')) === 'stable')
  await main.getByTestId('update-channel-error').waitFor({ state: 'detached' })
  await assert.rejects(main.evaluate(() => window.dmsg.setUpdateChannel('alpha')), /Invalid update channel/)

  await app.evaluate(({ ipcMain, BrowserWindow }) => {
    globalThis.__updateInfo = { version: '1.3.0', channel: 'stable', homebrew: false, applying: false, state: { kind: 'idle' } }
    globalThis.__updateEmit = () => BrowserWindow.getAllWindows().forEach(window => window.webContents.send('dmsg:event', { type: 'updates/changed' }))
    ipcMain.removeHandler('dmsg:updates')
    ipcMain.handle('dmsg:updates', () => JSON.parse(JSON.stringify(globalThis.__updateInfo)))
    ipcMain.removeHandler('dmsg:set-update-channel')
    ipcMain.handle('dmsg:set-update-channel', (_event, channel) => new Promise(resolve => {
      globalThis.__finishUpdateChannel = () => {
        globalThis.__updateInfo = { ...globalThis.__updateInfo, channel, state: { kind: 'latest' } }
        globalThis.__updateEmit()
        resolve(JSON.parse(JSON.stringify(globalThis.__updateInfo)))
      }
    }))
  })
  const show = info => app.evaluate((_electron, info) => { Object.assign(globalThis.__updateInfo, info); globalThis.__updateEmit() }, info)
  await show({ state: { kind: 'checking' } })
  assert.equal((await main.evaluate(() => window.dmsg.updates())).state.kind, 'checking')
  await waitFor(() => main.getByTestId('check-updates').isDisabled())
  assert.equal(await checkbox.isEnabled(), true)
  await checkbox.click()
  await waitFor(() => checkbox.isDisabled())
  await go('appearance')
  await app.evaluate(() => globalThis.__finishUpdateChannel())
  await go('about')
  await waitFor(async () => (await checkbox.getAttribute('aria-checked')) === 'true')
  assert.equal(await main.getByTestId('update-channel-error').count(), 0)

  await show({ state: { kind: 'downloading', version: '1.4.0-beta.10' } })
  await waitFor(async () => (await main.getByTestId('update-state').innerText()).includes('Downloading'))
  assert.equal(await checkbox.isEnabled(), true)
  await show({ state: { kind: 'ready', version: '1.4.0-beta.10' } })
  await main.getByTestId('apply-update').waitFor()
  await checkbox.click()
  await app.evaluate(() => globalThis.__finishUpdateChannel())
  await main.getByTestId('apply-update').waitFor({ state: 'detached' })
  await show({ version: '1.4.0-beta.10', channel: 'stable', state: { kind: 'latest' } })
  await waitFor(async () => (await main.getByTestId('update-state').innerText()).includes('Waiting for a stable release'))
  await show({ applying: true })
  await waitFor(() => checkbox.isDisabled())
  await show({ applying: false, homebrew: true, state: { kind: 'available', version: '1.4.0' } })
  await main.getByTestId('update-homebrew').waitFor()
  assert.equal(await checkbox.isDisabled(), true)
  assert.equal(await main.getByTestId('apply-update').count(), 0)
  await show({ homebrew: false, state: { kind: 'error', message: 'Fixture: update service unavailable; retry after reconnecting.' } })
  await waitFor(async () => (await main.getByTestId('about').innerText()).includes('Fixture:'))
  assert.equal(await main.getByTestId('check-updates').isEnabled(), true)

  const window = await app.browserWindow(main)
  for (const [locale, appearance, width, height] of [
    ['zh', 'light', 1180, 760], ['zh', 'light', 760, 540], ['zh', 'dark', 760, 540],
    ['fr', 'light', 760, 540], ['fr', 'dark', 760, 540], ['ar', 'light', 760, 540], ['ar', 'dark', 760, 540]
  ]) {
    await window.evaluate((window, [w, h]) => window.setContentSize(w, h), [width, height])
    await main.evaluate(async ({ locale, appearance }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, { locale, appearance })
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    await show({ version: '1.4.0-beta.123456789', channel: 'stable', state: { kind: 'latest' } })
    await waitFor(async () => (await main.getByTestId('app-version').innerText()).includes('123456789'))
    await main.getByTestId('check-updates').focus()
    await main.keyboard.press('Tab')
    assert.ok(await checkbox.evaluate(element => element === document.activeElement))
    assert.equal(await main.locator('html').getAttribute('dir'), locale === 'ar' ? 'rtl' : 'ltr')
    assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${locale}/${appearance} fits`)
    assert.ok(await main.getByTestId('about').evaluate(element => element.scrollWidth <= element.clientWidth + 1))
    await main.screenshot({ path: join(shots, `updates-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
  }
  console.log('Updates: default stable, keyboard opt-in, persistence, failed-save retry, pending/late replies, update states, Homebrew, and seven theme/language/size screenshots passed')
}
