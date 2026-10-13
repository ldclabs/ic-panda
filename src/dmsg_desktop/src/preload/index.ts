import { contextBridge, ipcRenderer } from 'electron'
import type { DaemonEvent, DesktopBridge } from '../shared/contract'

const unwrap = <T>(promise: Promise<T>) =>
  promise.catch((error: Error) => {
    // "CODE: message" from the main process, without Electron's prefix.
    throw new Error(String(error.message).replace(/^Error invoking remote method '[^']+': (Error: )?/, ''))
  })

const role = process.argv.includes('--dmsg-role=approval') ? 'approval' : 'main'

const api: DesktopBridge = {
  role,
  call: (method, params) => unwrap(ipcRenderer.invoke('dmsg:call', method, params ?? null)),
  decide: (id, digest, approve) => unwrap(ipcRenderer.invoke('dmsg:decide', id, digest, approve)),
  grant: (invite, pairing) => unwrap(ipcRenderer.invoke('dmsg:grant', invite, pairing)),
  login: (origin, bind) => unwrap(ipcRenderer.invoke('dmsg:login', origin, bind === true)),
  importFile: () => unwrap(ipcRenderer.invoke('dmsg:import-file')),
  exportFile: (id, name) => unwrap(ipcRenderer.invoke('dmsg:export-file', id, name)),
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
  }
}
contextBridge.exposeInMainWorld('dmsg', api)
