import assert from 'node:assert/strict'

const go = (main, route) => main.evaluate(route => { location.hash = route }, route)

// The held reply precedes this read on the socket. Let the renderer apply both.
const settleReplies = main => main.evaluate(async () => {
  await window.dmsg.call('daemon/status')
  await new Promise(requestAnimationFrame)
})

async function attachments({ main, methods, channel, file, capture }) {
  const alpha = file('a1'.repeat(32), 'Alpha notes.txt', 32)
  const beta = file('b1'.repeat(32), 'Beta · Rapport de réunion très détaillé.txt', 16)
  methods['files/search'] = ({ query }) => [alpha, beta].filter(f => f.name.includes(query))
  await go(main, '/channels?open=' + channel.id)
  const browse = async () => {
    await main.getByTestId('channel-attachment').click()
    await main.getByTestId('channel-browse').click()
  }
  await browse()
  const query = main.getByTestId('library-query'), results = main.getByTestId('library-results'), add = main.getByTestId('library-pick')
  const choice = name => results.getByRole('checkbox', { name, exact: true })
  await query.fill('Alpha')
  await choice(alpha.name).focus()
  await main.keyboard.press('Space')
  await query.fill('Beta')
  await choice(beta.name).click()
  await query.fill('Alpha')
  assert.equal(await choice(alpha.name).isChecked(), true)
  await choice(alpha.name).click()
  await main.getByTestId('library-picker').getByText('1 selected', { exact: true }).waitFor()
  await choice(alpha.name).click()
  await query.fill('no matches')
  await results.getByText('No files', { exact: true }).waitFor()
  assert.equal(await add.isEnabled(), true)
  await capture('attachment-selection-dialog', '[data-testid="library-picker"]')
  await add.focus()
  await main.keyboard.press('Enter')
  const attached = main.getByTestId('channel-attached')
  await attached.getByText(alpha.name, { exact: true }).waitFor()
  assert.equal(await attached.getByText(beta.name, { exact: true }).count(), 1)
  await browse()
  assert.equal(await add.isDisabled(), true, 'a new picker starts with no selection')
  await main.getByTestId('library-picker').getByTestId('dialog-close').click()
}

async function pairing({ main, methods, capture }) {
  let expiresIn = 4500, serial = 0
  const granted = []
  methods['connect/pairings'] = () => []
  methods['connect/invite'] = () => ({ invite_id: String(++serial).padStart(64, '0'),
    url: 'https://connect.dmsg.net/v1/pair#fixture', expires_at: Date.now() + expiresIn })
  methods['connect/candidates'] = ({ invite }) => ({ candidates: [{ pairing_id: invite, label: 'Local browser ' + Number(invite) }] })
  methods['connect/grant'] = p => { granted.push(p); return {} }
  await go(main, '/settings/browsers')
  const start = main.getByTestId('browser-copy'), connect = main.getByTestId('browser-grant'), expired = main.getByTestId('browser-expired')
  await start.click()
  await connect.waitFor()
  await expired.waitFor()
  assert.equal(await connect.count(), 0)
  assert.equal(await main.getByText(/Waiting for the browser that opened/).count(), 0)
  await capture('pairing-expired', '[data-testid="browser-expired"]')
  expiresIn = 0
  await start.click()
  await expired.waitFor()
  assert.equal(await connect.count(), 0, 'expiry also works without a candidate')
  await expired.getByRole('button', { name: 'Dismiss' }).click()
  await expired.waitFor({ state: 'detached' })
  expiresIn = 60_000
  await start.click()
  await main.getByText('Local browser 3', { exact: true }).waitFor()
  await connect.focus()
  await main.keyboard.press('Enter')
  await connect.waitFor({ state: 'detached' })
  assert.deepEqual(granted.map(p => [p.invite, p.pairing]), [['3'.padStart(64, '0'), '3'.padStart(64, '0')]])
}

