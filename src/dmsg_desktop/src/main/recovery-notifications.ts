/** Public recovery metadata, available even when the selected workspace is locked. */
export type RecoveryWarning = {
  account: string
  pending: boolean
  digest: string | null
  observed_at_ms: number
  stale: boolean
}

/** Once per request per desktop run; reconnects and lock/unlock do not repeat it. */
export class RecoveryNotifications {
  private shown = new Map<string, { digest: string; close: () => void }>()
  private notify: (account: string) => (() => void)

  constructor(notify: (account: string) => (() => void)) {
    this.notify = notify
  }

  update(account: string | null | undefined, warning: RecoveryWarning | null | undefined, supported: boolean) {
    if (!account || !warning || warning.account !== account) return
    const previous = this.shown.get(account)
    // An outage/unknown state must never erase a pending warning.
    if (!warning.pending) {
      if (!warning.stale) {
        previous?.close()
        this.shown.delete(account)
      }
      return
    }
    if (!supported || !warning.digest || previous?.digest === warning.digest) return
    previous?.close()
    this.shown.set(account, { digest: warning.digest, close: this.notify(account) })
  }
}
