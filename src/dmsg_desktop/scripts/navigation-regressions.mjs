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

async function libraryForms({ main, methods, file, waitFor, capture }) {
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

export async function navigationRegressions({ app, main, formMatrix, shots, ...fixture }) {
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await main.waitForSelector('html[lang="en"]')
  const capture = (name, selector) => formMatrix(app, main, shots, name, selector)
  for (const run of [palette, channelNavigation, libraryForms]) {
    await run({ main, capture, ...fixture })
    console.log('Desktop navigation regression passed:', run.name)
  }
}