async function agentDraft({ main, methods, hold, waitFor, capture }) {
  const names = []
  methods['agent/create'] = ({ label }) => {
    names.push(label)
    if (names.length === 1) throw { code: 'UNAVAILABLE' }
    return { label }
  }
  await go(main, '/agents/ids')
  const label = main.getByTestId('agent-label'), create = main.getByRole('button', { name: 'Create', exact: true })
  await label.fill('research-agent')
  await create.click()
  await waitFor(() => names.length === 1)
  await settleReplies(main)
  assert.equal(await label.inputValue(), 'research-agent')
  await capture('agent-draft', '[data-testid="agent-label"]')
  const release = hold('agent/create')
  await create.click()
  await waitFor(() => names.length === 2)
  await label.fill('next-agent')
  await release()
  await settleReplies(main)
  assert.equal(await label.inputValue(), 'next-agent')
  await create.focus()
  await main.keyboard.press('Enter')
  await main.waitForFunction(() => document.querySelector('[data-testid="agent-label"]').value === '')
  assert.deepEqual(names, ['research-agent', 'research-agent', 'next-agent'])

  // Capability grants share the action result: retain the choices on refusal.
  let grantAttempts = 0
  methods['grants/add'] = () => {
    if (++grantAttempts === 1) throw { code: 'UNAVAILABLE' }
    return {}
  }
  await go(main, '/agents/capabilities')
  for (const id of ['grant-agent', 'grant-origin']) {
    await main.getByTestId(id).click()
    const menu = main.getByTestId(id + '-menu')
    await (id === 'grant-agent' ? menu.locator('[data-value]').first() : menu.locator('[data-value="https://api.al.ink"]')).click()
  }
  const permission = main.getByTestId('service-mail.read')
  await permission.click()
  const grant = main.getByTestId('grant-capabilities')
  await grant.click()
  await waitFor(() => grantAttempts === 1)
  await settleReplies(main)
  assert.ok(await permission.isChecked())
  await grant.click()
  await waitFor(() => grantAttempts === 2)
  await settleReplies(main)
  assert.equal(await permission.isChecked(), false)
}

async function feedbackDraft({ main, methods, hold, waitFor, capture }) {
  let draft = null
  const saves = []
  methods['feedback/get'] = () => draft
  methods['feedback/save'] = p => {
    saves.push(p)
    if (saves.length === 1) throw { code: 'LOCAL_STORAGE_UNAVAILABLE' }
    draft = { id: p.id, revision: 1, recipient: '', text: p.text, context: p.context, screenshots: [], state: 'draft', error: null, editable: true }
    return draft
  }
  await go(main, '/settings/feedback')
  const text = main.getByTestId('feedback-text'), status = main.getByTestId('feedback-form').getByRole('status')
  const release = hold('feedback/save')
  await text.fill('Keep this feedback draft')
  await status.filter({ hasText: 'Saving encrypted draft…' }).waitFor()
  await release()
  await status.filter({ hasText: 'Your latest changes have not been saved.' }).waitFor()
  assert.equal(await text.inputValue(), 'Keep this feedback draft')
  await capture('feedback-unsaved', '[data-testid="feedback-error"]')
  const retrySave = hold('feedback/save')
  await main.getByTestId('feedback-error').getByRole('button').focus()
  await main.keyboard.press('Enter')
  await waitFor(() => saves.length === 2)
  await status.filter({ hasText: 'Saving encrypted draft…' }).waitFor()
  assert.equal(await main.getByTestId('feedback-error').count(), 0)
  await retrySave()
  await status.filter({ hasText: 'Draft saved encrypted on this device.' }).waitFor()
  assert.deepEqual(saves[1], saves[0], 'retry saves the same draft and revision')
}

