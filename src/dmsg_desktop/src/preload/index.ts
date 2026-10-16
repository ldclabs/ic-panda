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
  importFile: (dropped) => {
    if (dropped === undefined) return unwrap(ipcRenderer.invoke('dmsg:import-file'))
    // Only a file the person dropped or picked has a path; one a page made has none.
    let path = ''
    try {
      path = webUtils.getPathForFile(dropped)
    } catch {
      /* not a file */
    }
    return path ? unwrap(ipcRenderer.invoke('dmsg:import-file', path)) : Promise.reject(new Error('INVALID_INPUT: Not a file on this device'))
  },
  exportFile: (id, name) => unwrap(ipcRenderer.invoke('dmsg:export-file', id, name)),
  avatar: (remove) => unwrap(ipcRenderer.invoke('dmsg:avatar', remove === true)),
  copySecret: (id) => unwrap(ipcRenderer.invoke('dmsg:copy-secret', id)),
  channelDownload: (id, seq, name) => unwrap(ipcRenderer.invoke('dmsg:channel-download', id, seq, name)),
  attestAction: () => unwrap(ipcRenderer.invoke('dmsg:attest-action')),
  verifyBundle: (withFile) => unwrap(ipcRenderer.invoke('dmsg:verify-bundle', withFile === true)),
  exportBundle: (execution) => unwrap(ipcRenderer.invoke('dmsg:export-bundle', execution)),
  openLegacy: (link) => unwrap(ipcRenderer.invoke('dmsg:open-legacy', link)),
  legacyImport: (nonce, fingerprint, principal) =>
    unwrap(ipcRenderer.invoke('dmsg:legacy-import', nonce, fingerprint, principal)),
  legacyFrozen: (id) => unwrap(ipcRenderer.invoke('dmsg:legacy-frozen', id)),
  legacyFile: (id, source, name) => unwrap(ipcRenderer.invoke('dmsg:legacy-file', id, source, name)),
  pair: (copyLink) => unwrap(ipcRenderer.invoke('dmsg:pair', copyLink)),
  openApproval: (id) => unwrap(ipcRenderer.invoke('dmsg:open-approval', id)),
  connection: () => unwrap(ipcRenderer.invoke('dmsg:connection')),
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
  }
}
contextBridge.exposeInMainWorld('dmsg', api)
