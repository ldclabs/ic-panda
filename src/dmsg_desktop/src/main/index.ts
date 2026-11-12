/**
 * dMsg desktop, main process. It owns the daemon connection and the owner
 * credential; windows are sandboxed renderers that reach the daemon only
 * through `policy.ts`. The approval window is the only place a request is
 * decided; `dmsg://` links and notifications only open views. A packaged
 * app installs its bundled `dmsg` first (`runtime.ts`) and checks for its
 * updates; the tray and Settings › About dMsg apply one once the daemon
 * stopped for it (`updater.ts`).
 */
import {
  app,
  BrowserWindow,
  clipboard,
  ClipboardItem,
  dialog,
  ipcMain,
  Menu,
  nativeImage,
  nativeTheme,
  net,
  Notification,
  powerMonitor,
  screen,
  shell,
  session as electronSession,
  systemPreferences,
  Tray,
  type IpcMainInvokeEvent
} from 'electron'
import { readFileSync, rmSync } from 'node:fs'
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises'
import { homedir, tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { isAppearance, type Appearance } from '../shared/appearance'
import type { DaemonEvent } from '../shared/contract'
import { isLocale, resolveLocale, type Locale } from '../shared/locales'
import { single } from '../shared/single'
import { walletHistoryCsv } from '../shared/wallet-report'
import { Daemon, reasonOf } from './daemon'
import { DaemonError } from './error'
import { PreparedOperation } from './prepared-operation'
import { APPROVAL_MAIN_ROUTES, externalAllowed, legacyAllowed, parseLink, windowParams } from './policy'
import { requireRuntimeVersion, resolveRuntime } from './runtime'
import { Notifications } from './notifications'
import { isNoticeAccount, isNoticeHash, isNoticeSequence, isNoticeTarget, noticeCategories, noticeRoute, type NoticeCategory, type NoticeBatch, type NoticeItem, type NoticePreferences, type NoticeStatus, type NoticeTarget } from '../shared/notifications'
import { quietUntil, validTimeZone } from '../shared/notification-time'
import { notificationBody, notificationText } from '../shared/notification-text'
import type { NoticeVisibility } from '../shared/contract'
import { textFor } from './text'
import { boundsOnScreen, isViewRoute, readDesktopState, type DesktopState } from './desktop-state'
import { trayVariant } from './tray-state'
import { Updates } from './updater'
import { handleBrokenOutput } from './stdio'

handleBrokenOutput(process.stdout)
handleBrokenOutput(process.stderr)

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
/** The main window, closed in macOS full screen, hides once it left full screen; showing it cancels that. */
let hideAfterFullScreen = false
let approval: BrowserWindow | null = null
let tray: Tray | null = null
let trayImages: Record<ReturnType<typeof trayVariant>, Electron.NativeImage> | null = null
let quitting = false
let waiting = 0
let noticeEpoch = 0
let noticeHistoryLoaded = false
let noticeLegacyAttempted = false
let noticeActivation: string | null = null
let noticeLastResult: 'submitted' | 'shown' | 'failed' | undefined
let screenLocked = false
// Only the version explicitly opened may be acknowledged by its visible destination.
const noticeReads = new Map<number, { account: string; session: string; id: string; version: string }>()
const noticeViewports = new Map<number, { id: string; version: string; stage: number }>()
const noticeViewEpoch = new Map<number, number>()
let pendingLink: string | null = null
let trayState = 'connecting'
/** Window bounds, the last view and the desktop's own choices, kept in the app's profile. */
let desktop: DesktopState = { lockOnScreenLock: true }
const keepsDesktopState = !testMode || process.env.DMSG_DESKTOP_TEST_STATE === '1'
const desktopFile = () => join(app.getPath('userData'), 'desktop.json')
let desktopTimer: ReturnType<typeof setTimeout> | undefined

function keepDesktop(change: Partial<DesktopState>, now = false) {
  desktop = { ...desktop, ...change }
  clearTimeout(desktopTimer)
  if (!keepsDesktopState) return
  const save = () => void writeFile(desktopFile(), JSON.stringify(desktop)).catch(() => {})
  if (now) save()
  else desktopTimer = setTimeout(save, 500)
}
/** English until the app is ready and knows its locale. */
let locale: Locale = 'en'
let text = textFor(locale)
let appearance: Appearance = 'system'
const updates = new Updates(() => {
  updateTray(trayState)
  updateMenu()
  broadcast('dmsg:event', { type: 'updates/changed' })
})
/** A copied secret leaves the clipboard after this long, unless something replaced it; locking or quitting clears it sooner. */
const CLEAR_MS = 30_000
/** A person's input in a window is the idle lock's activity, reported at most this often. */
const ACTIVITY_MS = 10_000
const INPUT = new Set(['mouseDown', 'mouseWheel', 'rawKeyDown', 'keyDown', 'touchStart'])
let reported = 0

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
  // Process arguments keep the opening language even after a window reloads.
  // Replay the current choice once its renderer has installed the listeners.
  window.webContents.on('did-finish-load', () => window.webContents.send('dmsg:locale', locale))
  if (process.env.ELECTRON_RENDERER_URL) void window.loadURL(`${process.env.ELECTRON_RENDERER_URL}#${route}`)
  else void window.loadFile(join(__dirname, '../renderer/index.html'), { hash: route })
}

/** Right-click in text: spelling suggestions, then cut, copy, paste and select all in the window's language. */
function textMenu(window: BrowserWindow, params: Electron.ContextMenuParams) {
  const { editFlags, isEditable, selectionText, misspelledWord, dictionarySuggestions } = params
  const items: Electron.MenuItemConstructorOptions[] = []
  if (isEditable && misspelledWord) {
    for (const word of dictionarySuggestions.slice(0, 5)) items.push({ label: word, click: () => window.webContents.replaceMisspelling(word) })
    if (dictionarySuggestions.length) items.push({ type: 'separator' })
  }
  if (isEditable) {
    items.push(
      { role: 'cut', label: text.cut, enabled: editFlags.canCut },
      { role: 'copy', label: text.copy, enabled: editFlags.canCopy },
      { role: 'paste', label: text.paste, enabled: editFlags.canPaste },
      { type: 'separator' },
      { role: 'selectAll', label: text.selectAll, enabled: editFlags.canSelectAll }
    )
  } else if (selectionText.trim()) items.push({ role: 'copy', label: text.copy, enabled: editFlags.canCopy })
  if (items.length) Menu.buildFromTemplate(items).popup({ window })
}

function harden(window: BrowserWindow) {
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
  window.webContents.on('will-navigate', (event) => event.preventDefault())
  window.webContents.on('context-menu', (_event, params) => textMenu(window, params))
  const clearViewport = () => {
    const id = windowId
    noticeViewports.delete(id)
    noticeViewEpoch.set(id, (noticeViewEpoch.get(id) ?? 0) + 1)
    noticeEpoch++
    void refreshNotices()
  }
  const windowId = window.webContents.id
  window.on('closed', () => { noticeReads.delete(windowId); noticeViewports.delete(windowId); noticeViewEpoch.delete(windowId) })
  window.on('blur', clearViewport)
  window.on('hide', clearViewport)
  window.on('minimize', clearViewport)
  window.on('focus', () => {
    noticeEpoch++
    window.webContents.send('dmsg:event', { type: 'notifications/focus' })
    void refreshNotices()
  })
  window.on('show', () => void refreshNotices())
  window.webContents.on('did-navigate-in-page', clearViewport)
}

/** The language the person picked in the main window, kept in the app's profile. */
const localeFile = () => join(app.getPath('userData'), 'locale')

function applyLocale(next: Locale) {
  locale = next
  text = textFor(next)
  updateTray(trayState)
  updateMenu()
}

/** The appearance the person picked in the main window, kept in the app's profile. */
const appearanceFile = () => join(app.getPath('userData'), 'appearance')

function applyAppearance(next: Appearance) {
  appearance = next
  nativeTheme.themeSource = next
}

/** A window learns its role, the current language and appearance from its arguments. */
function webPreferences(role: 'main' | 'approval') {
  return {
    preload: join(__dirname, '../preload/index.js'),
    contextIsolation: true,
    sandbox: true,
    nodeIntegration: false,
    webSecurity: true,
    devTools: !app.isPackaged,
    additionalArguments: [`--dmsg-role=${role}`, `--dmsg-locale=${locale}`, `--dmsg-appearance=${appearance}`]
  }
}

/** The page ground of the current theme, shown before the renderer paints. */
const background = () => (nativeTheme.shouldUseDarkColors ? '#0e1512' : '#f7f8f4')