async function searchFailures({ main, methods, file, hold, waitFor, emit, capture }) {
  const match = file('cc'.repeat(32), 'Current file search.txt', 12)
  const old = 'superseded query', fresh = 'latest query', failed = 'retry this query'
  for (const picker of [false, true]) {
    let olderReads = 0, latestReads = 0, fail = true
    methods['files/search'] = ({ query }) => {
      if (query === old) { olderReads++; throw { code: 'INVALID_INPUT' } }
      if (query === fresh) latestReads++
      if (query === failed && fail) throw { code: 'UNAVAILABLE' }
      return [fresh, failed].includes(query) ? [match] : []
    }
    await go(main, picker ? '/vault' : '/files')
    if (picker) {
      await main.getByTestId('vault-new').click()
      await main.getByTestId('attach-file').click()
    }
    const input = main.getByTestId(picker ? 'file-picker-search' : 'files-search')
    const row = main.getByText(match.name, { exact: true }).first()
    const release = hold('files/search', p => p.query === old)
    await input.fill(old)
    await waitFor(() => olderReads === 1)
    await input.fill(fresh)
    await row.waitFor()
    await release()
    await settleReplies(main)
    assert.equal(await row.isVisible(), true, 'an old failure cannot discard a newer success')
    if (!picker) {
      const before = latestReads
      const refresh = hold('files/search', p => p.query === fresh)
      emit('files/changed')
      await waitFor(() => latestReads > before)
      assert.equal(await row.isVisible(), true, 'the same search stays visible during a metadata refresh')
      await refresh()
    }
    await input.fill(failed)
    const error = main.getByTestId('file-search-error')
    await error.waitFor()
    assert.equal(await main.getByTestId('files-empty').count(), 0)
    await capture(picker ? 'vault-search-error-dialog' : 'files-search-error', picker ? '[data-testid="file-picker"]' : '[data-testid="file-search-error"]')
    fail = false
    await error.getByRole('button').focus()
    await main.keyboard.press('Enter')
    await row.waitFor()
    assert.equal(await error.count(), 0)
    await input.fill('zero matches')
    if (picker) {
      await main.getByText('No files found', { exact: true }).waitFor()
      const late = hold('files/search', p => p.query === old)
      await input.fill(old)
      await waitFor(() => olderReads === 2)
      await main.getByTestId('file-picker').getByTestId('dialog-close').click()
      await main.getByTestId('attach-file').click()
      await input.fill(fresh)
      await row.waitFor()
      await late()
      await settleReplies(main)
      assert.equal(await row.isVisible(), true, 'the previous dialog cannot update the reopened picker')
    } else await main.getByTestId('files-empty').getByText(/zero matches/).waitFor()
  }
}

async function directChannel({ main, methods, peer, channel, capture }) {
  const creations = [], invitations = []
  methods['channel/list'] = () => [channel, ...creations]
  methods['channel/show'] = p => creations.find(c => c.id === p.id) ?? channel
  methods['channel/create'] = p => {
    const id = String(creations.length + 1).padStart(64, 'd')
    const created = { ...channel, id, name: p.name, ledger: { ...channel.ledger, channel_id: id, type: p.type } }
    creations.push(created)
    return created
  }
  methods['channel/invite'] = p => {
    invitations.push(p)
    if (invitations.length === 1) throw { code: 'UNAVAILABLE' }
    return { channel_id: p.id, account: p.account }
  }
  await go(main, '/channels?direct=' + peer)
  const dialog = main.getByTestId('channel-new-dialog')
  await main.getByTestId('channel-create').click()
  await dialog.getByRole('button', { name: 'Retry', exact: true }).waitFor()
  assert.equal(await main.getByTestId('channel-invite-pending').getByText(peer, { exact: true }).isVisible(), true)
  await capture('channel-invite-retry-dialog', '[data-testid="channel-new-dialog"]')
  await dialog.getByRole('button', { name: 'Retry', exact: true }).focus()
  await main.keyboard.press('Enter')
  await dialog.getByTestId('channel-invitation-link').waitFor()
  assert.match(await dialog.getByTestId('channel-invitation-link').inputValue(), /^dmsg:\/\/channel\/join#/)
  assert.equal(creations.length, 1)
  assert.deepEqual(invitations.map(p => [p.id, p.account]), [[creations[0].id, peer], [creations[0].id, peer]])
  await dialog.getByRole('button', { name: 'Done' }).click()
  await main.getByTestId('channel-new').click()
  await main.getByRole('menuitem', { name: 'New channel', exact: true }).click()
  await dialog.getByTestId('channel-kind-collaboration').click()
  await dialog.getByTestId('channel-name').fill('Next collaboration')
  await dialog.getByTestId('channel-create').click()
  await dialog.waitFor({ state: 'detached' })
  assert.equal(creations[1].ledger.type, 'collaboration')
  assert.equal(invitations.length, 2, 'a collaboration channel does not reuse the previous invitation')
}

export async function stateTransitionsFlow({ app, main, shots, formMatrix, ...fixture }) {
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await main.waitForSelector('html[lang="en"]')
  const capture = (name, selector) => formMatrix(app, main, shots, name, selector)
  for (const check of [attachments, pairing, agentDraft, feedbackDraft, searchFailures, directChannel]) {
    await check({ main, capture, ...fixture })
    console.log('Desktop regression passed:', check.name)
  }
}
