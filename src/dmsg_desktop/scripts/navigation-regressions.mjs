import assert from 'node:assert/strict'
import { mkdir, rename, rmdir } from 'node:fs/promises'
import { join } from 'node:path'

const go = (page, route) => page.evaluate(route => { location.hash = route }, route)
const composingEnter = input => input.dispatchEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true, cancelable: true })
const settle = page => page.evaluate(async () => {
  await window.dmsg.call('daemon/status')
  await new Promise(requestAnimationFrame)
})

async function palette({ main, methods, file, hold, waitFor, capture, profile }) {
  const vaultTitle = methods['vault/list']({}).find(entry => entry.item.title)?.item.title
  await main.evaluate(() => window.dmsg.setLocale('ar'))
  await main.waitForSelector('html[dir="rtl"]')
  await main.getByTestId('palette-open').click()
  const search = main.getByTestId('palette-input')
  await search.fill(vaultTitle)
  await main.getByTestId('palette-more').first().waitFor()
  await search.press('End')
  await search.press('ArrowLeft')
  await main.getByTestId('palette-back').waitFor()
  await search.press('ArrowRight')
  await main.getByTestId('palette-back').waitFor({ state: 'detached' })
  await search.press('Escape')
  await main.evaluate(() => window.dmsg.setLocale('en'))
  await main.waitForSelector('html[lang="en"]')

  const hit = file('17'.repeat(32), 'Already found.txt', 20)
  let pending = 0, fail = true
  methods['files/search'] = ({ query }) => {
    if (query === 'first fixture') return [hit]
    if (query === 'second fixture') {
      pending++
      if (fail) throw { code: 'UNAVAILABLE' }
      return [{ ...hit, name: 'Current result.txt' }]
    }
    return []
  }
  await main.getByTestId('palette-open').click()
  const input = main.getByTestId('palette-input'), dialog = main.getByTestId('palette')
  await input.fill('first fixture')
  await dialog.getByText(hit.name, { exact: true }).waitFor()
  const release = hold('files/search', p => p.query === 'second fixture')
  await input.fill('second fixture')
  await waitFor(() => pending === 1)
  assert.equal(await dialog.getByText(hit.name, { exact: true }).count(), 0, 'an old file cannot be opened for a new search')
  await release()
  const error = dialog.getByTestId('file-search-error')
  await error.waitFor()
  await capture('palette-file-error', '[data-testid="palette"]')
  fail = false
  await error.getByRole('button').focus()
  await main.keyboard.press('Enter')
  await dialog.getByText('Current result.txt', { exact: true }).waitFor()
  await main.keyboard.press('Escape')
  await dialog.waitFor({ state: 'detached' })

  for (const [fileName, choice, errorText] of [
    ['appearance', 'Appearance: Dark', 'Could not change the appearance. Try again.'],
    ['locale', 'Language: Français', 'Could not switch the language. Try again.']
  ]) {
    const path = join(profile, fileName), saved = path + '.saved'
    await rename(path, saved)
    await mkdir(path) // A directory at the preference path makes the write fail in this temporary profile.
    try {
      await main.getByTestId('palette-open').click()
      await input.fill(choice)
      await dialog.getByText(choice, { exact: true }).click()
      const notice = main.getByText(errorText, { exact: true })
      await notice.waitFor()
      await notice.locator('..').getByRole('button', { name: 'Dismiss', exact: true }).click()
    } finally {
      await rmdir(path)
      await rename(saved, path)
    }
  }
}

