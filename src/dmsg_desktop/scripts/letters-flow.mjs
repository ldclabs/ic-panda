import assert from 'node:assert/strict'
import { join } from 'node:path'

export async function lettersFlow({ app, main, shots, methods, calls, peer, emit, waitFor, formMatrix }) {
  await main.evaluate(() => { location.hash = '/inbox' })
  await main.getByTestId('letter-compose').waitFor()
  const native = await app.browserWindow(main)
  await native.evaluate(window => window.setContentSize(760, 540))
  const list = methods['inbox/list']
  const mark = methods['inbox/mark']
  const status = methods['daemon/status']
  let inboxUnread = 1
  const archived = { ...list()[0], order_id: 'ab'.repeat(32), archived: true, read: true, text: 'Archived letter' }
  const marks = () => calls.filter(c => c.method === 'inbox/mark' && c.params.order === archived.order_id)
  methods['inbox/list'] = () => [...list(), archived]
  methods['daemon/status'] = () => ({ ...status(), inbox_unread: inboxUnread })
  methods['inbox/mark'] = p => {
    if (p.order === archived.order_id) archived.read = p.read
    return mark(p)
  }
  try {
    emit('inbox/changed')
    await main.getByTestId('inbox-unread').getByText('1', { exact: true }).waitFor()
    await main.getByRole('tab', { name: 'Archived' }).click()
    await main.getByTestId('letter').getByText(archived.text).waitFor()
    await main.getByTestId('letter-compose').click()
    archived.read = false
    emit('inbox/changed')
    await main.getByTestId('letter-read').waitFor({ state: 'attached' })
    assert.equal(marks().length, 0)
    await main.keyboard.press('Escape')
    await main.getByTestId('letter-dialog').waitFor({ state: 'detached' })
    await main.getByTestId('inbox-received').click()
    await main.getByRole('tab', { name: 'Archived' }).focus()
    await main.keyboard.press('Enter')
    await waitFor(() => marks().length === 1)
    assert.deepEqual(marks()[0].params, { order: archived.order_id, read: true, archived: true, reopen_contact: false })
    emit('inbox/changed')
    await main.getByTestId('letter-read').waitFor({ state: 'detached' })
    assert.equal(marks().length, 1)
    await formMatrix(app, main, shots, 'archived-letter', '[data-testid="letter"]')
    inboxUnread = 0
    emit('inbox/changed')
    await main.getByTestId('inbox-unread').waitFor({ state: 'detached' })
    await native.evaluate(window => window.setContentSize(760, 540))
    await main.getByTestId('inbox-received').click()
  } finally {
    methods['inbox/list'] = list
    methods['inbox/mark'] = mark
    methods['daemon/status'] = status
  }
  const original = methods['inbox/send']
  const resolve = methods['handle/resolve']
  let resolved = peer
  methods['handle/resolve'] = p => p.name === 'panda' ? { name: 'panda', account: resolved, version: 1 } : null
  let failure = 'VERSION_CONFLICT'
  methods['inbox/send'] = p => {
    if (failure) throw { code: failure, message: failure }
    return original(p)
  }
  try {
    await main.getByTestId('letter-compose').click()
    await main.getByTestId('letter-text').fill('Keep the original letter · 保留原信')
    const recipient = main.getByTestId('letter-recipient')
    const notice = main.getByTestId('letter-send-error')
    const sent = () => calls.filter(c => c.method === 'inbox/send')
    const lookups = () => calls.filter(c => c.method === 'handle/resolve')
    const submit = async message => {
      await main.getByTestId('letter-send').click()
      await notice.waitFor()
      await main.waitForFunction(() => !document.querySelector('[data-testid="letter-send"]').disabled)
      assert.ok((await notice.textContent()).includes(message), await notice.textContent())
    }
    const before = sent().length
    await recipient.fill('unknown_name')
    await submit('Handle not found')
    assert.equal(sent().length, before, 'an unknown handle never submits a letter')
    await recipient.fill('_invalid')
    await notice.waitFor({ state: 'detached' })
    const reads = lookups().length
    await submit('Enter an account ID or handle')
    assert.equal(lookups().length, reads, 'invalid input is rejected before network work')
    assert.equal(sent().length, before)
    await recipient.fill('panda')
    failure = 'NOT_FOUND'
    await submit('Receiving settings were not found')
    assert.equal(sent().at(-1).params.recipient, peer)
    assert.equal(lookups().at(-1).params.name, 'panda')
    await recipient.fill(' @PANDA ')
    await main.getByTestId('letter-text').fill('Retry this handle letter')
    failure = 'EXECUTION_UNKNOWN'
    await submit('The result is not confirmed yet')
    assert.equal(lookups().at(-1).params.name, 'panda')
    const uncertain = sent().at(-1).params
    const resolvedOnce = lookups().length
    resolved = '040g2082040g2082040g'
    await recipient.fill('panda')
    failure = null
    await main.getByTestId('letter-send').click()
    await main.getByTestId('letter-dialog').waitFor({ state: 'detached' })
    assert.deepEqual(sent().at(-1).params, uncertain, 'a handle transfer cannot redirect an uncertain retry')
    assert.equal(lookups().length, resolvedOnce)
    await main.getByTestId('letter-state').getByRole('button').click()
    resolved = peer
    await main.getByTestId('letter-compose').click()
    await recipient.fill(peer)
    await main.getByTestId('letter-text').fill('An account ID also works')
    await main.getByTestId('letter-send').click()
    await main.getByTestId('letter-dialog').waitFor({ state: 'detached' })
    assert.equal(lookups().length, resolvedOnce, 'account IDs bypass handle resolution')
    assert.equal(sent().at(-1).params.recipient, peer)
    await main.getByTestId('letter-state').getByRole('button').click()
    // A closed composer must ignore a late handle lookup.
    let release, started
    const began = new Promise(resolve => { started = resolve })
    const normalResolve = methods['handle/resolve']
    methods['handle/resolve'] = () => { started(); return new Promise(resolve => { release = resolve }) }
    await main.getByTestId('letter-compose').click()
    await recipient.fill('panda')
    await main.getByTestId('letter-text').fill('Keep the original letter · 保留原信')
    const count = sent().length
    await main.getByTestId('letter-send').click()
    await began
    await main.keyboard.press('Escape')
    await main.getByTestId('letter-dialog').waitFor({ state: 'detached' })
    await main.getByTestId('letter-compose').click()
    release({ account: peer })
    await main.waitForFunction(() => !document.querySelector('[data-testid="letter-send"]').disabled)
    assert.equal(sent().length, count)
    methods['handle/resolve'] = normalResolve
    await recipient.fill('@PANDA')
    let first
    for (const language of ['zh', 'de', 'ar']) {
      for (const appearance of ['light', 'dark']) {
        await main.evaluate(async ([language, appearance]) => {
          await window.dmsg.setLocale(language)
          await window.dmsg.setAppearance(appearance)
        }, [language, appearance])
        await main.waitForFunction(([language, dark]) => document.documentElement.lang === language && matchMedia('(prefers-color-scheme: dark)').matches === dark,
          [language === 'zh' ? 'zh-Hans' : language, appearance === 'dark'])
        assert.equal(await recipient.evaluate(input => getComputedStyle(input).direction), 'ltr', 'handle spelling stays intact in RTL')
        for (const code of ['NOT_FOUND', 'FORBIDDEN', 'VERSION_CONFLICT', 'POLICY_STALE', 'REKEY_REQUIRED']) {
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
    if (resolve) methods['handle/resolve'] = resolve
    else delete methods['handle/resolve']
  }
}
