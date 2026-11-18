import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { preferencesWriter } from '../src/main/preferences.ts'

test('overlapping edits preserve each setting and reload the saved values in order', async t => {
  const home = await mkdtemp(join(tmpdir(), 'dmsg-preferences-'))
  t.after(() => rm(home, { recursive: true, force: true }))
  const path = join(home, 'preferences.json')
  const read = async () => JSON.parse(await readFile(path, 'utf8'))
  let confirmations = 0
  const save = preferencesWriter(home, async () => { confirmations++; return true }, read)
  const [mounted, locked] = await Promise.all([save({ files_mount: true, files_trash_days: 7 }), save({ idle_lock_minutes: 15 })])
  assert.deepEqual(mounted, { files_mount: true, files_trash_days: 7 })
  assert.deepEqual(locked, { files_mount: true, files_trash_days: 7, idle_lock_minutes: 15 })
  assert.deepEqual(await read(), locked)
  await save({ files_mount: true })
  assert.equal(confirmations, 1)
})

test('cancelling a mount and refusing unreadable settings preserve the previous file', async t => {
  const home = await mkdtemp(join(tmpdir(), 'dmsg-preferences-'))
  t.after(() => rm(home, { recursive: true, force: true }))
  const path = join(home, 'preferences.json')
  let reloads = 0
  const save = preferencesWriter(home, async () => false, async () => ++reloads)
  await writeFile(path, '{"idle_lock_minutes":5}')
  assert.equal(await save({ files_mount: true, files_trash_days: 7 }), null)
  assert.equal(await readFile(path, 'utf8'), '{"idle_lock_minutes":5}')
  await writeFile(path, '{"idle_lock_minutes":')
  await assert.rejects(save({ files_trash_days: 7 }), { code: 'LOCAL_STORAGE_UNAVAILABLE' })
  assert.equal(await readFile(path, 'utf8'), '{"idle_lock_minutes":')
  assert.equal(reloads, 0)
  await writeFile(path, '{}')
  await assert.rejects(save({ idle_lock_minutes: 0 }), { code: 'INVALID_INPUT' })
  assert.equal(await save({ idle_lock_minutes: 30 }), 1)
})
