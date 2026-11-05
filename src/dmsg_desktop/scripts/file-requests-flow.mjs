import assert from 'node:assert/strict'
import { join } from 'node:path'

const LONG = '项目交付资料 · Delivery documents with a long descriptive title · Documents de livraison très détaillés · وثائق المشروع المشتركة'
const LONG_NAME = '客户合同扫描件 · Signed contract with a long descriptive filename · Contrat signé · عقد موقع.pdf'

const link = (id, fields = {}) => ({
  request_id: id.repeat(32),
  url: `https://dmsg.test/r/account/${id.repeat(32)}#page-key-${id}`,
  title: 'Tax documents',
  instructions: 'Send last year’s statements.',
  label: null,
  expires_at: Date.now() + 14 * 86400000,
  max_total_bytes: 500 * 1048576,
  created_at: Date.now(),
  revoked_at: null,
  status: 'open',
  files: 0,
  bytes: 0,
  last_stored_at: null,
  unread: false,
  last_checked_at: null,
  ...fields
})

const received = (id, state, fields = {}) => ({
  request_id: 'a1'.repeat(32),
  upload_id: id.repeat(32),
  state,
  name: `${state}.pdf`,
  mime: 'application/pdf',
  size: 2048,
  sender: { name: 'Ada', note: '' },
  replaces: null,
  sent_at: Date.now(),
  stored_at: Date.now(),
  bytes: 2100,
  received_at: Date.now(),
  withdrawn_at: null,
  saved_file: null,
  ...fields
})

