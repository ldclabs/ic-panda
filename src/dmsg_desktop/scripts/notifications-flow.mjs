import assert from 'node:assert/strict'
import { join } from 'node:path'

/** Native APIs are observed, not sent to the real Notification Center. */
export async function notificationsFlow({ app, main, shots, methods, requests, emit, hold, waitFor, calls, model, account }) {
  const simulatedFocus = process.env.DMSG_DESKTOP_NOTIFICATION_FOCUS === 'mock'
  if (simulatedFocus) {
    console.log('Native focus is simulated for this run; this is not a native-focus acceptance test')
    await app.evaluate(({ BrowserWindow }) => {
      globalThis.__notificationFocusAllowed = true
      BrowserWindow.prototype.isFocused = function () {
        return globalThis.__notificationFocusAllowed && this.id === globalThis.__notificationFocusedWindow && this.isVisible() && !this.isMinimized()
      }
    })
    // This runner has no guaranteed macOS foreground focus. Both focus inputs
    // are explicit test doubles; the negative native-focus check below remains.
    await main.evaluate(() => { Document.prototype.hasFocus = () => true })
  }
  const native = await app.browserWindow(main)
  if (simulatedFocus) await native.evaluate((window) => {globalThis.__notificationFocusedWindow = window.id})
  const focus = async () => {
    if (simulatedFocus) await app.evaluate(() => {globalThis.__notificationFocusAllowed = true})
    await app.evaluate(({ app }) => app.focus({ steal: true }))
    await native.evaluate((window) => { window.show(); window.focus(); window.webContents.focus() })
    await waitFor(() => main.evaluate(() => document.hasFocus()))
    await waitFor(() => native.evaluate((window) => window.isVisible() && window.isFocused())).catch(async (error) => {
      console.log('native focus diagnostics', await native.evaluate((window) => ({ id:window.id, visible:window.isVisible(), focused:window.isFocused(), focusable:window.isFocusable(), enabled:window.isEnabled(), minimized:window.isMinimized(), bounds:window.getBounds() })), await app.evaluate(({app,BrowserWindow}) => ({ name:app.getName(), hidden:app.isHidden(), focused:BrowserWindow.getFocusedWindow()?.id })))
      throw error
    })
  }
  const route = async (path) => { await main.evaluate((path) => { location.hash = path }, path) }
  const notes = () => app.evaluate(() => globalThis.__notes.length)
  const latest = (method) => calls.filter((call) => call.method === method).at(-1)
  await focus()
  const request = [...requests.values()][0]
  request.updated_at = request.created_at
  model.row({ kind: 'approval', id: request.id }, { version: String(request.updated_at), deadline: Date.now() + 60_000 })
  model.row({ kind: 'task', id: 'conflicts:1' })
  emit('notifications/changed')

  // Settings save failures keep the draft, and retry uses the same scope.
  const releaseSettings = hold('notifications/settings', (params) => params?.account === account)
  await route('/settings/notifications')
  await main.getByTestId('notification-settings').waitFor()
  await main.getByTestId('notification-settings').getByText('Loading…', { exact: true }).waitFor()
  await releaseSettings()
  await main.getByTestId('notifications-enabled').waitFor()
  const save = methods['notifications/settings_set']
  let fail = true
  methods['notifications/settings_set'] = (params) => {
    if (fail) {
      fail = false
      const current = model.methods['notifications/settings']().global
      save({ scope: 'installation', expected_revision: current.revision, settings: { ...current, badge: 'actions' } })
      throw { code: 'VERSION_CONFLICT', message: 'Settings changed. Review and save again.' }
    }
    return save(params)
  }
  await main.getByTestId('notifications-enabled').click()
  await main.getByTestId('notifications-save').click()
  await main.getByTestId('notifications-error').waitFor()
  assert.equal(await main.getByTestId('notifications-enabled').getAttribute('aria-checked'), 'false')
  await main.getByTestId('notifications-save').click()
  await waitFor(() => model.methods['notifications/settings']().global.enabled === false)
  assert.equal(latest('notifications/settings_set').params.expected_revision, 2)
  await main.getByTestId('notifications-enabled').click()
  await main.getByTestId('notifications-save').click()
  await waitFor(() => model.methods['notifications/settings']().global.enabled === true)
  methods['notifications/settings_set'] = save
  // A remote CAS conflict stays pending until an explicit Retry, then an ACK
  // updates the status without making the untouched form dirty.
  const readSettings = methods['notifications/settings']
  let remoteConflict = true, remotePending = true
  methods['notifications/settings'] = (p) => ({...readSettings(p),remote_supported:true,global_conflict:remoteConflict,global_pending_sync:remotePending})
  emit('notifications/changed')
  await waitFor(async () => await main.getByTestId('notifications-save').textContent() === 'Retry')
  assert.equal(await main.getByTestId('notifications-save').isDisabled(), false)
  const revision = readSettings().global.revision
  await main.getByTestId('notifications-save').click()
  await waitFor(() => latest('notifications/settings_set').params.expected_revision === revision)
  await waitFor(async () => await main.getByTestId('notifications-save').isDisabled() === false)
  remoteConflict = false; remotePending = false
  model.settings.get(account).remote_revision = model.settings.get(account).revision
  emit('notifications/changed')
  await waitFor(async () => await main.getByTestId('notifications-save').isDisabled() === true)
  assert.equal(await main.getByTestId('notifications-enabled').getAttribute('aria-checked'), 'true')
  methods['notifications/settings'] = readSettings
  const beforeTest = await notes()
  await main.getByTestId('notifications-test').click()
  await waitFor(async () => await notes() === beforeTest + 1)
  assert.equal(latest('notifications/result')?.params?.system_id === 'dmsg-test', false)
  await main.locator('.pointer-events-auto button[aria-label="Dismiss"]').click()

  // The same shared controls must remain usable in narrow, translated themes.
  await main.getByTestId('notifications-quiet').click()
  for (const [language, appearance] of [['zh', 'light'], ['zh', 'dark'], ['de', 'light'], ['fr', 'dark'], ['ar', 'dark'], ['ur', 'light']]) {
    await native.evaluate((window) => window.setContentSize(760, 540))
    await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
    await main.waitForFunction(([language, appearance]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === (appearance === 'dark'), [language === 'zh' ? 'zh-Hans' : language, appearance])
    await main.evaluate(() => document.fonts.ready.then(() => undefined))
    await main.getByTestId('notification-settings').evaluate((element) => {
      let parent = element.parentElement
      while (parent && getComputedStyle(parent).overflowY !== 'auto') parent = parent.parentElement
      if (parent) parent.scrollTop = 0
    })
    await main.screenshot({ path: join(shots, `notifications-${language}-${appearance}-top.png`) })
    await main.getByTestId('notifications-save').scrollIntoViewIfNeeded()
    await main.screenshot({ path: join(shots, `notifications-${language}-${appearance}-bottom.png`) })
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
  }
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await main.getByTestId('notifications-quiet').scrollIntoViewIfNeeded()
  await main.getByTestId('notifications-quiet').click()
  // Keyboard access reaches the account selector and global reminder entry.
  await main.getByTestId('notifications-account').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('notifications-account-menu').waitFor()
  await main.keyboard.press('Escape')
  await main.getByTestId('reminders-open').click()
  await main.getByTestId('reminders-page').waitFor()
  await waitFor(async () => await main.getByTestId('reminder-item').count() === 2)
  const actions = calls.filter((call) => ['approval/respond', 'inbox/mark', 'plan/notice_ack'].includes(call.method)).length
  await main.getByTestId('reminder-item').first().getByRole('button', { name: 'Remind me later', exact: true }).click()
  await main.getByTestId('reminder-snooze').waitFor()
  assert.equal(await main.getByTestId('reminder-snooze').getByRole('button', { name: 'Pause for 15 minutes' }).isDisabled(), true)
  await main.keyboard.press('Escape')
  assert.equal(calls.filter((call) => ['approval/respond', 'inbox/mark', 'plan/notice_ack'].includes(call.method)).length, actions)

  // Inbox detail visibility suppresses only reminder duplication, not mail read state.
  const letters = await methods['inbox/list']({ refresh: false })
  const letter = letters.find((letter) => letter.state === 'visible')
  if (letter) {
    const item = model.row({ kind: 'inbox', id: letter.order_id })
    const marks = calls.filter((call) => call.method === 'inbox/mark').length
    await route(`/inbox?open=${letter.order_id}`)
    await main.getByTestId('letter').waitFor()
    await focus()
    await waitFor(() => item.observed === item.version).catch(async (error) => {
      console.log('notification visibility diagnostics', await main.evaluate(async () => {
        const node = document.querySelector('[data-testid="letter"] > header')
        const intersection = await new Promise((resolve) => { const observer = new IntersectionObserver(([entry]) => { observer.disconnect(); resolve({ intersecting: entry.isIntersecting, ratio: entry.intersectionRatio }) }); observer.observe(node) })
        return { focus: document.hasFocus(), inert: document.body.inert, visible: document.visibilityState,
          modal: !!document.querySelector('[aria-modal="true"]'), anchor: node?.getBoundingClientRect().toJSON(), intersection }
      }), await native.evaluate((window) => ({ visible: window.isVisible(), focused: window.isFocused(), destroyed: window.isDestroyed() })), calls.filter((call) => call.method === 'notifications/observed').slice(-3))
      await main.screenshot({ path: join(shots, 'notifications-visibility-diagnostic.png') })
      throw error
    })
    const before = await notes()
    model.enqueue(item)
    await waitFor(() => latest('notifications/next')?.params !== undefined)
    assert.equal(await notes(), before)
    assert.equal(calls.filter((call) => call.method === 'inbox/mark').length, marks)
    if (simulatedFocus) {
      const observed = calls.filter((call) => call.method === 'notifications/observed').length
      await app.evaluate(() => { globalThis.__notificationFocusAllowed = false })
      const accepted = await main.evaluate(async (id) => {
        const status = await window.dmsg.call('daemon/status')
        return window.dmsg.notificationVisible({account:status.account,session:status.session,target:{kind:'inbox',id}})
      }, letter.order_id)
      assert.equal(accepted, false, 'a focused DOM alone cannot claim actual visibility')
      assert.equal(calls.filter((call) => call.method === 'notifications/observed').length, observed)
      await app.evaluate(() => { globalThis.__notificationFocusAllowed = true })
    }
  }

  await route('/vault')
  // Timed mute is a channel preference, with shared menu/keyboard behavior.
  const channel = methods['channel/list']()[0]
  await route(`/channels?open=${channel.id}`)
  await main.getByTestId('channel-manage').waitFor()
  const membersClose = main.locator('aside').getByRole('button', {name:'Close',exact:true})
  if (await membersClose.count()) await membersClose.last().click()
  await main.getByTestId('channel-manage').click()
  await main.getByTestId('channel-mute-hour').click()
  await waitFor(() => latest('channel/settings')?.params.muted_until > Date.now() + 3_500_000)
  for (const [language, appearance] of [['zh','light'],['zh','dark'],['de','light'],['ar','dark']]) {
    await main.evaluate(async ([language,appearance]) => {await window.dmsg.setLocale(language);await window.dmsg.setAppearance(appearance)},[language,appearance])
    await main.waitForFunction((language) => document.documentElement.lang === language,language === 'zh' ? 'zh-Hans' : language)
    await main.getByTestId('channel-manage').click()
    await main.getByTestId('channel-manage-menu').waitFor()
    await main.screenshot({path:join(shots,`notifications-mute-${language}-${appearance}.png`),animations:'disabled'})
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth),false)
    await main.keyboard.press('Escape')
  }
  await main.evaluate(async () => {await window.dmsg.setLocale('en');await window.dmsg.setAppearance('light')})
  await route('/vault')
  await main.getByTestId('vault-item-title').filter({ hasText: 'Smoke note' }).waitFor()
  const foreground = model.row({ kind: 'summary', id: 'message' }, { version: '2', unread: 2 })
  const beforeForeground = await notes()
  model.enqueue(foreground)
  await main.getByTestId('notification-toast').waitFor()
  assert.equal(await notes(), beforeForeground)
  await main.locator('.pointer-events-auto button[aria-label="Dismiss"]').click()
  // A merged hint is opened from the main window too, with its original
  // account and recent-message scope instead of approval-only show-main IPC.
  model.enqueue(foreground, { delivery_id: 'e'.repeat(64), covered: [
    { id: foreground.id, version: foreground.version, stage: 1 },
    { id: 'f'.repeat(64), version: '1', stage: 1 }
  ] })
  await main.getByTestId('notification-toast').getByRole('button', { name: 'Open', exact: true }).click()
  await main.getByTestId('reminders-page').waitFor()
  const summaryRoute = await main.evaluate(() => location.hash)
  assert.ok(summaryRoute.includes(`account=${account}`))
  assert.ok(summaryRoute.includes('scope=recent'))
  await route('/vault')
  if (simulatedFocus) await app.evaluate(() => {globalThis.__notificationFocusAllowed = false})
  await native.evaluate((window) => {window.hide();window.emit('blur')})
  await waitFor(() => latest('notifications/context')?.params.context.foreground === false).catch(async error => {
    console.log('notification background diagnostics',await app.evaluate(({BrowserWindow}) => BrowserWindow.getAllWindows().map(w => ({id:w.id,visible:w.isVisible(),focused:w.isFocused()}))),latest('notifications/context')?.params,calls.slice(-8).map(c => c.method))
    throw error
  })
  const background = model.row({ kind: 'summary', id: 'message' }, { version: '3', unread: 3 })
  model.enqueue(background)
  await waitFor(async () => await notes() === beforeForeground + 1)
  const status = methods['daemon/status']
  methods['daemon/status'] = () => ({ ...status(), account: '080g2081040g2081040g' })
  emit('session/changed', { state: 'unlocked' })
  const switches = calls.filter((call) => call.method === 'session/switch').length
  await app.evaluate(() => globalThis.__lastNotification.emit('click'))
  await main.getByTestId('reminders-page').waitFor()
  await main.getByText('Switch to this account to open the item', { exact: false }).first().waitFor()
  assert.equal(calls.filter((call) => call.method === 'session/switch').length, switches)
  assert.ok((await main.evaluate(() => location.hash)).includes(account))
  await main.screenshot({ path: join(shots, 'notifications-original-account.png') })
  methods['daemon/status'] = status
  emit('session/changed', { state: 'unlocked' })
  await focus()
  await route('/reminders')
  await main.getByTestId('reminders-page').waitFor()
  await main.screenshot({ path: join(shots, 'notifications-reminders.png') })
  const oldAccounts = methods['session/accounts'], oldSettings = methods['notifications/settings'], oldNoticeStatus = methods['notifications/status']
  methods['session/accounts'] = () => []
  methods['notifications/settings'] = () => ({ ...oldSettings(), account: null, account_id: null })
  methods['daemon/status'] = () => ({ ...status(), account: null, state: 'logged_out', approvals_waiting: 0, pending_sync: 0 })
  methods['notifications/status'] = () => ({ counts:{actionable:0,unread:0,unverified:false},accounts:[],badge:0,enabled:true })
  emit('session/changed', { state: 'logged_out' })
  await route('/settings/notifications')
  await main.getByTestId('notifications-account').waitFor()
  assert.equal(await main.getByTestId('notifications-account').isDisabled(), true)
  assert.equal(await main.getByTestId('notifications-account-enabled').count(), 0)
  await main.screenshot({ path: join(shots, 'notifications-empty-account.png') })
  await main.getByTestId('notifications-account').scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'notifications-empty-account-detail.png') })
  methods['session/accounts'] = oldAccounts
  methods['notifications/settings'] = oldSettings
  methods['notifications/status'] = oldNoticeStatus
  methods['daemon/status'] = status
}