/** Show the main window; with a route, on that view, else on the one it showed. */
function showMain(route?: string) {
  if (main && !main.isDestroyed()) {
    hideAfterFullScreen = false
    if (route) main.webContents.send('dmsg:navigate', route)
    if (main.isMinimized()) main.restore()
    // A hidden window's page loses first responder, and macOS hands it back
    // on show as if Tab were pressed: the first control, the sidebar toggle,
    // took focus with its ring and tooltip. Focusing the page first keeps
    // whatever it had focused.
    main.webContents.focus()
    main.show()
    main.focus()
    return
  }
  const kept = boundsOnScreen(desktop.bounds, screen.getAllDisplays().map((display) => display.workArea))
  const window = new BrowserWindow({
    width: kept?.width ?? 1180,
    height: kept?.height ?? 760,
    ...(kept ? { x: kept.x, y: kept.y } : {}),
    minWidth: 760,
    minHeight: 540,
    title: 'dMsg',
    autoHideMenuBar: true,
    // On macOS the sidebar sits on the window's own material; the traffic
    // lights sit in the 60px title bar, centred on its line.
    ...(process.platform === 'darwin'
      ? { titleBarStyle: 'hidden', trafficLightPosition: { x: 18, y: 24 }, vibrancy: 'sidebar', backgroundColor: '#00000000' }
      : { backgroundColor: background() }),
    show: false,
    webPreferences: webPreferences('main')
  })
  main = window
  harden(window)
  // Shown once its first frame is ready, in the place it had, never a blank window.
  window.once('ready-to-show', () => {
    if (desktop.maximized && kept) window.maximize()
    if (!testMode || process.env.DMSG_DESKTOP_SHOW === '1') window.show()
  })
  const rememberBounds = () => {
    if (!window.isDestroyed() && !window.isFullScreen() && !window.isMinimized())
      keepDesktop({ bounds: window.isMaximized() ? desktop.bounds : window.getBounds(), maximized: window.isMaximized() })
  }
  window.on('resize', rememberBounds)
  window.on('move', rememberBounds)
  window.webContents.on('did-navigate-in-page', (_event, url) => {
    const shown = new URL(url).hash.slice(1)
    if (isViewRoute(shown)) keepDesktop({ route: shown })
  })
  load(window, route ?? (isViewRoute(desktop.route) ? desktop.route : '/channels'))
  let leavingFullScreen = false
  // Full screen hides the traffic lights; the title bar takes back their room.
  window.on('enter-full-screen', () => window.webContents.send('dmsg:fullscreen', true))
  window.on('leave-full-screen', () => {
    window.webContents.send('dmsg:fullscreen', false)
    leavingFullScreen = false
    if (!hideAfterFullScreen) return
    hideAfterFullScreen = false
    if (!quitting) window.hide()
  })
  window.on('close', (event) => {
    // Closing keeps the tray and the daemon; quitting is explicit.
    if (quitting) return
    event.preventDefault()
    closingHint()
    if (process.platform !== 'darwin' || !(leavingFullScreen || window.isFullScreen())) return window.hide()
    // Hiding a window in full screen leaves its Space behind, black. It hides
    // once the asynchronous exit finished, unless it is shown again first.
    hideAfterFullScreen = true
    if (leavingFullScreen) return
    leavingFullScreen = true
    window.setFullScreen(false)
  })
  window.on('closed', () => {
    main = null
    hideAfterFullScreen = false
  })
}

/** The first time a window closes, say once where dMsg went on running. */
function closingHint() {
  if (desktop.closeHintShown || !keepsDesktopState) return
  keepDesktop({ closeHintShown: true }, true)
  if (Notification.isSupported()) new Notification({ title: 'dMsg', body: process.platform === 'darwin' ? text.stillRunningMenuBar : text.stillRunningTray, silent: true }).show()
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
    width: 460,
    height: 720,
    minWidth: 400,
    minHeight: 560,
    resizable: true,
    title: 'dMsg approval',
    ...(process.platform === 'darwin' ? { titleBarStyle: 'hiddenInset' as const, trafficLightPosition: { x: 12, y: 11 } } : {}),
    backgroundColor: background(),
    autoHideMenuBar: true,
    show: false,
    webPreferences: webPreferences('approval')
  })
  harden(approval)
  const opened = approval
  opened.once('ready-to-show', () => {
    if (!testMode || process.env.DMSG_DESKTOP_SHOW === '1') opened.show()
  })
  load(approval, `/approval/${id}`)
  approval.on('closed', () => (approval = null))
}

