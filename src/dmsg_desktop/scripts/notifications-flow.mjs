import assert from 'node:assert/strict'
import { join } from 'node:path'

/** Presentation only: Rust tests cover filtering, deadline phases and replay. */
export async function membershipRemindersFlow({ app, main, shots, methods, emit, waitFor, calls, model }) {
  const native = await app.browserWindow(main)
  await app.evaluate(({ app }) => app.focus({ steal: true }))
  await native.evaluate(window => { window.show(); window.focus(); window.webContents.focus() })
  await waitFor(() => main.evaluate(() => document.hasFocus()))
  model.items.clear()
  emit('notifications/changed')
  await main.evaluate(() => location.hash = '/reminders?scope=recent')
  await main.getByTestId('reminders-list').waitFor()
  assert.equal(await main.getByTestId('reminder-item').count(), 0)
  const status = methods['plan/status']()
  const plan = model.row({ kind: 'plan', id: status.current.contract }, {
    deadline: status.current.ends_at_ms, stage: 2, notified: '1'
  })
  emit('notifications/changed')
  await main.getByTestId('reminder-item').waitFor()
  const businessActions = () => calls.filter(call => ['plan/notice_ack', 'plan/advance', 'notifications/observed'].includes(call.method)).length
  const before = businessActions()
  for (const [language, appearance, width] of [['zh', 'light', 1180], ['zh', 'dark', 760], ['de', 'light', 760], ['ar', 'dark', 760]]) {
    await native.evaluate((window, width) => window.setContentSize(width, width === 760 ? 540 : 760), width)
    await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
    await main.waitForFunction(([language, appearance]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === (appearance === 'dark'), [language === 'zh' ? 'zh-Hans' : language, appearance])
    await main.evaluate(() => document.fonts.ready.then(() => undefined))
    assert.equal(await main.getByTestId('reminder-item').count(), 1)
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
    await main.screenshot({ path: join(shots, `membership-reminder-${language}-${appearance}-${width}.png`), animations: 'disabled' })
    await main.getByTestId('reminders-open').focus()
    await main.keyboard.press('Enter')
    await main.getByTestId('reminders-popover-scope').locator('[data-value="recent"]').click()
    await main.getByTestId('reminder-row').waitFor()
    assert.equal(await main.getByTestId('reminder-row').count(), 1)
    assert.equal(await main.getByTestId('reminder-row').evaluate(el => getComputedStyle(el).direction), language === 'ar' ? 'rtl' : 'ltr')
    assert.equal(plan.read_version, '0', 'Listing does not acknowledge the reminder')
    await main.screenshot({ path: join(shots, `membership-reminder-popover-${language}-${appearance}-${width}.png`), animations: 'disabled' })
    await main.keyboard.press('Escape')
    await waitFor(() => main.getByTestId('reminders-open').evaluate(el => el === document.activeElement))
  }
  await main.getByTestId('reminder-open').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('membership').waitFor()
  await waitFor(() => plan.read_version === plan.version)
  assert.equal(businessActions(), before, 'Reading membership only acknowledges the local reminder')
  await main.evaluate(() => location.hash = '/reminders?scope=recent')
  await main.getByTestId('reminder-read-state').waitFor()
}

export async function reminderReadFlow({ app, main, shots, methods, emit, waitFor, calls, model, account }) {
  const native = await app.browserWindow(main)
  await app.evaluate(({ app }) => app.focus({ steal: true }))
  await native.evaluate(window => { window.show(); window.focus(); window.webContents.focus() })
  await waitFor(() => main.evaluate(() => document.hasFocus()))
  const route = async path => { await main.evaluate(path => { location.hash = path }, path) }
  const other = methods['session/accounts']().find(item => item.account !== account).account
  model.settings.set(other, { ...structuredClone(model.settings.get(account)), enabled: true })
  model.items.clear()
  const network = model.row({ kind: 'service', id: 'network:1' }, { actionable: true })
  const registration = model.row({ kind: 'service', id: 'registration:1' }, { actionable: true })
  for (let i = 0; i < 120; i++) model.row({ kind: 'channel', id: i.toString(16).padStart(64, '0') })
  model.row({ kind: 'channel', id: 'f'.repeat(64) }, {}, other)
  emit('notifications/changed')
  await route('/reminders')
  await main.getByTestId('reminder-item').first().waitFor()
  assert.equal(await main.getByTestId('reminder-item').count(), 2)
  const actions = calls.filter(call => ['approval/respond', 'inbox/mark', 'channel/seen', 'plan/notice_ack'].includes(call.method)).length
  const original = methods['notifications/mark_read']
  methods['notifications/mark_read'] = () => { throw { code: 'UNAVAILABLE', message: 'Temporary reminder save failure' } }
  await main.getByTestId('reminder-menu').first().click()
  await main.getByTestId('reminder-read').click()
  await main.getByText('This service is temporarily unavailable. Please try again later.', { exact: true }).waitFor()
  assert.equal(network.read_version, '0')
  methods['notifications/mark_read'] = original
  await main.getByTestId('reminder-menu').first().click()
  await main.getByTestId('reminder-read').focus()
  await main.keyboard.press('Enter')
  await waitFor(() => network.read_version === '1')
  await main.getByTestId('reminder-read-state').waitFor()
  await main.getByTestId('reminder-open').first().click()
  await main.waitForFunction(() => location.hash.startsWith('#/settings/session'))
  await route('/reminders')
  await main.getByTestId('reminder-open').nth(1).waitFor()
  // A newer projection arrives after the list was displayed, before its click.
  // Opening must succeed without acknowledging that unseen newer version.
  registration.version = '2'
  await main.getByTestId('reminder-open').nth(1).click()
  await main.waitForFunction(() => location.hash.startsWith('#/settings/notifications'))
  await waitFor(() => registration.read_version === '1').catch(async error => {
    console.log('reminder acknowledgement diagnostics', await main.evaluate(() => ({ route: location.hash, focus: document.hasFocus(), modal: !!document.querySelector('[aria-modal="true"]') })), await native.evaluate(w => ({ focused: w.isFocused(), visible: w.isVisible() })), calls.filter(c => c.method.startsWith('notifications/') || c.method === 'daemon/status').slice(-12))
    throw error
  })
  assert.equal(methods['notifications/status']().counts.actionable, 1)
  await route('/reminders')
  for (const [language, appearance, width] of [['zh', 'light', 1180], ['zh', 'dark', 760], ['de', 'light', 760], ['ar', 'dark', 760]]) {
    for (const item of model.items.values()) item.read_version = '0'
    emit('notifications/changed')
    await native.evaluate((window, width) => window.setContentSize(width, width === 760 ? 540 : 760), width)
    await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
    await main.waitForFunction(([language, appearance]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === (appearance === 'dark'), [language === 'zh' ? 'zh-Hans' : language, appearance])
    await main.evaluate(() => document.fonts.ready.then(() => undefined))
    await main.getByTestId('reminder-unread').first().waitFor()
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
    await main.screenshot({ path: join(shots, `reminders-read-${language}-${appearance}-${width}.png`), animations: 'disabled' })
    await main.getByTestId('reminders-open').click()
    await main.getByTestId('reminder-popover-menu').first().click()
    await main.getByTestId('reminder-popover-read').click()
    await waitFor(() => network.read_version === '1')
    await main.keyboard.press('Escape')
    await main.getByTestId('reminders-menu').click()
    await main.getByTestId('reminders-read-all').focus()
    await main.keyboard.press('Enter')
    await waitFor(() => [...model.items.values()].every(item => item.read_version === item.version))
    await waitFor(() => app.evaluate(({ app }) => process.platform !== 'darwin' || app.dock.getBadge() === ''))
    await main.keyboard.press('Escape')
    await main.getByTestId('reminders-menu').click()
    assert.equal(await main.getByTestId('reminders-read-all').getAttribute('aria-disabled'), 'true')
    await main.keyboard.press('Escape')
    assert.equal(await main.getByTestId('reminder-item').count(), 2)
    await main.screenshot({ path: join(shots, `reminders-cleared-${language}-${appearance}-${width}.png`), animations: 'disabled' })
  }
  assert.equal(calls.filter(call => ['approval/respond', 'inbox/mark', 'channel/seen', 'plan/notice_ack'].includes(call.method)).length, actions)
}

/** Exercise the real badge APIs with isolated daemon projections, including unknown counts. */
export async function notificationBadgeFlow({ app, methods, emit, waitFor }) {
  const settings = methods['notifications/settings'], status = methods['notifications/status']
  let mode = 'all', counts = { actionable: 0, unread: 0, unverified: false }
  methods['notifications/settings'] = (params) => {
    const value = settings(params)
    return { ...value, global: { ...value.global, badge: mode } }
  }
  methods['notifications/status'] = () => ({ ...status(), counts,
    badge: mode === 'off' ? 0 : counts.actionable + (mode === 'all' ? counts.unread : 0) })
  await app.evaluate(({ app }) => {
    globalThis.__badgeCounts = []
    globalThis.__setBadgeCount = app.setBadgeCount.bind(app)
    app.setBadgeCount = (count) => {
      globalThis.__badgeCounts.push(count)
      return globalThis.__setBadgeCount(count)
    }
  })
  try {
    for (const [preference, actionable, unread, unverified, expected, label] of [
      ['all', 0, 3, false, 3, '3'],
      ['all', 0, 0, true, 0, ''], // Read messages, with another account still awaiting verification.
      ['all', 0, 3, true, 3, '3'], // Keep the last verified count while syncing.
      ['all', 2, 3, false, 5, '5'],
      ['actions', 2, 3, true, 2, '2'],
      ['actions', 0, 3, true, 0, ''],
      ['all', 0, 105, false, 105, '99+'],
      ['off', 2, 3, true, 0, ''],
      ['all', 0, 0, false, 0, '']
    ]) {
      mode = preference
      counts = { actionable, unread, unverified }
      await app.evaluate(() => { globalThis.__badgeCounts = [] })
      emit('notifications/changed')
      await waitFor(() => app.evaluate(({ app }, { expected, label }) =>
        globalThis.__badgeCounts.at(-1) === expected
        && (process.platform !== 'darwin' || app.dock.getBadge() === label), { expected, label })).catch(async () => {
        const actual = await app.evaluate(({ app }) => ({ count: globalThis.__badgeCounts.at(-1), label: app.dock?.getBadge() }))
        assert.fail(`Badge ${JSON.stringify({ mode, ...counts })}: expected ${expected}/${JSON.stringify(label)}, got ${JSON.stringify(actual)}`)
      })
    }
  } finally {
    methods['notifications/settings'] = settings
    methods['notifications/status'] = status
    await app.evaluate(({ app }) => {
      app.setBadgeCount = globalThis.__setBadgeCount
      delete globalThis.__setBadgeCount
      delete globalThis.__badgeCounts
    })
    emit('notifications/changed')
  }
}

/** The bell and reminder list belong to the same active account. */
export async function remindersFlow({ app, main, shots, methods, emit, waitFor, account }) {
  const native = await app.browserWindow(main)
  const nextAccount = methods['session/accounts']().find((entry) => entry.account !== account).account
  await main.evaluate(() => { location.hash = '/reminders' })
  await main.getByTestId('reminders-page').waitFor()
  // Another account's reminder must not leave a badge over an empty workspace.
  const readNoticeStatus = methods['notifications/status']
  let currentCounts = { actionable: 0, unread: 0, unverified: false }, includeCurrent = true
  methods['notifications/status'] = () => ({
    ...readNoticeStatus(),
    counts: { ...currentCounts, actionable: currentCounts.actionable + 1, unverified: true },
    badge: currentCounts.actionable + currentCounts.unread + 1,
    accounts: [
      ...(includeCurrent ? [{ account, enabled: false, pending_sync: false, counts: { ...currentCounts } }] : []),
      { account: nextAccount, enabled: true, pending_sync: false, counts: { actionable: 1, unread: 0, unverified: true } }
    ]
  })
  emit('notifications/changed')
  await waitFor(async () => await main.getByTestId('reminders-count').count() === 0)
  const refreshReminders = main.getByTestId('reminders-page').getByRole('button', { name: 'Refresh', exact: true })
  await refreshReminders.click()
  await waitFor(async () => !await refreshReminders.isDisabled())
  assert.equal(await main.getByTestId('reminders-unverified').count(), 0)
  // Known counts clear on read/completion, even while the other account remains pending.
  // Turning system alerts off does not hide the selected account's business counts.
  for (const [actionable, unread, unverified, expected] of [
    [0, 3, false, '3'], [0, 0, false, null],
    [2, 3, false, '5'], [2, 0, false, '2'],
    [0, 0, true, null], [0, 105, false, '99+']
  ]) {
    currentCounts = { actionable, unread, unverified }
    emit('notifications/changed')
    await waitFor(async () => expected === null
      ? await main.getByTestId('reminders-count').count() === 0
      : await main.getByTestId('reminders-count').textContent() === expected)
  }
  includeCurrent = false
  emit('notifications/changed')
  await main.getByTestId('reminders-count').waitFor({ state: 'detached' })
  includeCurrent = true
  currentCounts = { actionable: 0, unread: 0, unverified: true }
  emit('notifications/changed')
  await main.getByTestId('reminders-unverified').waitFor()
  for (const [language, appearance] of [['zh', 'light'], ['zh', 'dark'], ['de', 'light'], ['ar', 'dark']]) {
    await native.evaluate((window) => window.setContentSize(760, 540))
    await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
    await main.waitForFunction(([language, appearance]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === (appearance === 'dark'), [language === 'zh' ? 'zh-Hans' : language, appearance])
    await main.evaluate(() => document.fonts.ready.then(() => undefined))
    await main.getByTestId('reminders-unverified').scrollIntoViewIfNeeded()
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
    await main.screenshot({ path: join(shots, `reminders-unverified-${language}-${appearance}.png`), animations: 'disabled' })
  }
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  const recent = main.getByTestId('reminders-unverified').getByRole('button', { name: 'Recent reminders', exact: true })
  await recent.focus()
  await main.keyboard.press('Enter')
  await recent.waitFor({ state: 'detached' })
  await main.getByTestId('reminders-unverified').waitFor()
  currentCounts.unverified = false
  await refreshReminders.focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('reminders-unverified').waitFor({ state: 'detached' })
  assert.equal(await main.getByTestId('reminders-count').count(), 0)
  methods['notifications/status'] = readNoticeStatus
  emit('notifications/changed')
  await native.evaluate((window) => window.setContentSize(1180, 760))
}

/** Native APIs are observed, not sent to the real Notification Center. */
export async function notificationsFlow({ app, main, shots, methods, requests, emit, hold, waitFor, calls, model, account }) {
  await notificationBadgeFlow({ app, methods, emit, waitFor })
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
  const readAgents = methods['agent/ids']
  const agents = [
    { agent_id: '61'.repeat(32), label: 'anda' },
    { agent_id: '62'.repeat(32), label: 'deepseek' },
    { agent_id: '63'.repeat(32), label: 'gpt' }
  ]
  methods['agent/ids'] = () => ({ ...readAgents(), agents })
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
  await waitFor(async () => await main.getByTestId('notifications-agent').count() === agents.length)
  assert.equal(await main.getByTestId('notifications-account').count(), 0, 'notification settings have no independent account picker')
  assert.equal(await main.getByTestId('notifications-account-menu').count(), 0)
  assert.equal(await main.getByTestId('notifications-account-id').count(), 1)
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

  // Each Agent Mail row is keyboard operable and saves only the current account.
  // A waiting or failed save keeps its selections and cannot write another account.
  const agentControls = main.getByTestId('notifications-agent')
  await agentControls.first().focus()
  await main.keyboard.press('Space')
  assert.equal(await agentControls.first().getAttribute('aria-checked'), 'true')
  const finishAgentSave = hold('notifications/settings_set', (params) => params.scope === 'account')
  await main.getByTestId('notifications-save').click()
  await waitFor(async () => await agentControls.first().isDisabled())
  await main.getByTestId('notifications-agents').scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'notifications-agents-saving.png') })
  await finishAgentSave()
  await waitFor(async () => !await agentControls.first().isDisabled())
  assert.equal(model.settings.get(account).agent_mail[agents[0].agent_id], true)
  assert.equal(latest('notifications/settings_set').params.account, account)
  assert.equal(await agentControls.nth(1).getAttribute('aria-checked'), 'false')
  methods['notifications/settings_set'] = () => { throw { code: 'INVALID_INPUT', message: 'Prepared notification settings failure' } }
  await agentControls.nth(1).focus()
  await main.keyboard.press('Space')
  await main.getByTestId('notifications-save').click()
  await main.getByTestId('notifications-error').waitFor()
  assert.equal(await agentControls.nth(1).getAttribute('aria-checked'), 'true')
  methods['notifications/settings_set'] = save
  await main.getByTestId('notifications-save').click()
  await waitFor(async () => await main.getByTestId('notifications-save').isDisabled())
  assert.equal(model.settings.get(account).agent_mail[agents[1].agent_id], true)
  assert.ok(calls.filter((call) => call.method === 'notifications/settings_set' && call.params.scope === 'account').every((call) => call.params.account === account))
  await main.getByTestId('notifications-account-section').scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'notifications-account-en-light.png') })
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
  agents[1].label = '用于项目协作和发布提醒的智能体 · Agent de coordination des notifications et des publications'
  agents[2].label = 'LongAgentNameWithoutSpaces'.repeat(4)
  await main.getByTestId('notifications-refresh').click()
  await waitFor(async () => !await main.getByTestId('notifications-refresh').isDisabled())
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
    await main.getByTestId('notifications-agents').scrollIntoViewIfNeeded()
    await main.screenshot({ path: join(shots, `notifications-agents-${language}-${appearance}.png`) })
    assert.ok(await main.getByTestId('notifications-agents').evaluate((node) => node.scrollWidth <= node.clientWidth), 'long agent names fit the group')
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
  }
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await main.getByTestId('notifications-quiet').scrollIntoViewIfNeeded()
  await main.getByTestId('notifications-quiet').click()
  // Empty agents have an explicit state; a later refresh restores the list.
  methods['agent/ids'] = () => ({ ...readAgents(), agents: [] })
  await main.getByTestId('notifications-refresh').click()
  await main.getByTestId('notifications-agents-empty').waitFor()
  await main.getByTestId('notifications-agents').scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'notifications-agents-empty.png') })
  methods['agent/ids'] = readAgents
  await main.getByTestId('notifications-refresh').click()
  await waitFor(async () => await main.getByTestId('notifications-agent').count() === readAgents().agents.length)
  // Keyboard access reaches the current account toggle and global reminder entry.
  await main.getByTestId('notifications-account-enabled').focus()
  await main.keyboard.press('Space')
  assert.equal(await main.getByTestId('notifications-account-enabled').getAttribute('aria-checked'), 'false')
  await main.keyboard.press('Space')

  // A session change, including one from another window, is the only way this
  // page changes account. A late read from the previous account is discarded.
  const originalStatus = methods['daemon/status']
  const nextAccount = methods['session/accounts']().find((entry) => !entry.current).account
  const originalSettings = structuredClone(model.settings.get(account))
  const nextAgent = { agent_id: '64'.repeat(32), label: 'Other account agent' }
  model.settings.set(nextAccount, { ...structuredClone(originalSettings), enabled: false, agent_mail: {} })
  const releaseOldAccount = hold('notifications/settings', (params) => params?.account === account)
  await main.getByTestId('notifications-refresh').click()
  await waitFor(async () => await main.getByTestId('notifications-refresh').isDisabled())
  methods['daemon/status'] = () => ({ ...originalStatus(), account: nextAccount })
  methods['agent/ids'] = () => ({ ...readAgents(), agents: [nextAgent] })
  emit('session/changed', { state: 'unlocked' })
  await main.getByRole('checkbox', { name: nextAgent.label, exact: true }).waitFor()
  await releaseOldAccount()
  // Wait for a later RPC response so the old settings reply has been consumed.
  await main.evaluate(() => window.dmsg.call('daemon/status'))
  assert.equal(await main.getByTestId('notifications-account-enabled').getAttribute('aria-checked'), 'false')
  assert.equal(await main.getByTestId('notifications-agent').getAttribute('aria-label'), nextAgent.label)
  await main.getByTestId('notifications-agent').click()
  await main.getByTestId('notifications-save').click()
  await waitFor(async () => await main.getByTestId('notifications-save').isDisabled())
  assert.equal(latest('notifications/settings_set').params.account, nextAccount)
  assert.equal(model.settings.get(nextAccount).agent_mail[nextAgent.agent_id], true)
  assert.deepEqual(model.settings.get(account), originalSettings)
  methods['daemon/status'] = originalStatus
  methods['agent/ids'] = readAgents
  emit('session/changed', { state: 'unlocked' })
  await main.getByRole('checkbox', { name: readAgents().agents[0].label, exact: true }).waitFor()
  assert.equal(await main.getByTestId('notifications-account-enabled').getAttribute('aria-checked'), 'true')
  model.settings.delete(nextAccount)
  await main.getByTestId('reminders-open').click()
  await main.getByTestId('reminders-popover').waitFor()
  await waitFor(async () => await main.getByTestId('reminder-row').count() === 2)
  await main.getByTestId('reminders-all').click()
  await main.getByTestId('reminders-page').waitFor()
  await waitFor(async () => await main.getByTestId('reminder-item').count() === 2)
  const actions = calls.filter((call) => ['approval/respond', 'inbox/mark', 'plan/notice_ack'].includes(call.method)).length
  await main.getByTestId('reminder-menu').first().click()
  await main.getByRole('menuitem', { name: 'Remind me later', exact: true }).click()
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
        return { focus: document.hasFocus(), inert: document.body.hasAttribute("data-daemon-unverified"), visible: document.visibilityState,
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
  await main.getByTestId('channel-mute-menu').click()
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
  await main.getByTestId('vault-list').getByText('Smoke note', { exact: true }).waitFor()
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
  await main.getByTestId('reminders-choose-account').waitFor()
  assert.equal(calls.filter((call) => call.method === 'session/switch').length, switches)
  assert.ok((await main.evaluate(() => location.hash)).includes(account))
  await main.screenshot({ path: join(shots, 'notifications-original-account.png') })
  methods['daemon/status'] = status
  emit('session/changed', { state: 'unlocked' })
  await focus()
  await route('/reminders')
  await main.getByTestId('reminders-page').waitFor()
  await main.screenshot({ path: join(shots, 'notifications-reminders.png') })
  await remindersFlow({ app, main, shots, methods, emit, waitFor, account })
  const oldAccounts = methods['session/accounts'], oldSettings = methods['notifications/settings'], oldNoticeStatus = methods['notifications/status']
  // A device with accounts but none open: one with no account at all opens on the first-run page.
  methods['notifications/settings'] = () => ({ ...oldSettings(), account: null, account_id: null })
  methods['daemon/status'] = () => ({ ...status(), account: null, state: 'logged_out', approvals_waiting: 0, pending_sync: 0 })
  methods['notifications/status'] = () => ({ counts:{actionable:0,unread:0,unverified:false},accounts:[],badge:0,enabled:true })
  emit('session/changed', { state: 'logged_out' })
  await route('/settings/notifications')
  await main.getByTestId('notifications-account-section').waitFor()
  assert.equal(await main.getByTestId('notifications-account').count(), 0)
  assert.equal(await main.getByTestId('notifications-account-id').count(), 0)
  assert.equal(await main.getByTestId('notifications-account-enabled').count(), 0)
  await main.screenshot({ path: join(shots, 'notifications-empty-account.png') })
  await main.getByTestId('notifications-account-section').scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'notifications-empty-account-detail.png') })
  methods['session/accounts'] = oldAccounts
  methods['notifications/settings'] = oldSettings
  methods['notifications/status'] = oldNoticeStatus
  methods['daemon/status'] = status
  emit('session/changed', { state: 'unlocked' })

  // New installations start off. A set-up account is pointed to the settings,
  // which opening never changes; registering a second account on this device
  // explains the link and needs an explicit confirmation.
  const second = methods['session/accounts']().find((entry) => !entry.current).account
  const own = model.settings.get(account)
  model.settings.set(account, { ...own, enabled: false })
  model.settings.set(second, { ...own, enabled: true })
  const writes = () => calls.filter((call) => call.method === 'notifications/settings_set').length
  const looks = async (name, testid) => {
    for (const [language, appearance] of [['zh', 'light'], ['zh', 'dark'], ['de', 'light'], ['ar', 'dark']]) {
      await native.evaluate((window) => window.setContentSize(760, 540))
      await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
      await main.waitForFunction(([language, appearance]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === (appearance === 'dark'), [language === 'zh' ? 'zh-Hans' : language, appearance])
      await main.evaluate(() => document.fonts.ready.then(() => undefined))
      await main.getByTestId(testid).scrollIntoViewIfNeeded()
      await main.screenshot({ path: join(shots, `notifications-${name}-${language}-${appearance}.png`), animations: 'disabled' })
      assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
    }
    await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  }
  emit('notifications/changed')
  await route('/settings/security')
  await main.getByTestId('notice-setup').waitFor()
  await looks('setup', 'notice-setup')
  const unchanged = writes()
  await main.getByTestId('notice-setup-open').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('notifications-account-enabled').waitFor()
  assert.equal(writes(), unchanged)
  assert.equal(await main.getByTestId('notifications-account-enabled').getAttribute('aria-checked'), 'false')
  await main.getByTestId('notifications-account-enabled').click()
  await main.getByTestId('notifications-save').click()
  await main.getByTestId('notifications-link').waitFor()
  assert.equal(await main.getByTestId('notifications-link-accounts').locator('li').count(), 1)
  await looks('link', 'notifications-link')
  // Escape cancels without registering and keeps the draft for later.
  await main.keyboard.press('Escape')
  await main.getByTestId('notifications-link').waitFor({ state: 'detached' })
  assert.equal(writes(), unchanged)
  assert.equal(await main.getByTestId('notifications-account-enabled').getAttribute('aria-checked'), 'true')
  await main.getByTestId('notifications-save').click()
  await main.getByTestId('notifications-link-confirm').click()
  await waitFor(() => model.settings.get(account).enabled === true)
  assert.equal(latest('notifications/settings_set').params.scope, 'account')

  // Not now hides the prompt on both pages; binding another login offers it again.
  model.settings.set(account, { ...model.settings.get(account), enabled: false })
  emit('notifications/changed')
  await route('/reminders')
  await main.getByTestId('notice-setup').waitFor()
  await main.getByTestId('notice-setup-later').click()
  await main.getByTestId('notice-setup').waitFor({ state: 'detached' })
  await route('/settings/security')
  await main.getByTestId('security').waitFor()
  assert.equal(await main.getByTestId('notice-setup').count(), 0)
  const readSecurity = methods['account/security']
  methods['account/security'] = (...args) => {
    const value = readSecurity(...args)
    return { ...value, logins: [...value.logins, { principal: '2vxsx-fae', current: false }] }
  }
  emit('approval/changed')
  await main.getByTestId('notice-setup').waitFor()
  methods['account/security'] = readSecurity
  model.settings.set(account, own)
  model.settings.delete(second)
  emit('notifications/changed')
  await main.getByTestId('notice-setup').waitFor({ state: 'detached' })
}

