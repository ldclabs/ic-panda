import assert from 'node:assert/strict'
import { join } from 'node:path'

/** The sidebar owns account selection; every page follows the same session. */
export async function accountsFlow({ app, main, shots, methods, emit, hold, waitFor, calls, model, account, other }) {
  const status = methods['daemon/status']
  let active = account, state = 'unlocked', fastUnlock = false
  methods['daemon/status'] = () => ({ ...status(), account: active, state, fast_unlock: false })
  methods['session/accounts'] = () => [account, other].map(id => ({ account: id, bound: true, current: id === active, fast_unlock: fastUnlock }))
  methods['session/switch'] = ({ account }) => {
    active = account; state = 'locked'
    return methods['daemon/status']()
  }
  methods['device/moved_list'] = () => []
  methods['device/successions'] = () => []
  const go = path => main.evaluate(path => { location.hash = path }, path)
  const latest = method => calls.filter(call => call.method === method).at(-1)
  const switchTo = async id => {
    await main.getByTestId('account-switcher').focus()
    await main.keyboard.press('Enter')
    const option = main.locator(`[data-testid="account-menu"] [data-value="${id}"]`)
    await option.focus()
    await main.keyboard.press('Enter')
    await waitFor(() => latest('session/switch')?.params.account === id)
    await main.getByTestId('account-menu').waitFor({ state: 'detached' })
  }
  model.items.clear()
  model.settings.set(other, structuredClone(model.settings.get(account)))
  const currentReminder = model.row({ kind: 'task', id: 'conflicts:1' })
  const otherApproval = '77'.repeat(32)
  const original = model.row({ kind: 'approval', id: otherApproval }, {}, other)
  model.row({ kind: 'task', id: 'conflicts:2' }, {}, other)

  await go('/settings/session')
  await main.getByTestId('session-state').waitFor()
  assert.equal(await main.getByTestId('local-accounts').count(), 0)
  assert.equal(await main.getByTestId('settings-login').count(), 0)
  assert.equal(await main.getByTestId('account-switcher').count(), 1)
  await main.keyboard.press(process.platform === 'darwin' ? 'Meta+k' : 'Control+k')
  await main.getByTestId('palette-input').waitFor()
  assert.equal(await main.getByTestId('palette').getByText('Log in with another identity', { exact: true }).count(), 0)
  await main.keyboard.press('Escape')

  emit('notifications/changed')
  await waitFor(async () => await main.getByTestId('reminders-count').textContent() === '1')
  // The tray can be nonzero while the active workspace is empty.
  currentReminder.open = false
  emit('notifications/changed')
  await waitFor(async () => await main.getByTestId('reminders-count').count() === 0)
  await main.getByTestId('account-switcher').focus()
  await main.keyboard.press('Enter')
  const accountBadge = id => main.locator(`[data-testid="account-menu"] [data-value="${id}"] [data-testid="account-reminders"]`)
  await waitFor(async () => (await accountBadge(other).textContent())?.trim() === '2')
  assert.equal(await accountBadge(account).count(), 0)
  const unread = model.row({ kind: 'channel', id: '88'.repeat(32) }, { unread: 120 }, other)
  emit('notifications/changed')
  await waitFor(async () => (await accountBadge(other).textContent())?.trim() === '99+')
  assert.equal(await accountBadge(other).getAttribute('aria-label'), 'Reminders (122)')
  unread.open = false
  emit('notifications/changed')
  await waitFor(async () => (await accountBadge(other).textContent())?.trim() === '2')
  model.items.delete(unread.id)
  currentReminder.open = true
  emit('notifications/changed')
  await waitFor(async () => (await accountBadge(account).textContent())?.trim() === '1')
  await main.keyboard.press('Escape')
  await waitFor(() => main.getByTestId('account-switcher').evaluate(el => el === document.activeElement))
  await main.getByTestId('reminders-open').focus()
  await main.keyboard.press('Enter')
  await waitFor(async () => await main.getByTestId('reminder-row').count() === 1)
  const releasePopover = hold('notifications/list', params => params?.account === account)
  const popoverReads = calls.filter(call => call.method === 'notifications/list').length
  emit('notifications/changed')
  await waitFor(() => calls.filter(call => call.method === 'notifications/list').length > popoverReads)
  // An external session change must also close the old popover, without a click outside it.
  active = other; state = 'locked'
  emit('session/changed')
  await main.getByTestId('reminders-popover').waitFor({ state: 'detached' })
  await waitFor(async () => await main.getByTestId('reminders-count').textContent() === '2')
  await releasePopover()
  await main.getByTestId('reminders-open').focus()
  await main.keyboard.press('Enter')
  await waitFor(async () => await main.getByTestId('reminder-row').count() === 2)
  assert.equal(latest('notifications/list').params.account, other)
  await main.keyboard.press('Escape')
  await switchTo(account)
  await waitFor(async () => await main.getByTestId('reminders-count').textContent() === '1')
  state = 'unlocked'
  emit('session/changed')
  await main.getByTestId('gate-lock').waitFor()

  const switches = calls.filter(call => call.method === 'session/switch').length
  await go(`/reminders?account=${other}&item=${original.id}`)
  await main.getByTestId('reminders-original-account').waitFor()
  assert.equal(await main.getByTestId('reminder-item').count(), 0, 'a foreign reminder does not mix its gate with another account’s list')
  assert.equal(await main.getByTestId('reminders-account').count(), 0)
  assert.equal(calls.filter(call => call.method === 'session/switch').length, switches)
  assert.equal(calls.filter(call => call.method === 'approval/show' && call.params.id === otherApproval).length, 0)

  await main.getByTestId('reminders-back').click()
  await waitFor(async () => await main.getByTestId('reminder-item').count() === 1)

  // A response from the old workspace must not repaint the new account's list.
  const release = hold('notifications/list', params => params?.account === account)
  const reads = calls.filter(call => call.method === 'notifications/list').length
  await main.getByRole('button', { name: 'Refresh', exact: true }).click()
  await waitFor(() => calls.filter(call => call.method === 'notifications/list').length > reads)
  await switchTo(other)
  await waitFor(async () => await main.getByTestId('reminder-item').count() === 2)
  await waitFor(async () => await main.getByTestId('reminders-count').textContent() === '2')
  assert.equal(latest('notifications/list').params.account, other)
  await release()
  assert.equal(await main.getByTestId('reminder-item').count(), 2)
  assert.equal(await main.getByTestId('reminders-original-account').count(), 0)
  assert.equal(calls.filter(call => call.method === 'approval/show' && call.params.id === otherApproval).length, 0, 'switching accounts leaves the workspace locked')
  await main.getByTestId('reminders-scope').locator('[data-value="recent"]').focus()
  await main.keyboard.press('Space')
  await main.getByTestId('reminders-menu').click()
  await main.getByRole('menuitem', { name: 'Clear finished reminders', exact: true }).click()
  await waitFor(() => latest('notifications/clear_recent')?.params.account === other)

  await switchTo(account)
  await waitFor(async () => await main.getByTestId('reminder-item').count() === 1)
  state = 'unlocked'
  emit('session/changed')
  await main.getByTestId('gate-lock').waitFor()
  fastUnlock = true
  const native = await app.browserWindow(main)
  await native.evaluate(window => window.setContentSize(760, 540))
  for (const [language, appearance] of [['zh', 'light'], ['zh', 'dark'], ['de', 'light'], ['ar', 'dark']]) {
    await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
    await main.waitForFunction(([language, appearance]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === (appearance === 'dark'), [language === 'zh' ? 'zh-Hans' : language, appearance])
    for (const [page, path] of [['reminders', '/reminders'], ['settings', '/settings/session']]) {
      await go(path)
      await main.getByTestId(page === 'reminders' ? 'reminders-scope' : 'session-state').waitFor()
      await main.evaluate(() => document.fonts.ready.then(() => undefined))
      await main.screenshot({ path: join(shots, `accounts-${page}-${language}-${appearance}.png`), animations: 'disabled' })
      assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
      assert.equal(await main.getByTestId('account-switcher').count(), 1)
      if (page === 'reminders') {
        await main.getByTestId('account-switcher').focus()
        await main.keyboard.press('Enter')
        await waitFor(async () => (await accountBadge(other).textContent())?.trim() === '2')
        await main.screenshot({ path: join(shots, `accounts-menu-${language}-${appearance}.png`), animations: 'disabled' })
        assert.ok(await main.getByTestId('account-menu').evaluate(el => {
          const rect = el.getBoundingClientRect()
          return rect.left >= 0 && rect.right <= innerWidth && rect.top >= 0 && rect.bottom <= innerHeight && el.scrollWidth <= el.clientWidth
        }))
        await main.keyboard.press('Escape')
        await main.getByTestId('reminders-open').focus()
        await main.keyboard.press('Enter')
        await waitFor(async () => await main.getByTestId('reminder-row').count() === 1)
        await main.screenshot({ path: join(shots, `accounts-bell-${language}-${appearance}.png`), animations: 'disabled' })
        assert.equal(await main.getByTestId('reminders-count').textContent(), '1')
        assert.ok(await main.getByTestId('reminders-popover').evaluate(el => {
          const rect = el.getBoundingClientRect()
          return rect.left >= 0 && rect.right <= innerWidth && rect.top >= 0 && rect.bottom <= innerHeight
        }))
        await main.keyboard.press('Escape')
        await waitFor(() => main.getByTestId('reminders-open').evaluate(el => el === document.activeElement))
      }
    }
  }
  // Logging out must not fall back to a different locally remembered account.
  active = null; state = 'logged_out'
  const before = calls.filter(call => call.method === 'notifications/list').length
  emit('session/changed')
  await go('/reminders')
  await main.getByTestId('reminders-list').waitFor()
  await waitFor(async () => await main.getByTestId('reminder-item').count() === 0)
  assert.equal(await main.getByTestId('reminders-count').count(), 0)
  assert.equal(calls.filter(call => call.method === 'notifications/list').length, before)
  console.log('Single account: scoped bell counts, keyboard switching, original-account routes, stale page/popover replies, scoped clear, empty session and 760×540 locale/theme screenshots passed')
}
