/** The narrow surface the renderer sees as `window.dmsg`; the daemon credential never crosses it. */
import type { Appearance } from './appearance'
import type { Locale } from './locales'
import type { NoticePermission, NoticeTarget } from './notifications'
import type { ShortcutBindings, ShortcutStatus } from './shortcuts'

/** A daemon event, as `/ws/app/v1` sends it. */
export interface DaemonEvent {
  seq?: number
  type: string
  data?: unknown
}

/** Where the desktop's connection to its daemon stands. */
export interface Connection {
  connected: boolean
  /** Why the last try failed, as `dmsg` said it; a window folds this under its details. */
  error?: string
  recovery?: 'idle' | 'recovering' | 'offline'
}

/**
 * Where the app's own update stands: `off` in a development run; `available`
 * when this package cannot install it (built without the update feed) and
 * its release page has it; `ready` once downloaded, to install after the
 * daemon stopped for it.
 */
export type UpdateState =
  | { kind: 'off' | 'idle' | 'checking' | 'latest' }
  | { kind: 'downloading' | 'available' | 'ready'; version: string }
  | { kind: 'error'; message: string }

export type UpdateChannel = 'stable' | 'beta'

/** The app's version and its update. */
export interface AppUpdates {
  version: string
  state: UpdateState
  channel: UpdateChannel
  homebrew: boolean
  applying: boolean
}

export interface NoticeVisibility {
  /** Loaded settings/plan destination: local acknowledgement only, never a business observation. */
  acknowledgeOnly?: boolean
  account: string
  session: string
  target?: NoticeTarget
  id?: string
  version?: string
  watermark?: string
}

export interface DesktopSettings {
  /** Lock dMsg when the screen locks or the computer sleeps. */
  lock_on_screen_lock: boolean
  open_at_login: boolean
}

