import { contextBridge, ipcRenderer, webUtils } from 'electron'
import { isAppearance } from '../shared/appearance'
import type { DaemonEvent, DesktopBridge } from '../shared/contract'
import { defaultLocale, isLocale, type Locale } from '../shared/locales'

const unwrap = <T>(promise: Promise<T>) =>
  promise.catch((error: Error) => {
    // "CODE: message" from the main process, without Electron's prefix.
    throw new Error(String(error.message).replace(/^Error invoking remote method '[^']+': (Error: )?/, ''))
  })

const role = process.argv.includes('--dmsg-role=approval') ? 'approval' : 'main'
const locale = process.argv.find((a) => a.startsWith('--dmsg-locale='))?.slice('--dmsg-locale='.length)
const appearance = process.argv.find((a) => a.startsWith('--dmsg-appearance='))?.slice('--dmsg-appearance='.length)

const api: DesktopBridge = {
  role,
  platform: process.platform,
  locale: isLocale(locale) ? locale : defaultLocale,
  setLocale: (next) => unwrap(ipcRenderer.invoke('dmsg:set-locale', next)),
  appearance: isAppearance(appearance) ? appearance : 'system',
  setAppearance: (next) => unwrap(ipcRenderer.invoke('dmsg:set-appearance', next)),
  call: (method, params) => unwrap(ipcRenderer.invoke('dmsg:call', method, params ?? null)),
  decide: (id, digest, approve) => unwrap(ipcRenderer.invoke('dmsg:decide', id, digest, approve)),
  grant: (invite, pairing) => unwrap(ipcRenderer.invoke('dmsg:grant', invite, pairing)),
  login: (origin, bind) => unwrap(ipcRenderer.invoke('dmsg:login', origin, bind === true)),
  openLogin: (url) => unwrap(ipcRenderer.invoke('dmsg:open-login', url)),
  runtime: () => unwrap(ipcRenderer.invoke('dmsg:runtime')),
  filesAdd: (folder, dropped, directory) => {
    if (dropped === undefined) return unwrap(ipcRenderer.invoke('dmsg:files-add', folder, null, directory === true))
    // Only a file the person dropped or picked has a path; one a page made has none.
    const paths: string[] = []
    for (const file of dropped) {
      try {
        const path = webUtils.getPathForFile(file)
        if (path) paths.push(path)
      } catch {
        /* not a file */
      }
    }
    return paths.length === dropped.length && paths.length
      ? unwrap(ipcRenderer.invoke('dmsg:files-add', folder, paths, false))
      : Promise.reject(new Error('INVALID_INPUT: Not files on this device'))
  },
  filesExport: (id, version, name) => unwrap(ipcRenderer.invoke('dmsg:files-export', id, version, name)),
  filesExportMany: (items) => unwrap(ipcRenderer.invoke('dmsg:files-export-many', items)),
  filesPreferences: (changes) => unwrap(ipcRenderer.invoke('dmsg:files-preferences', changes)),
  filesOpen: (id, version) => unwrap(ipcRenderer.invoke('dmsg:files-open', id, version)),
  avatar: () => unwrap(ipcRenderer.invoke('dmsg:avatar')),
  payerPem: (login) => unwrap(ipcRenderer.invoke('dmsg:payer-pem', login === true)),
  copySecret: (id, field, revision) => unwrap(ipcRenderer.invoke('dmsg:copy-secret', id, field ?? null, revision ?? null)),
  secretScanDirectory: () => unwrap(ipcRenderer.invoke('dmsg:secret-scan')),
  secretMountPrepare: (input, windowMs) => unwrap(ipcRenderer.invoke('dmsg:secret-mount-prepare', input, windowMs)),
  secretMountSave: (session) => unwrap(ipcRenderer.invoke('dmsg:secret-mount-save', session)),
  openExtensionSettings: () => unwrap(ipcRenderer.invoke('dmsg:open-extension-settings')),
  vaultImportPreview: () => unwrap(ipcRenderer.invoke('dmsg:vault-import-preview')),
  vaultImport: (session, picks, attachments = [], folder = 'root') => unwrap(ipcRenderer.invoke('dmsg:vault-import', session, picks, attachments, folder)),
  vaultImportCancel: () => unwrap(ipcRenderer.invoke('dmsg:vault-import-cancel')),
  vaultOtpScan: (source) => unwrap(ipcRenderer.invoke('dmsg:vault-otp-scan', source)),
  openUrl: (url) => unwrap(ipcRenderer.invoke('dmsg:open-url', url)),
  channelDownload: (id, seq, name) => unwrap(ipcRenderer.invoke('dmsg:channel-download', id, seq, name)),
  attestAction: () => unwrap(ipcRenderer.invoke('dmsg:attest-action')),
  attestFile: (statement) => unwrap(ipcRenderer.invoke('dmsg:attest-file', statement)),
  checkFile: (id) => unwrap(ipcRenderer.invoke('dmsg:check-file', id)),
  showMain: (route) => unwrap(ipcRenderer.invoke('dmsg:show-main', route)),
  verifyBundle: (withFile) => unwrap(ipcRenderer.invoke('dmsg:verify-bundle', withFile === true)),
  verifyDropped: (bundle, original) => {
    // Only files the person dropped have paths; one a page made has none.
    const path = (file?: File) => {
      try {
        return file ? webUtils.getPathForFile(file) || null : null
      } catch {
        return null
      }
    }
    const bundlePath = path(bundle)
    if (!bundlePath || (original && !path(original))) return Promise.reject(new Error('INVALID_INPUT: Not files on this device'))
    return unwrap(ipcRenderer.invoke('dmsg:verify-dropped', bundlePath, path(original)))
  },
  verifyEntry: (execution) => unwrap(ipcRenderer.invoke('dmsg:verify-entry', execution)),
  saveReport: (report) => unwrap(ipcRenderer.invoke('dmsg:save-report', report)),
  exportBundle: (execution) => unwrap(ipcRenderer.invoke('dmsg:export-bundle', execution)),
  openLegacy: (link) => unwrap(ipcRenderer.invoke('dmsg:open-legacy', link)),
  legacyImport: (nonce, fingerprint, principal) =>
    unwrap(ipcRenderer.invoke('dmsg:legacy-import', nonce, fingerprint, principal)),
  legacyFrozen: (id) => unwrap(ipcRenderer.invoke('dmsg:legacy-frozen', id)),
  legacyFile: (id, source, name) => unwrap(ipcRenderer.invoke('dmsg:legacy-file', id, source, name)),
  pair: (copyLink) => unwrap(ipcRenderer.invoke('dmsg:pair', copyLink)),
  openApproval: (id) => unwrap(ipcRenderer.invoke('dmsg:open-approval', id)),
  connection: () => unwrap(ipcRenderer.invoke('dmsg:connection')),
  updates: () => unwrap(ipcRenderer.invoke('dmsg:updates')),
  checkUpdates: () => unwrap(ipcRenderer.invoke('dmsg:check-updates')),
  applyUpdate: () => unwrap(ipcRenderer.invoke('dmsg:apply-update')),
  onEvent: (listener) => {
    const handler = (_event: Electron.IpcRendererEvent, data: DaemonEvent) => listener(data)
    ipcRenderer.on('dmsg:event', handler)
    return () => ipcRenderer.removeListener('dmsg:event', handler)
  },
  onNavigate: (listener) => {
    const handler = (_event: Electron.IpcRendererEvent, route: string) => listener(route)
    ipcRenderer.on('dmsg:navigate', handler)
    return () => ipcRenderer.removeListener('dmsg:navigate', handler)
  },
  onLocale: (listener) => {
    const handler = (_event: Electron.IpcRendererEvent, next: Locale) => listener(next)
    ipcRenderer.on('dmsg:locale', handler)
    return () => ipcRenderer.removeListener('dmsg:locale', handler)
  },
  onFullScreen: (listener) => {
    const handler = (_event: Electron.IpcRendererEvent, fullScreen: boolean) => listener(fullScreen === true)
    ipcRenderer.on('dmsg:fullscreen', handler)
    // A reloaded window asks where it stands; the events tell each change.
    ipcRenderer.invoke('dmsg:fullscreen').then(listener, () => {})
    return () => ipcRenderer.removeListener('dmsg:fullscreen', handler)
  }
}
contextBridge.exposeInMainWorld('dmsg', api)
