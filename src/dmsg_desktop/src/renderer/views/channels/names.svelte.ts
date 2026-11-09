import { call, CallError } from '../../rpc'
import { shortId } from '../../shell/session'

/**
 * Display names of accounts, as their own certified PrincipalDocuments
 * publish them. A name is asked for the first time a view shows the account,
 * in batches. Freshness and persistence belong to the unlocked daemon.
 */
type Name = { name: string | null; avatar_url?: string | null; stale: boolean }
const known = $state<Record<string, Name>>({})
const avatars = $state<Record<string, string | null>>({})
const avatarRequests = new Map<string, number>()
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
  for (const url of Object.keys(avatars)) delete avatars[url]
  avatarRequests.clear()
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
        const names = await call<Record<string, Name>>('channel/names', { accounts })
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
export function nameOf(account: string | undefined, fetch = true): string | null {
  if (!account || !session) return null
  const entry = known[account]
  if (fetch && !asked.has(account)) ask(account)
  return entry?.name ?? null
}

/** How a view names an account: its display name, else its short ID. */
export function label(account: string | undefined, fetch = true): string {
  return nameOf(account, fetch) ?? shortId(account ?? '')
}

/** Only the daemon fetches published images; rows share one request per URL. */
export function avatarOf(account: string | undefined): string | null {
  if (!account || !session) return null
  if (!asked.has(account)) ask(account)
  const url = known[account]?.avatar_url
  if (!url) return null
  if ((avatarRequests.get(url) ?? 0) <= Date.now()) {
    avatarRequests.set(url, Infinity)
    const read = generation
    void (async () => {
      for (const refresh of [false, true]) {
        if (read !== generation) return
        const image = await call<{ url: string } | null>('channel/avatar', { avatar_url: url, refresh })
        if (read === generation) avatars[url] = image?.url ?? null
      }
    })()
      .catch(() => { /* A missing avatar keeps the initials. */ })
      .finally(() => {
        if (read === generation) avatarRequests.set(url, Date.now() + (avatars[url] ? 86_400_000 : 60_000))
      })
  }
  return avatars[url] ?? null
}

const XID = /^[0-9a-v]{19}[0g]$/

/** The account an account ID or a dMsg name stands for. */
export async function accountOf(text: string): Promise<string> {
  const value = text.trim().replace(/^@/, '')
  if (XID.test(value)) return value
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_]{0,18}$/.test(value)) throw new CallError('INVALID_INPUT: recipient')
  const found = await call<{ account: string } | null>('handle/resolve', { name: value.toLowerCase() })
  if (!found) throw new CallError('NOT_FOUND: name')
  if (!XID.test(found.account)) throw new CallError('INTEGRITY_FAILED: recipient')
  return found.account
}
