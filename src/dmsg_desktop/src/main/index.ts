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
  Notification,
  powerMonitor,
  shell,
  Tray,
  type IpcMainInvokeEvent
} from 'electron'
import { readFileSync } from 'node:fs'
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises'
import { homedir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { isAppearance, type Appearance } from '../shared/appearance'
import type { DaemonEvent } from '../shared/contract'
import { isLocale, resolveLocale, type Locale } from '../shared/locales'
import { single } from '../shared/single'
import { walletHistoryCsv } from '../shared/wallet-report'
import { Daemon, reasonOf } from './daemon'
import { DaemonError } from './error'
import { APPROVAL_MAIN_ROUTES, externalAllowed, legacyAllowed, parseLink, windowParams } from './policy'
import { resolveRuntime } from './runtime'
import { Notifications } from './notifications'
import { isNoticeAccount, isNoticeHash, isNoticeSequence, isNoticeTarget, noticeRoute, type NoticeBatch, type NoticeItem, type NoticePreferences, type NoticeStatus, type NoticeTarget } from '../shared/notifications'
import { quietUntil, validTimeZone } from '../shared/notification-time'
import { notificationBody, notificationText } from '../shared/notification-text'
import type { NoticeVisibility } from '../shared/contract'
import { textFor } from './text'
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
const noticeViewports = new Map<number, { id: string; version: string; stage: number }>()
const noticeViewEpoch = new Map<number, number>()
let pendingLink: string | null = null
let trayState = 'connecting'
/** English until the app is ready and knows its locale. */
let locale: Locale = 'en'
let text = textFor(locale)
let appearance: Appearance = 'system'
const updates = new Updates(() => {
  updateTray(trayState)
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

function harden(window: BrowserWindow) {
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
  window.webContents.on('will-navigate', (event) => event.preventDefault())
  const clearViewport = () => {
    const id = windowId
    noticeViewports.delete(id)
    noticeViewEpoch.set(id, (noticeViewEpoch.get(id) ?? 0) + 1)
    noticeEpoch++
    void refreshNotices()
  }
  const windowId = window.webContents.id
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
  const window = new BrowserWindow({
    width: 1180,
    height: 760,
    minWidth: 760,
    minHeight: 540,
    title: 'dMsg',
    // On macOS the sidebar sits on the window's own material; the traffic
    // lights sit in the 60px title bar, centred on its line.
    ...(process.platform === 'darwin'
      ? { titleBarStyle: 'hidden', trafficLightPosition: { x: 18, y: 24 }, vibrancy: 'sidebar', backgroundColor: '#00000000' }
      : { backgroundColor: background() }),
    show: !testMode || process.env.DMSG_DESKTOP_SHOW === '1',
    webPreferences: webPreferences('main')
  })
  main = window
  harden(window)
  load(window, route ?? '/channels')
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
  if (['notifications/settings_set', 'notifications/snooze', 'notifications/clear_recent'].includes(method)) trust(event, 'main')
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
handle('dmsg:login', 'main', async (_event, origin: string, bind: boolean) => {
  const started = (await daemon.call('session/login', { origin, bind: bind === true, source: 'gui' })) as { url: string }
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
handle('dmsg:files-add', 'main', async (event, folder: string, dropped: string[] | null, directory: boolean) => {
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
  let keepBoth = false
  if (taken.length) {
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
 * The file library's settings, kept in the home's `preferences.json`, which
 * the daemon reads and never takes from a request. Allowing the disk mount
 * asks in a system dialog first, which no window can answer.
 */
handle('dmsg:files-preferences', 'main', async (_event, changes: { files_mount?: boolean; files_trash_days?: number }) => {
  const path = join(home, 'preferences.json')
  let kept: { files_mount?: boolean; files_trash_days?: number } = {}
  try {
    kept = JSON.parse(await readFile(path, 'utf8'))
  } catch {
    /* none yet */
  }
  const next = { files_mount: kept.files_mount === true, files_trash_days: kept.files_trash_days }
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
  const secret = shown.type === 'otp' ? ((await daemon.call('vault/otp', { id: params.id, field: shown.field })) as { code: string }).code : shown.value
  if (!secret) throw new DaemonError('NOT_FOUND', 'No value to copy')
  await forgetCopied()
  try {
    await clipboard.write([new ClipboardItem({ 'text/plain': secret, ...concealing() })])
  } catch {
    // A platform that refuses a marker still gets the text.
    await clipboard.writeText(secret)
  }
  copied = { text: secret, timer: setTimeout(() => void forgetCopied(), CLEAR_MS) }
  return { clear_after_ms: CLEAR_MS }
})
/** The export last previewed, by the path the person picked; the window never names a path. */
let importing: { path: string; digest: string } | null = null
let importSession = 0
function cancelImport() {
  importSession += 1
  importing = null
}
let secretMountSerial = 0
let preparedSecretMount: { session: number; epoch: number; params: any } | null = null
handle('dmsg:secret-scan', 'main', async () => {
  const session = importSession
  const selected = await dialog.showOpenDialog(main!, { title: 'Scan selected directory / 扫描所选目录', properties: ['openDirectory'] })
  if (selected.canceled || !selected.filePaths[0] || session !== importSession) return null
  return daemon.call('secrets/discovery/start', { path: selected.filePaths[0] })
})
handle('dmsg:secret-mount-prepare', 'main', async (_event, input: any, windowMs: number) => {
  preparedSecretMount = null
  const session = ++secretMountSerial
  const epoch = importSession
  if (!input || typeof input.project_label !== 'string' || !Array.isArray(input.entries)) throw new DaemonError('INVALID_INPUT', 'Mapping')
  const selected = await dialog.showOpenDialog(main!, { title: 'Choose project directory / 选择项目目录', properties: ['openDirectory'] })
  if (selected.canceled || !selected.filePaths[0] || epoch !== importSession || session !== secretMountSerial) return null
  const project = selected.filePaths[0]
  const params = { input: { project_label: input.project_label, project_directory: project, mount_point: join(project, '.dmsg-secrets'), entries: input.entries, aliases: input.aliases ?? [] }, ...(process.platform === 'darwin' ? { window_ms: windowMs } : {}) }
  const prepared: any = await daemon.call('secrets/mounts/prepare', params)
  if (epoch !== importSession || session !== secretMountSerial) return null
  preparedSecretMount = { session, epoch, params: { ...params, input: prepared.input, source: 'gui' } }
  return { ...prepared, session }
})
handle('dmsg:secret-mount-save', 'main', async (_event, session: number) => {
  const prepared = preparedSecretMount
  if (!prepared || prepared.session !== session || prepared.epoch !== importSession || session !== secretMountSerial) throw new DaemonError('VERSION_CONFLICT', 'Mapping preview expired')
  preparedSecretMount = null
  return daemon.call('secrets/mounts/save_request', prepared.params)
})
/** A fixed System Settings pane; a window never names what to open. */
handle('dmsg:open-extension-settings', 'main', async () => {
  if (process.platform !== 'darwin') throw new DaemonError('UNSUPPORTED_PROTOCOL', 'macOS only')
  await shell.openExternal('x-apple.systempreferences:com.apple.LoginItems-Settings.extension')
})
handle('dmsg:vault-import-cancel', 'main', cancelImport)
handle('dmsg:vault-import-preview', 'main', async () => {
  cancelImport()
  const session = importSession
  const path = await pickFile({
    title: text.pickExport,
    filters: [
      { name: text.exportFile, extensions: ['1pux', 'csv', 'json'] },
      { name: text.allFiles, extensions: ['*'] }
    ]
  })
  if (!path || session !== importSession) return null
  const preview = (await daemon.call('vault/import_preview', { path })) as { digest: string }
  if (session !== importSession) return null
  importing = { path, digest: preview.digest }
  return { ...preview, session }
})
handle('dmsg:vault-import', 'main', async (_event, session: number, picks: number[], attachments: string[] = [], folder = 'root') => {
  if (!importing || session !== importSession) throw new DaemonError('VERSION_CONFLICT', 'Preview expired')
  if (!Array.isArray(picks) || picks.some((n) => !Number.isInteger(n) || n < 0) || !Array.isArray(attachments) || attachments.some((v) => typeof v !== 'string') || typeof folder !== 'string') throw new DaemonError('INVALID_INPUT', 'Import selection')
  const chosen = picks
  const source = importing
  // Consume before awaiting: two clicks must never submit the same batch twice.
  importing = null
  return daemon.call('vault/import', { ...source, picks: chosen, attachments, folder })
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
handle('dmsg:connection', 'any', () => ({ connected: daemon.connected, error: daemon.lastError ? 'DAEMON_DISCONNECTED' : undefined, recovery: daemon.recovery.state }))
/** What an MCP client runs to reach this daemon: the installed binary and its home. */
handle('dmsg:runtime', 'main', () => ({ binary: daemon.options.binary, home: daemon.options.home }))
handle('dmsg:fullscreen', 'main', () => main?.isFullScreen() === true)
handle('dmsg:updates', 'main', () => ({ version: app.getVersion(), state: updates.state }))
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
  open: (plan) => openNotice(plan.account, plan.target, plan.covered.length === 1 ? plan.covered[0].id : undefined, true),
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

async function openNotice(account: string, target: NoticeTarget, id?: string, native = false) {
  if (!isNoticeAccount(account) || !isNoticeTarget(target)) throw new DaemonError('INVALID_INPUT', 'Notification target')
  const entry = `/reminders?account=${account}${id ? `&item=${id}` : ''}`
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
    if (error instanceof DaemonError && ['NOT_FOUND', 'FORBIDDEN', 'EXPIRED'].includes(error.code)) return showMain(`${entry}&ended=1`)
    // The original view can show its last trusted state and offer a retry offline.
  }
  const current = await daemon.call('daemon/status').catch(() => null) as { account?: string; state?: string; session?: string } | null
  if (!current || current.account !== account || current.state !== 'unlocked' || current.session !== status.session) return showMain(entry)
  if (target.kind === 'approval') return showApproval(target.id)
  showMain(target.kind === 'summary' ? `${noticeRoute(target)}&account=${account}` : noticeRoute(target))
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
    if (process.platform === 'darwin') app.dock?.setBadge(badge > 99 ? '99+' : badge ? String(badge) : preferences.global.badge !== 'off' && counts.counts.unverified ? '•' : '')
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
handle('dmsg:notification-open', 'any', async (_event, account: string, id: string) => {
  if (!isNoticeAccount(account) || !isNoticeHash(id)) throw new DaemonError('INVALID_INPUT', 'Notification identity')
  const list = await daemon.call('notifications/list', { account, id }) as { items: NoticeItem[] }
  const item = list.items.find((item) => item.id === id && item.account === account)
  if (!item) return showMain(`/reminders?account=${account}&ended=1`)
  await openNotice(account, item.target, id)
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
    || (value.watermark !== undefined && !isNoticeSequence(value.watermark))) throw new DaemonError('INVALID_INPUT', 'Notification visibility')
  const seen = await daemon.call('notifications/observed', value) as { id: string; version: string; stage: number }
  if (noticeViewEpoch.get(sender) !== revision || window.isDestroyed() || !window.isFocused() || !window.isVisible()) return false
  noticeViewports.set(sender, seen)
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
    text
  )
}

/** A check asked for from the tray says what it found; the window shows it in Settings › About dMsg. */
async function checkFromTray() {
  const state = await updates.check()
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

daemon.on('event', (event: DaemonEvent) => {
  if (event.type === 'session/changed' || event.type === 'resync') cancelImport()
  broadcast('dmsg:event', event)
  if (['notifications/changed', 'session/changed', 'resync', 'sync/changed'].includes(event.type)) {
    noticeEpoch++
    if (event.type === 'session/changed' || event.type === 'resync') noticeViewports.clear()
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
  powerMonitor.on('lock-screen', () => { screenLocked = true; noticeViewports.clear(); noticeEpoch++; void refreshNotices() })
  powerMonitor.on('unlock-screen', () => { screenLocked = false; noticeEpoch++; void refreshNotices() })
  powerMonitor.on('resume', () => { noticeEpoch++; void refreshNotices() })
  if (process.platform === 'win32') Notification.handleActivation((details) => {
    const match = /^dmsg-notice:([A-Za-z0-9_-]{16})$/.exec(details.arguments)
    if (!match) { noticeActivation = 'unknown'; showMain('/reminders?ended=1'); return }
    if (!noticeHistoryLoaded) noticeActivation = match[1]
    else void nativeNotices.activate(match[1]).then((opened) => { if (!opened) showMain('/reminders?ended=1') })
  })
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
    updateTray('connecting')
  }
  // Opened at login, the app starts in the tray.
  if (!process.argv.includes('--hidden') && !app.getLoginItemSettings().wasOpenedAtLogin) showMain(noticeActivation ? '/reminders' : undefined)
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
  const link = pendingLink ?? process.argv.find((a) => a.startsWith('dmsg://'))
  if (link) void openLink(link)
})
