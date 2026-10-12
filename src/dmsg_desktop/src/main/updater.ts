/**
 * Updates of a release package from its GitHub release feed (electron-updater;
 * only packages built with electron-builder.release.cjs carry the feed). An
 * update downloads in the background and is applied only from the tray, after
 * the daemon stopped for it (`update.ts`); the new app then installs and
 * starts its own `dmsg`.
 */
import { app, dialog } from 'electron'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { autoUpdater } from 'electron-updater'
import { stopForUpdate } from './update'

const CHECK_MS = 6 * 60 * 60 * 1000

export type UpdateState =
  | { kind: 'off' }
  | { kind: 'idle' | 'checking' | 'downloading' }
  | { kind: 'ready'; version: string }
  | { kind: 'error'; message: string }

export class Updates {
  state: UpdateState = { kind: 'off' }
  private changed: () => void

  constructor(changed: () => void) {
    this.changed = changed
  }

  /** Check now and every six hours, when this package has a feed. */
  start() {
    if (!app.isPackaged || !existsSync(join(process.resourcesPath, 'app-update.yml'))) return
    autoUpdater.autoDownload = true
    autoUpdater.autoInstallOnAppQuit = false
    autoUpdater.on('checking-for-update', () => this.set({ kind: 'checking' }))
    autoUpdater.on('update-not-available', () => this.set({ kind: 'idle' }))
    autoUpdater.on('download-progress', () => this.state.kind !== 'downloading' && this.set({ kind: 'downloading' }))
    autoUpdater.on('update-downloaded', (info) => this.set({ kind: 'ready', version: info.version }))
    autoUpdater.on('error', (error) => this.set({ kind: 'error', message: error.message }))
    this.set({ kind: 'idle' })
    void this.check()
    setInterval(() => void this.check(), CHECK_MS).unref()
  }

  async check() {
    if (this.state.kind === 'off' || this.state.kind === 'ready') return
    try {
      await autoUpdater.checkForUpdates()
    } catch {
      /* reported through the error event */
    }
  }

  /** Stop the daemon once idle, then quit and install; otherwise say what holds it. */
  async apply(call: (method: string, params: unknown) => Promise<unknown>, quit: () => void) {
    if (this.state.kind !== 'ready') return
    const stopped = await stopForUpdate(call)
    if (!stopped.stopped) {
      await dialog.showMessageBox({
        type: 'info',
        message: 'dMsg will update when its work in flight settles',
        detail: `Still in flight: ${stopped.busy}. Unlock dMsg so it can finish, then choose Restart to Update again.`
      })
      return
    }
    quit()
    autoUpdater.quitAndInstall()
  }

  private set(state: UpdateState) {
    this.state = state
    this.changed()
  }
}