async function channelNavigation({ main, methods, hold, waitFor, channel, peer, capture }) {
  const first = { ...channel, id: '81'.repeat(32), name: 'First audit channel', read: 2, message_cursor: 2 }
  const second = { ...first, id: '82'.repeat(32), name: 'Second audit channel' }
  const channels = [first, second]
  const drafts = [], searches = [], jumps = []
  const findChannel = id => channels.find(c => c.id === id)
  const row = (id, text) => ({ seq: 2, account: peer, message_id: id, epoch: 1, stored_at: Date.now(), text })
  methods['channel/list'] = () => channels
  methods['channel/show'] = ({ id }) => findChannel(id)
  methods['channel/message_page'] = ({ id }) => ({ items: [row(id, findChannel(id).name + ' timeline')], has_more: false, next_before: null })
  methods['channel/draft'] = p => { drafts.push(p); findChannel(p.id).draft = p.text; return {} }
  methods['channel/search'] = p => { searches.push(p); return [row(p.id, findChannel(p.id).name + ' match')] }
  methods['channel/messages'] = p => { jumps.push(p); return [row(p.id, 'Old surrounding messages')] }

  const timeline = main.getByTestId('channel-messages'), query = main.getByTestId('channel-search-query')
  const results = main.getByTestId('channel-search-results')
  const open = async c => {
    await go(main, '/channels?open=' + c.id)
    await timeline.getByText(c.name + ' timeline', { exact: true }).waitFor()
  }
  const search = async () => {
    await main.getByTestId('channel-search-toggle').click()
    await query.fill('same words')
    await query.press('Enter')
  }
  await open(first)
  await main.getByTestId('channel-text').fill('Keep this draft on its own channel')
  await open(second)
  await waitFor(() => drafts.some(p => p.id === first.id && p.text === 'Keep this draft on its own channel'))
  const lateSearch = hold('channel/search', p => p.id === second.id)
  await search()
  await waitFor(() => searches.some(p => p.id === second.id))
  await open(first)
  await search()
  await results.getByText(first.name + ' match', { exact: true }).waitFor()
  await lateSearch()
  await settle(main)
  assert.equal(await results.getByText(first.name + ' match', { exact: true }).count(), 1)
  await capture('channel-current-search', '[data-testid="channel-search"]')
  await query.fill('other words')
  assert.equal(await results.count(), 0, 'editing clears the old search before submitting')
  await composingEnter(query)
  await settle(main)
  assert.equal(searches.some(p => p.query === 'other words'), false)
  await query.press('Enter')
  const lateJump = hold('channel/messages', p => p.id === first.id)
  await results.getByRole('button').click()
  await waitFor(() => jumps.length === 1)
  await open(second)
  await lateJump()
  await settle(main)
  assert.equal(await timeline.getByText(second.name + ' timeline', { exact: true }).count(), 1)
  assert.equal(await timeline.getByText('Old surrounding messages', { exact: true }).count(), 0)

  let hiding = false
  methods['channel/hide'] = ({ id }) => { hiding = true; return { ...row(id, 'Hidden in the second channel'), hidden: true } }
  const lateHide = hold('channel/hide')
  await timeline.locator('[data-seq="2"]').hover()
  await timeline.getByTestId('message-more').click()
  await main.getByTestId('message-hide').click()
  await waitFor(() => hiding)
  await open(first)
  await lateHide()
  await settle(main)
  assert.equal(await timeline.getByText(first.name + ' timeline', { exact: true }).count(), 1, 'hiding sequence 2 in another channel leaves this row visible')

  let failRead = true, refused = 0
  second.read = 0
  methods['channel/mark'] = p => {
    if (failRead) { refused++; throw { code: 'UNAVAILABLE' } }
    second.read = p.through
    return second
  }
  await open(second)
  // Reproduce a visible, focused timeline even when the test window is hidden.
  const seeEnd = () => timeline.evaluate(el => {
    const focus = Object.getOwnPropertyDescriptor(document, 'hasFocus')
    Object.defineProperty(document, 'hasFocus', { configurable: true, value: () => true })
    try { el.dispatchEvent(new Event('scroll')) } finally {
      if (focus) Object.defineProperty(document, 'hasFocus', focus)
      else delete document.hasFocus
    }
  })
  await seeEnd()
  await waitFor(() => refused > 0)
  await settle(main)
  const readError = main.getByText('This service is temporarily unavailable. Please try again later.', { exact: true })
  await readError.waitFor()
  failRead = false
  await seeEnd()
  await waitFor(() => second.read === 2)
  await readError.locator('..').getByRole('button', { name: 'Dismiss', exact: true }).click()
}

