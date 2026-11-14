import assert from 'node:assert/strict'
import { join } from 'node:path'
import { locales, localeDir } from '../src/shared/locales.ts'

/** The sidebar's session controls must fit every translation, including at minimum height. */
export async function gateRailFlow({ app, main, methods, emit, hold, waitFor, shots }) {
  const native = await app.browserWindow(main)
  const originalStatus = methods['daemon/status']
  let session = { state: 'locked', fast_unlock: true, fast_presence: 'biometrics' }
  methods['daemon/status'] = () => ({
    ...originalStatus(), ...session, pending_sync: 0,
    ...(session.state === 'logged_out' ? { account: null } : {})
  })
  const change = (value) => { session = value; emit('session/changed') }
  await main.evaluate(() => location.hash = '/settings/appearance')

  const layout = async (label) => {
    const errors = await main.evaluate(() => {
      const errors = []
      const rail = document.querySelector('[data-testid="gate-rail"]')
      const sidebar = document.querySelector('[data-testid="sidebar"]')
      const bounds = rail.getBoundingClientRect()
      const side = sidebar.getBoundingClientRect()
      if (bounds.left < side.left || bounds.right > side.right || bounds.bottom > innerHeight) errors.push('rail outside sidebar')
      if (sidebar.querySelector('nav').getBoundingClientRect().bottom > bounds.top) errors.push('navigation overlaps rail')
      for (const element of rail.querySelectorAll('button, [data-testid="gate-state"]')) {
        const rect = element.getBoundingClientRect()
        if (rect.left < bounds.left || rect.right > bounds.right || rect.bottom > bounds.bottom) errors.push('control outside rail')
        if (element.scrollWidth > element.clientWidth + 1 || element.scrollHeight > element.clientHeight + 1) errors.push('control content overflows')
        const range = document.createRange()
        range.selectNodeContents(element)
        for (const text of range.getClientRects()) {
          if (text.left < rect.left - 1 || text.right > rect.right + 1 || text.bottom > rect.bottom + 1) errors.push('text clipped')
        }
      }
      const action = rail.querySelector('[data-testid="gate-unlock"]')
      if (action) {
        const rect = action.getBoundingClientRect()
        const state = rail.querySelector('[data-testid="gate-state"]').getBoundingClientRect()
        const row = action.parentElement.getBoundingClientRect()
        if (rect.top < state.bottom || Math.abs(rect.width - row.width) > 1) errors.push('action needs its own full-width row')
        const sync = rail.querySelector('[data-testid="gate-sync"]')?.getBoundingClientRect()
        if (sync && sync.top < rect.bottom) errors.push('action overlaps sync')
      }
      return errors
    })
    assert.deepEqual(errors, [], label)
  }
  const expanded = async () => {
    if (await main.getByTestId('sidebar-toggle').getAttribute('aria-expanded') !== 'true') {
      await main.getByTestId('sidebar-toggle').focus()
      await main.keyboard.press('Enter')
    }
    await waitFor(async () => Math.abs((await main.getByTestId('sidebar').boundingBox()).width - 236) < 1)
  }

  for (const language of locales) {
    const catalog = (await import(`../src/renderer/i18n/messages/${language}.ts`)).default
    const tag = language === 'zh' ? 'zh-Hans' : language
    await main.evaluate(language => window.dmsg.setLocale(language), language)
    await main.waitForSelector(`html[lang="${tag}"][dir="${localeDir(language)}"]`)
    const show = async (value, action) => {
      change(value)
      await waitFor(async () => (await main.getByTestId('gate-state').textContent()) === catalog.sessionStates[value.state]
        && (await main.getByTestId('gate-unlock').textContent()).trim() === catalog.messages[action])
    }
    for (const [width, height, theme] of [[1180, 760, 'light'], [760, 540, 'dark']]) {
      await native.evaluate((window, [width, height]) => window.setContentSize(width, height), [width, height])
      await main.evaluate(theme => window.dmsg.setAppearance(theme), theme)
      await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, theme === 'dark')
      await expanded()
      for (const [value, action] of [
        [{ state: 'logged_out', fast_unlock: false, fast_presence: null }, 'login'],
        [{ state: 'moved', fast_unlock: false, fast_presence: null }, 'movedReview'],
        [{ state: 'locked', fast_unlock: true, fast_presence: null }, 'fastUnlock'],
        [{ state: 'locked', fast_unlock: true, fast_presence: 'biometrics' }, 'touchIdUnlock']
      ]) {
        await show(value, action)
        await layout(`${language} ${width} ${action}`)
      }
      await main.evaluate(() => document.fonts.ready)
      await main.getByTestId('sidebar').screenshot({ path: join(shots, `gate-rail-${language}-${theme}-${width}.png`), animations: 'disabled' })
    }

    // The busy translation also fits; cancelling leaves an enabled retry action.
    methods['session/unlock'] = () => { throw { code: 'FORBIDDEN', message: 'Fixture presence cancelled' } }
    const finish = hold('session/unlock')
    await main.getByTestId('gate-unlock').focus()
    await main.keyboard.press('Enter')
    await main.waitForSelector('[data-testid="gate-unlock"][aria-busy="true"]')
    assert.equal(await main.getByTestId('gate-unlock').isDisabled(), true)
    await layout(`${language} unlocking`)
    await finish()
    await main.waitForSelector('[data-testid="gate-unlock"][aria-busy="false"]')
    assert.equal(await main.getByTestId('gate-unlock').isEnabled(), true)

    // Collapsing keeps the action reachable from the keyboard in both directions.
    await main.getByTestId('sidebar-toggle').focus()
    await main.keyboard.press('Enter')
    await main.getByTestId('gate-toggle').waitFor()
    await waitFor(async () => Math.abs((await main.getByTestId('sidebar').boundingBox()).width - 64) < 1)
    await layout(`${language} collapsed`)
  }

  // A failed unlock stays locked and can be retried successfully with the keyboard.
  await main.evaluate(() => window.dmsg.setLocale('en'))
  await main.waitForSelector('html[lang="en"]')
  await expanded()
  methods['session/unlock'] = () => { throw { code: 'UNAVAILABLE', message: 'Fixture unlock failure' } }
  await main.getByTestId('gate-unlock').focus()
  await main.keyboard.press('Enter')
  const en = (await import('../src/renderer/i18n/messages/en.ts')).default
  await main.getByText(en.messages.serviceUnavailable, { exact: true }).waitFor()
  assert.equal(await main.getByTestId('gate-state').textContent(), en.sessionStates.locked)
  methods['session/unlock'] = () => { change({ state: 'unlocked', fast_unlock: true, fast_presence: 'biometrics' }); return null }
  await main.getByTestId('gate-unlock').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('gate-lock').waitFor()
  await layout('unlocked after retry')
  console.log(`Gate rail passed: ${locales.length} languages, light/dark, 1180×760/760×540, expanded/collapsed, login/fast unlock/Touch ID/moved, busy/cancel/failure/retry and keyboard`)
}
