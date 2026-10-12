/** The narrow surface the renderer sees as `window.dmsg`; the daemon credential never crosses it. */

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
  /** A daemon method the desktop allows a window to call. */
  call(method: string, params?: unknown): Promise<unknown>
  /** Approve or reject a request after the person saw it; the approval window only. */
  decide(id: string, digest: string, approve: boolean): Promise<unknown>
  /** Connect a browser whose label the person confirmed; the main window only. */
  grant(invite: string, pairing: string): Promise<unknown>
  /** Start a login in the system browser. */
  login(origin: string): Promise<unknown>
  /** Start pairing a browser: opens the pairing page in the system browser. */
  pair(copyLink: boolean): Promise<{ invite_id: string; url: string; expires_at: number }>
  /** Open the approval window on a request. */
  openApproval(id: string): Promise<void>
  connection(): Promise<Connection>
  onEvent(listener: (event: DaemonEvent) => void): () => void
  onNavigate(listener: (route: string) => void): () => void
}