async function libraryForms({ app, main, methods, file, waitFor, capture }) {
  let fail = true
  const folder = { id: '69'.repeat(32), parent: 'root', name: 'Project folder', path: [], created_at: 1, updated_at: 1 }
  const creations = [], renames = []
  methods['files/tree'] = () => [folder]
  methods['files/list'] = () => ({ folders: [folder], files: [] })
  methods['files/search'] = p => {
    if (p.folder !== folder.id) return []
    if (fail) throw { code: 'UNAVAILABLE' }
    return [file('61'.repeat(32), 'Counts are available again.txt', 42)]
  }
  methods['files/mkdir'] = p => { creations.push(p); return {} }
  methods['files/rename'] = p => { renames.push(p); return {} }
  await go(main, '/files')
  const list = main.getByTestId('files-list')
  await list.locator('[data-id="' + folder.id + '"]').click()
  const detail = main.getByTestId('files-folder-detail')
  const failure = detail.getByTestId('file-search-error')
  await failure.waitFor()
  await capture('folder-count-error', '[data-testid="files-folder-detail"]')
  fail = false
  await failure.getByRole('button').click()
  await detail.getByText(/42/).waitFor()
  await detail.getByRole('button', { name: 'Rename', exact: true }).click()
  const rename = main.getByTestId('files-rename-input')
  await rename.press('Enter')
  await settle(main)
  assert.equal(renames.length, 0, 'the unchanged name stays disabled for the keyboard too')
  await rename.fill('新名称')
  await composingEnter(rename)
  await settle(main)
  assert.equal(renames.length, 0)
  await rename.press('Enter')
  await waitFor(() => renames.length === 1)
  await main.getByTestId('files-rename-dialog').waitFor({ state: 'detached' })
  await main.getByTestId('files-new-folder').click()
  const name = main.getByTestId('files-folder-name')
  await name.fill('新的文件夹')
  await composingEnter(name)
  await settle(main)
  assert.equal(creations.length, 0)
  await capture('folder-name-dialog', '[data-testid="files-new-folder-dialog"]')
  await name.press('Enter')
  await waitFor(() => creations.length === 1)
  assert.deepEqual(creations[0], { parent: 'root', name: '新的文件夹' })
  await main.getByTestId('files-new-folder-dialog').waitFor({ state: 'detached' })

  await go(main, '/vault')
  await main.getByTestId('vault-new').click()
  const form = main.getByTestId('vault-form')
  const title = form.getByTestId('vault-title')
  await main.waitForFunction(() => document.activeElement?.dataset.testid === 'vault-title')
  await title.fill('Unsaved login')
  assert.equal(await form.getByRole('textbox', { name: 'Password', exact: true }).count(), 1)
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
  for (const key of [mod + '+2', mod + '+k', mod + '+/', process.platform === 'darwin' ? 'Meta+[' : 'Alt+ArrowLeft']) {
    await title.press(key)
    assert.equal(await main.evaluate(() => location.hash), '#/vault')
    assert.equal(await main.locator('[aria-modal="true"][data-state="open"]').count(), 1)
  }
  if (process.platform === 'darwin') {
    await app.evaluate(({ Menu, BrowserWindow }) => {
      const settings = Menu.getApplicationMenu().items[0].submenu.items.find(item => item.accelerator === 'Cmd+,')
      settings.click({}, BrowserWindow.getAllWindows()[0], null)
    })
    await settle(main)
    assert.equal(await main.evaluate(() => location.hash), '#/vault', 'the native Settings menu keeps the editor open')
    assert.equal(await title.inputValue(), 'Unsaved login')
  }
  await main.getByTestId('vault-type-api').click()
  const keep = main.getByRole('button', { name: 'Keep editing', exact: true })
  await keep.waitFor()
  assert.equal(await keep.evaluate(el => el === document.activeElement), true)
  await capture('vault-discard-type', '[data-testid="vault-new-leave"]')
  await keep.press('Enter')
  assert.equal(await title.inputValue(), 'Unsaved login')
  assert.equal(await main.getByTestId('vault-type-login').getAttribute('aria-checked'), 'true')
  const tags = form.getByRole('textbox', { name: 'Tags', exact: true })
  await tags.fill('工作')
  await composingEnter(tags)
  assert.equal(await tags.inputValue(), '工作')
  await tags.press('Enter')
  await form.getByRole('button', { name: /工作/ }).waitFor()
  const batch = ['a'.repeat(24), 'b'.repeat(24)]
  await tags.fill(batch.join(','))
  await tags.press('Enter')
  for (const tag of batch) await form.getByText(tag, { exact: true }).waitFor()
  await capture('vault-tag-dialog', '[data-testid="vault-item-tags"]')
  await go(main, '/channels')
}

