/**
 * Updates of a package from the GitHub release feed (electron-updater). A
 * release package carries the feed (electron-builder.release.cjs): its update
 * downloads in the background and is applied only on request, after the
 * daemon stopped for it (`update.ts`); the new app then installs and starts
 * its own `dmsg`. A package without the feed (a local one, or a macOS release
 * built without the Developer ID) cannot install a signed release, so its
 * check only finds the update, and the release page has it.
 */
import { app, dialog, shell } from 'electron'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { autoUpdater } from 'electron-updater'
import type { UpdateState } from '../shared/contract'
import type { Text } from './text'
import { describe, stopForUpdate } from './update'

const CHECK_MS = 6 * 60 * 60 * 1000

/** The feed release packages carry as `app-update.yml`; dmsg-v* releases are the repository's latest release. */
const FEED = { provider: 'github', owner: 'ldclabs', repo: 'ic-panda', releaseType: 'release' } as const

/** The release page of a version, which has its packages. */
const releasePage = (version: string) => `https://github.com/ldclabs/ic-panda/releases/tag/dmsg-v${encodeURIComponent(version)}`

export class Updates {
  state: UpdateState = { kind: 'off' }
  private changed: () => void

  constructor(changed: () => void) {
    this.changed = changed
  }

  /** Check now and every six hours in a packaged app; only a package with the feed downloads. */
  start() {
    if (!app.isPackaged) return
    const installs = existsSync(join(process.resourcesPath, 'app-update.yml'))
    if (!installs) autoUpdater.setFeedURL(FEED)
    autoUpdater.autoDownload = installs
    autoUpdater.autoInstallOnAppQuit = false
    autoUpdater.on('checking-for-update', () => this.set({ kind: 'checking' }))
    autoUpdater.on('update-not-available', () => this.set({ kind: 'latest' }))
    autoUpdater.on('update-available', (info) => this.set({ kind: installs ? 'downloading' : 'available', version: info.version }))
    autoUpdater.on('update-downloaded', (info) => this.set({ kind: 'ready', version: info.version }))
    // The first line only: an HTTP error goes on with the request and the response.
    autoUpdater.on('error', (error) => this.set({ kind: 'error', message: error.message.split('\n')[0] }))
    this.set({ kind: 'idle' })
    void this.check()
    setInterval(() => void this.check(), CHECK_MS).unref()
  }

  /** Check now, unless updates are off or one is downloading or downloaded; resolves with where the update then stands. */
  async check(): Promise<UpdateState> {
    if (!['off', 'downloading', 'ready'].includes(this.state.kind)) {
      try {
        await autoUpdater.checkForUpdates()
      } catch {
        /* reported through the error event */
      }
    }
    return this.state
  }

  /**
   * Open the release page of an update this package cannot install; for a
   * downloaded one, stop the daemon once idle, then quit and install,
   * otherwise say what holds it.
   */
  async apply(call: (method: string, params: unknown) => Promise<unknown>, quit: () => void, text: Text) {
    const state = this.state
    if (state.kind === 'available') return shell.openExternal(releasePage(state.version))
    if (state.kind !== 'ready') return
    const stopped = await stopForUpdate(call)
    if (!stopped.stopped) {
      await dialog.showMessageBox({
        type: 'info',
        message: text.updateLater,
        detail: text.updateBusy(describe(stopped.inFlight, text))
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