export interface DesktopBridge {
  /** Ask the OS for camera access from the main window's scan action. */
  cameraAccess(): Promise<boolean>
  /** Scan only an image chosen in the OS picker or on the clipboard. */
  deviceQrImage(source: 'file' | 'clipboard'): Promise<unknown>
  /** OS support is distinct from permission; unqueryable permission stays system-managed. */
  notificationPermission(): Promise<NoticePermission>
  /** Open the fixed OS notifications settings page. */
  notificationSettings(): Promise<void>
  /** A generic, silent local test; no business counters or watermarks change. */
  notificationTest(): Promise<void>
  /** Open an item or category summary under its original local account. */
  notificationOpen(account: string, id: string, version?: string): Promise<void>
  /** Report only loaded, actually visible business data; null clears this window's context. */
  notificationVisible(value: NoticeVisibility | null): Promise<boolean>
  /** Which window this is: the main window or the approval window. */
  role: 'main' | 'approval'
  /** The operating system, for the window chrome: `darwin`, `win32` or `linux`. */
  platform: string
  /** The interface language when this window opened; `onLocale` follows changes. */
  locale: Locale
  /** Keep the language the person picked; the main window only. The tray, dialogs and every window follow. */
  setLocale(locale: Locale): Promise<void>
  /** The appearance when this window opened. */
  appearance: Appearance
  /** Keep the appearance the person picked; the main window only. Every window follows through `prefers-color-scheme`. */
  setAppearance(appearance: Appearance): Promise<void>
  /** A daemon method the desktop allows a window to call. */
  call(method: string, params?: unknown): Promise<unknown>
  /** Approve or reject a request after the person saw it; the approval window only. */
  decide(id: string, digest: string, approve: boolean): Promise<unknown>
  /** Connect a browser whose label the person confirmed; the main window only. */
  grant(invite: string, pairing: string): Promise<unknown>
  /**
   * Start a login in the system browser; with `bind`, a second login to bind
   * to the unlocked account. Resolves to `{url, expires_at}` once the page opened.
   */
  login(origin: string, bind?: boolean, options?: { provider?: 'ii' | 'passkey'; action?: 'login' | 'register' | 'logout_all' | 'reset_sessions'; credentialId?: string }): Promise<{ url: string; expires_at: number; provider?: 'ii' | 'passkey' }>
  /** Open a started login's page again, such as after its browser tab was closed. */
  openLogin(url: string): Promise<void>
  /** The `dmsg` binary and home an MCP client runs `dmsg mcp` with. */
  runtime(): Promise<{ binary: string; home: string }>
  /**
   * Add files to the library's `folder`: those the person dropped on the
   * window, else the ones they pick in a system dialog (files, or with
   * `directory` one folder): `{names, files}`, what was picked and the
   * daemon's imported files, or null when cancelled, also at the question a
   * name already in the folder asks. A `files/adding` event names what was
   * picked, and the paths of the folders among it, before the import starts.
   */
  filesAdd(folder: string, dropped?: File[], directory?: boolean, attach?: boolean): Promise<unknown>
  /**
   * Add what was pasted to the library's `folder`, both kept when a name is
   * taken: copied files by their paths, else the image on the system
   * clipboard, read by the main process. Resolves as `filesAdd` does.
   */
  filesPaste(folder: string, pasted: File[]): Promise<unknown>
  /** Export library files' current versions into a folder the person picks: `{dir, count}`, or null when cancelled. */
  filesExportMany(items: { id: string; name: string }[]): Promise<unknown>
  /**
   * Change the file library's settings, `files_mount` (after the person
   * allows it in a system dialog) and `files_trash_days`; resolves to the
   * daemon's `files/preferences`, or null when the dialog was dismissed.
   * `idle_lock_minutes` is kept in the same file.
   */
  filesPreferences(changes: { files_mount?: boolean; files_trash_days?: number; idle_lock_minutes?: number }): Promise<unknown>
  /** Export a library file's version to a place the person picks in a system dialog; null when cancelled. */
  filesExport(id: string, version: string | null, name: string): Promise<unknown>
  /** Open a library file's version in its program, from a temporary copy the daemon cleans up. */
  /** The system's own preview of a file, macOS only. */
  filesPreview(id: string, name: string): Promise<void>
  /** Hands a dragged file's decrypted copy to the system's drag. */
  filesDrag(id: string, name: string): Promise<void>
  /** The approval window has done its work. */
  closeApproval(): Promise<void>
  filesOpen(id: string, version: string | null): Promise<unknown>
  /** Upload an image the person picks as the public avatar; null when cancelled. */
  avatar(): Promise<unknown>
  /** Pay with the key of a PEM file the person picks, or with `login` with the login again; null when cancelled. */
  payerPem(login: boolean): Promise<unknown>
  /**
   * Copy a vault item's hidden field (by default its main secret; a
   * one-time password as its current code) to the clipboard, marked to stay
   * out of clipboard histories and cleared after `clear_after_ms`, or when
   * dMsg locks or quits; `revision` reads an earlier version. The main
   * window only.
   */
  copySecret(id: string, field?: string | null, revision?: string | null): Promise<{ clear_after_ms: number }>
  /** Read a password manager's export the person picks, without hidden values; null when cancelled. */
  secretScanDirectory(): Promise<any>
  secretMountPrepare(input: any, windowMs: number): Promise<any>
  secretMountSave(session: number): Promise<any>
  /** macOS only: System Settings at Login Items & Extensions, where the secret-files extension is enabled. */
  openExtensionSettings(): Promise<void>
  vaultImportPreview(): Promise<unknown>
  /** Add the selected items of this preview session, once: `{ids}`. */
  vaultImport(session: number, picks: number[], attachments?: string[], folder?: string): Promise<{ ids: string[] }>
  /** Discard the current preview and any file read still in progress. */
  vaultImportCancel(): Promise<void>
  /** The text of a QR code in an image the person picks, or on the clipboard: `{uri}`; null when cancelled. */
  vaultOtpScan(source: 'file' | 'clipboard'): Promise<{ uri: string } | null>
  /** Open an http or https address in the system browser. */
  openUrl(url: string): Promise<void>
  /** Save a channel message's file where the person picks; null when cancelled. */
  channelDownload(id: string, seq: number, name: string): Promise<unknown>
  /** Ask to attest an application action from a file the person picks; the approval request, or null. */
  attestAction(): Promise<unknown>
  /**
   * Ask to attest a digest or file statement (`statement` without its
   * `sha256`) over a file the person picks, hashed now and again on
   * approval; the approval request, or null when cancelled.
   */
  attestFile(statement: Record<string, unknown>): Promise<unknown>
  /**
   * Compare a file the person picks with the digest of request `id`; the
   * request with the comparison, or null when cancelled. The approval window only.
   */
  checkFile(id: string): Promise<unknown>
  /** Show the main window at `route`, such as `/login` to log in; the approval window only. */
  showMain(route: string): Promise<void>
  /** Verify an attestation bundle the person picks, with its original file when `withFile`; null when cancelled. */
  verifyBundle(withFile: boolean): Promise<unknown>
  /** Verify a bundle the person dropped on the window, with the original file dropped with it. */
  verifyDropped(bundle: File, original?: File): Promise<unknown>
  /** Verify one of the account's own attestations by its execution ID, as a recipient would its bundle. */
  verifyEntry(execution: string): Promise<unknown>
  /** Save a verification report where the person picks; null when cancelled. */
  saveReport(report: unknown): Promise<{ path: string } | null>
  /** Save a finished attestation's bundle where the person picks; null when cancelled. */
  exportBundle(execution: string): Promise<{ path: string } | null>
  /** Open an old-site migration link (`/legacy#pair=`) in the system browser. */
  openLegacy(link: string): Promise<void>
  /** Import a legacy transfer file the person picks in a system dialog; null when cancelled. */
  legacyImport(nonce: string, fingerprint: string, principal: string): Promise<unknown>
  /** Compare a kept archive with a frozen-history file the person picks; null when cancelled. */
  legacyFrozen(id: string): Promise<unknown>
  /** Save an old attachment where the person picks; null when cancelled. */
  legacyFile(id: string, source: string, name: string): Promise<unknown>
  /** Start pairing a browser: opens the pairing page in the system browser. */
  pair(copyLink: boolean): Promise<{ invite_id: string; url: string; expires_at: number }>
  /** Open the approval window on a request. */
  openApproval(id: string): Promise<void>
  connection(): Promise<Connection>
  /** Try the daemon again now instead of at the next backoff. */
  reconnect(): Promise<void>
  /** Quit the app; for a window that cannot reach its daemon. */
  quit(): Promise<void>
  /** This app's own choices; `desktop/changed` events follow a change from the tray. */
  desktopSettings(): Promise<DesktopSettings>
  setDesktopSettings(change: Partial<DesktopSettings>): Promise<DesktopSettings>
  shortcuts(): Promise<ShortcutStatus>
  setShortcuts(bindings: ShortcutBindings): Promise<ShortcutStatus>
  captureShortcut(active: boolean): Promise<void>
  /** Follow a `dmsg://` link found in a message or letter; it moves the view and executes nothing. */
  openLink(url: string): Promise<void>
  /** Copy an image a window shows, by its data address. */
  copyImage(url: string): Promise<void>
  /** The app's version and where its update stands; the main window only. `updates/changed` events follow it. */
  updates(): Promise<AppUpdates>
  /** Opt this installation into beta releases, or stop receiving them without downgrading. */
  setUpdateChannel(channel: UpdateChannel): Promise<AppUpdates>
  /** Check for an update now; resolves with where the update then stands. */
  checkUpdates(): Promise<UpdateState>
  /** Restart into a downloaded update once the daemon stopped for it, or open the release page of one this package cannot install. */
  applyUpdate(): Promise<void>
  onEvent(listener: (event: DaemonEvent) => void): () => void
  onNavigate(listener: (route: string) => void): () => void
  onLocale(listener: (locale: Locale) => void): () => void
  /** Whether the main window is in full screen, now and at each change; macOS hides the traffic lights there. */
  onFullScreen(listener: (fullScreen: boolean) => void): () => void
}