async function vaultBack({ main, capture }) {
  await go(main, '/files')
  await main.getByTestId('files-list').waitFor()
  await go(main, '/vault')
  await main.getByTestId('vault-list').locator(`[data-id="${'24'.repeat(32)}"]`).click()
  await main.getByTestId('vault-edit-open').click()
  const title = main.getByTestId('vault-title')
  const savedTitle = await title.inputValue()
  const back = main.getByTestId('nav-back'), forward = main.getByTestId('nav-forward')
  const review = main.getByTestId('vault-unsaved')
  await title.fill('Unsaved title')
  await back.press('Enter')
  await review.waitFor()
  await capture('vault-back-with-unsaved-edit', '[data-testid="vault-unsaved"]')
  await review.getByRole('button', { name: 'Keep editing', exact: true }).press('Enter')
  await review.waitFor({ state: 'detached' })
  assert.equal(await title.inputValue(), 'Unsaved title')
  assert.equal(await forward.isDisabled(), true)
  await back.press('Enter')
  await review.getByTestId('vault-unsaved-discard').click()
  await main.getByTestId('files-list').waitFor()
  await forward.click()
  await main.getByTestId('vault-list').waitFor()
  await main.getByTestId('vault-edit-open').click()
  await title.fill('Another edit')
  await back.press('Enter')
  await review.getByRole('button', { name: 'Keep editing', exact: true }).press('Enter')
  await review.waitFor({ state: 'detached' })
  await title.fill(savedTitle)
  await main.getByTestId('vault-form').getByRole('button', { name: 'Cancel', exact: true }).click()
  await go(main, '/files')
  await main.getByTestId('files-list').waitFor()
  await back.click()
  await main.getByTestId('vault-list').waitFor()
}

async function planNavigation({ main, methods, emit, hold, waitFor, capture }) {
  const tab = value => main.getByTestId('plan-tab-' + value)
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
  await go(main, '/plan')
  await tab('names').click()
  await tab('payments').click()
  await main.getByTestId('nav-back').click()
  await main.getByTestId('handle-name').waitFor()
  await main.getByTestId('nav-forward').click()
  await main.getByTestId('plan-orders').waitFor()
  await main.keyboard.press(mod + '+2')
  await main.getByTestId('files-list').waitFor()
  await main.keyboard.press(mod + '+7')
  await main.getByTestId('plan-orders').waitFor()

  const read = methods['handle/price']
  let delayed = 0, failing = true
  methods['handle/price'] = params => {
    if (params.name === 'secondname') delayed++
    if (params.name === 'thirdname' && failing) throw { code: 'UNAVAILABLE' }
    return read(params)
  }
  try {
    await go(main, '/plan?names=firstname')
    const input = main.getByTestId('handle-name'), result = main.getByTestId('handle-state')
    await result.waitFor()
    assert.equal(await main.evaluate(() => location.hash), '#/plan?tab=names')
    assert.equal(await input.inputValue(), 'firstname')
    await input.fill('secondname')
    assert.equal(await result.count(), 0)
    const release = hold('handle/price', p => p.name === 'secondname')
    await input.press('Enter')
    await waitFor(() => delayed === 1)
    await input.fill('thirdname')
    await input.press('Enter')
    const error = main.getByTestId('handle-lookup-error')
    await error.waitFor()
    await release()
    await settle(main)
    assert.equal(await result.count(), 0)
    assert.ok(await error.isVisible())
    await capture('name-lookup-retry', '[data-testid="handle-lookup-error"]')
    failing = false
    await main.getByTestId('handle-lookup').click()
    await result.filter({ hasText: 'thirdname' }).waitFor()

    // The approval-driven refresh can recover after failing for the same query.
    failing = true
    emit('approval/changed')
    await error.waitFor()
    failing = false
    emit('approval/changed')
    await result.filter({ hasText: 'thirdname' }).waitFor()
    await main.getByTestId('nav-back').click()
    await main.getByTestId('plan-orders').waitFor()
  } finally { methods['handle/price'] = read }
}