/** The whole page against a stubbed daemon; the main process's own method policy is real. */
export async function fileRequestsFlow({ app, main, shots, methods, hold, waitFor, calls, emit, setSession }) {
  const go = route => main.evaluate(route => (location.hash = route), route)
  const baseUsage = methods['files/cloud_usage']
  const baseList = methods['files/list']
  const baseShow = methods['files/show']
  const baseOpen = methods['files/open']
  const window = await app.browserWindow(main)
  let limit = 0
  let links = []
  let files = []
  let failCreate = null
  let failSave = false
  let failRevoke = false
  let serial = 0

  methods['files/cloud_usage'] = () => ({ ...baseUsage(), storage_limit: 5120 * 1048576, file_request_limit: limit })
  methods['files/requests'] = () => structuredClone(links)
  methods['files/request_create'] = p => {
    if (failCreate?.(p)) throw { code: 'UNAVAILABLE', message: 'Fixture create failure' }
    assert.equal(p.expires_in_ms, 30 * 86400000)
    assert.equal(p.max_total_bytes, 500 * 1048576)
    const made = link((0xb0 + serial++).toString(16), { title: p.title, instructions: p.instructions, label: p.label || null })
    links = [...links, made]
    return structuredClone(made)
  }
  methods['files/request_revoke'] = p => {
    if (failRevoke) throw { code: 'INVALID_INPUT', message: 'Fixture close failure' }
    links = links.map(item => (item.request_id === p.request_id ? { ...item, status: 'revoked', revoked_at: Date.now() } : item))
    return structuredClone(links.find(item => item.request_id === p.request_id))
  }
  methods['files/request_files'] = () => structuredClone(files)
  methods['files/request_save'] = p => {
    if (failSave) throw { code: 'INTEGRITY_FAILED', message: 'Fixture save failure' }
    assert.equal(typeof p.root, 'string')
    files = files.map(file => (file.upload_id === p.upload_id ? { ...file, state: 'saved', saved_file: 'f1'.repeat(32) } : file))
    return { id: 'f1'.repeat(32), name: 'saved.pdf' }
  }
  methods['files/request_delete_file'] = p => {
    files = files.filter(file => file.upload_id !== p.upload_id)
    return null
  }

  // Free: the entry is there and explains itself; no link can be asked for.
  await go('/files')
  await main.getByTestId('files-list').waitFor()
  await main.getByTestId('files-scope-requests').click()
  await main.getByTestId('files-requests-page').waitFor()
  await main.getByTestId('requests-empty').waitFor()
  await waitFor(() => main.getByTestId('requests-refresh').isEnabled())
  await main.getByTestId('requests-new').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('requests-upgrade').waitFor()
  assert.equal(await main.getByTestId('request-composer').count(), 0)
  await main.screenshot({ path: join(shots, 'file-requests-upgrade.png'), animations: 'disabled' })
  await main.keyboard.press('Escape')
  await main.getByTestId('requests-upgrade').waitFor({ state: 'detached' })
  await main.getByTestId('requests-new').click()
  await main.getByTestId('requests-plans').click()
  await waitFor(async () => (await main.evaluate(() => location.hash)).includes('/plan'))

  // Plus: one link per name, and a failure keeps the names still to do.
  limit = 10
  await go('/files/requests')
  await main.getByTestId('files-requests-page').waitFor()
  await main.getByTestId('requests-usage').waitFor()
  assert.match(await main.getByTestId('requests-usage').innerText(), /0 of 10 active links/)
  await main.getByTestId('requests-new').click()
  await main.getByTestId('request-composer').waitFor()
  assert.ok(await main.getByTestId('request-create').isDisabled())
  await main.getByTestId('request-title').fill('Tax documents')
  await main.getByTestId('request-instructions').fill('Send last year’s statements.')
  await main.getByTestId('request-names').fill(Array.from({ length: 51 }, (_, n) => `Client ${n}`).join('\n'))
  assert.ok(await main.getByTestId('request-create').isDisabled())
  await main.getByTestId('request-names').fill('Ada\nLin\n\nMira')
  await main.click('[data-testid="request-days"]')
  await main.click('[data-testid="request-days-menu"] [data-value="30"]')
  assert.match(await main.getByTestId('request-create').innerText(), /3 links/)
  await main.screenshot({ path: join(shots, 'file-requests-composer.png'), animations: 'disabled' })
  failCreate = p => p.label === 'Lin'
  await main.getByTestId('request-create').click()
  await main.getByTestId('request-partial').waitFor()
  assert.equal(await main.getByTestId('request-names').inputValue(), 'Lin\nMira')
  assert.equal(await main.getByTestId('request-created-list').locator('li').count(), 1)
  failCreate = null
  await main.getByTestId('request-create').click()
  await main.getByTestId('request-created').waitFor()
  assert.equal(await main.getByTestId('request-created-list').locator('li').count(), 3)
  await main.evaluate(() => {
    window.__requestWrite = navigator.clipboard.writeText.bind(navigator.clipboard)
    navigator.clipboard.writeText = async value => { window.__requestCopied = value }
  })
  await main.getByTestId('request-copy-all').click()
  await waitFor(async () => (await main.evaluate(() => window.__requestCopied ?? '')).startsWith('Ada,https://dmsg.test/r/'))
  assert.equal((await main.evaluate(() => window.__requestCopied)).split('\n').length, 3)
  await main.screenshot({ path: join(shots, 'file-requests-created.png'), animations: 'disabled' })
  await main.getByTestId('request-done').click()
  await main.getByTestId('request-composer').waitFor({ state: 'detached' })
  await main.getByTestId('requests-open').nth(2).waitFor()

  // A failure with nothing made keeps the whole draft.
  await main.getByTestId('requests-new').click()
  await main.getByTestId('request-composer').waitFor()
  assert.equal(await main.getByTestId('request-title').inputValue(), '')
  await main.click('[data-testid="request-days"]')
  await main.click('[data-testid="request-days-menu"] [data-value="30"]')
  await main.getByTestId('request-title').fill('Kept draft')
  failCreate = () => true
  await main.getByTestId('request-create').click()
  await main.getByTestId('request-partial').waitFor()
  assert.equal(await main.getByTestId('request-title').inputValue(), 'Kept draft')
  failCreate = null
  await main.keyboard.press('Escape')
  await main.getByTestId('request-composer').waitFor({ state: 'detached' })

  // Other library changes re-read what is saved; only a new arrival reads the relay.
  await waitFor(() => main.getByTestId('requests-refresh').isEnabled())
  const reads = refresh => calls.filter(call => call.method === 'files/requests' && !!call.params?.refresh === refresh).length
  const [saved, relay] = [reads(false), reads(true)]
  emit('files/changed', null)
  await waitFor(() => reads(false) > saved)
  assert.equal(reads(true), relay)
  emit('files/changed', { method: 'file-request' })
  await waitFor(() => reads(true) > relay)
  await waitFor(() => main.getByTestId('requests-refresh').isEnabled())

  // A failed read keeps what is on the screen, and states stay in words.
  links = [
    link('a1', { title: 'Tax documents', label: 'Ada', files: 4, bytes: 4 * 1048576, unread: true }),
    link('a2', { title: LONG, label: 'Long client name', status: 'full', files: 12, bytes: 500 * 1048576 }),
    link('a3', { title: 'Paused link', status: 'paused' }),
    link('a4', { title: 'Old link', status: 'expired' }),
    link('a5', { title: 'Closed link', status: 'revoked', revoked_at: Date.now() }),
    link('a6', { title: null, instructions: null, url: null })
  ]
  await main.getByTestId('requests-refresh').click()
  await main.getByTestId('requests-unread').waitFor()
  assert.match(await main.getByTestId('requests-usage').innerText(), /4 of 10 active links/)
  assert.equal(await main.locator('[data-status="revoked"] [data-testid="requests-close"]').count(), 0)
  assert.equal(await main.locator('[data-status="expired"] [data-testid="requests-copy"]').count(), 0)
  await main.getByTestId('requests-search').fill('no such link')
  await main.getByTestId('requests-empty').waitFor()
  await main.getByTestId('requests-search').fill('tax')
  await waitFor(async () => (await main.getByTestId('requests-open').count()) === 1)
  await main.getByTestId('requests-search').fill('')

  // Closing asks first; a failure stays in the dialog with the link still open.
  await main.locator('[data-status="paused"] [data-testid="requests-close"]').click()
  await main.getByTestId('requests-confirm').waitFor()
  failRevoke = true
  await main.getByTestId('requests-confirm-close').click()
  await main.getByTestId('requests-confirm').getByRole('alert').waitFor()
  failRevoke = false
  await main.getByTestId('requests-confirm-close').click()
  await main.getByTestId('requests-confirm').waitFor({ state: 'detached' })
  await main.locator(`[data-request="${'a3'.repeat(32)}"][data-status="revoked"]`).waitFor()

  // What one link received: every state, saved one at a time on request.
  files = [
    received('c1', 'received', { name: 'statement.pdf', sender: { name: 'Ada', note: 'Page 3 is the one you asked for.' } }),
    received('c2', 'saved', { name: LONG_NAME, saved_file: 'f2'.repeat(32), replaces: 'c1'.repeat(32) }),
    received('c3', 'withdrawn', { name: 'withdrawn.pdf', withdrawn_at: Date.now() }),
    received('c4', 'unreadable', { name: null, mime: null, size: null, sender: null })
  ]
  await main.locator(`[data-request="${'a1'.repeat(32)}"] [data-testid="requests-open"]`).focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('request-detail').waitFor()
  await main.getByTestId('request-files').waitFor()
  assert.match(await main.getByTestId('request-sender').first().innerText(), /Ada/)
  await main.getByTestId('request-unreadable').waitFor()
  assert.equal(await main.locator('[data-state="withdrawn"] [data-testid="request-save"]').count(), 0)
  assert.equal(await main.locator('[data-state="withdrawn"] [data-testid="request-delete"]').count(), 0)
  assert.equal(await main.locator('[data-state="unreadable"] [data-testid="request-save"]').count(), 0)
  assert.match(await main.locator('[data-state="saved"]').innerText(), /Replaces statement\.pdf/)
  await main.screenshot({ path: join(shots, 'file-requests-detail.png'), animations: 'disabled' })

  failSave = true
  const uploadRow = main.locator(`[data-upload="${'c1'.repeat(32)}"]`)
  await uploadRow.getByTestId('request-save').click()
  await main.getByTestId('request-detail-error').waitFor()
  assert.ok(await uploadRow.getByTestId('request-save').isEnabled())
  failSave = false

  const finishSave = hold('files/request_save')
  await uploadRow.getByTestId('request-save').click()
  await waitFor(() => uploadRow.getByTestId('request-save').isDisabled())
  emit('files/progress', { job: 'request', path: 'c1'.repeat(32), stage: 'download', done: 1048576, total: 4 * 1048576 })
  await uploadRow.getByTestId('request-progress').waitFor()
  assert.match(await uploadRow.getByTestId('request-progress').innerText(), /Downloading/)
  // Another transfer's progress is not this one's.
  emit('files/progress', { job: 'backup', path: 'c1'.repeat(32), stage: 'save', done: 9, total: 9 })
  assert.match(await uploadRow.getByTestId('request-progress').innerText(), /Downloading/)
  emit('files/progress', { job: 'request', path: 'c1'.repeat(32), stage: 'save', done: 2, total: 4 })
  await waitFor(async () => /Saving/.test(await uploadRow.getByTestId('request-progress').innerText()))
  await finishSave()
  await uploadRow.getByTestId('request-show').waitFor()
  const saveCall = calls.filter(call => call.method === 'files/request_save').at(-1)
  assert.equal(saveCall.params.upload_id, 'c1'.repeat(32))
  assert.equal(saveCall.params.folder, undefined)

  // A cloud copy goes only after asking, and Escape keeps it.
  const unreadableRow = main.locator(`[data-upload="${'c4'.repeat(32)}"]`)
  await unreadableRow.getByTestId('request-delete').click()
  await main.getByTestId('request-delete-confirm').waitFor()
  await main.keyboard.press('Escape')
  await main.getByTestId('request-delete-confirm').waitFor({ state: 'detached' })
  assert.equal(await unreadableRow.count(), 1)
  await unreadableRow.getByTestId('request-delete').click()
  await main.getByTestId('request-delete-yes').click()
  await main.getByTestId('request-delete-confirm').waitFor({ state: 'detached' })
  await unreadableRow.waitFor({ state: 'detached' })

  // A reply to a page already left shows nowhere.
  const lateFiles = hold('files/request_files')
  await main.getByTestId('request-refresh').click()
  await main.getByTestId('request-back').click()
  await main.getByTestId('files-requests-page').waitFor()
  await lateFiles()
  assert.equal(await main.getByTestId('request-detail').count(), 0)

  // Locking clears what this page shows; a reply still on its way does not bring it back.
  await waitFor(() => main.getByTestId('requests-refresh').isEnabled())
  const lateLinks = hold('files/requests', p => p?.refresh === true)
  await main.getByTestId('requests-refresh').click()
  setSession('locked'); emit('session/changed', { state: 'locked' })
  await waitFor(async () => !(await main.getByTestId('requests-new').count()) || main.getByTestId('requests-new').isDisabled())
  await lateLinks()
  assert.equal(await main.getByText('Tax documents', { exact: true }).count(), 0)
  setSession('unlocked'); emit('session/changed', { state: 'unlocked' })
  await main.getByTestId('requests-open').first().waitFor()
  await waitFor(() => main.getByTestId('requests-refresh').isEnabled())

  // Locale, theme and the narrowest window, on the list, the composer and the details.
  files = [
    received('d1', 'received', { name: LONG_NAME, sender: { name: 'عائشة بنت محمد', note: 'الملف الموقع.' } }),
    received('d2', 'saved', { name: 'statement.pdf', saved_file: 'f3'.repeat(32) }),
    received('d3', 'unreadable', { name: null, mime: null, size: null, sender: null })
  ]
  for (const [locale, appearance, width, height] of [
    ['zh', 'light', 1180, 760], ['zh', 'dark', 1180, 760],
    ['zh', 'light', 760, 540], ['zh', 'dark', 760, 540],
    ['de', 'light', 760, 540], ['fil', 'dark', 760, 540],
    ['ar', 'light', 760, 540], ['ar', 'dark', 760, 540], ['ur', 'light', 1180, 760]
  ]) {
    await window.evaluate((window, [w, h]) => window.setContentSize(w, h), [width, height])
    await main.evaluate(async ({ locale, appearance }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, { locale, appearance })
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    const shot = name => main.screenshot({ path: join(shots, `file-requests-${name}-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    assert.ok(await main.getByTestId('files-requests-page').evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await shot('list')
    await main.getByTestId('requests-new').click()
    await main.getByTestId('request-composer').waitFor()
    assert.ok(await main.getByTestId('request-composer').evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await shot('composer')
    await main.keyboard.press('Escape')
    await main.getByTestId('request-composer').waitFor({ state: 'detached' })
    await main.locator(`[data-request="${'a1'.repeat(32)}"] [data-testid="requests-open"]`).click()
    await main.getByTestId('request-files').waitFor()
    assert.ok(await main.getByTestId('request-detail').evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await shot('detail')
    await main.getByTestId('request-back').click()
    await main.getByTestId('files-requests-page').waitFor()
    await waitFor(() => main.getByTestId('requests-refresh').isEnabled())
  }
  await window.evaluate(window => window.setContentSize(1180, 760))
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await main.waitForSelector('html[lang="en"]')

  // A file that would run asks first, in the library, wherever it came from.
  const program = { ...(await baseList({})).files[0], id: 'e1'.repeat(32), name: 'setup.exe' }
  methods['files/list'] = p => (p?.trash ? baseList(p) : { ...baseList(p), files: [program, ...baseList(p).files] })
  methods['files/show'] = p => (p.id === program.id ? { file: program, versions: [] } : baseShow(p))
  methods['files/open'] = () => ({ path: '/nonexistent/dmsg-smoke/setup.exe' })
  await go('/files')
  await main.getByTestId('files-list').waitFor()
  await main.getByText('setup.exe', { exact: true }).first().dblclick()
  await main.getByTestId('files-open-program').waitFor()
  await main.screenshot({ path: join(shots, 'file-requests-program.png'), animations: 'disabled' })
  await main.keyboard.press('Escape')
  await main.getByTestId('files-open-program').waitFor({ state: 'detached' })
  assert.equal(calls.some(call => call.method === 'files/open' && call.params.id === program.id), false)
  await main.getByText('setup.exe', { exact: true }).first().dblclick()
  await main.getByTestId('files-open-program-yes').click()
  await waitFor(() => calls.some(call => call.method === 'files/open' && call.params.id === program.id))

  await main.evaluate(() => { navigator.clipboard.writeText = window.__requestWrite })
  methods['files/cloud_usage'] = baseUsage
  methods['files/list'] = baseList
  methods['files/show'] = baseShow
  if (baseOpen) methods['files/open'] = baseOpen
  else delete methods['files/open']
}
