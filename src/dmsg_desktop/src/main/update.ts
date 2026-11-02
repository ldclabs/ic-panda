/**
 * Applying an update stops the daemon first, the way `dmsg update` does: the
 * daemon stops only when nothing approved, no account update and no root
 * rotation is left unsettled (plan DT10). After a minute, the person can
 * defer or explicitly interrupt: the saved work resumes in the new daemon.
 */
import type { Text } from './text'

export interface InFlight {
  approvals?: number
  account_update?: boolean
  root_rotation?: boolean
}

/** What is in flight, for people. */
export function describe(inFlight: InFlight | undefined, text: Text): string {
  const parts: string[] = []
  if (inFlight?.approvals) parts.push(text.inFlightApprovals(inFlight.approvals))
  if (inFlight?.account_update) parts.push(text.inFlightAccount)
  if (inFlight?.root_rotation) parts.push(text.inFlightRoot)
  return parts.join(text.separator)
}

/**
 * Ask the daemon to stop once idle, every two seconds until `waitMs`. Only
 * an explicit choice may interrupt unfinished work. Errors and lost replies
 * do not prove the daemon stopped, so they must not allow installation.
 */
export async function stopForUpdate(
  call: (method: string, params: unknown) => Promise<unknown>,
  confirmInterrupt: (inFlight: InFlight) => Promise<boolean>,
  waitMs = 60_000,
  sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
  now: () => number = Date.now
): Promise<boolean> {
  const deadline = now() + waitMs
  let ifIdle = true
  for (;;) {
    const reply = await call('daemon/stop', { if_idle: ifIdle }) as { stopped?: boolean; in_flight?: InFlight } | null
    if (reply?.stopped === true) return true
    if (reply?.stopped !== false || !ifIdle) throw new Error('The dMsg daemon did not confirm that it stopped.')
    if (now() >= deadline) {
      if (!await confirmInterrupt(reply.in_flight ?? {})) return false
      ifIdle = false
    } else {
      await sleep(2_000)
    }
  }
}
