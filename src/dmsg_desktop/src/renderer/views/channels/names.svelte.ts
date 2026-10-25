import { call } from '../../rpc'
import { shortId } from '../../shell/session'

/**
 * Display names of accounts, as their own certified PrincipalDocuments
 * publish them. A name is asked for the first time a view shows the account,
 * in batches. Freshness and persistence belong to the unlocked daemon.
 */
const known = $state<Record<string, { name: string | null; stale: boolean }>>({})
const asked = new Set<string>()
const pending = new Set<string>()
const active = new Set<string>()
const rerun = new Set<string>()
let session: string | null = null
let generation = 0
const waiting = new Set<string>()
let timer: ReturnType<typeof setTimeout> | null = null

/** Called before publishing a new session to views; late responses cannot refill it. */
export function namesSession(status: any) {
  const next = status?.state === 'unlocked' ? `${status.session ?? ''}:${status.account}` : null
  if (next === session) return
  session = next
  generation++
  if (timer) clearTimeout(timer)
  timer = null
  waiting.clear()
  asked.clear()
  pending.clear()
  active.clear()
  rerun.clear()
  for (const id of Object.keys(known)) delete known[id]
}

/** Only incomplete/stale local reads need rereading after a batch commits. */
export function namesChanged() {
  if (!session) return
  for (const id of pending) {
    if (active.has(id)) { rerun.add(id); continue }
    asked.delete(id)
    ask(id)
  }
}

/** A visible window periodically rereads locally; only the daemon owns TTLs. */
export function refreshNames() {
  if (!session) return
  for (const id of [...asked]) {
    if (active.has(id)) { rerun.add(id); continue }
    asked.delete(id)
    ask(id)
  }
}

function ask(account: string) {
  if (!session || asked.has(account)) return
  asked.add(account)
  pending.add(account)
  waiting.add(account)
  timer ??= setTimeout(async () => {
    const read = generation
    const all = [...waiting]
    waiting.clear()
    timer = null
    // The daemon reads at most 100 accounts a call.
    for (let i = 0; i < all.length; i += 100) {
      const accounts = all.slice(i, i + 100)
      for (const account of accounts) active.add(account)
      try {
        const names = await call<Record<string, { name: string | null; stale: boolean }>>('channel/names', { accounts })
        if (read !== generation) return
        for (const account of accounts) {
          // Freshness belongs to the daemon. Events can arrive before this
          // response, so in-flight names stay eligible for one local reread.
          if (account in names) {
            known[account] = names[account]
          }
          if (!(account in names) || names[account].stale) pending.add(account)
          else pending.delete(account)
        }
      } catch {
        // Asked again the next time a view shows them.
        if (read !== generation) return
        for (const account of accounts) pending.add(account)
      }
      for (const account of accounts) {
        active.delete(account)
        if (rerun.delete(account)) { asked.delete(account); ask(account) }
      }
    }
  }, 30)
}

/** The account's published display name, or null; asks and refreshes it. */
export function nameOf(account: string | undefined): string | null {
  if (!account || !session) return null
  const entry = known[account]
  if (!asked.has(account)) ask(account)
  return entry?.name ?? null
}

/** How a view names an account: its display name, else its short ID. */
export function label(account: string | undefined): string {
  return nameOf(account) ?? shortId(account ?? '')
}

const XID = /^[0-9a-v]{19}[0g]$/

/** The account an account ID or a dMsg name stands for. */
export async function accountOf(text: string): Promise<string> {
  const value = text.trim().replace(/^@/, '')
  if (XID.test(value)) return value
  const found = await call<{ account: string } | null>('handle/resolve', { name: value })
  if (!found?.account) throw new Error('NOT_FOUND: name')
  return found.account
}
