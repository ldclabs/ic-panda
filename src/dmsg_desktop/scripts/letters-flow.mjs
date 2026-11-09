import assert from 'node:assert/strict'
import { join } from 'node:path'

export async function lettersFlow({ app, main, shots, methods, calls, peer }) {
  await main.evaluate(() => { location.hash = '/inbox' })
  await main.getByTestId('letter-compose').waitFor()
  const native = await app.browserWindow(main)
  await native.evaluate(window => window.setContentSize(760, 540))
  const original = methods['inbox/send']
  let failure = 'VERSION_CONFLICT'
  methods['inbox/send'] = p => {
    if (failure) throw { code: failure, message: failure }
    return original(p)
  }
  try {
    await main.getByTestId('letter-compose').click()
    await main.getByTestId('letter-recipient').fill(peer)
    await main.getByTestId('letter-text').fill('Keep the original letter · 保留原信')
    let first
    for (const language of ['zh', 'de', 'ar']) {
      for (const appearance of ['light', 'dark']) {
        await main.evaluate(async ([language, appearance]) => {
          await window.dmsg.setLocale(language)
          await window.dmsg.setAppearance(appearance)
        }, [language, appearance])
        await main.waitForFunction(([language, dark]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === dark,
          [language === 'zh' ? 'zh-Hans' : language, appearance === 'dark'])
        for (const code of ['VERSION_CONFLICT', 'POLICY_STALE', 'REKEY_REQUIRED']) {
          failure = code
          await main.getByTestId('letter-text').focus()
          await main.keyboard.press(process.platform === 'darwin' ? 'Meta+Enter' : 'Control+Enter')
          const notice = main.getByTestId('letter-dialog').getByRole('alert')
          await notice.waitFor()
          await main.waitForFunction(() => !document.querySelector('[data-testid="letter-send"]').disabled)
          const request = calls.filter(c => c.method === 'inbox/send').at(-1).params
          first ??= request
          assert.deepEqual(request, first, 'every retry keeps the original order and text')
          assert.ok(!(await notice.textContent()).includes(code), 'localized recovery instead of a raw error code')
          assert.equal(await main.getByTestId('letter-text').inputValue(), first.text)
          await notice.scrollIntoViewIfNeeded()
          assert.equal(await main.evaluate(() => document.documentElement.scrollWidth > innerWidth), false)
          await main.screenshot({ path: join(shots, `letter-${code}-${language}-${appearance}.png`), animations: 'disabled' })
        }
      }
    }
    await main.keyboard.press('Escape')
    await main.getByTestId('letter-dialog').waitFor({ state: 'detached' })
    await main.getByTestId('letter-compose').focus()
    await main.keyboard.press('Enter')
    assert.equal(await main.getByTestId('letter-text').inputValue(), first.text)
    failure = null
    await main.getByTestId('letter-send').focus()
    await main.keyboard.press('Enter')
    await main.getByTestId('letter-dialog').waitFor({ state: 'detached' })
    assert.deepEqual(calls.filter(c => c.method === 'inbox/send').at(-1).params, first)
  } finally {
    methods['inbox/send'] = original
  }
}
