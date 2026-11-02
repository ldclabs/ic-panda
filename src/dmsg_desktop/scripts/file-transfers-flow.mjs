import assert from 'node:assert/strict'

/** Real Electron renderer/main IPC; the relay fixture controls task outcomes. */
export async function fileTransfersFlow({ app, main, shots, methods, emit, calls, waitFor, formMatrix, setSession }) {
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
  setSession('locked'); emit('session/changed')
  await main.getByTestId('files-locked').waitFor()
  assert.match(await main.locator('main').innerText().catch(() => main.locator('body').innerText()), /Locking pauses transfers/)
}
