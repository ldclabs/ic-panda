/**
 * Applying an update stops the daemon first, the way `dmsg update` does: the
 * daemon stops only when nothing approved, no account update and no root
 * rotation is left unsettled (plan DT10). The desktop waits up to a minute
 * and otherwise leaves the update for later; nothing is lost either way.
 */
import type { Text } from './text'

export interface InFlight {
  approvals?: number
  account_update?: boolean
  root_rotation?: boolean
}

export type Stopped = { stopped: true } | { stopped: false; inFlight: InFlight }

/** What is in flight, for people. */
export function describe(inFlight: InFlight | undefined, text: Text): string {
  const parts: string[] = []
  if (inFlight?.approvals) parts.push(text.inFlightApprovals(inFlight.approvals))
  if (inFlight?.account_update) parts.push(text.inFlightAccount)
  if (inFlight?.root_rotation) parts.push(text.inFlightRoot)
  return parts.join(text.separator)
}

/**
 * Ask the daemon to stop once idle, every two seconds until `waitMs`. A daemon
 * that does not answer is not running, which is as good as stopped.
 */
export async function stopForUpdate(
  call: (method: string, params: unknown) => Promise<unknown>,
  waitMs = 60_000,
  sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
  now: () => number = Date.now
): Promise<Stopped> {
  const deadline = now() + waitMs
  for (;;) {
    let reply: { stopped?: boolean; in_flight?: InFlight }
    try {
      reply = (await call('daemon/stop', { if_idle: true })) as typeof reply
    } catch {
      return { stopped: true }
    }
    if (reply.stopped) return { stopped: true }
    if (now() >= deadline) return { stopped: false, inFlight: reply.in_flight ?? {} }
    await sleep(2_000)
  }
}
