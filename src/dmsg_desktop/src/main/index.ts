/**
 * dMsg desktop, main process. It owns the daemon connection and the owner
 * credential; windows are sandboxed renderers that reach the daemon only
 * through `policy.ts`. The approval window is the only place a request is
 * decided; `dmsg://` links and notifications only open views. A packaged
 * app installs its bundled `dmsg` first (`runtime.ts`) and updates itself
 * from the tray once the daemon stopped for it (`updater.ts`).
 */
import {
  app,
  BrowserWindow,
  clipboard,
  dialog,
  ipcMain,
  Menu,
  nativeImage,
  Notification,
  shell,
  Tray,
  type IpcMainInvokeEvent
} from 'electron'
import { readFileSync } from 'node:fs'
import { readFile, writeFile } from 'node:fs/promises'
import { homedir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import type { DaemonEvent } from '../shared/contract'
import { isLocale, resolveLocale, type Locale } from '../shared/locales'
import { Daemon, DaemonError, reasonOf } from './daemon'
import { externalAllowed, legacyAllowed, parseLink, windowParams } from './policy'
import { resolveRuntime } from './runtime'
import { textFor } from './text'
import { Updates } from './updater'

const testMode = !app.isPackaged && process.env.DMSG_DESKTOP_TEST === '1'
if (testMode) {
  if (!process.env.DMSG_DESKTOP_USER_DATA) throw new Error('Desktop tests need an isolated profile')
  app.setPath('userData', process.env.DMSG_DESKTOP_USER_DATA)
}
const homeArg = process.argv.find((a) => a.startsWith('--dmsg-home='))?.slice(12)
const home = resolve(homeArg || process.env.DMSG_HOME || join(homedir(), '.dmsg'))
const daemon = new Daemon({
  binary: process.env.DMSG_BIN || 'dmsg',
  home,
  fixed:
    testMode && process.env.DMSG_DESKTOP_DAEMON
      ? { url: process.env.DMSG_DESKTOP_DAEMON, token: process.env.DMSG_DESKTOP_TOKEN ?? '' }
      : undefined
})

app.setName('dMsg')
if (!app.requestSingleInstanceLock()) app.quit()

let main: BrowserWindow | null = null
let approval: BrowserWindow | null = null
let tray: Tray | null = null
let trayImages: { idle: Electron.NativeImage; waiting: Electron.NativeImage } | null = null
let quitting = false
let waiting = 0
let pendingRoute: string | null = null
let trayState = 'connecting'
/** English until the app is ready and knows its locale. */
let locale: Locale = 'en'
let text = textFor(locale)
const updates = new Updates(() => updateTray(trayState))
/** A copied secret leaves the clipboard after this long, unless something replaced it. */
const CLEAR_MS = 30_000

/** Login and pairing pages the desktop may open, from the home's configuration. */
function pages(): { login: string[]; pairing: string[] } {
  let text = ''
  try {
    text = readFileSync(join(home, 'config.toml'), 'utf8')
  } catch {
    /* defaults only */
  }
  const value = (name: string) => text.match(new RegExp(`^${name}\\s*=\\s*"([^"]+)"`, 'm'))?.[1]
  return {
    login: ['https://dmsg.net/_/login', value('login_url')].filter((v): v is string => !!v),
    pairing: ['https://connect.dmsg.net', value('connect_url')?.replace(/\/$/, '')].filter((v): v is string => !!v)
  }
}

function load(window: BrowserWindow, route: string) {
  if (process.env.ELECTRON_RENDERER_URL) void window.loadURL(`${process.env.ELECTRON_RENDERER_URL}#${route}`)
  else void window.loadFile(join(__dirname, '../renderer/index.html'), { hash: route })
}

function harden(window: BrowserWindow) {
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
  window.webContents.on('will-navigate', (event) => event.preventDefault())
}

/** The language the person picked in the main window, kept in the app's profile. */
const localeFile = () => join(app.getPath('userData'), 'locale')

function applyLocale(next: Locale) {
  locale = next
  text = textFor(next)
  updateTray(trayState)
}

/** A window learns its role and the current language from its arguments. */
function webPreferences(role: 'main' | 'approval') {
  return {
    preload: join(__dirname, '../preload/index.js'),
    contextIsolation: true,
    sandbox: true,
    nodeIntegration: false,
    webSecurity: true,
    additionalArguments: [`--dmsg-role=${role}`, `--dmsg-locale=${locale}`]
  }
}

function showMain(route = '/vault') {
  if (main && !main.isDestroyed()) {
    main.webContents.send('dmsg:navigate', route)
    main.show()
    main.focus()
    return
  }
  main = new BrowserWindow({
    width: 1080,
    height: 720,
    minWidth: 720,
    minHeight: 520,
    title: 'dMsg',
    show: !testMode || process.env.DMSG_DESKTOP_SHOW === '1',
    webPreferences: webPreferences('main')
  })
  harden(main)
  load(main, route)
  main.on('close', (event) => {
    // Closing keeps the tray and the daemon; quitting is explicit.
    if (!quitting && !testMode) {
      event.preventDefault()
      main?.hide()
    }
  })
  main.on('closed', () => (main = null))
}

/** The approval window shows one request from the daemon; it takes no outside parameters but the ID. */
function showApproval(id: string) {
  if (!/^[0-9a-f]{64}$/.test(id)) return
  if (approval && !approval.isDestroyed()) {
    approval.webContents.send('dmsg:navigate', `/approval/${id}`)
    approval.show()
    approval.focus()
    return
  }
  approval = new BrowserWindow({
    width: 480,
    height: 680,
    resizable: true,
    title: 'dMsg approval',
    show: !testMode || process.env.DMSG_DESKTOP_SHOW === '1',
    webPreferences: webPreferences('approval')
  })
  harden(approval)
  load(approval, `/approval/${id}`)
  approval.on('closed', () => (approval = null))
}

/** A `dmsg://` link moves the view; it executes nothing (connect-v1 §8). */
async function openLink(url: string) {
  const link = parseLink(url)
  if (!link) return
  if (link.route === 'browsers') return showMain('/settings/browsers')
  // The request ID in a link is only a hint: find the daemon's request for
  // it, by the relay request a page links or by the approval's own ID.
  try {
    await daemon.call('connect/sync')
    const list = (await daemon.call('approval/list', {})) as { id: string; detail?: { requests?: string[] } }[]
    const found = list.find((r) => r.id === link.request || !!r.detail?.requests?.includes(link.request))
    if (found) return showApproval(found.id)
  } catch {
    /* locked or offline: show the main window */
  }
  showMain('/approvals')
}

function trust(event: IpcMainInvokeEvent, role: 'main' | 'approval' | 'any') {
  const sender = event.sender
  const isMain = !!main && !main.isDestroyed() && sender === main.webContents
  const isApproval = !!approval && !approval.isDestroyed() && sender === approval.webContents
  const ok = role === 'any' ? isMain || isApproval : role === 'main' ? isMain : isApproval
  if (!ok) throw new DaemonError('FORBIDDEN', 'Not allowed from this window')
}

async function guard<T>(work: () => Promise<T>): Promise<T> {
  try {
    return await work()
  } catch (error) {
    const code = error instanceof DaemonError ? error.code : 'UNAVAILABLE'
    throw new Error(`${code}: ${error instanceof Error ? error.message : String(error)}`)
  }
}

ipcMain.handle('dmsg:call', (event, method: string, params: unknown) =>
  guard(async () => {
    trust(event, 'any')
    return daemon.call(method, windowParams(method, params))
  })
)
ipcMain.handle('dmsg:decide', (event, id: string, digest: string, approve: boolean) =>
  guard(async () => {
    trust(event, 'approval')
    return daemon.call('approval/respond', { id, digest, approve: approve === true, human: true, source: 'gui' })
  })
)
ipcMain.handle('dmsg:grant', (event, invite: string, pairing: string) =>
  guard(async () => {
    trust(event, 'main')
    return daemon.call('connect/grant', { invite, pairing, human: true })
  })
)
ipcMain.handle('dmsg:login', (event, origin: string, bind: boolean) =>
  guard(async () => {
    trust(event, 'main')
    const started = (await daemon.call('session/login', { origin, bind: bind === true, source: 'gui' })) as {
      url: string
    }
    if (externalAllowed(started.url, pages().login, [])) await shell.openExternal(started.url)
    return started
  })
)
/** Files reach the daemon only by paths the person picked in a system dialog. */
ipcMain.handle('dmsg:import-file', (event) =>
  guard(async () => {
    trust(event, 'main')
    const picked = await dialog.showOpenDialog(main!, { properties: ['openFile'] })
    if (picked.canceled || !picked.filePaths[0]) return null
    return daemon.call('file/import', { path: picked.filePaths[0] })
  })
)
ipcMain.handle('dmsg:export-file', (event, id: string, name: string) =>
  guard(async () => {
    trust(event, 'main')
    const picked = await dialog.showSaveDialog(main!, { defaultPath: basename(String(name)) })
    if (picked.canceled || !picked.filePath) return null
    return daemon.call('file/export', { id: String(id), path: picked.filePath })
  })
)
ipcMain.handle('dmsg:open-legacy', (event, link: string) =>
  guard(async () => {
    trust(event, 'main')
    if (!legacyAllowed(String(link))) throw new DaemonError('INVALID_INPUT', 'Not an old-site migration link')
    await shell.openExternal(String(link))
  })
)
ipcMain.handle('dmsg:legacy-import', (event, nonce: string, fingerprint: string, principal: string) =>
  guard(async () => {
    trust(event, 'main')
    const picked = await dialog.showOpenDialog(main!, {
      properties: ['openFile'],
      filters: [
        { name: text.migrationFile, extensions: ['dmsg-migration'] },
        { name: text.allFiles, extensions: ['*'] }
      ]
    })
    if (picked.canceled || !picked.filePaths[0]) return null
    return daemon.call('legacy/import', {
      path: picked.filePaths[0],
      nonce: String(nonce),
      fingerprint: String(fingerprint),
      principal: String(principal)
    })
  })
)
ipcMain.handle('dmsg:legacy-frozen', (event, id: string) =>
  guard(async () => {
    trust(event, 'main')
    const picked = await dialog.showOpenDialog(main!, {
      properties: ['openFile'],
      filters: [
        { name: text.frozenFile, extensions: ['cbor'] },
        { name: text.allFiles, extensions: ['*'] }
      ]
    })
    if (picked.canceled || !picked.filePaths[0]) return null
    return daemon.call('legacy/frozen', { id: String(id), path: picked.filePaths[0] })
  })
)
ipcMain.handle('dmsg:legacy-file', (event, id: string, source: string, name: string) =>
  guard(async () => {
    trust(event, 'main')
    const picked = await dialog.showSaveDialog(main!, { defaultPath: basename(String(name)) })
    if (picked.canceled || !picked.filePath) return null
    return daemon.call('legacy/file', { id: String(id), source: String(source), path: picked.filePath })
  })
)
/** The secret goes from the daemon to the clipboard; the window that asked never holds it. */
ipcMain.handle('dmsg:copy-secret', (event, id: string) =>
  guard(async () => {
    trust(event, 'main')
    const entry = (await daemon.call('vault/show', { id: String(id), reveal: true })) as { item: { secret: string } }
    const secret = entry.item.secret
    if (!secret) throw new DaemonError('NOT_FOUND', 'No secret field')
    await clipboard.writeText(secret)
    setTimeout(async () => {
      if ((await clipboard.readText()) === secret) clipboard.clear()
    }, CLEAR_MS)
    return { clear_after_ms: CLEAR_MS }
  })
)
ipcMain.handle('dmsg:channel-download', (event, id: string, seq: number, name: string) =>
  guard(async () => {
    trust(event, 'main')
    const picked = await dialog.showSaveDialog(main!, { defaultPath: basename(String(name)) })
    if (picked.canceled || !picked.filePath) return null
    return daemon.call('channel/download', { id: String(id), seq: Number(seq), path: picked.filePath })
  })
)
/** The public avatar: an image the person picks, or none. */
ipcMain.handle('dmsg:avatar', (event, remove: boolean) =>
  guard(async () => {
    trust(event, 'main')
    if (remove === true) return daemon.call('profile/avatar', { remove: true })
    const picked = await dialog.showOpenDialog(main!, {
      title: text.pickAvatar,
      properties: ['openFile'],
      filters: [{ name: text.imageFile, extensions: ['png', 'jpg', 'jpeg', 'webp'] }]
    })
    if (picked.canceled || !picked.filePaths[0]) return null
    return daemon.call('profile/avatar', { path: picked.filePaths[0] })
  })
)
/** An application action its product prepared, as the canonical CBOR in a file the person picks. */
ipcMain.handle('dmsg:attest-action', (event) =>
  guard(async () => {
    trust(event, 'main')
    const picked = await dialog.showOpenDialog(main!, { title: text.pickAction, properties: ['openFile'] })
    if (picked.canceled || !picked.filePaths[0]) return null
    const action = (await readFile(picked.filePaths[0])).toString('base64url')
    return daemon.call('attest/action', { action, source: 'gui' })
  })
)
/** Verify a bundle the person picks, with the original file of a digest or file statement when asked. */
ipcMain.handle('dmsg:verify-bundle', (event, withFile: boolean) =>
  guard(async () => {
    trust(event, 'main')
    const bundle = await dialog.showOpenDialog(main!, {
      title: text.pickBundle,
      properties: ['openFile'],
      filters: [
        { name: text.bundleFile, extensions: ['json'] },
        { name: text.allFiles, extensions: ['*'] }
      ]
    })
    if (bundle.canceled || !bundle.filePaths[0]) return null
    let parsed: unknown
    try {
      parsed = JSON.parse(await readFile(bundle.filePaths[0], 'utf8'))
    } catch {
      throw new DaemonError('INVALID_INPUT', 'Not an attestation bundle')
    }
    let file: string | undefined
    if (withFile === true) {
      const original = await dialog.showOpenDialog(main!, { title: text.pickOriginal, properties: ['openFile'] })
      if (original.canceled || !original.filePaths[0]) return null
      file = original.filePaths[0]
    }
    return daemon.call('attest/verify', { bundle: parsed, file })
  })
)
ipcMain.handle('dmsg:export-bundle', (event, execution: string) =>
  guard(async () => {
    trust(event, 'main')
    const bundle = await daemon.call('attest/export', { execution: String(execution) })
    const picked = await dialog.showSaveDialog(main!, {
      defaultPath: `dmsg-attestation-${String(execution).slice(0, 12)}.json`,
      filters: [{ name: text.bundleFile, extensions: ['json'] }]
    })
    if (picked.canceled || !picked.filePath) return null
    await writeFile(picked.filePath, JSON.stringify(bundle, null, 2))
    return { path: picked.filePath }
  })
)
ipcMain.handle('dmsg:pair', (event, copyLink: boolean) =>
  guard(async () => {
    trust(event, 'main')
    const invite = (await daemon.call('connect/invite')) as { url: string }
    if (!copyLink && externalAllowed(invite.url, [], pages().pairing)) await shell.openExternal(invite.url)
    return invite
  })
)
ipcMain.handle('dmsg:open-approval', (event, id: string) =>
  guard(async () => {
    trust(event, 'main')
    showApproval(id)
  })
)
/** A language picked in the main window is kept; the tray, dialogs and every window follow it. */
ipcMain.handle('dmsg:set-locale', (event, next: string) =>
  guard(async () => {
    trust(event, 'main')
    if (!isLocale(next)) throw new DaemonError('INVALID_INPUT', 'Not a supported language')
    await writeFile(localeFile(), next)
    applyLocale(next)
    broadcast('dmsg:locale', next)
  })
)
ipcMain.handle('dmsg:connection', (event) =>
  guard(async () => {
    trust(event, 'any')
    return { connected: daemon.connected, error: daemon.lastError }
  })
)

function broadcast(channel: string, payload: unknown) {
  for (const window of [main, approval]) if (window && !window.isDestroyed()) window.webContents.send(channel, payload)
}

/** Notify without saying from where; clicking opens the request, never decides it. */
async function refreshWaiting(hint = false) {
  let status: { approvals_waiting?: number; state?: string } = {}
  try {
    status = (await daemon.call('daemon/status')) as typeof status
  } catch {
    return
  }
  const now = status.approvals_waiting ?? 0
  if ((now > waiting || hint) && Notification.isSupported()) {
    const note = new Notification({ title: 'dMsg', body: text.requestWaiting, silent: false })
    note.on('click', async () => {
      try {
        const list = (await daemon.call('approval/list', {})) as { id: string; state: string }[]
        const first = list.find((r) => r.state === 'awaiting_user')
        if (first) return showApproval(first.id)
      } catch {
        /* locked */
      }
      showMain('/approvals')
    })
    note.show()
  }
  waiting = now
  updateTray(status.state ?? 'unknown')
}

/** The tray's update item: nothing without a feed, else the feed's state. */
function updateItem(): Electron.MenuItemConstructorOptions[] {
  const state = updates.state
  switch (state.kind) {
    case 'off':
      return []
    case 'ready':
      return [
        {
          label: text.restartToUpdate(state.version),
          click: () =>
            void updates.apply(
              (method, params) => daemon.call(method, params),
              () => {
                quitting = true
                daemon.close()
              },
              text
            )
        }
      ]
    case 'checking':
    case 'downloading':
      return [{ label: state.kind === 'checking' ? text.checking : text.downloading, enabled: false }]
    default:
      return [{ label: text.checkUpdates, click: () => void updates.check() }]
  }
}

/**
 * The tray shows the Gate: empty, or with its diamond while a request waits.
 * macOS tints template images for the menu bar; elsewhere the mark keeps its
 * tile so it shows on dark and light taskbars.
 */
function trayImage(waitingNow: boolean) {
  const mac = process.platform === 'darwin'
  const name = `tray${waitingNow ? 'Waiting' : ''}${mac ? 'Template' : ''}.png`
  const image = nativeImage.createFromPath(join(__dirname, '../../resources', name))
  if (mac) image.setTemplateImage(true)
  return image
}

function updateTray(state: string) {
  trayState = state
  if (!tray) return
  if (trayImages) tray.setImage(waiting ? trayImages.waiting : trayImages.idle)
  tray.setToolTip(text.tooltip(app.getVersion(), text.state(state), waiting))
  tray.setContextMenu(
    Menu.buildFromTemplate([
      { label: `dMsg: ${text.state(state)}`, enabled: false },
      { label: text.waiting(waiting), click: () => showMain('/approvals') },
      { type: 'separator' },
      { label: text.open, click: () => showMain() },
      { label: text.lock, click: () => void daemon.call('session/lock').catch(() => {}) },
      { type: 'separator' },
      ...updateItem(),
      {
        label: text.openAtLogin,
        type: 'checkbox',
        checked: app.getLoginItemSettings().openAtLogin,
        click: (item) => app.setLoginItemSettings({ openAtLogin: item.checked, args: ['--hidden'] })
      },
      { type: 'separator' },
      {
        label: text.quit,
        click: () => {
          quitting = true
          app.quit()
        }
      }
    ])
  )
}

/** Agents' MCP Events: how many arrived or need renewing, never from where or what. */
function notifyEvents(data: unknown) {
  const { stored = 0, refresh_due = 0 } = (data ?? {}) as { stored?: number; refresh_due?: number }
  if (!(stored > 0 || refresh_due > 0) || !Notification.isSupported()) return
  const note = new Notification({ title: 'dMsg', body: stored > 0 ? text.eventsArrived(stored) : text.refreshDue })
  note.on('click', () => showMain('/agents'))
  note.show()
}

/** When each kind of hint last notified, so a busy channel notifies once a minute. */
const hinted: Record<string, number> = {}

/** A locked daemon's hint: something new arrived, never from whom or what. */
function notifyHint(data: unknown) {
  const kind = (data as { kind?: string } | null)?.kind
  const body = kind === 'channel' ? text.newMessages : kind === 'inbox' ? text.newLetter : null
  if (!kind || !body || !Notification.isSupported() || Date.now() - (hinted[kind] ?? 0) < 60_000) return
  hinted[kind] = Date.now()
  const note = new Notification({ title: 'dMsg', body })
  note.on('click', () => showMain(kind === 'channel' ? '/channels' : '/inbox'))
  note.show()
}

daemon.on('event', (event: DaemonEvent) => {
  broadcast('dmsg:event', event)
  if (event.type === 'approval/changed' || event.type === 'session/changed') void refreshWaiting()
  if (event.type === 'connect/hint') void refreshWaiting(true)
  if (event.type === 'mcp-events/changed') notifyEvents(event.data)
  if (event.type === 'hint') notifyHint(event.data)
})
daemon.on('state', () => broadcast('dmsg:event', { type: 'connection/changed' }))

app.on('second-instance', (_event, argv) => {
  const link = argv.find((a) => a.startsWith('dmsg://'))
  if (link) void openLink(link)
  else showMain()
})
app.on('open-url', (event, url) => {
  event.preventDefault()
  if (app.isReady()) void openLink(url)
  else pendingRoute = url
})
app.on('web-contents-created', (_event, contents) => {
  contents.on('will-attach-webview', (event) => event.preventDefault())
})
app.on('before-quit', () => {
  quitting = true
  daemon.close()
})
app.on('window-all-closed', () => {
  if (testMode || quitting) app.quit()
})

app.whenReady().then(async () => {
  let stored: string | null = null
  try {
    stored = readFileSync(localeFile(), 'utf8').trim()
  } catch {
    /* no choice yet */
  }
  applyLocale(resolveLocale(stored, [...app.getPreferredSystemLanguages(), app.getLocale()]))
  if (!testMode) {
    if (process.defaultApp && process.argv[1]) app.setAsDefaultProtocolClient('dmsg', process.execPath, [resolve(process.argv[1])])
    else app.setAsDefaultProtocolClient('dmsg')
    trayImages = { idle: trayImage(false), waiting: trayImage(true) }
    tray = new Tray(trayImages.idle)
    updateTray('connecting')
  }
  // Opened at login, the app starts in the tray.
  if (!process.argv.includes('--hidden') && !app.getLoginItemSettings().wasOpenedAtLogin) showMain()
  let installed = true
  if (!daemon.options.fixed) {
    try {
      daemon.options.binary = (await resolveRuntime(app.isPackaged, process.resourcesPath)).binary
    } catch (error) {
      // Without the shared `dmsg` nothing can start the daemon; the next app start installs again.
      installed = false
      daemon.lastError = `dmsg install failed: ${reasonOf(error)}`
      daemon.emit('state')
    }
  }
  updates.start()
  if (installed) await daemon.connect()
  await refreshWaiting()
  const link = pendingRoute ?? process.argv.find((a) => a.startsWith('dmsg://'))
  if (link) void openLink(link)
})
