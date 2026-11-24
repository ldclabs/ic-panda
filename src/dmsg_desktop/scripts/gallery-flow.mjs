import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'

/**
 * `DMSG_DESKTOP_SMOKE=gallery`: the main views in five looks (English light
 * and dark, Chinese, Arabic dark, and the 760px window) on the smoke fixture,
 * in `test-results/gallery/`, to compare a visual change before and after.
 */
export async function galleryFlow({ app, main, shots }) {
  const out = join(shots, 'gallery')
  await mkdir(out, { recursive: true })
  const window = await app.browserWindow(main)
  const go = (route) => main.evaluate((route) => (location.hash = route), route)
  const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
  const click = (locator) => async () => { if (await locator.count()) await locator.first().click() }
  const routes = [
    ['channels', '/channels', 'channel-list', click(main.getByTestId('channel-item'))],
    ['files', '/files', 'files-list'],
    ['vault', '/vault', 'vault-list', click(main.getByText('Smoke note'))],
    ['inbox', '/inbox', 'letter-search'],
    ['approvals', '/approvals'],
    ['agents', '/agents', 'agents-tab-capabilities'],
    ['plan', '/plan', 'membership'],
    ['wallet', '/wallet', 'wallet-view'],
    ['reminders', '/reminders', 'reminders-page'],
    ['settings-session', '/settings/session', 'session-details'],
    ['settings-security', '/settings/security'],
    ['settings-appearance', '/settings/appearance']
  ]
  const looks = [['en', 'light', 1180, 760], ['en', 'dark', 1180, 760], ['zh', 'light', 1180, 760], ['ar', 'dark', 1180, 760], ['en', 'light', 760, 540]]
  for (const [locale, theme, width, height] of looks) {
    await window.evaluate((w, [width, height]) => w.setContentSize(width, height), [width, height])
    await main.evaluate(async ({ locale, theme }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(theme) }, { locale, theme })
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction((dark) => matchMedia('(prefers-color-scheme: dark)').matches === dark, theme === 'dark')
    for (const [name, route, testid, then] of routes) {
      await go(route)
      if (testid) await main.getByTestId(testid).first().waitFor({ timeout: 10_000 }).catch(() => console.log(`gallery: ${name} not ready`))
      await pause(700)
      if (then) await then().catch((error) => console.log(`gallery: ${name} ${error.message.split('\n')[0]}`))
      await pause(600)
      await main.screenshot({ path: join(out, `${name}-${locale}-${theme}-${width}.png`), animations: 'disabled' })
    }
  }
  await window.evaluate((w) => w.setContentSize(1180, 760))
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
}
