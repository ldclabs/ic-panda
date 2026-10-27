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
import { APPROVAL_MAIN_ROUTES, externalAllowed, legacyAllowed, parseLink, reminderDue, windowParams } from './policy'
import { resolveRuntime } from './runtime'
import { RecoveryNotifications, type RecoveryWarning } from './recovery-notifications'
import { textFor } from './text'
import { trayVariant } from './tray-state'
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
/** The main window, closed in macOS full screen, hides once it left full screen; showing it cancels that. */
let hideAfterFullScreen = false
let approval: BrowserWindow | null = null
let tray: Tray | null = null
let trayImages: Record<ReturnType<typeof trayVariant>, Electron.NativeImage> | null = null
let quitting = false
let waiting = 0
/** When a waiting request last notified. */
let requestNotifiedAt = 0
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

handle('dmsg:call', 'any', (_event, method: string, params: unknown) => daemon.call(method, windowParams(method, params)))
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

/** A `connect/hint` arrived since the last refresh: a web request waits at the relay. */
let hinted = false

const recoveryNotifications = new RecoveryNotifications((account) => {
  const note = new Notification({ title: 'dMsg', body: text.recoveryPending, silent: false })
  note.on('click', async () => {
    // A notification may outlive an account switch; never cancel anything here.
    const status = await daemon.call('daemon/status').catch(() => null) as { account?: string } | null
    showMain(status?.account === account ? '/settings/security' : '/settings/session')
  })
  note.show()
  return () => note.close()
})

/** Notify without saying from where; clicking opens the request, never decides it. */
const refreshWaiting = single(async () => {
  const hint = hinted
  hinted = false
  let status: { approvals_waiting?: number; channels_unread?: number; state?: string; account?: string; recovery?: RecoveryWarning | null } = {}
  try {
    status = (await daemon.call('daemon/status')) as typeof status
  } catch {
    return
  }
  if (status.state !== 'unlocked') void forgetCopied()
  recoveryNotifications.update(status.account, status.recovery, Notification.isSupported())
  const now = status.approvals_waiting ?? 0
  // A burst of requests, such as an agent's, notifies once.
  const quiet = Date.now() - requestNotifiedAt < 10_000
  if ((now > waiting || hint) && !quiet && Notification.isSupported()) {
    requestNotifiedAt = Date.now()
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
  // The dock counts the requests waiting for a decision and what is unread
  // in channels that are not muted.
  app.setBadgeCount(now + (status.channels_unread ?? 0))
  updateTray(status.state ?? 'unknown')
})

/** When each channel last notified, so a busy one notifies at most every ten seconds. */
const channelNotifiedAt: Record<string, number> = {}

/**
 * New messages in a channel that is not muted, while unlocked and the main
 * window is not in front: the channel's name and how many, never who wrote
 * what. Clicking opens the channel.
 */
function notifyChannel(data: unknown) {
  const { id, name, received = 0, muted } = (data ?? {}) as { id?: string; name?: string; received?: number; muted?: boolean }
  if (!id || received <= 0 || muted || !Notification.isSupported()) return
  if (main && !main.isDestroyed() && main.isVisible() && main.isFocused()) return
  if (Date.now() - (channelNotifiedAt[id] ?? 0) < 10_000) return
  channelNotifiedAt[id] = Date.now()
  const note = new Notification({ title: name || 'dMsg', body: text.channelMessages(received) })
  note.on('click', () => showMain(`/channels?open=${id}`))
  note.show()
}

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

/** A newly decrypted Agent letter; never disclose its sender or contents. */
function notifyAgentMail(data: unknown) {
  const { agent, received = 0 } = (data ?? {}) as { agent?: string; received?: number }
  if (!agent || received <= 0 || !Notification.isSupported()) return
  const note = new Notification({ title: 'dMsg', body: text.agentMailArrived(received) })
  note.on('click', () => showMain(`/agents/mail/${encodeURIComponent(agent)}`))
  note.show()
}

/** When each kind of hint last notified, so a busy channel notifies once a minute. */
const hintedAt: Record<string, number> = {}

/** A locked daemon's hint: something new arrived, never from whom or what. */
function notifyHint(data: unknown) {
  const kind = (data as { kind?: string } | null)?.kind
  const body = kind === 'channel' ? text.newMessages : kind === 'inbox' ? text.newLetter : null
  if (!kind || !body || !Notification.isSupported() || Date.now() - (hintedAt[kind] ?? 0) < 60_000) return
  hintedAt[kind] = Date.now()
  const note = new Notification({ title: 'dMsg', body })
  note.on('click', () => showMain(kind === 'channel' ? '/channels' : '/inbox'))
  note.show()
}

/** Which reminder each term showed last, in days before its end, so each shows once. */
const remindersFile = () => join(app.getPath('userData'), 'reminders.json')
/** When the plan was last read for a reminder: at most once an hour. */
let remindedAt = 0

/**
 * A term that ends without a renewal queued: 30, 7 and 1 days before its
 * end, once each, read while unlocked. Clicking opens the plan; paying is
 * the approval window's.
 */
const remindPlan = single(async () => {
  if (Date.now() - remindedAt < 3_600_000 || !Notification.isSupported()) return
  type Term = { contract: string; plan: string; ends_at_ms: number; terminated_at_ms?: number | null }
  let status: { current?: Term | null; next?: unknown }
  try {
    const snapshot = await daemon.call('plan/status', { mode: 'local' }) as { data: typeof status | null }
    if (!snapshot.data) return
    status = snapshot.data
  } catch {
    return
  }
  remindedAt = Date.now()
  const current = status.current
  if (!current || status.next || current.terminated_at_ms != null) return
  let shown: Record<string, number> = {}
  try {
    shown = JSON.parse(await readFile(remindersFile(), 'utf8')) as Record<string, number>
  } catch {
    /* none shown yet */
  }
  const left = current.ends_at_ms - Date.now()
  const due = reminderDue(left, shown[current.contract])
  if (due === null) return
  shown[current.contract] = due
  await writeFile(remindersFile(), JSON.stringify(shown)).catch(() => undefined)
  const note = new Notification({ title: 'dMsg', body: text.planEnds(Math.max(1, Math.ceil(left / 86_400_000)), current.plan) })
  note.on('click', () => showMain('/plan'))
  note.show()
})

daemon.on('event', (event: DaemonEvent) => {
  if (event.type === 'session/changed' || event.type === 'resync') cancelImport()
  broadcast('dmsg:event', event)
  if (event.type === 'session/changed') void remindPlan()
  if (event.type === 'connect/hint') hinted = true
  if (['approval/changed', 'session/changed', 'connect/hint', 'resync', 'channel/changed', 'channel/read', 'recovery/changed'].includes(event.type)) void refreshWaiting()
  if (event.type === 'channel/changed') notifyChannel(event.data)
  if (event.type === 'mcp-events/changed') notifyEvents(event.data)
  if (event.type === 'agent/mail') notifyAgentMail(event.data)
  if (event.type === 'hint') notifyHint(event.data)
})
// The tray follows the connection: the daemon's state once connected, else connecting again.
daemon.on('state', () => {
  if (!daemon.connected) cancelImport()
  broadcast('dmsg:event', { type: 'connection/changed' })
  if (daemon.connected) {
    void refreshWaiting()
    void remindPlan()
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

app.whenReady().then(async () => {
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
  // A running app checks again for a term about to end every six hours.
  setInterval(() => void remindPlan(), 6 * 3_600_000)
  const link = pendingLink ?? process.argv.find((a) => a.startsWith('dmsg://'))
  if (link) void openLink(link)
})
