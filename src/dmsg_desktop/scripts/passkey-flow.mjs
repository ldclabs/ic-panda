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
  const browser = () => main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  const choose = async () => {
    const before = calls.filter(c => c.method === 'session/login').length
    await main.getByTestId('welcome-login').click()
    await main.getByTestId('passkey-choices').waitFor()
    assert.equal(calls.filter(c => c.method === 'session/login').length, before, 'opening the entry does not create a credential or start login')
  }
  const fail = (code, message = '') => {
    methods['session/login_result'] = () => ({ pending: false, checking: false, done: false, error: { code, message } })
    emit('login/changed')
  }
  methods['session/accounts'] = () => []
  setState('logged_out'); emit('session/changed')
  await main.getByTestId('welcome-login').waitFor()
  assert.equal(await main.getByTestId('welcome-create-passkey').count(), 0)
  await main.getByTestId('welcome-login').focus(); await main.keyboard.press('Enter')
  await main.getByTestId('passkey-choices').waitFor()
  await close()
  assert.ok(await main.getByTestId('welcome-login').evaluate(el => el === document.activeElement), 'Escape returns focus to the entry')
  await choose()
  await main.getByTestId('passkey-login').focus(); await main.keyboard.press('Enter')
  await browser()
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
  await browser()
  assert.equal(last().provider, 'ii')
  await close()
  failed = true
  await choose()
  await main.getByTestId('passkey-create').click()
  await main.getByTestId('login-error').waitFor()
  assert.equal(await main.getByTestId('login-use-saved').count(), 0, 'preflight failure does not claim a credential was created')
  failed = false
  await main.getByTestId('login-retry').click()
  await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  assert.equal(last().provider, 'passkey'); assert.equal(last().action, 'register')
  fail('EXECUTION_UNKNOWN')
  await main.getByTestId('login-use-saved').waitFor()
  assert.equal(await main.getByTestId('login-retry').count(), 0, 'an unconfirmed registration must not offer blind creation retry')
  assert.match(await main.getByTestId('login-error').innerText(), /If a passkey was saved/)
  const registrations = calls.filter(c => c.method === 'session/login' && c.params.action === 'register').length
  methods['session/login_result'] = pending
  await main.getByTestId('login-use-saved').click(); await browser()
  assert.equal(last().action, 'login')
  assert.equal(calls.filter(c => c.method === 'session/login' && c.params.action === 'register').length, registrations)
  fail('NOT_FOUND')
  await main.getByTestId('login-error').waitFor()
  assert.match(await main.getByTestId('login-error').innerText(), /not registered with dMsg/)
  methods['session/login_result'] = pending
  await close()
  const window = await app.browserWindow(main)
  for (const [locale, appearance, width, height] of [['zh','light',1180,760], ['zh','dark',760,540], ['fr','light',760,540], ['ar','dark',760,540]]) {
    await window.evaluate((w, [width,height]) => w.setContentSize(width,height), [width,height])
    await main.evaluate(async ({locale,appearance}) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, {locale,appearance})
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    assert.ok(await main.getByTestId('welcome').evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await main.screenshot({ path: join(shots, `passkey-welcome-${locale}-${appearance}.png`), animations: 'disabled' })
    await choose()
    await main.keyboard.press('Tab')
    assert.ok(await main.getByTestId('login-dialog').evaluate(el => el.contains(document.activeElement)))
    assert.ok(await main.getByTestId('login-dialog').evaluate(el => el.scrollWidth <= el.clientWidth + 1), `choices fit ${locale}`)
    await main.screenshot({ path: join(shots, `passkey-choices-${locale}-${appearance}.png`), animations: 'disabled' })
    await main.getByTestId('passkey-login').click(); await browser()
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
    await choose()
    await main.getByTestId('passkey-create').click(); await browser()
    fail('EXECUTION_UNKNOWN')
    await main.getByTestId('login-use-saved').waitFor()
    assert.ok(await main.getByTestId('login-dialog').evaluate(el => el.scrollWidth <= el.clientWidth + 1), `registration recovery fits ${locale}`)
    await main.screenshot({ path: join(shots, `passkey-registration-${locale}-${appearance}.png`), animations: 'disabled' })
    methods['session/login_result'] = pending
    await main.getByTestId('login-use-saved').click(); await browser()
    fail('NOT_FOUND')
    await main.getByTestId('login-error').waitFor()
    await main.screenshot({ path: join(shots, `passkey-missing-${locale}-${appearance}.png`), animations: 'disabled' })
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
  await browser()
  assert.equal(last().provider, 'passkey'); assert.equal(last().bind, true); assert.equal(last().action, 'register')
  fail('EXECUTION_UNKNOWN')
  await main.getByTestId('login-use-saved').waitFor()
  methods['session/login_result'] = pending
  await main.getByTestId('login-use-saved').click(); await browser()
  assert.equal(last().bind, true, 'using the saved passkey still binds to the current account')
  assert.equal(last().action, 'login')
  await close()
  await main.getByTestId('account-switcher').click()
  await main.getByTestId('account-login').waitFor()
  assert.equal(await main.getByTestId('account-create-passkey').count(), 0)
  const beforeAccountLogin = calls.filter(c => c.method === 'session/login').length
  await main.getByTestId('account-login').click()
  await main.getByTestId('passkey-choices').waitFor()
  assert.equal(calls.filter(c => c.method === 'session/login').length, beforeAccountLogin)
  await main.getByTestId('passkey-login-ii').click(); await browser()
  assert.equal(last().provider, 'ii')
  await close()
  await main.getByTestId('logout-all-logins').click()
  await browser()
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
  console.log('Passkey UI passed: one entry, explicit login/creation, preflight retry, unconfirmed registration recovery, unregistered credentials, account switching, II fallback, capacity vs quota, binding, logout-all, keyboard and locale/theme/width matrix')
}
