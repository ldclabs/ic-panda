import assert from 'node:assert/strict'
import { join } from 'node:path'

export async function feedbackFlow({ app, main, shots, methods, calls, account, hold, waitFor }) {
  const recipient = 'dbrt466u97gecq800000'
  let draft = null, fail = true, loseSave = false
  methods['feedback/get'] = () => structuredClone(draft)
  methods['feedback/save'] = p => {
    assert.equal(p.account, account)
    if (p.revision !== (draft?.revision ?? 0)) throw { code: 'VERSION_CONFLICT' }
    draft = { id: p.id, revision: p.revision + 1, recipient, text: p.text, context: p.context,
      screenshots: p.files.map(file => ({ file, version: 'fixture-version', name: 'Screenshot.png', size: 100, mime: 'image/png' })), state: 'draft', error: null, editable: true }
    if (loseSave) { loseSave = false; throw { code: 'NETWORK_UNAVAILABLE' } }
    return structuredClone(draft)
  }
  methods['feedback/send'] = p => {
    assert.equal(p.id, draft.id); assert.equal(p.revision, draft.revision)
    assert.ok(!Object.hasOwn(p, 'recipient') && !Object.hasOwn(p, 'pay'), 'the core fixes recipient and free delivery')
    draft.state = 'pending'
    if (fail) { draft.error = 'NETWORK_UNAVAILABLE'; throw { code: draft.error } }
    draft.state = 'sent'; draft.error = null; draft.editable = false
    return structuredClone(draft)
  }
  methods['feedback/clear'] = () => { draft = null }
  methods['feedback/edit'] = () => { draft.state = 'draft'; draft.error = null; draft.revision++; return structuredClone(draft) }
  const text = main.getByTestId('feedback-text')
  const send = main.getByTestId('feedback-send')
  const open = async () => { await main.evaluate(() => { location.hash = '/settings/feedback' }); await text.waitFor() }
  await open()
  assert.equal(await send.isDisabled(), true, 'an empty report cannot be sent')
  await text.fill('A private report · 保留原始描述')
  await main.waitForFunction(() => !document.querySelector('[data-testid="feedback-form"]').textContent.includes('Saving encrypted'))
  const id = draft.id
  await main.evaluate(() => { location.hash = '/settings/about' })
  await open()
  assert.equal(await text.inputValue(), 'A private report · 保留原始描述')
  draft.context = 'dMsg desktop previous version'
  await main.reload(); await text.waitFor()
  assert.equal(await text.inputValue(), 'A private report · 保留原始描述', 'reload restores the core draft')
  // Electron clipboard bridge import; the mock daemon only stands in for local encryption.
  const originalImport = methods['files/import']
  methods['files/import'] = () => [{ file: { id: 'fixture-image', current: 'fixture-version', name: 'Screenshot.png', mime: 'image/png', size: 100 } }]
  const screenshotBytes = await main.screenshot()
  methods['files/preview'] = () => ({ kind: 'image', mime: 'image/png', content_base64url: screenshotBytes.toString('base64url'), thumbnail: true })
  await app.evaluate(({ clipboard }, bytes) => {
    clipboard.read = async () => [{ types: ['image/png'], getType: async () => new Blob([Buffer.from(bytes)], { type: 'image/png' }) }]
  }, Array.from(screenshotBytes))
  await text.evaluate(el => {
    const data = new DataTransfer()
    data.items.add(new File([new Uint8Array([137, 80, 78, 71])], 'Screenshot.png', { type: 'image/png' }))
    el.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }))
  })
  await main.getByTestId('feedback-screenshot').waitFor()
  await send.click()
  await main.getByTestId('feedback-error').waitFor()
  assert.equal(draft.id, id)
  assert.equal(await text.inputValue(), 'A private report · 保留原始描述')
  assert.equal(await text.getAttribute('readonly'), '')
  const first = calls.filter(c => c.method === 'feedback/send').at(-1).params
  const native = await app.browserWindow(main)
  await native.evaluate(w => w.setContentSize(760, 540))
  for (const language of ['zh', 'de', 'ar']) for (const appearance of ['light', 'dark']) {
    await main.evaluate(async ([language, appearance]) => { await window.dmsg.setLocale(language); await window.dmsg.setAppearance(appearance) }, [language, appearance])
    await main.waitForFunction(([lang, dark]) => document.documentElement.lang === lang && matchMedia('(prefers-color-scheme: dark)').matches === dark, [language === 'zh' ? 'zh-Hans' : language, appearance === 'dark'])
    await main.getByTestId('feedback-form').evaluate(el => { for (let p = el.parentElement; p; p = p.parentElement) { if (p.scrollHeight > p.clientHeight && /auto|scroll/.test(getComputedStyle(p).overflowY)) { p.scrollTop = 0; break } } })
    await main.screenshot({ path: join(shots, `feedback-form-${language}-${appearance}-760.png`), animations: 'disabled' })
    await send.scrollIntoViewIfNeeded()
    await main.screenshot({ path: join(shots, `feedback-${language}-${appearance}-760.png`), animations: 'disabled' })
    assert.equal(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true)
  }
  fail = false
  await send.focus(); await main.keyboard.press('Enter')
  await main.getByTestId('feedback-new').waitFor()
  assert.deepEqual(calls.filter(c => c.method === 'feedback/send').at(-1).params, first)
  await main.getByTestId('feedback-new').click()
  await text.fill('Second draft after successful feedback')
  await main.waitForFunction(() => !document.querySelector('[data-testid="feedback-send"]').disabled)
  await main.evaluate(() => { location.hash = '/settings/about' }); await open()
  assert.equal(await text.inputValue(), 'Second draft after successful feedback')
  await waitFor(() => draft?.text === 'Second draft after successful feedback')
  assert.notEqual(draft.context, 'dMsg desktop previous version', 'a new report uses the current app version')
  // A save running while navigation changes still persists, but never navigates back.
  const release = hold('feedback/save')
  await text.fill('Latest draft during navigation')
  await main.evaluate(() => { location.hash = '/settings/about' })
  await release()
  await open(); assert.equal(await text.inputValue(), 'Latest draft during navigation')
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  loseSave = true
  await text.fill('A save whose reply was lost')
  await main.getByTestId('feedback-error').waitFor()
  await text.fill('Keep the latest edit after a lost save reply')
  await main.waitForFunction(() => !document.querySelector('[data-testid="feedback-form"]').textContent.includes('Saving encrypted'))
  await main.reload(); await text.waitFor()
  assert.equal(await text.inputValue(), 'Keep the latest edit after a lost save reply')
  await text.fill('信'.repeat(1501))
  assert.equal(await send.isDisabled(), true, 'UTF-8 byte limit blocks submission')
  await main.waitForFunction(() => !document.querySelector('[data-testid="feedback-form"]').textContent.includes('Saving encrypted'))
  await main.reload(); await text.waitFor()
  assert.equal(await text.inputValue(), '信'.repeat(1501), 'an oversized draft remains recoverable')
  methods['files/import'] = originalImport
  console.log('Feedback: empty, autosave, reload, clipboard screenshot, failure, original-ID retry, new report, navigation, keyboard and locale/theme screenshots and UTF-8 draft limits passed.')
}
