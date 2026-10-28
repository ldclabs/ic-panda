import assert from 'node:assert/strict'
import { join } from 'node:path'

/** All Settings routes, with synthetic data and controlled replies in the smoke runner. */
export async function settingsFlow({ app, main, shots, methods, requests, emit, hold, waitFor, calls, setSession }) {
  const count = method => calls.filter(call => call.method === method).length
  const last = method => calls.filter(call => call.method === method).at(-1)
  const go = part => main.evaluate(part => location.hash = `/settings/${part}`, part)
  const close = async id => {
    const tooltip = await main.locator('[data-tooltip-content]').count()
    await main.keyboard.press('Escape')
    if (tooltip) await main.keyboard.press('Escape')
    await main.getByTestId(id).waitFor({ state: 'detached' })
  }
  let published = methods['profile/show'](), queued, failPublish = false
  methods['profile/show'] = () => structuredClone(published)
  methods['profile/pending'] = () => !!queued
  methods['agent/principal'] = () => ({ published: !queued })
  methods['profile/publish'] = document => {
    if (failPublish) throw { code: 'INVALID_INPUT', message: 'Fixture publication failure' }
    queued = structuredClone(document)
    return { published: false }
  }
  methods['profile/retry'] = () => { published = queued; queued = null; return { published: true } }
  methods['profile/resolve'] = ({ account }) => ({ ...published, id: `https://dmsg.test/${account}`, name: account })

  // The published profile is readable without a form; reopening starts from its applied values.
  await go('profile')
  await main.getByTestId('profile-edit').click()
  assert.equal(await main.getByTestId('profile-publish').isDisabled(), true)
  const originalName = await main.getByTestId('profile-name').inputValue()
  await main.getByTestId('profile-name').fill(` ${originalName} `)
  assert.equal(await main.getByTestId('profile-publish').isDisabled(), true)
  await main.getByTestId('profile-name').fill('x'.repeat(81))
  assert.equal(await main.getByTestId('profile-publish').isDisabled(), true)
  assert.equal(await main.getByTestId('profile-name').getAttribute('aria-invalid'), 'true')
  await main.getByTestId('profile-name').fill('Edited profile')
  await main.getByRole('button', { name: 'Add link', exact: true }).click()
  const link = main.getByTestId('profile-link-row').first()
  assert.equal(await main.getByTestId('profile-publish').isDisabled(), true)
  await link.getByLabel('Link name', { exact: true }).fill('Project')
  await link.getByLabel('Link', { exact: true }).fill('http://example.test')
  assert.equal(await main.getByTestId('profile-publish').isDisabled(), true)
  await link.getByLabel('Link', { exact: true }).fill('https://example.test')
  assert.equal(await main.getByTestId('profile-publish').isEnabled(), true)
  failPublish = true
  await main.getByTestId('profile-publish').click()
  await main.getByTestId('profile-edit-error').waitFor()
  assert.equal(await main.getByTestId('profile-name').inputValue(), 'Edited profile')
  failPublish = false
  const finishPublish = hold('profile/publish')
  await main.getByTestId('profile-publish').click()
  await waitFor(() => !!queued)
  assert.equal(await main.getByTestId('profile-publish').isDisabled(), true)
  assert.equal(await main.getByTestId('profile').getByTestId('dialog-close').isDisabled(), true)
  await finishPublish()
  await waitFor(async () => (await main.getByTestId('profile-publish').innerText()).includes('Retry'))
  const writes = count('profile/publish')
  await main.getByTestId('profile-publish').click()
  await waitFor(() => !queued)
  await waitFor(() => main.getByTestId('profile-publish').isDisabled())
  assert.equal(count('profile/publish'), writes)
  assert.equal(published.name, 'Edited profile')
  await close('profile')
  assert.ok((await main.getByTestId('profile-summary').innerText()).includes('Edited profile'))
  assert.equal(await main.locator('input:visible, textarea:visible').count(), 0)

  // Changing a lookup invalidates both its shown result and its in-flight reply.
  await main.getByTestId('profile-lookup-open').click()
  await main.getByTestId('profile-lookup-account').fill('first-account')
  const finishLookup = hold('profile/resolve')
  await main.getByRole('button', { name: 'Look up', exact: true }).click()
  await waitFor(() => count('profile/resolve') > 0)
  await main.getByTestId('profile-lookup-account').fill('second-account')
  await finishLookup()
  assert.equal(await main.getByTestId('profile-found').count(), 0)
  await main.getByRole('button', { name: 'Look up', exact: true }).click()
  await main.getByTestId('profile-found').waitFor()
  assert.ok((await main.getByTestId('profile-found').innerText()).includes('second-account'))
  await main.getByTestId('profile-lookup-account').fill('third-account')
  assert.equal(await main.getByTestId('profile-found').count(), 0)
  await close('profile-lookup')

  // Capability edits compare sets, keep drafts after failure, and send only once while waiting.
  await go('devices')
  await main.getByTestId('device-capabilities').nth(1).click()
  assert.equal(await main.getByTestId('capabilities-submit').isDisabled(), true)
  await main.getByTestId('device-editor').getByRole('checkbox', { name: 'ContentSign', exact: true }).click()
  await main.getByTestId('device-editor').getByRole('checkbox', { name: 'ContentSign', exact: true }).click()
  assert.equal(await main.getByTestId('capabilities-submit').isDisabled(), true)
  await main.getByTestId('device-editor').getByRole('checkbox', { name: 'FormalApprove', exact: true }).click()
  const finishCapabilities = hold('device/capabilities')
  await main.getByTestId('capabilities-submit').click()
  await waitFor(() => count('device/capabilities') > 0)
  assert.equal(await main.getByTestId('capabilities-submit').isDisabled(), true)
  await finishCapabilities()
  await main.getByTestId('device-editor').waitFor({ state: 'detached' })
  assert.deepEqual(last('device/capabilities').params.capabilities, ['VaultUnlock', 'ContentSign', 'FormalApprove'])
  await main.getByRole('button', { name: 'View approval', exact: true }).click()
  assert.equal(count('device/capabilities'), 1)
  assert.equal(count('device/revoke'), 0)
  for (const request of requests.values()) if (request.type === 'device_capabilities') request.state = 'rejected'
  emit('approval/changed')
  await waitFor(() => main.getByTestId('device-capabilities').nth(1).isEnabled())

  // File preferences are one explicit save. Cancelling the OS mount prompt keeps the draft.
  await go('files')
  await main.getByTestId('files-settings-edit').click()
  assert.equal(await main.getByTestId('files-settings-save').isDisabled(), true)
  await main.getByTestId('files-settings-mount').click()
  await app.evaluate(() => globalThis.__box.push(1))
  await main.getByTestId('files-settings-save').click()
  await waitFor(() => main.getByTestId('files-settings-save').isEnabled())
  assert.equal(await main.getByTestId('files-settings-editor').isVisible(), true)
  assert.equal(count('files/preferences_reload'), 0)
  await app.evaluate(() => globalThis.__box.push(0))
  await main.getByTestId('files-settings-save').click()
  await main.getByTestId('files-settings-editor').waitFor({ state: 'detached' })
  assert.equal(count('files/preferences_reload'), 1)
  await main.getByTestId('files-settings-edit').click()
  assert.equal(await main.getByTestId('files-settings-mount').getAttribute('aria-checked'), 'true')
  assert.equal(await main.getByTestId('files-settings-save').isDisabled(), true)
  await close('files-settings-editor')
  assert.equal(await main.getByTestId('cache-clear').isDisabled(), true, 'empty cache cannot be cleared')
  methods['cache/status'] = () => ({ domains: [{ sealed_bytes: 1024 }] })
  methods['cache/clear'] = () => { methods['cache/status'] = () => ({ domains: [] }); return null }
  await go('appearance'); await main.getByTestId('language-appearance').waitFor(); await go('files')
  await waitFor(() => main.getByTestId('cache-clear').isEnabled())
  await main.getByTestId('cache-clear').click()
  await waitFor(() => main.getByTestId('cache-clear').isDisabled())
  assert.equal(count('cache/clear'), 1)

  // Browser pairing has a cancel path; a late candidate cannot reappear after navigating away.
  await go('browsers')
  await main.getByTestId('browser-copy').click()
  await main.getByTestId('browser-cancel').waitFor()
  await main.getByTestId('browser-cancel').click()
  assert.equal(await main.getByTestId('browser-grant').count(), 0)
  const releaseCandidates = hold('connect/candidates')
  await main.getByTestId('browser-copy').click()
  await waitFor(() => count('connect/candidates') > 0)
  await go('appearance')
  await releaseCandidates()
  await go('browsers')
  await main.getByTestId('browser-copy').waitFor()
  assert.equal(await main.getByTestId('browser-grant').count(), 0)

  // Navigation hides the error of an action started on another Settings page.
  const oldFastUnlock = methods['session/fast_unlock']
  methods['session/fast_unlock'] = () => { throw { code: 'INVALID_INPUT', message: 'Fixture fast unlock failure' } }
  await go('session')
  const finishFast = hold('session/fast_unlock')
  const fastCount = count('session/fast_unlock')
  await main.getByTestId('fast-unlock-toggle').click()
  await waitFor(() => count('session/fast_unlock') > fastCount)
  assert.equal(await main.getByTestId('fast-unlock-toggle').isDisabled(), true)
  await go('appearance')
  await finishFast()
  assert.equal(await main.getByRole('alert').count(), 0)
  methods['session/fast_unlock'] = oldFastUnlock

  // Confirmation belongs to the entered identity; cancelling a system file picker is not success.
  await go('legacy')
  await main.getByRole('radio').first().click()
  await main.getByTestId('legacy-principal').fill('aaaaa-aa')
  const confirmation = main.getByRole('checkbox', { name: 'Fingerprint, target device and old identity match on both sides' })
  await confirmation.click()
  await main.getByTestId('legacy-principal').fill('rdmx6-jaaaa-aaaaa-aaadq-cai')
  assert.equal(await confirmation.getAttribute('aria-checked'), 'false')
  await confirmation.click()
  await app.evaluate(({ dialog }) => {
    globalThis.__previousOpenDialog = dialog.showOpenDialog
    dialog.showOpenDialog = async () => ({ canceled: true, filePaths: [] })
  })
  await main.getByRole('button', { name: 'Choose the encrypted migration file and import', exact: true }).click()
  assert.equal(await main.getByText('Archive checked offline and kept encrypted.', { exact: true }).count(), 0)
  await app.evaluate(({ dialog }) => { dialog.showOpenDialog = globalThis.__previousOpenDialog })

  // Overview and editor screenshots at the normal and smallest supported sizes.
  const window = await app.browserWindow(main)
  for (const [locale, appearance, width, height] of [['zh', 'light', 1180, 760], ['zh', 'dark', 760, 540], ['fr', 'light', 760, 540], ['ar', 'dark', 760, 540], ['de', 'light', 760, 540], ['zh-Hant', 'dark', 760, 540], ['hi', 'light', 760, 540], ['vi', 'light', 760, 540], ['th', 'dark', 760, 540], ['fil', 'light', 760, 540], ['ms', 'dark', 760, 540], ['bn', 'light', 760, 540], ['ur', 'dark', 760, 540], ['pl', 'light', 760, 540], ['nl', 'dark', 760, 540]]) {
    await window.evaluate((window, [w, h]) => window.setContentSize(w, h), [width, height])
    await main.evaluate(async ({ locale, appearance }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, { locale, appearance })
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    for (const [part, selector, opener, editor] of [
      ['session','session-state'], ['security','security'], ['devices','devices','device-capabilities','device-editor'],
      ['profile','profile-summary','profile-edit','profile'], ['browsers','browser-pair'],
      ['files','files-settings','files-settings-edit','files-settings-editor'], ['appearance','language-appearance'], ['legacy','legacy'], ['about','about']
    ]) {
      await go(part)
      await main.getByTestId(selector).waitFor()
      assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${part} fits ${locale}`)
      await main.screenshot({ path: join(shots, `settings-${part}-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
      if (opener) {
        await main.getByTestId(opener).first().click()
        const dialog = main.getByTestId(editor)
        await dialog.waitFor()
        await main.keyboard.press('Tab')
        assert.ok(await dialog.evaluate(element => element.contains(document.activeElement)))
        assert.ok(await dialog.evaluate(element => element.scrollWidth <= element.clientWidth + 1))
        await main.screenshot({ path: join(shots, `settings-${part}-edit-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
        await close(editor)
      }
    }
  }

  // Setup requests cannot reuse a token for another account, nor finish recovery before eligibility.
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  setSession('no_account'); emit('session/changed')
  await go('session')
  await main.getByTestId('join-account').getByRole('button').click()
  methods['account/bind_request'] = () => ({ token: 'fixture-bind-token' })
  await main.getByTestId('bind-account').fill('0123456789abcdefghig')
  await main.getByRole('button', { name: 'Create a binding request', exact: true }).click()
  await main.getByTestId('bind-token').waitFor()
  await main.getByTestId('bind-account').fill('0k0g2081040g2081040g')
  assert.equal(await main.getByTestId('bind-token').count(), 0)
  methods['recovery/status'] = () => ({ ready: false, member: false, pending: null })
  methods['recovery/request'] = () => ({ ready: false, member: false, pending: { this_device: true, new_auth: 'aaaaa-aa', device_id: '11'.repeat(32), execute_after: Date.now() + 86400000 } })
  setSession('pairing_required'); emit('session/changed')
  await main.getByTestId('recovery-request').waitFor()
  await waitFor(() => main.getByTestId('recovery-request').isEnabled())
  assert.equal(await main.getByTestId('recovery-complete').isDisabled(), true)
  await main.getByTestId('recovery-request').click()
  await main.getByTestId('recovery-pending').waitFor()
  assert.equal(await main.getByTestId('recovery-complete').isDisabled(), true)
  assert.equal(await main.getByTestId('recovery-request').isDisabled(), true)
  console.log('Settings: all 9 routes, profile drafts/retry, device permissions, OS mount cancellation, empty cache, browser cancellation, late errors, setup/recovery, keyboard and overview/editor screenshots passed')
}
