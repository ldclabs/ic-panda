import assert from 'node:assert/strict'
import { join } from 'node:path'

/** Focused Electron coverage, using the smoke runner's isolated profile and daemon fixture. */
export async function securityFlow({ app, main, shots, methods, requests, emit, hold, waitFor, calls, created }) {
  let security = methods['account/security']()
  let keys = methods['device/key']()
  let failPolicy = false, failKeys = false, failSecurity = false
  let sequence = 160
  const last = method => calls.filter(c => c.method === method).at(-1)
  const count = method => calls.filter(c => c.method === method).length
  const make = (type, detail) => created((++sequence).toString(16).padStart(2, '0').repeat(32), type, detail)
  methods['account/security'] = () => {
    if (failSecurity) throw { code: 'INVALID_INPUT', message: 'Fixture security read failure' }
    return structuredClone(security)
  }
  methods['device/key'] = () => {
    if (failKeys) throw { code: 'INVALID_INPUT', message: 'Fixture hardware read failure' }
    return structuredClone(keys)
  }
  methods['account/policy'] = p => {
    if (failPolicy) throw { code: 'INVALID_INPUT', message: 'Fixture policy failure' }
    return make('account', { change: 'policy', ...security.policy, ...p, previous: security.policy })
  }
  methods['account/recovery_policy'] = p => make('account', { change: 'recovery_policy', enabled: p.enabled, delay_ms: p.days * 86400000 })
  methods['account/hardware_policy'] = p => make('hardware_policy', { purposes: p.purposes, previous: keys.policy, missing_devices: [] })
  methods['device/key_approve'] = p => make('approval_key', { device_id: p.device, key_id: '5a'.repeat(32) })
  const refresh = async () => {
    const before = count('device/key')
    emit('approval/changed')
    await waitFor(() => count('device/key') > before)
  }
  const dismissRequest = async (group, state = 'rejected') => {
    for (const r of requests.values()) if (r.source === 'gui') r.state = state
    await refresh()
    await main.getByTestId(`security-pending-${group}`).waitFor({ state: 'detached' })
  }
  const closeEditor = async () => {
    await main.keyboard.press('Escape')
    await main.getByTestId('security-editor').waitFor({ state: 'detached' })
  }
  const submit = main.getByTestId('security-submit')
  await main.evaluate(() => location.hash = '/settings/security')
  await main.getByTestId('security-policy-edit').waitFor()
  await main.getByTestId('approval-keys').waitFor()
  assert.equal(await main.locator('input:visible, textarea:visible').count(), 0, 'overview has no open forms')
  assert.equal(await main.getByTestId('approval-keys-require').isDisabled(), true, 'register a local key before requiring it')

  // Keyboard entry, unchanged values, a toggle restored in another order and the server's bounds.
  await main.getByTestId('security-policy-edit').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('daily-executions').waitFor()
  assert.equal(await submit.isDisabled(), true)
  await main.screenshot({ path: join(shots, 'security-policy-unchanged.png'), animations: 'disabled' })
  await main.getByTestId('security-purpose-FileAttestation').click()
  assert.equal(await submit.isEnabled(), true)
  await main.getByTestId('security-purpose-FileAttestation').click()
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('daily-executions').fill('020')
  assert.equal(await submit.isDisabled(), true)
  for (const value of ['', '-1', '1.5', '65', '999999999999999999']) {
    await main.getByTestId('daily-executions').fill(value)
    assert.equal(await main.getByTestId('daily-executions').getAttribute('aria-invalid'), 'true')
    assert.equal(await submit.isDisabled(), true)
  }
  await main.getByTestId('daily-executions').fill('0')
  assert.equal(await submit.isEnabled(), true)
  await main.getByTestId('daily-executions').fill('7')
  await main.getByTestId('security-purpose-FileAttestation').click()
  failPolicy = true
  await submit.click()
  await main.getByTestId('security-editor-error').waitFor()
  await refresh()
  assert.equal(await main.getByTestId('daily-executions').inputValue(), '7')
  assert.equal(await main.getByTestId('security-purpose-FileAttestation').getAttribute('aria-checked'), 'false')
  assert.equal(await main.getByTestId('security-editor-error').isVisible(), true, 'refresh does not erase a submission error')
  await main.screenshot({ path: join(shots, 'security-policy-retry.png'), animations: 'disabled' })
  security.policy.daily_executions = 22
  await refresh()
  assert.equal(await main.getByTestId('daily-executions').inputValue(), '7', 'changed server value cannot erase a dirty draft')
  failPolicy = false
  const release = hold('account/policy')
  const before = count('account/policy')
  await submit.click()
  await waitFor(() => count('account/policy') > before)
  assert.equal(await submit.isDisabled(), true)
  assert.equal(await main.getByTestId('daily-executions').isDisabled(), true)
  assert.equal(await main.getByTestId('security-purpose-Statement').isDisabled(), true)
  assert.equal(await main.getByTestId('security-editor').getByTestId('dialog-close').isDisabled(), true)
  assert.deepEqual(last('account/policy').params, { daily_executions: 7, allowed_purposes: ['Statement', 'AppAction'], source: 'gui' })
  await release()
  await main.getByTestId('security-editor').waitFor({ state: 'detached' })
  await main.getByTestId('security-pending-policy').waitFor()
  assert.equal(await main.getByTestId('security-policy-edit').isDisabled(), true)
  await main.getByTestId('security-pending-policy').getByRole('button').click()
  assert.equal(count('account/policy'), before + 1, 'viewing a receipt never resubmits')
  assert.equal((await main.getByTestId('security-policy-daily').innerText()).trim(), '22', 'request creation is not applied state')
  await main.screenshot({ path: join(shots, 'security-policy-pending.png'), animations: 'disabled' })
  await dismissRequest('policy')
  await main.getByTestId('security-policy-edit').click()
  await main.getByTestId('daily-executions').fill('8')
  security.policy.allowed_purposes = ['FileAttestation', 'Statement']
  await refresh()
  await waitFor(async () => await main.getByTestId('security-purpose-AppAction').getAttribute('aria-checked') === 'false')
  assert.equal(await main.getByTestId('daily-executions').inputValue(), '8', 'untouched fields rebase independently from a dirty field')
  await closeEditor()

  // Recovery change, reverting and cancellation all have distinct outcomes.
  await main.getByTestId('security-recovery-edit').click()
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('recovery-days').locator('[data-value="4"]').click()
  assert.equal(await submit.isEnabled(), true)
  await main.getByTestId('recovery-days').locator('[data-value="3"]').click()
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('recovery-days').locator('[data-value="4"]').click()
  await submit.click()
  await main.getByTestId('security-editor').waitFor({ state: 'detached' })
  await main.getByTestId('security-pending-recovery').waitFor()
  assert.deepEqual(last('account/recovery_policy').params, { enabled: true, days: 4, source: 'gui' })
  security.recovery_delay_ms = 4 * 86400000
  await dismissRequest('recovery', 'done')
  await main.getByTestId('security-recovery-edit').click()
  assert.equal(await main.getByTestId('recovery-days').locator('[data-value="4"]').getAttribute('data-state'), 'on')
  assert.equal(await submit.isDisabled(), true, 'completed policy becomes the new unchanged baseline')
  await closeEditor()

  // Turning recovery off keeps a single review and hides irrelevant delay controls.
  await main.getByTestId('security-recovery-edit').click()
  await main.getByTestId('recovery-enabled').focus()
  await main.keyboard.press('Space')
  assert.equal(await main.getByTestId('recovery-enabled').getAttribute('aria-checked'), 'false')
  assert.equal(await main.getByTestId('recovery-days').count(), 0)
  await main.screenshot({ path: join(shots, 'security-recovery-off-review.png'), animations: 'disabled' })
  await submit.click()
  await main.getByTestId('security-editor').waitFor({ state: 'detached' })
  await main.getByTestId('security-pending-recovery').waitFor()
  assert.deepEqual(last('account/recovery_policy').params, { enabled: false, days: 4, source: 'gui' })
  security.recovery_enabled = false
  await dismissRequest('recovery', 'done')
  await main.getByTestId('security-recovery-edit').click()
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('recovery-enabled').click()
  assert.equal(await main.getByTestId('recovery-days').count(), 1)
  await closeEditor()

  // Hardware policy has its own draft and compares a set, not checkbox order.
  keys.key = { here: true, biometry_only: false, key_id: '5a'.repeat(32) }
  keys.policy = ['account_control', 'device_control', 'principal_control', 'payment']
  await refresh()
  await waitFor(() => main.getByTestId('approval-keys-require').isEnabled())
  await main.getByTestId('approval-keys-require').click()
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('security-hardware-payment').click()
  await main.getByTestId('security-hardware-payment').click()
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('security-hardware-attestation').click()
  await submit.click()
  await main.getByTestId('security-editor').waitFor({ state: 'detached' })
  await main.getByTestId('security-pending-hardware').waitFor()
  assert.deepEqual(last('account/hardware_policy').params, { purposes: [...keys.policy, 'attestation'], source: 'gui' })
  await dismissRequest('hardware')

  // An inputless key action is an explicit task; its biometric preference cannot leak into another task.
  await main.getByTestId('security-key-edit').click()
  await main.getByTestId('security-biometry').click()
  await closeEditor()
  await main.getByTestId('device-bound-toggle').click()
  assert.equal(await main.getByTestId('security-biometry').getAttribute('aria-checked'), 'false')
  await closeEditor()
  await main.getByTestId('security-key-transfer').getByRole('button').click()
  await main.getByTestId('security-key-approve').click()
  await main.getByTestId('approval-key-request').fill('fixture-key-request')
  await main.getByTestId('approve-shown').fill('not-a-device')
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('approve-shown').fill('34'.repeat(32))
  assert.equal(await submit.isEnabled(), true)
  await closeEditor()
  await main.getByTestId('security-approve-others').click()
  assert.equal(await submit.isDisabled(), true)
  await main.getByTestId('approve-request').fill('pairing-token')
  await main.getByTestId('approve-shown').fill('34'.repeat(32))
  await submit.click()
  await main.getByTestId('security-editor').waitFor({ state: 'detached' })
  assert.deepEqual(last('device/approve').params, { token: 'pairing-token', device: '34'.repeat(32), source: 'gui' })

  // A failed hardware read is visible and retryable, without hiding account settings.
  failKeys = true
  await refresh()
  await main.getByTestId('security-key-error').waitFor()
  assert.equal(await main.getByTestId('security-key-edit').isDisabled(), true)
  assert.equal(await main.getByTestId('security-policy-edit').isEnabled(), true)
  failKeys = false
  await main.getByTestId('security-key-error').getByRole('button').click()
  await main.getByTestId('security-key-error').waitFor({ state: 'detached' })

  // Screenshot matrix covers the overview and every editor at the minimum window size.
  const window = await app.browserWindow(main)
  const cases = [['zh', 'light', 1180, 760], ['zh', 'dark', 760, 540], ['fr', 'light', 760, 540], ['ar', 'dark', 760, 540]]
  for (const [locale, appearance, width, height] of cases) {
    await window.evaluate((window, [width, height]) => window.setContentSize(width, height), [width, height])
    await main.evaluate(async ({ locale, appearance }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, { locale, appearance })
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    await main.getByTestId('security').scrollIntoViewIfNeeded()
    await main.screenshot({ path: join(shots, `security-overview-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    for (const [name, button] of [['policy', 'security-policy-edit'], ['recovery', 'security-recovery-edit'], ['hardware', 'approval-keys-require'], ['others', 'security-approve-others'], ['key', 'security-key-edit'], ['bound', 'device-bound-toggle'], ['key-approve', 'security-key-approve'], ['key-request', 'security-key-request']]) {
      await main.getByTestId(button).click()
      const dialog = main.getByTestId('security-editor')
      await dialog.waitFor()
      if (name === 'policy') await main.getByTestId('daily-executions').fill('65')
      if (name === 'others' || name === 'key-approve') {
        await main.getByTestId(name === 'others' ? 'approve-request' : 'approval-key-request').fill('fixture-request-'.repeat(100))
        await main.getByTestId('approve-shown').fill('34'.repeat(32))
      }
      if (name === 'key-request') {
        await submit.click()
        await main.getByTestId('approval-key-token').waitFor()
        assert.equal(await main.getByTestId('approval-key-token').evaluate(el => el.readOnly && el.labels.length === 1), true)
      }
      assert.ok(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1), `${name} fits ${locale}`)
      await main.keyboard.press('Tab')
      assert.ok(await dialog.evaluate(el => el.contains(document.activeElement)), 'focus stays within the dialog')
      await main.screenshot({ path: join(shots, `security-${name}-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
      if (name === 'recovery') {
        await main.getByTestId('recovery-enabled').click()
        await main.getByTestId('recovery-days').waitFor()
        assert.ok(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1), `enabled recovery fits ${locale}`)
        await main.screenshot({ path: join(shots, `security-recovery-on-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
      }
      await closeEditor()
    }
    assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `overview fits ${locale}`)
  }

  // Empty and failed reads remain usable; no invented applied values.
  security.logins = []
  await refresh()
  await main.getByTestId('security').scrollIntoViewIfNeeded()
  assert.equal(await main.getByTestId('logins').locator('li').count(), 1)
  failSecurity = true
  await refresh()
  await waitFor(() => main.getByTestId('security-policy-edit').isDisabled())
  failSecurity = false
  await refresh()
  await waitFor(() => main.getByTestId('security-policy-edit').isEnabled())

  // Leaving during a request cannot open a late approval or resurrect the editor.
  await main.getByTestId('security-policy-edit').click()
  await main.getByTestId('daily-executions').fill('9')
  const finishLate = hold('account/policy')
  const windowsBefore = app.windows().length
  const approvalBefore = app.windows().find(page => page !== main)
  const approvalUrl = approvalBefore?.url()
  await submit.click()
  await main.evaluate(() => location.hash = '/channels')
  await main.getByTestId('channel-list').waitFor()
  await finishLate()
  await main.getByTestId('security-editor').waitFor({ state: 'detached' })
  assert.equal(app.windows().length, windowsBefore)
  assert.equal(approvalBefore?.url(), approvalUrl, 'a late response cannot navigate the existing approval window')
  console.log('Security: unchanged/invalid guards, combined approval, retry and refresh drafts, pending receipts, recovery, hardware, late replies, keyboard and recovery-state screenshots passed')
}