/** A `dmsg://` link moves the view; it executes nothing (connect-v1 §8). */
async function openLink(url: string) {
  const link = parseLink(url)
  if (!link) return
  if (link.route === 'browsers') return showMain('/settings/browsers')
  if (link.route === 'join') return showMain(`/channels?join=${link.invitation}`)
  if (link.route === 'wallet') return showMain(`/wallet/${link.wallet}${link.proposal==null?'':`/${link.proposal}`}`)
  if (link.route === 'accept') return showMain(`/plan?accept=${link.token}`)
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

type Role = 'main' | 'approval' | 'any'

function trust(event: IpcMainInvokeEvent, role: Role) {
  const sender = event.sender
  const isMain = !!main && !main.isDestroyed() && sender === main.webContents
  const isApproval = !!approval && !approval.isDestroyed() && sender === approval.webContents
  const ok = role === 'any' ? isMain || isApproval : role === 'main' ? isMain : isApproval
  if (!ok) throw new DaemonError('FORBIDDEN', 'Not allowed from this window')
}

/** A bridge method only `role`'s window may call; a failure reaches the window as "CODE: message". */
function handle(channel: string, role: Role, work: (event: IpcMainInvokeEvent, ...args: any[]) => unknown) {
  ipcMain.handle(channel, async (event, ...args) => {
    try {
      trust(event, role)
      return await work(event, ...args)
    } catch (error) {
      const code = error instanceof DaemonError ? error.code : 'UNAVAILABLE'
      throw new Error(`${code}: ${error instanceof Error ? error.message : String(error)}`)
    }
  })
}

/** A file the person picks in the main window's open dialog; null when cancelled. */
async function pickFile(options: Electron.OpenDialogOptions = {}): Promise<string | null> {
  const picked = await dialog.showOpenDialog(main!, { properties: ['openFile'], ...options })
  return picked.canceled ? null : (picked.filePaths[0] ?? null)
}

/** Where the person saves, from the main window's save dialog; null when cancelled. */
async function pickSave(options: Electron.SaveDialogOptions): Promise<string | null> {
  const picked = await dialog.showSaveDialog(main!, options)
  return picked.canceled ? null : picked.filePath || null
}

handle('dmsg:call', 'any', (event, method: string, params: unknown) => {
  if (['notifications/settings_set', 'notifications/snooze', 'notifications/clear_recent', 'notifications/mark_read', 'notifications/mark_all_read'].includes(method)) trust(event, 'main')
  if (method === 'notifications/settings_set') {
    const zone = (params as { settings?: { quiet_hours?: { timezone?: unknown } } })?.settings?.quiet_hours?.timezone
    if (zone !== undefined && (typeof zone !== 'string' || !validTimeZone(zone))) throw new DaemonError('INVALID_INPUT', 'Time zone')
  }
  if (['notifications/settings_set', 'notifications/snooze', 'channel/settings'].includes(method)) noticeEpoch++
  return daemon.call(method, windowParams(method, params))
})
handle('dmsg:decide', 'approval', (_event, id: string, digest: string, approve: boolean) =>
  daemon.call('approval/respond', { id, digest, approve: approve === true, human: true, source: 'gui' })
)
handle('dmsg:grant', 'main', (_event, invite: string, pairing: string) => daemon.call('connect/grant', { invite, pairing, human: true }))
handle('dmsg:login', 'main', async (_event, origin: string, bind: boolean, options?: { provider?: string; action?: string; credentialId?: string }) => {
  if (options?.provider !== undefined && !['ii', 'passkey'].includes(options.provider)) throw new DaemonError('INVALID_INPUT', 'Login provider')
  if (options?.action !== undefined && !['login', 'register', 'logout_all', 'reset_sessions'].includes(options.action)) throw new DaemonError('INVALID_INPUT', 'Login action')
  if (options?.credentialId !== undefined && (typeof options.credentialId !== 'string' || !options.credentialId.length || options.credentialId.length > 1368)) throw new DaemonError('INVALID_INPUT', 'Passkey credential')
  const started = (await daemon.call('session/login', { origin, bind: bind === true, provider: options?.provider, action: options?.action, credential_id: options?.credentialId, source: 'gui' })) as { url: string }
  if (externalAllowed(started.url, pages().login, [])) await shell.openExternal(started.url)
  return started
})
handle('dmsg:open-login', 'main', async (_event, url: string) => {
  if (!externalAllowed(String(url), pages().login, [])) throw new DaemonError('INVALID_INPUT', 'Not a login page')
  await shell.openExternal(String(url))
})
/**
 * Files reach the daemon only by paths the person picked in a system dialog
 * or dropped on the window: the preload resolves a dropped file to its path,
 * which a window cannot name. A name already in the folder is asked about
 * first: a new version of that file (or the files into that folder), or both
 * kept. The window then hears the names, and the paths of the folders among
 * them, before the import starts, for its progress; the daemon's
 * `files/progress` events follow. The answer names them again, since a
 * folder's files carry their own names.
 */
handle('dmsg:files-add', 'main', async (event, folder: string, dropped: string[] | null, directory: boolean, attach: boolean) => {
  let paths = Array.isArray(dropped) ? dropped.filter((p): p is string => typeof p === 'string' && !!p) : []
  if (!paths.length) {
    const picked = await dialog.showOpenDialog(main!, {
      title: directory === true ? text.pickFolder : text.pickFiles,
      properties: directory === true ? ['openDirectory'] : ['openFile', 'multiSelections']
    })
    if (picked.canceled || !picked.filePaths.length) return null
    paths = picked.filePaths
  }
  const target = String(folder || 'root')
  const folders: string[] = []
  for (const path of paths) if ((await stat(path).catch(() => null))?.isDirectory()) folders.push(path)
  const listing = (await daemon.call('files/list', { folder: target })) as { files: { name: string }[]; folders: { name: string }[] }
  const taken = paths.filter((p) => (folders.includes(p) ? listing.folders : listing.files).some((f) => f.name === basename(p)))
  // Files attached to a message never ask: both are kept, under the channel attachments folder.
  let keepBoth = attach === true
  if (taken.length && !keepBoth) {
    const { response } = await dialog.showMessageBox(main!, {
      type: 'question',
      message: text.nameTaken(taken.length, basename(taken[0])),
      detail: text.nameTakenDetail,
      buttons: [text.asNewVersion, text.keepBoth, text.cancel],
      defaultId: 0,
      cancelId: 2
    })
    if (response === 2) return null
    keepBoth = response === 1
  }
  const names = paths.map((p) => basename(p))
  event.sender.send('dmsg:event', { type: 'files/adding', data: { names, folders } })
  return { names, files: await daemon.call('files/import', { paths, folder: target, keep_both: keepBoth }) }
})
/**
 * The image on the clipboard (a screenshot has no path), read by the main
 * process itself, so a page cannot pass off bytes it made as something pasted.
 * It enters the library through a temporary file, removed once imported.
 */
handle('dmsg:files-paste-image', 'main', async (_event, folder: string) => {
  const item = (await clipboard.read()).find((entry) => entry.types.includes('image/png'))
  if (!item) throw new DaemonError('NOT_FOUND', 'No image on the clipboard')
  const blob = (await item.getType('image/png')) as Blob
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-paste-'))
  try {
    const stamp = new Date().toISOString().replace(/\.\d+Z$/, '').replace(/[-:]/g, '').replace('T', '-')
    const path = join(dir, `Pasted-${stamp}.png`)
    await writeFile(path, Buffer.from(await blob.arrayBuffer()), { mode: 0o600 })
    return { names: [basename(path)], files: await daemon.call('files/import', { paths: [path], folder: String(folder || 'root'), keep_both: true }) }
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})
/** Decrypted copies handed to the system: each goes after a while, and all of them when the session changes or the app quits. */
const systemCopies = new Set<string>()
function dropSystemCopies() {
  for (const dir of systemCopies) rmSync(dir, { recursive: true, force: true })
  systemCopies.clear()
}
async function systemCopy(id: string, name: string): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-copy-'))
  systemCopies.add(dir)
  const forget = () => {
    systemCopies.delete(dir)
    return rm(dir, { recursive: true, force: true })
  }
  try {
    const path = join(dir, basename(name) || 'file')
    await daemon.call('files/export', { id, version: null, path })
    setTimeout(forget, 10 * 60_000).unref()
    return path
  } catch (error) {
    await forget()
    throw error
  }
}
app.on('will-quit', dropSystemCopies)
/** Space in the library: the system's own preview of a copy. */
handle('dmsg:files-preview', 'main', async (_event, id: string, name: string) => {
  main?.previewFile(await systemCopy(String(id), String(name)), basename(String(name)))
})
/** The file's icon dragged out of the window: the system moves a copy to where it is dropped. */
handle('dmsg:files-drag', 'main', async (event, id: string, name: string) => {
  const path = await systemCopy(String(id), String(name))
  event.sender.startDrag({ file: path, icon: await app.getFileIcon(path) })
})
/** `name` in `dir`, or `name (2)` and so on when taken. */
async function freePath(dir: string, name: string): Promise<string> {
  const dot = name.lastIndexOf('.')
  const [stem, extension] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, '']
  for (let n = 1; ; n++) {
    const path = join(dir, n === 1 ? name : `${stem} (${n})${extension}`)
    if (!(await stat(path).catch(() => null))) return path
  }
}
/** Several library files at once, into a folder the person picks; null when cancelled. */
handle('dmsg:files-export-many', 'main', async (_event, items: { id: string; name: string }[]) => {
  const list = Array.isArray(items) ? items : []
  const picked = await dialog.showOpenDialog(main!, { title: text.pickExportFolder, properties: ['openDirectory', 'createDirectory'] })
  const dir = picked.canceled ? null : picked.filePaths[0]
  if (!dir) return null
  for (const item of list) await daemon.call('files/export', { id: String(item.id), version: null, path: await freePath(dir, basename(String(item.name))) })
  return { dir, count: list.length }
})
/**
 * The file library's settings and the idle time, kept in the home's
 * `preferences.json`, which the daemon reads and never takes from a request. Allowing the disk mount
 * asks in a system dialog first, which no window can answer.
 */