/** Original-account gates, failure recovery, versioned acknowledgement and keyboard navigation. */
export async function reminderJourneyFlow({ app, main, shots, methods, emit, waitFor, calls, model, account, other }) {
  const native = await app.browserWindow(main)
  await app.evaluate(({ app }) => app.focus({ steal: true }))
  await native.evaluate(window => { window.show(); window.focus(); window.webContents.focus() })
  await waitFor(() => main.evaluate(() => document.hasFocus()))
  const originalStatus = methods['daemon/status'], originalUnlock = methods['session/unlock'], originalSwitch = methods['session/switch']
  let active = account, state = 'locked', revision = 1, decline = true
  methods['daemon/status'] = () => ({ ...originalStatus(), account: active, state, session: `reminder-journey-${revision}`, fast_unlock: true, fast_presence: 'biometrics' })
  methods['session/unlock'] = () => {
    if (decline) throw { code: 'FORBIDDEN', message: 'Cancelled authentication' }
    state = 'unlocked'; revision++
    return methods['daemon/status']()
  }
  methods['session/switch'] = params => {
    active = params.account; state = 'locked'; revision++
    return methods['daemon/status']()
  }
  model.items.clear()
  const registration = model.row({ kind: 'service', id: 'registration:1' }, { actionable: true })
  emit('session/changed', { state })
  await main.evaluate(async ({ account, id }) => window.dmsg.notificationOpen(account, id, '1'), { account, id: registration.id })
  await main.getByTestId('reminders-unlock').waitFor()
  assert.equal(registration.read_version, '0')
  await main.getByTestId('reminders-unlock').focus()
  await main.keyboard.press('Enter')
  await waitFor(() => calls.some(call => call.method === 'session/unlock'))
  assert.equal(registration.read_version, '0', 'cancelled authentication is not reading')
  assert.equal(await main.getByTestId('reminders-choose-account').count(), 0, 'the right locked account never asks for a switch')
  await main.screenshot({ path: join(shots, 'reminder-locked-cancelled.png'), animations: 'disabled' })
  decline = false
  await main.getByTestId('reminders-unlock').click()
  await main.getByTestId('notification-settings').waitFor()
  await waitFor(() => registration.read_version === '1')

  // Opening readable content is independent of a failed local acknowledgement.
  registration.version = '2'
  const read = methods['notifications/mark_read']
  methods['notifications/mark_read'] = () => { throw { code: 'UNAVAILABLE', message: 'Acknowledgement unavailable' } }
  await main.evaluate(() => { location.hash = '/reminders' })
  await main.getByTestId('reminder-open').waitFor()
  await main.getByTestId('reminder-open').click()
  await main.getByTestId('notification-settings').waitFor()
  assert.equal(registration.read_version, '1')
  methods['notifications/mark_read'] = read

  model.settings.set(other, { ...structuredClone(model.settings.get(account)), enabled: true })
  const foreign = model.row({ kind: 'service', id: 'network:2' }, { actionable: true }, other)
  await main.evaluate(async ({ account, id }) => window.dmsg.notificationOpen(account, id, '1'), { account: other, id: foreign.id })
  await main.getByTestId('reminders-choose-account').waitFor()
  assert.equal(active, account, 'notification click never selects an account')
  assert.equal(foreign.read_version, '0')
  for (const [language, appearance, width] of [['zh', 'light', 1180], ['zh', 'dark', 760], ['de', 'light', 760], ['ar', 'dark', 760]]) {
    await native.evaluate((window, width) => window.setContentSize(width, width === 760 ? 540 : 760), width)
    await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
    await main.waitForFunction(([language, appearance]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === (appearance === 'dark'), [language === 'zh' ? 'zh-Hans' : language, appearance])
    await main.evaluate(() => document.fonts.ready.then(() => undefined))
    await main.getByTestId('reminders-choose-account').waitFor()
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
    await main.screenshot({ path: join(shots, `reminder-account-${language}-${appearance}-${width}.png`), animations: 'disabled' })
    await main.getByTestId('reminders-choose-account').focus()
    await main.keyboard.press('Enter')
    await main.getByTestId('account-menu').waitFor()
    await main.keyboard.press('Escape')
    assert.equal(active, account)
  }
  await main.getByTestId('reminders-choose-account').click()
  await main.locator(`[data-testid="account-menu"] [data-value="${other}"]`).click()
  await main.getByTestId('reminders-unlock').waitFor()
  assert.equal(active, other)
  assert.equal(foreign.read_version, '0')
  await main.getByTestId('reminders-unlock').click()
  await main.getByTestId('settings-sync').waitFor()
  await waitFor(() => foreign.read_version === '1')

  // Known terminal errors remain local to the original item, with a clear return.
  const gone = model.row({ kind: 'channel', id: 'e'.repeat(64) }, {}, other)
  const show = methods['channel/show']
  methods['channel/show'] = () => { throw { code: 'EXPIRED', message: 'Request expired' } }
  await main.evaluate(async ({ account, id }) => window.dmsg.notificationOpen(account, id, '1'), { account: other, id: gone.id })
  await main.getByTestId('reminders-back').waitFor()
  await main.screenshot({ path: join(shots, 'reminder-expired-rtl.png'), animations: 'disabled' })
  assert.equal(gone.read_version, '0')
  await main.getByTestId('reminders-back').click()
  await main.getByTestId('reminders-list').waitFor()
  assert.equal(await main.getByTestId('reminder-state').count(), 0)
  methods['channel/show'] = show
  methods['daemon/status'] = originalStatus
  methods['session/unlock'] = originalUnlock
  methods['session/switch'] = originalSwitch
}
