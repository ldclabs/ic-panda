import assert from 'node:assert/strict'

/** Session boundaries and selections, using only the smoke runner's isolated data. */
export async function uxRegressionsFlow({ app, main, shots, methods, emit, hold, waitFor, calls, account, formMatrix, setSession }) {
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
  const go = route => main.evaluate(route => { location.hash = route }, route)
  const errors = []
  const check = async (name, run) => {
    try { await run(); console.log(`PASS: ${name}`) }
    catch (error) { errors.push(`${name}: ${error.message}`); console.error(`FAIL: ${name}: ${error.message}`) }
  }
  let state = 'unlocked'
  const session = async next => {
    state = next
    setSession(next)
    emit('session/changed', { state: next })
    await main.getByTestId(next === 'unlocked' ? 'gate-lock' : 'gate-unlock').waitFor()
    await main.waitForFunction(() => !document.body.hasAttribute('data-daemon-unverified'))
  }
  const original = { ...methods }
  const channel = { ...methods['channel/list']()[0], name: 'Private reminder channel', read: 200, unread: 0 }
  const rows = Array.from({ length: 100 }, (_, i) => ({
    seq: i + 1, account, device: '55'.repeat(32), message_id: (i + 1).toString(16).padStart(64, '0'),
    digest: '66'.repeat(32), epoch: 1, created_at: Date.now(), stored_at: Date.now(),
    text: `Message ${i + 1}`, file: null
  }))
  const unlocked = value => {
    if (state !== 'unlocked') throw { code: 'LOCKED', message: 'Locked' }
    return value
  }
  methods['channel/list'] = () => unlocked([channel])
  methods['channel/show'] = () => unlocked(channel)
  methods['channel/message_page'] = p => {
    const before = rows.filter(row => row.seq < p.before)
    const items = before.slice(-p.limit), has_more = items.length < before.length
    return unlocked({ items, has_more, next_before: has_more ? items[0].seq : null })
  }
  methods['notifications/list'] = () => ({ items: [{
    id: 'notice-channel', account, version: '1', target: { kind: 'channel', id: channel.id },
    updated_at: Date.now(), actionable: false, unread: 1, open: true
  }], next_cursor: null })
  methods['inbox/list'] = p => unlocked(original['inbox/list'](p))
  methods['approval/list'] = p => unlocked(original['approval/list'](p))

  await check('a lock closes reminder names, including a late response', async () => {
    await main.getByTestId('reminders-open').click()
    await main.getByTestId('reminder-row').filter({ hasText: channel.name }).waitFor()
    await formMatrix(app, main, shots, 'review-reminders-popover', '[data-testid="reminders-popover"]')
    const reads = calls.filter(c => c.method === 'channel/list').length
    const finish = hold('channel/list')
    emit('notifications/changed')
    await waitFor(() => calls.filter(c => c.method === 'channel/list').length > reads)
    await session('locked')
    await finish()
    await main.waitForTimeout(150)
    assert.equal(await main.getByTestId('reminders-popover').count(), 0, 'the open popover must close with its session')
    await main.getByTestId('reminders-open').click()
    await main.getByTestId('reminder-row').waitFor()
    assert.ok(!(await main.getByTestId('reminders-popover').textContent()).includes(channel.name))
  })
  await main.keyboard.press('Escape')
  await session('unlocked')

  await check('a message link confirmation disappears on lock', async () => {
    rows[99].text = 'https://example.test/private?token=synthetic-message-token'
    await go(`/channels?open=${channel.id}`)
    await main.getByTestId('message-link').click()
    await main.getByTestId('link-confirm').waitFor()
    await formMatrix(app, main, shots, 'review-link-dialog', '[data-testid="link-confirm"]')
    await session('locked')
    assert.equal(await main.getByTestId('link-confirm').count(), 0, 'a link from decrypted content must not reopen while locked')
  })
  await main.keyboard.press('Escape')
  await session('unlocked')

  await check('Delete uses the remaining selected row after deselecting the anchor', async () => {
    await go('/vault')
    await main.getByTestId('vault-list').waitFor()
    const a = '22'.repeat(32), b = '24'.repeat(32)
    await main.locator(`[data-id="${a}"]`).click()
    await main.locator(`[data-id="${b}"]`).click({ modifiers: [mod] })
    await main.locator(`[data-id="${a}"]`).click({ modifiers: [mod] })
    await main.waitForSelector(`[data-id="${b}"][data-chosen="true"]`)
    const count = calls.filter(c => c.method === 'vault/delete').length
    await main.keyboard.press('Delete')
    await waitFor(() => calls.filter(c => c.method === 'vault/delete').length > count)
    const params = calls.filter(c => c.method === 'vault/delete').at(-1).params
    assert.deepEqual(params.ids ?? [params.id], [b])
  })

  await check('an edit outside the last fifty messages updates the open timeline', async () => {
    await go(`/channels?open=${channel.id}`)
    await main.locator('[data-seq="1"]').waitFor()
    rows[0].text = 'Edited earlier message'
    rows[0].edited_at = Date.now()
    emit('channel/changed', { id: channel.id })
    await main.waitForFunction(() => document.querySelector('[data-seq="1"]')?.textContent.includes('Edited earlier message'), null, { timeout: 3000 })
  })

  await check('a sent letter follows its order after payment', async () => {
    let orderState = 'quoted'
    methods['inbox/order'] = () => ({ state: orderState })
    await go('/inbox')
    await main.getByTestId('inbox-sent').click()
    await main.getByTestId('order-state').filter({ hasText: 'Waiting for payment' }).waitFor()
    orderState = 'visible'
    emit('approval/changed')
    emit('inbox/changed')
    await main.waitForFunction(() => document.querySelector('[data-testid="order-state"]')?.textContent === 'Delivered', null, { timeout: 3000 })
    await formMatrix(app, main, shots, 'review-sent-letter', '[data-testid="sent-letter"]')
  })

  await check('a restored letter draft retries the same uncertain order', async () => {
    methods['inbox/send'] = () => { throw { code: 'EXECUTION_UNKNOWN', message: 'Reply lost' } }
    await main.getByTestId('letter-compose').click()
    await main.getByTestId('letter-recipient').fill('0123456789abcdefghi0')
    await main.getByTestId('letter-text').fill('Retry this same letter')
    await main.getByTestId('letter-send').click()
    await waitFor(() => calls.some(c => c.method === 'inbox/send' && c.params.text === 'Retry this same letter'))
    await main.getByTestId('letter-send').waitFor({ state: 'visible' })
    await main.waitForFunction(() => !document.querySelector('[data-testid="letter-send"]')?.disabled)
    const first = calls.filter(c => c.method === 'inbox/send').at(-1).params.order
    await main.keyboard.press('Escape')
    await go('/vault')
    await main.getByTestId('vault-list').waitFor()
    await go('/inbox')
    await main.getByTestId('letter-compose').click()
    assert.equal(await main.getByTestId('letter-text').inputValue(), 'Retry this same letter')
    const count = calls.filter(c => c.method === 'inbox/send').length
    await main.getByTestId('letter-send').click()
    await waitFor(() => calls.filter(c => c.method === 'inbox/send').length > count)
    assert.equal(calls.filter(c => c.method === 'inbox/send').at(-1).params.order, first)
  })
  await main.keyboard.press('Escape')

  await check('the vault restores its tag filter after returning from another view', async () => {
    await go('/vault')
    await main.getByTestId('vault-scope-tag:team').click()
    await go('/files')
    await main.getByTestId('files-list').waitFor()
    await go('/vault')
    await main.getByTestId('vault-scope-tag:team').waitFor()
    assert.equal(await main.getByTestId('vault-scope-tag:team').getAttribute('aria-current'), 'true')
  })

  if (!errors.length) {
    await go(`/channels?open=${channel.id}`)
    await main.getByTestId('channel-messages').waitFor()
    await formMatrix(app, main, shots, 'review-channel', '[data-testid="channel-messages"]')
    await go('/vault')
    await main.getByTestId('vault-list').waitFor()
    await formMatrix(app, main, shots, 'review-vault', '[data-testid="vault-list"]')
  }
  Object.assign(methods, original)
  assert.deepEqual(errors, [])
}