handle('dmsg:files-preferences', 'main', async (_event, changes: { files_mount?: boolean; files_trash_days?: number; idle_lock_minutes?: number }) => {
  const path = join(home, 'preferences.json')
  let kept: { files_mount?: boolean; files_trash_days?: number; idle_lock_minutes?: number } = {}
  try {
    kept = JSON.parse(await readFile(path, 'utf8'))
  } catch {
    /* none yet */
  }
  const next = { files_mount: kept.files_mount === true, files_trash_days: kept.files_trash_days, idle_lock_minutes: kept.idle_lock_minutes }
  if (typeof changes?.files_mount === 'boolean' && changes.files_mount !== next.files_mount) {
    if (changes.files_mount) {
      const { response } = await dialog.showMessageBox(main!, {
        type: 'warning',
        message: text.mountAllow,
        detail: text.mountAllowDetail,
        buttons: [text.allow, text.cancel],
        defaultId: 1,
        cancelId: 1
      })
      if (response !== 0) return null
    }
    next.files_mount = changes.files_mount
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
  await mkdir(home, { recursive: true })
  await writeFile(path, `${JSON.stringify(next, null, 2)}\n`, { mode: 0o600 })
  return daemon.call('files/preferences_reload')
})
handle('dmsg:files-export', 'main', async (_event, id: string, version: string | null, name: string) => {
  const path = await pickSave({ defaultPath: basename(String(name)) })
  return path && daemon.call('files/export', { id: String(id), version: version ? String(version) : null, path })
})
/** A temporary copy for the file's program; the daemon removes it later and asks before a change is lost. */
handle('dmsg:files-open', 'main', async (_event, id: string, version: string | null) => {
  const opened = (await daemon.call('files/open', { id: String(id), version: version ? String(version) : null })) as { path: string }
  const failure = await shell.openPath(opened.path)
  if (failure) throw new DaemonError('UNAVAILABLE', failure)
  return opened
})
handle('dmsg:open-legacy', 'main', async (_event, link: string) => {
  if (!legacyAllowed(String(link))) throw new DaemonError('INVALID_INPUT', 'Not an old-site migration link')
  await shell.openExternal(String(link))
})
handle('dmsg:legacy-import', 'main', async (_event, nonce: string, fingerprint: string, principal: string) => {
  const path = await pickFile({
    filters: [
      { name: text.migrationFile, extensions: ['dmsg-migration'] },
      { name: text.allFiles, extensions: ['*'] }
    ]
  })
  return path && daemon.call('legacy/import', { path, nonce: String(nonce), fingerprint: String(fingerprint), principal: String(principal) })
})
handle('dmsg:legacy-frozen', 'main', async (_event, id: string) => {
  const path = await pickFile({
    filters: [
      { name: text.frozenFile, extensions: ['cbor'] },
      { name: text.allFiles, extensions: ['*'] }
    ]
  })
  return path && daemon.call('legacy/frozen', { id: String(id), path })
})
handle('dmsg:legacy-file', 'main', async (_event, id: string, source: string, name: string) => {
  const path = await pickSave({ defaultPath: basename(String(name)) })
  return path && daemon.call('legacy/file', { id: String(id), source: String(source), path })
})
/**
 * A copied secret: the text, and when it leaves the clipboard unless
 * something replaced it. Locking dMsg or quitting the app clears it at once.
 */
let copied: { text: string; timer: ReturnType<typeof setTimeout> } | null = null

/** Clear the clipboard if it still holds the secret dMsg copied. */
async function forgetCopied() {
  const last = copied
  copied = null
  if (!last) return
  clearTimeout(last.timer)
  if ((await clipboard.readText().catch(() => '')) === last.text) clipboard.clear()
}

/**
 * Formats that keep a copied secret out of clipboard histories and the
 * universal clipboard: macOS's `org.nspasteboard` markers, Windows'
 * history and cloud exclusions, and KDE's password-manager hint.
 */
function concealing(): Record<string, string> {
  const raw = (format: string) => `electron application/osclipboard;format="${format}"`
  if (process.platform === 'darwin') return { [raw('org.nspasteboard.ConcealedType')]: '', [raw('org.nspasteboard.TransientType')]: '' }
  if (process.platform === 'win32')
    return { [raw('ExcludeClipboardContentFromMonitorProcessing')]: '', [raw('CanIncludeInClipboardHistory')]: '\0\0\0\0', [raw('CanUploadToCloudClipboard')]: '\0\0\0\0' }
  return { [raw('x-kde-passwordManagerHint')]: 'secret' }
}

/**
 * A hidden field goes from the daemon to the clipboard (a one-time
 * password as its current code); the window that asked never holds it.
 * `field` defaults to the item's main secret, `revision` to the current
 * version.
 */
handle('dmsg:copy-secret', 'main', async (_event, id: string, field: string | null, revision: string | null) => {
  const params = { id: String(id), field: field ? String(field) : undefined, revision: revision ? String(revision) : undefined }
  const shown = (await daemon.call('vault/reveal', params)) as { type: string; value: string; field: string }
  let secret = shown.value
  let expiresAt = Infinity
  if (shown.type === 'otp') {
    const read = async () => {
      const requestedAt = Date.now()
      const otp = (await daemon.call('vault/otp', { id: params.id, field: shown.field })) as { code: string; remaining_ms: number }
      // Count RPC latency too: a copied code must never outlive its validity.
      return { code: otp.code, expiresAt: requestedAt + otp.remaining_ms }
    }
    let otp = await read()
    // A code that expired on the way has a successor, so the click still copies a valid one.
    if (otp.expiresAt <= Date.now()) otp = await read()
    secret = otp.code
    expiresAt = otp.expiresAt
  }
  if (!secret) throw new DaemonError('NOT_FOUND', 'No value to copy')
  await forgetCopied()
  if (expiresAt <= Date.now()) throw new DaemonError('UNAVAILABLE', 'Code expired; copy the current code')
  try {
    await clipboard.write([new ClipboardItem({ 'text/plain': secret, ...concealing() })])
  } catch {
    // A platform that refuses a marker still gets the text.
    await clipboard.writeText(secret)
  }
  const clearAfter = Math.max(0, Math.min(CLEAR_MS, expiresAt - Date.now()))
  copied = { text: secret, timer: setTimeout(() => void forgetCopied(), clearAfter) }
  if (!clearAfter) {
    await forgetCopied()
    throw new DaemonError('UNAVAILABLE', 'Code expired; copy the current code')
  }
  return { clear_after_ms: clearAfter }
})
/** The export last previewed, by the path the person picked; the window never names a path. */
const importing = new PreparedOperation<{ path: string; digest: string }>()
const preparedSecretMount = new PreparedOperation<any>()
let importSession = 0
function cancelImport() {
  importSession += 1
  importing.cancel()
  preparedSecretMount.cancel()
}
handle('dmsg:secret-scan', 'main', async () => {
  const session = importSession
  const selected = await dialog.showOpenDialog(main!, { title: text.pickScanDirectory, properties: ['openDirectory'] })
  if (selected.canceled || !selected.filePaths[0] || session !== importSession) return null
  return daemon.call('secrets/discovery/start', { path: selected.filePaths[0] })
})
handle('dmsg:secret-mount-prepare', 'main', async (_event, input: any, windowMs: number) => {
  const session = preparedSecretMount.begin()
  try {
    if (!input || typeof input.project_label !== 'string' || !Array.isArray(input.entries)) throw new DaemonError('INVALID_INPUT', 'Mapping')
    const selected = await dialog.showOpenDialog(main!, { title: text.pickProject, properties: ['openDirectory'] })
    if (selected.canceled || !selected.filePaths[0] || !preparedSecretMount.preparing(session)) return null
    const project = selected.filePaths[0]
    const params = { input: { project_label: input.project_label, project_directory: project, mount_point: join(project, '.dmsg-secrets'), entries: input.entries, aliases: input.aliases ?? [] }, ...(process.platform === 'darwin' ? { window_ms: windowMs } : {}) }
    const prepared: any = await daemon.call('secrets/mounts/prepare', params)
    if (!preparedSecretMount.ready(session, { ...params, input: prepared.input, source: 'gui' })) return null
    return { ...prepared, session }
  } finally { preparedSecretMount.finish(session) }
})
handle('dmsg:secret-mount-save', 'main', async (_event, session: number) => {
  const params = preparedSecretMount.take(session)
  if (!params) throw new DaemonError('VERSION_CONFLICT', 'Mapping preview expired')
  try { return await daemon.call('secrets/mounts/save_request', params) }
  finally { preparedSecretMount.finish(session) }
})
/** A fixed System Settings pane; a window never names what to open. */
handle('dmsg:open-extension-settings', 'main', async () => {
  if (process.platform !== 'darwin') throw new DaemonError('UNSUPPORTED_PROTOCOL', 'macOS only')
  await shell.openExternal('x-apple.systempreferences:com.apple.LoginItems-Settings.extension')
})
handle('dmsg:vault-import-cancel', 'main', cancelImport)
handle('dmsg:vault-import-preview', 'main', async () => {
  cancelImport()
  const session = importing.begin()
  try {
    const path = await pickFile({
      title: text.pickExport,
      filters: [
        { name: text.exportFile, extensions: ['1pux', 'csv', 'json'] },
        { name: text.allFiles, extensions: ['*'] }
      ]
    })
    if (!path || !importing.preparing(session)) return null
    const preview = (await daemon.call('vault/import_preview', { path })) as { digest: string }
    if (!importing.ready(session, { path, digest: preview.digest })) return null
    return { ...preview, session }
  } finally { importing.finish(session) }
})
handle('dmsg:vault-import', 'main', async (_event, session: number, picks: number[], attachments: string[] = [], folder = 'root') => {
  if (!Array.isArray(picks) || picks.some((n) => !Number.isInteger(n) || n < 0) || !Array.isArray(attachments) || attachments.some((v) => typeof v !== 'string') || typeof folder !== 'string') throw new DaemonError('INVALID_INPUT', 'Import selection')
  // Consume before awaiting: two clicks must never submit the same batch twice.
  const source = importing.take(session)
  if (!source) throw new DaemonError('VERSION_CONFLICT', 'Preview expired')
  try { return await daemon.call('vault/import', { ...source, picks, attachments, folder }) }
  finally { importing.finish(session) }
})
/** Camera permission and picked images stay behind the main-process bridge. */
handle('dmsg:camera-access', 'main', () => process.platform === 'darwin' ? systemPreferences.askForMediaAccess('camera') : true)
handle('dmsg:device-qr-image', 'main', async (_event, source: string) => {
  const limit = 8 * 1024 * 1024
  let bytes: Buffer
  if (source === 'clipboard') {
    const items = await clipboard.read()
    const item = items.find((item) => item.types.includes('image/png'))
    if (!item) throw new DaemonError('NOT_FOUND', 'No image on the clipboard')
    const blob = (await item.getType('image/png')) as Blob
    if (blob.size > limit) throw new DaemonError('QUOTA_EXCEEDED', 'QR image too large')
    bytes = Buffer.from(await blob.arrayBuffer())
  } else if (source === 'file') {
    const path = await pickFile({ title: text.pickQrImage, filters: [{ name: text.images, extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }] })
    if (!path) return null
    if ((await stat(path)).size > limit) throw new DaemonError('QUOTA_EXCEEDED', 'QR image too large')
    bytes = await readFile(path)
  } else throw new DaemonError('INVALID_INPUT', 'QR image source')
  if (bytes.length > limit) throw new DaemonError('QUOTA_EXCEEDED', 'QR image too large')
  return daemon.call('device/pair_scan', { image: bytes.toString('base64') })
})

/** The text of a QR code, from an image the person picks or the one on the clipboard. */
handle('dmsg:vault-otp-scan', 'main', async (_event, source: string) => {
  if (source === 'clipboard') {
    const items = await clipboard.read()
    const item = items.find((i) => i.types.includes('image/png'))
    if (!item) throw new DaemonError('NOT_FOUND', 'No image on the clipboard')
    const blob = (await item.getType('image/png')) as Blob
    const image = Buffer.from(await blob.arrayBuffer()).toString('base64')
    return daemon.call('vault/otp_scan', { image })
  }
  const path = await pickFile({ title: text.pickQrImage, filters: [{ name: text.images, extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }] })
  return path && daemon.call('vault/otp_scan', { path })
})
/** An item's web address, in the system browser: only http and https. */
handle('dmsg:open-url', 'main', async (_event, url: string) => {
  let parsed: URL
  try {
    parsed = new URL(String(url))
  } catch {
    throw new DaemonError('INVALID_INPUT', 'Not a web address')
  }
  if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') throw new DaemonError('INVALID_INPUT', 'Not a web address')
  await shell.openExternal(parsed.toString())
})
handle('dmsg:channel-download', 'main', async (_event, id: string, seq: number, name: string) => {
  const path = await pickSave({ defaultPath: basename(String(name)) })
  return path && daemon.call('channel/download', { id: String(id), seq: Number(seq), path })
})
/** The public avatar: an image the person picks. */
handle('dmsg:avatar', 'main', async () => {
  const path = await pickFile({ title: text.pickAvatar, filters: [{ name: text.imageFile, extensions: ['png', 'jpg', 'jpeg', 'webp'] }] })
  return path && daemon.call('profile/avatar', { path })
})
/** Who pays: the key of a PEM file the person picks, or with `login` the login again; null when cancelled. */
handle('dmsg:payer-pem', 'main', async (_event, login: boolean) => {
  if (login === true) return daemon.call('payer/use', {})
  const path = await pickFile({ title: text.pickPayerPem, filters: [{ name: text.pemFile, extensions: ['pem'] }] })
  return path && daemon.call('payer/use', { pem: path })
})
/** An application action its product prepared, as the canonical CBOR in a file the person picks. */
handle('dmsg:attest-action', 'main', async () => {
  const path = await pickFile({ title: text.pickAction })
  return path && daemon.call('attest/action', { action: (await readFile(path)).toString('base64url'), source: 'gui' })
})
/** A digest or file statement over a local file the person picks; only this path names a local file for one. */
handle('dmsg:attest-file', 'main', async (_event, statement: unknown) => {
  const fields = statement && typeof statement === 'object' && !Array.isArray(statement) ? { ...(statement as Record<string, unknown>) } : {}
  if (fields.profile !== 'digest' && fields.profile !== 'file_statement') throw new DaemonError('INVALID_INPUT', 'Not a file statement')
  delete fields.sha256
  const path = await pickFile({ title: text.pickSignFile })
  return path && daemon.call('attest/request', { statement: fields, file: path, source: 'gui' })
})
/** Compare a file the person picks with the digest of the request the approval window shows; null when cancelled. */
handle('dmsg:check-file', 'approval', async (_event, id: string) => {
  const picked = await dialog.showOpenDialog(approval!, { properties: ['openFile'], title: text.pickCompare })
  if (picked.canceled || !picked.filePaths[0]) return null
  return daemon.call('approval/check_file', { id: String(id), path: picked.filePaths[0] })
})
/** The views the approval window sends the person to: a login, the signatures, the account's security, the plan. */
handle('dmsg:show-main', 'approval', (_event, route: string) => {
  if (!APPROVAL_MAIN_ROUTES.has(String(route))) throw new DaemonError('INVALID_INPUT', 'Not a view')
  showMain(String(route))
})

/** A bundle's JSON from a file. */
async function readBundle(path: string): Promise<unknown> {
  try {
    return JSON.parse(await readFile(path, 'utf8'))
  } catch {
    throw new DaemonError('INVALID_INPUT', 'Not an attestation bundle')
  }
}

/** Verify a bundle the person picks, with the original file of a digest or file statement when asked. */
handle('dmsg:verify-bundle', 'main', async (_event, withFile: boolean) => {
  const path = await pickFile({
    title: text.pickBundle,
    filters: [
      { name: text.bundleFile, extensions: ['json'] },
      { name: text.allFiles, extensions: ['*'] }
    ]
  })
  if (!path) return null
  const bundle = await readBundle(path)
  const file = withFile === true ? await pickFile({ title: text.pickOriginal }) : undefined
  if (file === null) return null
  return daemon.call('attest/verify', { bundle, file })
})
/** Verify a bundle the person dropped on the window, with the original file dropped with it; the preload resolves both. */
handle('dmsg:verify-dropped', 'main', async (_event, bundlePath: string, filePath: string | null) => {
  const bundle = await readBundle(String(bundlePath))
  return daemon.call('attest/verify', { bundle, file: typeof filePath === 'string' ? filePath : undefined })
})
/** Verify one of the account's own attestations as a recipient would its bundle. */
handle('dmsg:verify-entry', 'main', async (_event, execution: string) => {
  const bundle = await daemon.call('attest/export', { execution: String(execution) })
  return daemon.call('attest/verify', { bundle })
})
/** Save a verification report where the person picks; null when cancelled. */
handle('dmsg:save-report', 'main', async (_event, report: unknown) => {
  if ((report as { format?: string })?.format === 'dmsg-wallet-history/1') {
    const csv = walletHistoryCsv(report)
    const path = await pickSave({ defaultPath: 'dmsg-wallet-history.csv', filters: [{ name: 'CSV', extensions: ['csv'] }] })
    if (!path) return null
    await writeFile(path, csv)
    return { path }
  }
  const path = await pickSave({ defaultPath: 'dmsg-verification.json', filters: [{ name: text.reportFile, extensions: ['json'] }] })
  if (!path) return null
  await writeFile(path, JSON.stringify(report, null, 2))
  return { path }
})
handle('dmsg:export-bundle', 'main', async (_event, execution: string) => {
  const bundle = await daemon.call('attest/export', { execution: String(execution) })
  const path = await pickSave({
    defaultPath: `dmsg-attestation-${String(execution).slice(0, 12)}.json`,
    filters: [{ name: text.bundleFile, extensions: ['json'] }]
  })
  if (!path) return null
  await writeFile(path, JSON.stringify(bundle, null, 2))
  return { path }
})
handle('dmsg:pair', 'main', async (_event, copyLink: boolean) => {
  const invite = (await daemon.call('connect/invite')) as { url: string }
  if (!copyLink && externalAllowed(invite.url, [], pages().pairing)) await shell.openExternal(invite.url)
  return invite
})
handle('dmsg:open-approval', 'main', (_event, id: string) => showApproval(id))
/** A language picked in the main window is kept; the tray, dialogs and every window follow it. */
handle('dmsg:set-locale', 'main', async (_event, next: string) => {
  if (!isLocale(next)) throw new DaemonError('INVALID_INPUT', 'Not a supported language')
  await writeFile(localeFile(), next)
  applyLocale(next)
  broadcast('dmsg:locale', next)
})
/** An appearance picked in the main window is kept; every window, menu and dialog follows it. */
handle('dmsg:set-appearance', 'main', async (_event, next: string) => {
  if (!isAppearance(next)) throw new DaemonError('INVALID_INPUT', 'Not a supported appearance')
  await writeFile(appearanceFile(), next)
  applyAppearance(next)
})
handle('dmsg:connection', 'any', () => ({ connected: daemon.connected, error: daemon.lastError, recovery: daemon.recovery.state }))
handle('dmsg:reconnect', 'any', () => (daemon.connected ? undefined : daemon.connect()))
handle('dmsg:quit', 'any', () => {
  quitting = true
  app.quit()
})
/** What only this app's windows and system integration know. */
const desktopSettings = () => ({ lock_on_screen_lock: desktop.lockOnScreenLock, open_at_login: app.getLoginItemSettings().openAtLogin })
handle('dmsg:desktop-settings', 'main', () => desktopSettings())
/** The approval window closes itself once decided. The flows keep it open to look at it, unless they test this. */
handle('dmsg:close-approval', 'approval', () => {
  if (!testMode || process.env.DMSG_DESKTOP_TEST_APPROVAL_CLOSE === '1') approval?.close()
})
handle('dmsg:set-desktop-settings', 'main', (_event, change: { lock_on_screen_lock?: boolean; open_at_login?: boolean }) => {
  if (typeof change?.lock_on_screen_lock === 'boolean') keepDesktop({ lockOnScreenLock: change.lock_on_screen_lock }, true)
  if (typeof change?.open_at_login === 'boolean') {
    app.setLoginItemSettings({ openAtLogin: change.open_at_login, args: ['--hidden'] })
    updateTray(trayState)
  }
  return desktopSettings()
})
/** A `dmsg://` link in a message or letter moves the view, as one from outside the app does. */
handle('dmsg:open-link', 'main', async (_event, url: string) => {
  if (!parseLink(String(url))) throw new DaemonError('INVALID_INPUT', 'Not a dMsg link')
  await openLink(String(url))
})
/** An image shown in a window, to the clipboard: only an image's own data address. */
handle('dmsg:copy-image', 'main', async (_event, url: string) => {
  const image = /^data:image\/(png|jpeg|gif|webp);base64,/.test(String(url)) ? nativeImage.createFromDataURL(String(url)) : null
  if (!image || image.isEmpty()) throw new DaemonError('INVALID_INPUT', 'Not an image')
  await clipboard.write([new ClipboardItem({ 'image/png': new Blob([new Uint8Array(image.toPNG())], { type: 'image/png' }) })])
})
/** What an MCP client runs to reach this daemon: the installed binary and its home. */
handle('dmsg:runtime', 'main', () => ({ binary: daemon.options.binary, home: daemon.options.home }))
handle('dmsg:fullscreen', 'main', () => main?.isFullScreen() === true)
handle('dmsg:updates', 'main', () => updates.info())
handle('dmsg:set-update-channel', 'main', (_event, channel) => updates.setChannel(channel))
handle('dmsg:check-updates', 'main', () => updates.check())
handle('dmsg:apply-update', 'main', () => applyUpdate())

function broadcast(channel: string, payload: unknown) {
  for (const window of [main, approval]) if (window && !window.isDestroyed()) window.webContents.send(channel, payload)
}

/** Actual native focus is required; a hidden or locked screen is never a viewport. */
function noticeForeground() {
  return !screenLocked && [main, approval].some((window) => window && !window.isDestroyed() && window.isVisible() && window.isFocused() && !window.isMinimized())
}

const xmlText = (value: string) => value.replace(/[&<>"']/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' })[character]!)
const nativeNotices = new Notifications({
  create: (options) => new Notification({ ...options, ...(process.platform === 'win32' ? {
    toastXml: `<toast launch="dmsg-notice:${options.id}"><visual><binding template="ToastGeneric"><text>dMsg</text><text>${xmlText(options.body)}</text></binding></visual>${options.silent ? '<audio silent="true"/>' : ''}</toast>`
  } : {}) }),
  body: (category, count) => notificationBody(locale, category, count),
  foreground: noticeForeground,
  inApp: (plan) => {
    const window = [main, approval].find((window) => window && !window.isDestroyed() && window.isFocused() && window.isVisible())
    window?.webContents.send('dmsg:event', { type: 'notifications/present', data: plan })
  },
  open: (plan) => openNotice(plan.account, plan.target, plan.covered.length === 1 ? plan.covered[0].id : undefined, true, plan.covered.length === 1 ? plan.covered[0].version : undefined),
  result: (plan, result) => {
    if (!plan.in_app && result !== 'suppressed') noticePlatformResult(result)
    return daemon.call('notifications/result', { account: plan.account, system_id: plan.system_id, delivery_id: plan.delivery_id, result })
  },
  remove: (id) => { if (process.platform === 'darwin') Notification.remove(id) },
  activation: process.platform === 'win32'
})

function noticePlatformResult(result: 'submitted' | 'shown' | 'failed') {
  noticeLastResult = result
  broadcast('dmsg:event', { type: 'notifications/platform' })
}

async function openNotice(account: string, target: NoticeTarget, id?: string, native = false, version?: string) {
  if (!isNoticeAccount(account) || !isNoticeTarget(target)) throw new DaemonError('INVALID_INPUT', 'Notification target')
  const entry = `/reminders?account=${account}${id ? `&item=${id}` : ''}${version ? `&version=${version}` : ''}${target.kind === 'summary' && ['message', 'service'].includes(target.id) ? '&scope=recent' : ''}`
  const status = await daemon.call('daemon/status').catch(() => null) as { account?: string; state?: string; session?: string } | null
  if (!status) return showMain(entry)
  if (native) {
    const prefs = await daemon.call('notifications/settings', { account }).catch(() => null) as NoticePreferences | null
    if (!prefs?.account?.enabled) return showMain(`${entry}&ended=1`)
  }
  if (status.account !== account || status.state !== 'unlocked') return showMain(entry)
  // Read through the original business interface before navigating. A completed
  // approval still opens that approval's result, never another waiting request.
  try {
    switch (target.kind) {
      case 'approval': await daemon.call('approval/show', { id: target.id }); break
      case 'channel': await daemon.call('channel/show', { id: target.id }); break
      case 'inbox': await daemon.call('inbox/list'); break
      case 'recovery': await daemon.call('account/security'); break
      case 'wallet': {
        const proposal = Number(target.id.proposal)
        if (!Number.isSafeInteger(proposal)) return showMain(`${entry}&ended=1`)
        await daemon.call('wallet/proposal', { wallet: target.id.wallet, id: proposal })
        break
      }
      case 'plan': await daemon.call('plan/status', { mode: 'fresh' }); break
    }
  } catch (error) {
    if (error instanceof DaemonError && ['NOT_FOUND', 'FORBIDDEN', 'EXPIRED'].includes(error.code)) return showMain(`${entry}&ended=1&reason=${error.code}`)
    // The original view can show its last trusted state and offer a retry offline.
  }
  const current = await daemon.call('daemon/status').catch(() => null) as { account?: string; state?: string; session?: string } | null
  if (!current || current.account !== account || current.state !== 'unlocked' || current.session !== status.session) return showMain(entry)
  if (target.kind === 'approval') showApproval(target.id)
  else {
    let route = noticeRoute(target)
    if (target.kind === 'summary') route += `&account=${account}`
    else if (id) route += `${route.includes('?') ? '&' : '?'}reminder=${id}`
    showMain(route)
  }
  const window = target.kind === 'approval' ? approval : main
  if (window && id && version && current.session) {
    noticeReads.set(window.webContents.id, { account, session: current.session, id, version })
  }
}

/** How often the main process looks for a returning connection; Electron has no event for it. */
const NETWORK_POLL_MS = 10_000

/** A wake or a returning connection ends the daemon's waits for the network. */
function networkReturned() {
  if (daemon.connected) daemon.call('app/network').catch(() => {})
}

/** One serialized native consumer; no renderer can call next/result/context. */
const refreshNotices = single(async () => {
  if (!daemon.connected || quitting) return
  const epoch = noticeEpoch
  try {
    if (!noticeLegacyAttempted) {
      noticeLegacyAttempted = true
      try {
        let plans = {}
        try { plans = JSON.parse(await readFile(join(app.getPath('userData'), 'reminders.json'), 'utf8')) }
        catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error }
        await daemon.call('notifications/migrate', { plans })
      } catch { console.warn('Notification reminder migration is pending') }
    }
    const status = await daemon.call('daemon/status') as { account?: string; state?: string; session?: string }
    if (status.state !== 'unlocked') void forgetCopied()
    const preferences = await daemon.call('notifications/settings') as NoticePreferences
    const counts = await daemon.call('notifications/status') as NoticeStatus
    if (epoch !== noticeEpoch || !preferences.global || !counts.counts || !status.session) return
    waiting = counts.counts.actionable
    const badge = Math.max(0, Math.min(2_147_483_647, counts.badge || 0))
    app.setBadgeCount(badge)
    // The daemon applies the badge preference. Pending verification alone is
    // not unread work and must not turn a zero count back into a Dock dot.
    if (process.platform === 'darwin') app.dock?.setBadge(badge > 99 ? '99+' : badge ? String(badge) : '')
    updateTray(status.state ?? 'unknown')
    const visible = noticeForeground() ? [...noticeViewports.values()] : []
    await daemon.call('notifications/context', { session: status.session, context: {
      now: 0, allowed: Notification.isSupported() && preferences.global.enabled,
      foreground: noticeForeground(), viewing: false, visible, remote_viewed: false,
      quiet_until: quietUntil(preferences.global, Date.now()), can_update: false, remote_owner: false
    } })
    if (epoch !== noticeEpoch) return
    const batch = await daemon.call('notifications/next', { restore: !noticeHistoryLoaded }) as NoticeBatch
    if (epoch !== noticeEpoch) return
    if (!noticeHistoryLoaded) {
      const history = !testMode && process.platform === 'darwin' && Notification.isSupported() ? await Notification.getHistory().catch(() => []) : []
      if (epoch !== noticeEpoch) return
      nativeNotices.restore(history, batch.history ?? [])
      noticeHistoryLoaded = true
      if (noticeActivation) {
        const id = noticeActivation
        noticeActivation = null
        if (!await nativeNotices.activate(id)) showMain('/reminders?ended=1')
      }
    }
    nativeNotices.remove((batch.remove ?? []).filter(isNoticeHash))
    for (const plan of batch.plans ?? []) nativeNotices.present(plan)
  } catch {
    // Durable projections remain queryable; reconnect/events retry presentation.
  }
})

handle('dmsg:notification-permission', 'main', () => ({
  supported: Notification.isSupported(), permission: Notification.isSupported() ? 'system_managed' : 'unsupported',
  settings_available: ['darwin', 'win32'].includes(process.platform), last_result: noticeLastResult
}))
handle('dmsg:notification-settings', 'main', async () => {
  if (process.platform === 'darwin') await shell.openExternal('x-apple.systempreferences:com.apple.Notifications-Settings.extension')
  else if (process.platform === 'win32') await shell.openExternal('ms-settings:notifications')
})
handle('dmsg:notification-test', 'main', () => {
  if (!Notification.isSupported()) throw new DaemonError('UNSUPPORTED_PROTOCOL', 'System notifications')
  nativeNotices.test(notificationText(locale).noticeTest, noticePlatformResult)
})
handle('dmsg:notification-open', 'any', async (_event, account: string, id: string, version?: string) => {
  if (version !== undefined && (!isNoticeSequence(version) || version === '0')) throw new DaemonError('INVALID_INPUT', 'Notification version')
  if (!isNoticeAccount(account)) throw new DaemonError('INVALID_INPUT', 'Notification identity')
  if (noticeCategories.includes(id as NoticeCategory)) {
    // Validate the original local account before opening its summary. This IPC
    // is available in either window, without granting arbitrary navigation.
    await daemon.call('notifications/settings', { account })
    return openNotice(account, { kind: 'summary', id: id as NoticeCategory })
  }
  if (!isNoticeHash(id)) throw new DaemonError('INVALID_INPUT', 'Notification identity')
  const list = await daemon.call('notifications/list', { account, id }) as { items: NoticeItem[] }
  const item = list.items.find((item) => item.id === id && item.account === account)
  if (!item) return showMain(`/reminders?account=${account}&ended=1`)
  if (version && BigInt(version) > BigInt(item.version)) throw new DaemonError('VERSION_CONFLICT', 'Notification version')
  await openNotice(account, item.target, id, false, version ?? item.version)
})
handle('dmsg:notification-visible', 'any', async (event, value: NoticeVisibility | null) => {
  const sender = event.sender.id
  const revision = (noticeViewEpoch.get(sender) ?? 0) + 1
  noticeViewEpoch.set(sender, revision)
  noticeViewports.delete(sender)
  const window = BrowserWindow.fromWebContents(event.sender)
  if (!value || !window?.isFocused() || !window.isVisible() || screenLocked) { void refreshNotices(); return false }
  if (!isNoticeAccount(value.account) || typeof value.session !== 'string'
    || (value.id !== undefined && !isNoticeHash(value.id))
    || (value.target !== undefined && !isNoticeTarget(value.target))
    || (value.version !== undefined && !isNoticeSequence(value.version))
    || (value.watermark !== undefined && !isNoticeSequence(value.watermark))
    || (value.acknowledgeOnly !== undefined && typeof value.acknowledgeOnly !== 'boolean')) throw new DaemonError('INVALID_INPUT', 'Notification visibility')
  if (value.acknowledgeOnly) {
    const opened = noticeReads.get(sender)
    if (!opened || opened.id !== value.id || opened.account !== value.account || opened.session !== value.session) return false
    const current = await daemon.call('daemon/status') as { account?: string; session?: string; state?: string }
    if (noticeReads.get(sender) !== opened || current.account !== opened.account || current.session !== opened.session || current.state !== 'unlocked'
      || noticeViewEpoch.get(sender) !== revision || window.isDestroyed() || !window.isFocused() || !window.isVisible() || screenLocked) return false
    try {
      await daemon.call('notifications/mark_read', { account: opened.account, id: opened.id, version: opened.version })
      if (noticeReads.get(sender) === opened) noticeReads.delete(sender)
      void refreshNotices()
      return true
    } catch { return false }
  }
  const seen = await daemon.call('notifications/observed', value) as { id: string; version: string; stage: number }
  if (noticeViewEpoch.get(sender) !== revision || window.isDestroyed() || !window.isFocused() || !window.isVisible() || screenLocked) return false
  noticeViewports.set(sender, seen)
  const opened = noticeReads.get(sender)
  if (opened && opened.account === value.account && opened.session === value.session && opened.id === seen.id && BigInt(opened.version) <= BigInt(seen.version)) {
    // Local acknowledgement is independent of business receipts and never blocks
    // the loaded view. A newer reminder stays unread; a failed save can retry on focus.
    try {
      await daemon.call('notifications/mark_read', { account: opened.account, id: opened.id, version: opened.version })
      if (noticeReads.get(sender) === opened) noticeReads.delete(sender)
    } catch { /* Keep the original acknowledgement pending. */ }
  }
  void refreshNotices()
  return true
})


/** Restart into a downloaded update, or open the release page of one this package cannot install. */
function applyUpdate() {
  return updates.apply(
    (method, params) => daemon.call(method, params),
    // A copied secret leaves first, so the installing quit is never held.
    async () => {
      quitting = true
      daemon.close()
      await forgetCopied()
    },
    async () => {
      quitting = false
      await daemon.connect()
    },
    text
  )
}

/** A check asked for from the tray says what it found; the window shows it in Settings › About dMsg. */
async function checkFromTray() {
  const state = await updates.check()
  if (updates.info().homebrew) return showMain('/settings/about')
  if (state.kind === 'latest') {
    await dialog.showMessageBox({ type: 'info', message: text.upToDate(app.getVersion()) })
  } else if (state.kind === 'error') {
    await dialog.showMessageBox({ type: 'warning', message: text.updateFailed, detail: state.message })
  } else if (state.kind === 'downloading') {
    await dialog.showMessageBox({ type: 'info', message: text.updateAvailable(state.version), detail: text.updateDownloading })
  } else if (state.kind === 'available') {
    const { response } = await dialog.showMessageBox({
      type: 'info',
      message: text.updateAvailable(state.version),
      detail: text.updateManual,
      buttons: [text.downloadUpdate(state.version), text.later],
      defaultId: 0,
      cancelId: 1
    })
    if (response === 0) await applyUpdate()
  }
}

/** The tray's update item: nothing in a development run, else where the update stands. */
function updateItem(): Electron.MenuItemConstructorOptions[] {
  const state = updates.state
  if (updates.info().homebrew) return [{ label: text.checkUpdates, click: () => void checkFromTray() }]
  switch (state.kind) {
    case 'off':
      return []
    case 'ready':
      return [{ label: text.restartToUpdate(state.version), click: () => void applyUpdate() }]
    case 'available':
      return [{ label: text.downloadUpdate(state.version), click: () => void applyUpdate() }]
    case 'checking':
    case 'downloading':
      return [{ label: state.kind === 'checking' ? text.checking : text.downloading, enabled: false }]
    default:
      return [{ label: text.checkUpdates, click: () => void checkFromTray() }]
  }
}

/**
 * The tray shows the Gate with its diamond, and a badge at the top right
 * while a request waits; a separate bottom-right lock marks a locked account.
 * macOS tints template images for the menu bar;
 * elsewhere the mark keeps its tile so it shows on dark and light taskbars.
 */
function trayImage(variant: ReturnType<typeof trayVariant>) {
  const mac = process.platform === 'darwin'
  const badge = variant === 'idle' ? '' : variant[0].toUpperCase() + variant.slice(1)
  const name = `tray${badge}${mac ? 'Template' : ''}.png`
  const image = nativeImage.createFromPath(join(__dirname, '../../resources', name))
  if (mac) image.setTemplateImage(true)
  return image
}

function updateTray(state: string) {
  trayState = state
  if (!tray) return
  if (trayImages) tray.setImage(trayImages[trayVariant(state, waiting)])
  tray.setToolTip(text.tooltip(app.getVersion(), text.state(state), waiting))
  tray.setContextMenu(
    Menu.buildFromTemplate([
      { label: `dMsg: ${text.state(state)}`, enabled: false },
      { label: `${notificationText(locale).noticeTitle} (${waiting})`, click: () => showMain('/reminders') },
      { type: 'separator' },
      { label: text.open, click: () => showMain() },
      { label: text.lock, enabled: state === 'unlocked', click: () => void daemon.call('session/lock').catch(() => {}) },
      { type: 'separator' },
      ...updateItem(),
      {
        label: text.openAtLogin,
        type: 'checkbox',
        checked: app.getLoginItemSettings().openAtLogin,
        click: (item) => {
          app.setLoginItemSettings({ openAtLogin: item.checked, args: ['--hidden'] })
          broadcast('dmsg:event', { type: 'desktop/changed' })
        }
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

/**
 * The menu bar in the interface language, rebuilt when it changes. Packaged
 * apps have no reload or developer tools; every other command is the system's
 * own, and no accelerator takes a key the windows answer themselves.
 */
function updateMenu() {
  if (!app.isReady()) return
  const mac = process.platform === 'darwin'
  const quit = () => {
    quitting = true
    app.quit()
  }
  const template: Electron.MenuItemConstructorOptions[] = [
    ...(mac
      ? [{
          label: 'dMsg',
          submenu: [
            { role: 'about', label: text.about },
            { type: 'separator' },
            { label: text.settings, accelerator: 'Cmd+,', click: () => showMain('/settings') },
            ...updateItem(),
            { type: 'separator' },
            { role: 'services', label: text.services },
            { type: 'separator' },
            { role: 'hide', label: text.hide },
            { role: 'hideOthers', label: text.hideOthers },
            { role: 'unhide', label: text.showAll },
            { type: 'separator' },
            { label: text.quit, accelerator: 'Cmd+Q', click: quit }
          ]
        } as Electron.MenuItemConstructorOptions]
      : []),
    {
      label: text.menuEdit,
      submenu: [
        { role: 'undo', label: text.undo },
        { role: 'redo', label: text.redo },
        { type: 'separator' },
        { role: 'cut', label: text.cut },
        { role: 'copy', label: text.copy },
        { role: 'paste', label: text.paste },
        { role: 'selectAll', label: text.selectAll }
      ]
    },
    {
      label: text.menuView,
      submenu: [
        ...(app.isPackaged ? [] : ([{ role: 'reload' }, { role: 'toggleDevTools' }, { type: 'separator' }] as Electron.MenuItemConstructorOptions[])),
        { role: 'resetZoom', label: text.actualSize },
        { role: 'zoomIn', label: text.zoomIn },
        { role: 'zoomOut', label: text.zoomOut },
        { type: 'separator' },
        { role: 'togglefullscreen', label: text.fullScreen }
      ]
    },
    {
      label: text.menuWindow,
      submenu: [
        { role: 'minimize', label: text.minimize },
        ...(mac ? ([{ role: 'zoom', label: text.zoom }] as Electron.MenuItemConstructorOptions[]) : []),
        { role: 'close', label: text.close },
        ...(mac ? ([{ type: 'separator' }, { role: 'front', label: text.bringToFront }] as Electron.MenuItemConstructorOptions[]) : [])
      ]
    },
    {
      label: text.menuHelp,
      role: 'help',
      submenu: [
        { label: text.website, click: () => void shell.openExternal('https://dmsg.net') },
        ...(mac ? [] : [{ type: 'separator' } as Electron.MenuItemConstructorOptions, ...updateItem(), { label: text.quit, click: quit }])
      ]
    }
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

daemon.on('event', (event: DaemonEvent) => {
  if (event.type === 'session/changed' || event.type === 'resync') cancelImport()
  if (event.type === 'session/changed') dropSystemCopies()
  broadcast('dmsg:event', event)
  if (['notifications/changed', 'session/changed', 'resync', 'sync/changed', 'sync/finished'].includes(event.type)) {
    noticeEpoch++
    if (event.type === 'session/changed' || event.type === 'resync') noticeViewports.clear()
    if (event.type === 'session/changed') noticeReads.clear()
    void refreshNotices()
  }
})
// The tray follows the connection: the daemon's state once connected, else connecting again.
daemon.on('state', () => {
  if (!daemon.connected) cancelImport()
  broadcast('dmsg:event', { type: 'connection/changed' })
  if (daemon.connected) {
    noticeHistoryLoaded = false
    noticeEpoch++
    void refreshNotices()
  } else updateTray('connecting')
})

app.on('second-instance', (_event, argv) => {
  const link = argv.find((a) => a.startsWith('dmsg://'))
  if (link) void openLink(link)
  else showMain()
})
app.on('open-url', (event, url) => {
  event.preventDefault()
  if (app.isReady()) void openLink(url)
  else pendingLink = url
})
// The Dock icon brings back the main window that closing hid.
app.on('activate', () => {
  if (app.isReady()) showMain()
})
app.on('web-contents-created', (_event, contents) => {
  contents.on('will-attach-webview', (event) => event.preventDefault())
  contents.on('input-event', (_event, input) => {
    if (!INPUT.has(input.type) || Date.now() - reported < ACTIVITY_MS) return
    reported = Date.now()
    daemon.active()
  })
})
app.on('before-quit', (event) => {
  quitting = true
  noticeEpoch++
  // Whether the clipboard still holds a copied secret is read
  // asynchronously, and Electron does not wait for that: without a window
  // the read finished only after `quit`. The quit waits for it instead.
  if (copied) {
    event.preventDefault()
    void forgetCopied().finally(() => app.quit())
    return
  }
  daemon.close()
})
app.on('window-all-closed', () => {
  if (testMode || quitting) app.quit()
})

app.on('ready', (_event, launchInfo) => {
  const info = launchInfo as Partial<Electron.NotificationResponse>
  if (typeof info?.identifier === 'string' && !info.actionIdentifier?.endsWith('DismissActionIdentifier'))
    noticeActivation = /^[A-Za-z0-9_-]{16}$/.test(info.identifier) ? info.identifier : 'unknown'
})

app.whenReady().then(async () => {
  // Camera frames belong only to the main app's top frame, never the approval
  // window or an embedded frame. No microphone permission is granted here.
  electronSession.defaultSession.setPermissionCheckHandler((contents, permission, _origin, details) =>
    permission !== 'media' || (!!main && contents === main.webContents && details.isMainFrame === true && details.mediaType === 'video'))
  electronSession.defaultSession.setPermissionRequestHandler((contents, permission, respond, details) =>
    respond(permission !== 'media' || (!!main && contents === main.webContents && details.isMainFrame === true && 'mediaTypes' in details && details.mediaTypes?.length === 1 && details.mediaTypes[0] === 'video')))
  // A person who stepped away from this computer also left dMsg open; the toggle is under Security.
  const lockForAbsence = () => {
    if (desktop.lockOnScreenLock && daemon.connected) daemon.call('session/lock').catch(() => {})
  }
  powerMonitor.on('lock-screen', () => { screenLocked = true; noticeViewports.clear(); noticeEpoch++; lockForAbsence(); void refreshNotices() })
  powerMonitor.on('suspend', lockForAbsence)
  powerMonitor.on('unlock-screen', () => { screenLocked = false; noticeEpoch++; void refreshNotices() })
  powerMonitor.on('resume', () => { noticeEpoch++; void refreshNotices(); networkReturned() })
  let online = net.isOnline()
  setInterval(() => {
    const now = net.isOnline()
    if (now && !online) networkReturned()
    online = now
  }, NETWORK_POLL_MS).unref()
  if (process.platform === 'win32') Notification.handleActivation((details) => {
    const match = /^dmsg-notice:([A-Za-z0-9_-]{16})$/.exec(details.arguments)
    if (!match) { noticeActivation = 'unknown'; showMain('/reminders?ended=1'); return }
    if (!noticeHistoryLoaded) noticeActivation = match[1]
    else void nativeNotices.activate(match[1]).then((opened) => { if (!opened) showMain('/reminders?ended=1') })
  })
  if (keepsDesktopState) {
    try {
      desktop = readDesktopState(readFileSync(desktopFile(), 'utf8'))
    } catch {
      /* the first run */
    }
  }
  let stored: string | null = null
  try {
    stored = readFileSync(localeFile(), 'utf8').trim()
  } catch {
    /* no choice yet */
  }
  applyLocale(resolveLocale(stored, [...app.getPreferredSystemLanguages(), app.getLocale()]))
  try {
    const kept = readFileSync(appearanceFile(), 'utf8').trim()
    if (isAppearance(kept)) applyAppearance(kept)
  } catch {
    /* the system's appearance */
  }
  if (!testMode) {
    if (process.defaultApp && process.argv[1]) app.setAsDefaultProtocolClient('dmsg', process.execPath, [resolve(process.argv[1])])
    else app.setAsDefaultProtocolClient('dmsg')
  }
  if (!testMode || process.env.DMSG_DESKTOP_TEST_TRAY === '1') {
    trayImages = { idle: trayImage('idle'), waiting: trayImage('waiting'), locked: trayImage('locked'), lockedWaiting: trayImage('lockedWaiting') }
    tray = new Tray(trayImages.idle)
    // On Windows the tray icon's left click is how a closed window comes back.
    if (process.platform === 'win32') tray.on('click', () => showMain())
    updateTray('connecting')
  }
  // Opened at login, the app starts in the tray.
  if (!process.argv.includes('--hidden') && !app.getLoginItemSettings().wasOpenedAtLogin) showMain(noticeActivation ? '/reminders' : undefined)
  let installed = true
  let homebrew = false
  if (!daemon.options.fixed) {
    try {
      const runtime = await resolveRuntime(app.isPackaged, process.resourcesPath)
      homebrew = runtime.action === 'homebrew'
      daemon.options.binary = runtime.binary
      if (app.isPackaged) {
        daemon.options.minimumVersion = app.getVersion()
        requireRuntimeVersion(runtime.version, app.getVersion())
      }
    } catch (error) {
      // Without the shared `dmsg` nothing can start the daemon; the next app start installs again.
      installed = false
      daemon.lastError = `dmsg install failed: ${reasonOf(error)}`
      daemon.emit('state')
    }
  }
  updates.start(homebrew)
  if (installed) await daemon.connect()
  const link = pendingLink ?? process.argv.find((a) => a.startsWith('dmsg://'))
  if (link) void openLink(link)
})
