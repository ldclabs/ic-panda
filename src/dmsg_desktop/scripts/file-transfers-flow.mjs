import assert from 'node:assert/strict'

/** Real Electron renderer/main IPC; the relay fixture controls task outcomes. */
export async function fileTransfersFlow({ app, main, shots, methods, emit, calls, hold, waitFor, formMatrix, setSession }) {
  const file = '23'.repeat(32), upload = '72'.repeat(32), download = '73'.repeat(32)
  const total = 150 * 1024 * 1024
  let backups = [{ file, version: 'v1', upload_id: upload, size: total, sent: 4, chunks: 150, stage: 'uploading', bytes: 4 * 1024 * 1024, total_bytes: total, created_at: Date.now(), error: null }]
  let downloads = []
  methods['files/backup_status'] = () => structuredClone(backups)
  methods['files/download_status'] = () => structuredClone(downloads)
  methods['files/backup_retry'] = () => { backups[0].stage = 'uploading'; backups[0].error = null; backups[0].retry_at = null; emit('files/changed'); return { queued: true } }
  methods['files/download_retry'] = () => { downloads[0].stage = 'queued'; downloads[0].error = null; emit('files/changed'); return { queued: true } }
  methods['files/download_cancel'] = () => { downloads = []; emit('files/changed'); return { cancelled: true } }
  methods['files/cloud_list'] = () => [{ upload_id: download, name: 'Large encrypted backup · 很长的文件名 · rapport confidentiel de plusieurs équipes.bin', size: total, bytes: total, device: '31'.repeat(32), committed_at: Date.now(), local: false, needs_root: false }]
  methods['files/cloud_status'] = () => ({ complete: true })
  methods['files/cloud_refresh'] = () => ({ cached: true })
  methods['files/cloud_fetch'] = p => {
    assert.equal(p.background, true)
    if (!downloads.length) downloads.push({ upload_id: download, verify: false, stage: 'queued', done: 0, total: 150, bytes: 0, total_bytes: total, error: null })
    emit('files/changed'); return downloads[0]
  }
  await main.evaluate(() => location.hash = '/files')
  await main.getByTestId('files-list').waitFor()
  const backup = main.locator('[data-testid="files-job"][data-kind="backup"]')
  await backup.waitFor()
  backups[0] = { ...backups[0], sent: 150, bytes: total, stage: 'committing' }
  emit('files/progress', { ...backups[0], job: 'backup', path: file, done: 150, total: 150 })
  emit('files/changed')
  await main.waitForFunction(() => document.querySelector('[data-kind="backup"][data-testid="files-job"]')?.dataset.stage === 'committing')
  assert.equal(await backup.count(), 1)
  assert.equal(await main.locator('[data-kind="backup"][data-stage="done"]').count(), 0)
  await formMatrix(app, main, shots, 'transfer-commit', '[data-testid="files-job"][data-kind="backup"]')
  backups[0] = { ...backups[0], error: 'UNAVAILABLE', stage: 'retrying', retry_at: Date.now() + 60_000 }
  emit('files/changed')
  await main.waitForFunction(() => document.querySelector('[data-kind="backup"][data-testid="files-job"]')?.dataset.stage === 'retrying')
  const retry = backup.getByRole('button', { name: 'Retry', exact: true })
  await retry.focus(); await main.keyboard.press('Enter')
  await waitFor(() => calls.some(c => c.method === 'files/backup_retry'))
  await main.waitForFunction(() => document.querySelector('[data-kind="backup"][data-testid="files-job"]')?.dataset.stage === 'running')
  assert.equal(await backup.count(), 1)
  backups = []; emit('files/changed')
  await main.waitForFunction(() => document.querySelector('[data-kind="backup"][data-testid="files-job"]')?.dataset.stage === 'done')

  await main.evaluate(() => location.hash = '/files/cloud')
  await main.getByTestId('files-cloud-fetch').waitFor()
  await main.getByTestId('files-cloud-fetch').click()
  await waitFor(() => downloads.length === 1)
  downloads[0] = { ...downloads[0], stage: 'verifying', done: 150, bytes: total }
  emit('files/progress', { ...downloads[0], job: 'fetch', path: download })
  emit('files/changed')
  const fetch = main.locator('[data-testid="files-job"][data-kind="fetch"]')
  await fetch.waitFor()
  await main.waitForFunction(() => document.querySelector('[data-kind="fetch"][data-testid="files-job"]')?.dataset.stage === 'verifying')
  await main.waitForTimeout(6500)
  await formMatrix(app, main, shots, 'transfer-verify', '[data-testid="files-job"][data-kind="fetch"]')
  assert.ok(await fetch.evaluate(el => { const r = el.getBoundingClientRect(); return r.x >= 0 && r.right <= innerWidth }))
  downloads[0].stage = 'failed'; downloads[0].error = 'INTEGRITY_FAILED'; emit('files/changed')
  await main.waitForFunction(() => document.querySelector('[data-kind="fetch"][data-testid="files-job"]')?.dataset.stage === 'error')
  assert.equal(await main.locator('[data-kind="fetch"][data-stage="done"]').count(), 0)
  await fetch.getByRole('button', { name: 'Retry', exact: true }).click()
  await waitFor(() => calls.some(c => c.method === 'files/download_retry'))
  await fetch.getByTestId('files-job-cancel').click()
  await fetch.waitFor({ state: 'detached' })

  // Synchronous CLI downloads have no persisted status row. Their terminal
  // events must finish the same card without offering a queue-only retry.
  const synchronous = { job: 'fetch', path: '74'.repeat(32), done: 1, total: 1 }
  emit('files/progress', { ...synchronous, stage: 'verifying' })
  await fetch.waitFor()
  emit('files/progress', { ...synchronous, stage: 'failed', error: 'INTEGRITY_FAILED' })
  await main.waitForFunction(() => document.querySelector('[data-kind="fetch"][data-testid="files-job"]')?.dataset.stage === 'error')
  assert.equal(await fetch.getByRole('button', { name: 'Retry', exact: true }).count(), 0)
  await formMatrix(app, main, shots, 'transfer-cli-error', '[data-testid="files-job"][data-kind="fetch"]')
  emit('files/progress', { ...synchronous, stage: 'verifying' })
  emit('files/progress', { ...synchronous, stage: 'done' })
  await main.waitForFunction(() => document.querySelector('[data-kind="fetch"][data-testid="files-job"]')?.dataset.stage === 'done')
  assert.equal(await fetch.count(), 1)

  // A backup too large for a link cannot start one; a smaller one shares
  // without joining the library, and the dialog follows the daemon's stages.
  const shared = '75'.repeat(32)
  const listed = methods['files/cloud_list']()
  methods['files/cloud_list'] = () => [...listed, { upload_id: shared, file_id: '76'.repeat(32), version_id: 'v2', name: 'Only in the cloud.pdf', size: 4 * 1024 * 1024, bytes: 4 * 1024 * 1024 + 4096, device: '31'.repeat(32), committed_at: Date.now(), local: false, needs_root: false }]
  emit('files/changed')
  const row = main.locator(`[data-upload="${shared}"]`)
  await row.waitFor()
  assert.ok(await main.locator(`[data-upload="${download}"]`).getByTestId('files-cloud-link').isDisabled())
  await row.getByTestId('files-cloud-link').click()
  const linkDialog = main.getByTestId('files-link-dialog')
  await linkDialog.getByTestId('files-link-remote').waitFor()
  await main.waitForSelector('[data-testid="files-link-create"]:enabled')
  assert.ok(calls.some((c) => c.method === 'files/share_quote' && c.params.upload_id === shared))
  const finishLink = hold('files/share_link')
  await main.getByTestId('files-link-create').click()
  await linkDialog.getByText('Encrypting a new copy…').waitFor()
  emit('files/progress', { job: 'share', path: shared, stage: 'downloading', done: 2, total: 4 })
  await linkDialog.getByText('Downloading from the cloud…').waitFor()
  await formMatrix(app, main, shots, 'link-from-cloud', '[data-testid="files-link-dialog"]')
  emit('files/progress', { job: 'share', path: shared, stage: 'uploading', done: 1, total: 5 })
  await linkDialog.getByText('Uploading the encrypted copy…').waitFor()
  assert.equal(await main.locator('[data-testid="files-job"][data-kind="share"]').count(), 0)
  // The fixture's first link fails: the dialog keeps the draft to try again.
  await finishLink()
  await linkDialog.getByRole('alert').waitFor()
  assert.equal(await linkDialog.getByTestId('files-link-progress').count(), 0)
  await main.getByTestId('files-link-create').click()
  await main.getByTestId('files-link-url').waitFor()
  const created = calls.findLast((c) => c.method === 'files/share_link')
  assert.equal(created.params.upload_id, shared)
  assert.equal(created.params.id, undefined)
  await linkDialog.getByRole('button', { name: 'Done', exact: true }).click()
  await linkDialog.waitFor({ state: 'detached' })

  setSession('locked'); emit('session/changed')
  await main.getByTestId('files-locked').waitFor()
  assert.match(await main.locator('main').innerText().catch(() => main.locator('body').innerText()), /Locking pauses transfers/)
}
