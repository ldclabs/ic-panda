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
  /** Start a login in the system browser; with `bind`, a second login to bind to the unlocked account. */
  login(origin: string, bind?: boolean): Promise<unknown>
  /**
   * Import a file into the vault: one the person dropped on the window, else
   * one they pick in a system dialog; null when cancelled. A `file/importing`
   * event names it before the import starts.
   */
  importFile(dropped?: File): Promise<unknown>
  /** Publish an image the person picks as the public avatar, or remove it; null when cancelled. */
  avatar(remove: boolean): Promise<unknown>
  /** Export a vault file to a place the person picks in a system dialog; null when cancelled. */
  exportFile(id: string, name: string): Promise<unknown>
  /** Copy a vault item's secret field to the clipboard, cleared after `clear_after_ms`; the main window only. */
  copySecret(id: string): Promise<{ clear_after_ms: number }>
  /** Save a channel message's file where the person picks; null when cancelled. */
  channelDownload(id: string, seq: number, name: string): Promise<unknown>
  /** Ask to attest an application action from a file the person picks; the approval request, or null. */
  attestAction(): Promise<unknown>
  /** Verify an attestation bundle the person picks, with its original file when `withFile`; null when cancelled. */
  verifyBundle(withFile: boolean): Promise<unknown>
  /** Save a finished attestation's bundle where the person picks; null when cancelled. */
  exportBundle(execution: string): Promise<{ path: string } | null>
  /** Open an old-site migration link (`/legacy#pair=` or `#approve=`) in the system browser. */
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
  onEvent(listener: (event: DaemonEvent) => void): () => void
  onNavigate(listener: (route: string) => void): () => void
  onLocale(listener: (locale: Locale) => void): () => void
}
