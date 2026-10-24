/** The narrow surface the renderer sees as `window.dmsg`; the daemon credential never crosses it. */
import type { Appearance } from './appearance'
import type { Locale } from './locales'

/** A daemon event, as `/ws/app/v1` sends it. */
export interface DaemonEvent {
  seq?: number
  type: string
  data?: unknown
}

/** Where the desktop's connection to its daemon stands. */
export interface Connection {
  connected: boolean
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

/** The app's version and its update. */
export interface AppUpdates {
  version: string
  state: UpdateState
}

export interface DesktopBridge {
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
  login(origin: string, bind?: boolean): Promise<{ url: string; expires_at: number }>
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
  filesAdd(folder: string, dropped?: File[], directory?: boolean): Promise<unknown>
  /** Export library files' current versions into a folder the person picks: `{dir, count}`, or null when cancelled. */
  filesExportMany(items: { id: string; name: string }[]): Promise<unknown>
  /**
   * Change the file library's settings, `files_mount` (after the person
   * allows it in a system dialog) and `files_trash_days`; resolves to the
   * daemon's `files/preferences`, or null when the dialog was dismissed.
   */
  filesPreferences(changes: { files_mount?: boolean; files_trash_days?: number }): Promise<unknown>
  /** Export a library file's version to a place the person picks in a system dialog; null when cancelled. */
  filesExport(id: string, version: string | null, name: string): Promise<unknown>
  /** Open a library file's version in its program, from a temporary copy the daemon cleans up. */
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
  /** The app's version and where its update stands; the main window only. `updates/changed` events follow it. */
  updates(): Promise<AppUpdates>
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
