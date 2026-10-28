import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { locales, localeDir } from '../src/shared/locales.ts'

/** Real Electron menus, IPC, both windows and the persisted language choice. */
export async function languagesFlow({ app, main, approval, shots, profile }) {
  const mainWindow = await app.browserWindow(main)
  const approvalWindow = await app.browserWindow(approval)
  await mainWindow.evaluate(window => window.setContentSize(760, 540))
  await approvalWindow.evaluate(window => window.setContentSize(400, 560))
  for (const language of locales) {
    await main.evaluate(() => location.hash = '/settings/appearance')
    const trigger = main.getByTestId('settings-language')
    await trigger.focus()
    await main.keyboard.press('Enter')
    const menu = main.getByTestId('settings-language-menu')
    await menu.waitFor()
    assert.equal(await menu.locator('[role="menuitemradio"]').count(), locales.length)
    // Exercise scrolling and selection with the keyboard, including the last item.
    await menu.focus()
    await main.keyboard.press('Home')
    await main.waitForFunction(() => document.activeElement?.getAttribute('data-value') === 'en')
    for (let index = 0; index < locales.indexOf(language); index++) {
      await main.keyboard.press('ArrowDown')
      await main.waitForFunction(value => document.activeElement?.getAttribute('data-value') === value, locales[index + 1])
    }
    await main.keyboard.press('Enter')
    const tag = language === 'zh' ? 'zh-Hans' : language
    const dir = localeDir(language)
    await main.waitForSelector(`html[lang="${tag}"][dir="${dir}"]`)
    await approval.waitForSelector(`html[lang="${tag}"][dir="${dir}"]`)
    assert.equal(await readFile(join(profile, 'locale'), 'utf8'), language)
    for (const theme of ['light', 'dark']) {
      await main.evaluate(theme => window.dmsg.setAppearance(theme), theme)
      await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, theme === 'dark')
      for (const [name, page] of [['settings', main], ['approval', approval]]) {
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${name} fits ${language}`)
        await page.screenshot({ path: join(shots, `language-${language}-${theme}-${name}.png`), animations: 'disabled' })
      }
    }
    await trigger.focus()
    await main.keyboard.press('Enter')
    await menu.waitFor()
    await main.keyboard.press('Escape')
    await menu.waitFor({ state: 'detached' })
    assert.equal(await trigger.evaluate(element => element === document.activeElement), true)
  }
  // A window reload must receive the choice saved by the main process.
  await main.reload()
  await main.waitForSelector(`html[lang="${locales.at(-1)}"]`)
  await approval.reload()
  await approval.waitForSelector(`html[lang="${locales.at(-1)}"]`)
  await mainWindow.evaluate(window => window.setContentSize(1180, 760))
  await approvalWindow.evaluate(window => window.setContentSize(460, 720))
  await main.evaluate(async () => {
    await window.dmsg.setLocale('fr')
    await window.dmsg.setAppearance('light')
    location.hash = '/settings/appearance'
  })
  await main.waitForSelector('html[lang="fr"]')
  console.log(`${locales.length} desktop localizations: keyboard switching, persistence, both windows and theme screenshots passed`)
}
