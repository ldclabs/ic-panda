import { mkdir, readFile, rename, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { DesktopBridge } from '../shared/contract'
import { DaemonError } from './error.ts'

type Preferences = Parameters<DesktopBridge['filesPreferences']>[0]

/** One read, edit and daemon reload at a time, including the native mount confirmation. */
export function preferencesWriter(home: string, confirmMount: () => Promise<boolean>, reload: () => Promise<unknown>) {
  const path = join(home, 'preferences.json')
  let pending = Promise.resolve()

  async function save(changes: Preferences) {
    let next: Preferences = { files_mount: false }
    try {
      const saved = JSON.parse(await readFile(path, 'utf8'))
      if (!saved || typeof saved !== 'object' || Array.isArray(saved)) throw new Error('Invalid preferences')
      next = { ...next, ...saved }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw new DaemonError('LOCAL_STORAGE_UNAVAILABLE', 'Could not read preferences')
    }

    if (changes?.files_trash_days !== undefined) {
      const days = Number(changes.files_trash_days)
      if (!Number.isInteger(days) || days < 1 || days > 365) throw new DaemonError('INVALID_INPUT', 'files_trash_days')
      next.files_trash_days = days
    }
    if (changes?.idle_lock_minutes !== undefined) {
      const minutes = Number(changes.idle_lock_minutes)
      if (!Number.isInteger(minutes) || minutes < 1 || minutes > 24 * 60) throw new DaemonError('INVALID_INPUT', 'idle_lock_minutes')
      next.idle_lock_minutes = minutes
    }
    if (typeof changes?.files_mount === 'boolean') {
      if (changes.files_mount && next.files_mount !== true && !await confirmMount()) return null
      next.files_mount = changes.files_mount
    }
    try {
      await mkdir(home, { recursive: true })
      await writeFile(`${path}.tmp`, JSON.stringify(next, null, 2) + '\n', { mode: 0o600 })
      await rename(`${path}.tmp`, path)
    } catch {
      throw new DaemonError('LOCAL_STORAGE_UNAVAILABLE', 'Could not save preferences')
    }
    return reload()
  }

  return (changes: Preferences) => {
    const result = pending.then(() => save(changes))
    pending = result.then(() => {}, () => {})
    return result
  }
}
