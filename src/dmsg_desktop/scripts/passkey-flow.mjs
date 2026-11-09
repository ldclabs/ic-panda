import assert from 'node:assert/strict'
import { join } from 'node:path'

/** UI coverage with the smoke daemon; real delegation verification lives in PocketIC. */
export async function passkeyFlow({ app, main, shots, methods, emit, calls, waitFor, setState }) {
  let available = true, renewable = false, failed = false
  const read = methods['daemon/status']
  methods['daemon/status'] = () => ({ ...read(), passkey_available: available, login_renewable: renewable, login_expires_at: 1 })
  methods['session/login'] = p => {
    if (failed) throw { code: 'UNAVAILABLE', message: 'Fixture network unavailable' }
    return { url: 'https://dmsg.net/_/login#passkey-fixture', expires_at: Date.now() + 300_000, provider: p.provider ?? (available ? 'passkey' : 'ii') }
  }
  const last = () => calls.filter(c => c.method === 'session/login').at(-1).params
  const close = async () => { await main.keyboard.press('Escape'); await main.getByTestId('login-dialog').waitFor({ state: 'detached' }) }
  methods['session/accounts'] = () => []
  setState('logged_out'); emit('session/changed')
  await main.getByTestId('welcome-create-passkey').waitFor()
  await main.getByTestId('welcome-login').focus(); await main.keyboard.press('Enter')
  await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  assert.match(await main.getByTestId('login-steps').innerText(), /passkey/i)
  assert.equal(last().action, 'login')
  const pending = methods['session/login_result']
  methods['session/login_result'] = () => ({ pending: false, checking: false, done: false, error: { code: 'QUOTA_EXCEEDED' } })
  emit('login/changed')
  await main.getByTestId('login-error').waitFor()
  assert.equal(await main.getByTestId('login-reset-devices').count(), 0, 'global limits must not offer signing out devices')
  methods['session/login_result'] = pending
  await main.getByTestId('login-retry').click()
  await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  methods['session/login_result'] = () => ({ pending: false, checking: false, done: false, error: { code: 'SESSION_LIMIT_EXCEEDED' } })
  emit('login/changed')
  await main.getByTestId('login-reset-devices').waitFor()
  methods['session/login_result'] = pending
  await main.getByTestId('login-reset-devices').click()
  await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  assert.equal(last().action, 'logout_all', 'a new device can reset full sessions with fresh Passkey verification')
  await close()
  await main.getByTestId('welcome-login-ii').click()
  await main.getByTestId('login-dialog').waitFor()
  assert.equal(last().provider, 'ii')
  await close()
  failed = true
  await main.getByTestId('welcome-create-passkey').click()
  await main.getByTestId('login-error').waitFor()
  failed = false
  await main.getByTestId('login-retry').click()
  await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  assert.equal(last().provider, 'passkey'); assert.equal(last().action, 'register')
  await close()
  const window = await app.browserWindow(main)
  for (const [locale, appearance, width, height] of [['zh','light',1180,760], ['zh','dark',760,540], ['fr','light',760,540], ['ar','dark',760,540]]) {
    await window.evaluate((w, [width,height]) => w.setContentSize(width,height), [width,height])
    await main.evaluate(async ({locale,appearance}) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, {locale,appearance})
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    assert.ok(await main.getByTestId('welcome').evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await main.screenshot({ path: join(shots, `passkey-welcome-${locale}-${appearance}.png`), animations: 'disabled' })
    await main.getByTestId('welcome-login').click()
    await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
    await main.keyboard.press('Tab')
    assert.ok(await main.getByTestId('login-dialog').evaluate(el => el.contains(document.activeElement)))
    await main.screenshot({ path: join(shots, `passkey-login-${locale}-${appearance}.png`), animations: 'disabled' })
    methods['session/login_result'] = () => ({ pending: false, checking: false, done: false, error: { code: 'QUOTA_EXCEEDED' } })
    emit('login/changed')
    await main.getByTestId('login-error').waitFor()
    assert.equal(await main.getByTestId('login-reset-devices').count(), 0, `global limits have no device reset action in ${locale}`)
    await main.screenshot({ path: join(shots, `passkey-quota-${locale}-${appearance}.png`), animations: 'disabled' })
    methods['session/login_result'] = pending
    await main.getByTestId('login-retry').click()
    await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
    methods['session/login_result'] = () => ({ pending: false, checking: false, done: false, error: { code: 'SESSION_LIMIT_EXCEEDED' } })
    emit('login/changed')
    await main.getByTestId('login-reset-devices').waitFor()
    assert.ok(await main.getByTestId('login-dialog').evaluate(el => el.scrollWidth <= el.clientWidth + 1), `session-limit actions fit ${locale}`)
    await main.screenshot({ path: join(shots, `passkey-capacity-${locale}-${appearance}.png`), animations: 'disabled' })
    methods['session/login_result'] = pending
    await close()
  }
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  renewable = true
  setState('unlocked'); emit('session/changed')
  await main.getByTestId('welcome-done').click()
  await main.evaluate(() => location.hash = '/settings/security')
  await main.getByTestId('bind-passkey').waitFor()
  assert.equal(await main.getByTestId('security-login').count(), 0, 'expired renewable login has no forced sign-in banner')
  await main.getByTestId('bind-passkey').click()
  await main.getByTestId('login-dialog').waitFor()
  assert.equal(last().provider, 'passkey'); assert.equal(last().bind, true); assert.equal(last().action, 'register')
  await close()
  await main.getByTestId('logout-all-logins').click()
  await main.getByTestId('login-dialog').waitFor()
  await waitFor(() => last().action === 'logout_all')
  assert.match(await main.getByTestId('login-dialog').innerText(), /12 hours/)
  await main.screenshot({ path: join(shots, 'passkey-logout-all.png'), animations: 'disabled' })
  await close()
  available = false; renewable = false
  setState('logged_out'); emit('session/changed')
  await main.getByTestId('welcome-login').waitFor()
  await main.getByTestId('welcome-create-passkey').waitFor({state:'detached'})
  assert.match(await main.getByTestId('welcome-login').innerText(), /Log in/)
  await main.getByTestId('welcome-login').click()
  await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  assert.match(await main.getByTestId('login-steps').innerText(), /Internet Identity/)
  await close()
  console.log('Passkey UI passed: primary login, II fallback, registration retry, session capacity vs quota errors, binding, logout-all, renewable expiry, keyboard and locale/theme/width matrix')
}