async function compactNavigation({ app, main, wallets, capture }) {
  const window = await app.browserWindow(main)
  const menu = main.getByTestId('settings-section')
  await window.evaluate(w => w.setContentSize(760, 540))
  await go(main, '/settings/appearance')
  await menu.press('Enter')
  const choices = main.getByTestId('settings-section-menu')
  assert.equal(await choices.getByRole('menuitemradio').count(), 13)
  await main.keyboard.press('End')
  await main.waitForFunction(() => document.activeElement?.dataset.value === 'feedback')
  await main.keyboard.press('Enter')
  await main.waitForFunction(() => location.hash === '#/settings/feedback')
  assert.ok(await menu.isVisible())
  await go(main, '/agents/audit')
  await main.evaluate(() => window.dmsg.setLocale('fr'))
  await main.waitForSelector('html[lang="fr"]')
  const nav = main.getByTestId('agents-tab-audit').locator('..')
  assert.ok(await nav.evaluate(n => [...n.querySelectorAll('button')].every(b => {
    const rect = b.getBoundingClientRect(), bounds = n.getBoundingClientRect()
    return rect.left >= bounds.left && rect.right <= bounds.right
  })))
  await capture('agents-all-tabs', '[data-testid="agent-audit"]')

  const enabled = wallets.state.enabled
  wallets.state.enabled = false // A direct link can resolve before the local wallet list is populated.
  try {
    await go(main, '/wallet/' + wallets.a)
    await main.getByTestId('wallet-identity').waitFor()
    assert.ok((await main.getByTestId('wallet-select').innerText()).includes(wallets.view(wallets.a).name))
    const page = main.locator('main .h-full.overflow-y-auto')
    assert.ok(await page.evaluate(el => { el.scrollTop = el.scrollHeight; return el.scrollTop > 0 }))
    await main.getByTestId('wallet-tab-team').click()
    await main.waitForFunction(() => location.hash.endsWith('?tab=team'))
    assert.equal(await page.evaluate(el => el.scrollTop), 0)
  } finally { wallets.state.enabled = enabled }
}

async function inboxFeedback({ app, main, methods, emit, capture }) {
  const outgoing = methods['inbox/outgoing']
  let failureCode = 'UNAVAILABLE'
  let cached = []
  methods['inbox/outgoing'] = () => {
    if (failureCode) throw { code: failureCode }
    return cached
  }
  try {
    await go(main, '/inbox')
    await main.getByTestId('inbox-sent').click()
    const failure = main.getByTestId('inbox-error')
    await failure.waitFor()
    assert.equal(await main.getByTestId('letters-empty').count(), 0)
    failureCode = ''
    await failure.getByRole('button').press('Enter')
    const empty = main.getByTestId('letters-empty')
    await empty.waitFor()
    await capture('inbox-empty', '[data-testid="inbox"]')
    const window = await app.browserWindow(main)
    await window.evaluate(w => w.setContentSize(760, 540))
    await main.waitForFunction(() => innerWidth === 760)
    assert.ok(await empty.evaluate(el => el.getBoundingClientRect().bottom <= el.closest('ul').getBoundingClientRect().bottom))

    cached = outgoing()
    emit('approval/changed')
    await main.getByTestId('sent-letter').waitFor()
    await main.evaluate(() => window.dmsg.setLocale('fr'))
    await main.waitForSelector('html[lang="fr"]')
    failureCode = 'LOCAL_STORAGE_UNAVAILABLE'
    emit('approval/changed')
    await failure.waitFor()
    await capture('inbox-sent-read-failed', '[data-testid="inbox-error"]')
    await window.evaluate(w => w.setContentSize(760, 540))
    await main.waitForFunction(() => innerWidth === 760)
    const row = main.getByTestId('letters').getByRole('button').filter({ hasText: 'A letter I sent' })
    await row.focus()
    assert.ok(await row.evaluate(el => {
      const bounds = el.closest('ul').getBoundingClientRect()
      const row = el.getBoundingClientRect()
      return Math.min(bounds.bottom, row.bottom) - Math.max(bounds.top, row.top) >= 28
    }), 'saved sent letters stay reachable after a read failure')
    failureCode = ''
    await failure.getByRole('button').press('Enter')
    await failure.waitFor({ state: 'detached' })
    await window.evaluate(w => w.setContentSize(1180, 760))
  } finally { methods['inbox/outgoing'] = outgoing }
}

export async function navigationRegressions({ app, main, formMatrix, shots, ...fixture }) {
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await main.waitForSelector('html[lang="en"]')
  const capture = (name, selector) => formMatrix(app, main, shots, name, selector)
  for (const run of [palette, channelNavigation, libraryForms, vaultBack, planNavigation, compactNavigation, inboxFeedback]) {
    await run({ app, main, capture, ...fixture })
    console.log('Desktop navigation regression passed:', run.name)
  }
}
