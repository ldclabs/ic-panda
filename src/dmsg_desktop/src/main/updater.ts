/** GitHub release selection and a local channel preference; electron-updater owns verified downloads and installation. */
import { app, dialog, net, shell } from 'electron'
import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { autoUpdater, CancellationToken } from 'electron-updater'
import type { AppUpdates, UpdateChannel, UpdateState } from '../shared/contract'
import type { Text } from './text'
import { ReleaseFeed, RELEASES, type DesktopRelease } from './release-feed'
import { UpdateSession } from './update-session'
import { describe, stopForUpdate } from './update'

const CHECK_MS = 6 * 60 * 60 * 1000

export class Updates {
  private session?: UpdateSession
  private homebrew = false
  private resumeInstallation?: () => Promise<void>

  constructor(private changed: () => void) {}

  get state(): UpdateState {
    return this.session?.state ?? { kind: 'off' }
  }

  info(): AppUpdates {
    return { version: app.getVersion(), state: this.state, channel: this.session?.channel ?? 'stable', homebrew: this.homebrew, applying: this.session?.applying ?? false }
  }

  private saveChannel(channel: UpdateChannel) {
    const file = join(app.getPath('userData'), 'update-channel')
    writeFileSync(`${file}.tmp`, channel, { mode: 0o600 })
    renameSync(`${file}.tmp`, file)
  }

  setChannel(channel: UpdateChannel): AppUpdates {
    if (channel !== 'stable' && channel !== 'beta') throw new Error('Invalid update channel.')
    if (!this.session || this.session.applying) throw new Error('The update channel cannot change during installation.')
    if (channel === 'beta' && this.homebrew) throw new Error('Homebrew manages this daemon. Use the standard dMsg installation to receive beta updates.')
    this.saveChannel(channel)
    this.session.changeChannel(channel)
    return this.info()
  }

  /** The preference is loaded even in development; network checks require a packaged app. */
  start(homebrew = false) {
    this.homebrew = homebrew
    let channel: UpdateChannel = 'stable'
    try {
      if (readFileSync(join(app.getPath('userData'), 'update-channel'), 'utf8').trim() === 'beta' && !homebrew) channel = 'beta'
    } catch { /* A missing or unreadable preference never opts into beta. */ }
    const installs = existsSync(join(process.resourcesPath, 'app-update.yml')) && !homebrew
    // A prerelease build does not imply consent. Only this session starts downloads.
    autoUpdater.autoDownload = false
    autoUpdater.autoInstallOnAppQuit = false
    autoUpdater.allowDowngrade = false
    // The awaited operations report failures for their own generation; late events cannot change the UI.
    autoUpdater.on('error', (error) => {
      // macOS verifies with Squirrel only after quitAndInstall; a failure leaves the old app running.
      const resume = this.resumeInstallation
      if (resume) {
        this.resumeInstallation = undefined
        this.session?.installFailed(error)
        void resume()
      }
    })
    const feed = new ReleaseFeed((url, options) => net.fetch(url, options))
    this.session = new UpdateSession({
      enabled: app.isPackaged,
      channel,
      changed: this.changed,
      find: (selected, signal) => feed.latest(selected, app.getVersion(), process.platform, process.arch, signal),
      download: installs ? (release, signal) => this.download(release, signal) : undefined
    })
    this.changed()
    if (!app.isPackaged) return
    void this.check()
    setInterval(() => void this.check(), CHECK_MS).unref()
  }

  private async download(release: DesktopRelease, signal: AbortSignal) {
    signal.throwIfAborted()
    const channel = release.channel === 'beta' ? 'beta' : 'latest'
    autoUpdater.setFeedURL({ provider: 'generic', url: `${RELEASES}/download/${release.version}/`, channel, useMultipleRangeRequest: false })
    autoUpdater.channel = channel
    autoUpdater.allowPrerelease = release.channel === 'beta'
    // Setting channel turns this on in electron-updater; undo it on every check.
    autoUpdater.allowDowngrade = false
    const result = await autoUpdater.checkForUpdates()
    signal.throwIfAborted()
    if (!result?.isUpdateAvailable || result.updateInfo.version !== release.version ||
        !result.updateInfo.files.some((file) => file.url === release.installer) ||
        !result.updateInfo.files.every((file) => release.assets.includes(file.url))) {
      throw new Error('The update manifest does not match the selected dMsg release.')
    }
    const token = result.cancellationToken ?? new CancellationToken()
    const cancel = () => token.cancel()
    signal.addEventListener('abort', cancel, { once: true })
    try {
      await autoUpdater.downloadUpdate(token)
    } finally {
      signal.removeEventListener('abort', cancel)
    }
  }

  check(): Promise<UpdateState> {
    return this.session?.check() ?? Promise.resolve(this.state)
  }

  async apply(call: (method: string, params: unknown) => Promise<unknown>, quit: () => Promise<void>, resume: () => Promise<void>, text: Text) {
    if (this.homebrew) throw new Error('Update this installation with brew upgrade dmsg dmsg-desktop.')
    const release = this.session?.beginApply()
    if (!release) return
    let installing = false
    let prepared = false
    try {
      if (this.state.kind === 'available') {
        await shell.openExternal(`${RELEASES}/tag/${release.version}`)
        return
      }
      const stopped = await stopForUpdate(call, async (inFlight) => {
        const { response } = await dialog.showMessageBox({
          type: 'warning',
          message: text.updateBlocked,
          detail: text.updateBusy(describe(inFlight, text)),
          buttons: [text.later, text.restartToUpdate(release.version)],
          defaultId: 0,
          cancelId: 0,
          noLink: true
        })
        return response === 1
      })
      if (!stopped) return
      prepared = true
      await quit()
      this.resumeInstallation = resume
      autoUpdater.quitAndInstall()
      installing = true
    } catch (error) {
      this.resumeInstallation = undefined
      if (prepared) await resume()
      // The tray has no page to render a rejected promise. Both entry points
      // show the failure and keep the downloaded release available to retry.
      await dialog.showMessageBox({ type: 'error', message: text.updateFailed, detail: error instanceof Error ? error.message : String(error) })
    } finally {
      if (!installing) this.session?.endApply()
    }
  }
}
