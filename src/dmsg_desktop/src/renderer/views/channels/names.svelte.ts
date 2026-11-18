import { untrack } from 'svelte'
import type { Attachment } from 'svelte/attachments'
import { visibility } from '../../components/visibility'
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
const asked = new Map<string, number>()
let refresh = $state(0)
let changed = $state(0)
const pending = new Set<string>()
const active = new Set<string>()
const rerun = new Set<string>()
let session: string | null = null
let generation = $state(0)
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
  if (!session || !pending.size) return
  for (const id of pending) asked.delete(id)
  pending.clear()
  changed++
}

/** Only current display consumers react. Historical accounts stay cached until shown again. */
export function refreshNames() {
  if (session) refresh++
}

// A search index needs initial names (including their completion events), not a periodic refresh of every candidate.
type Demand = boolean | 'search'

/** List rows render cached text. Only intersecting rows subscribe to refreshes. */
export function visibleName(account: string | undefined): Attachment {
  return node => {
    if (!account) return
    let visible = $state(false)
    $effect(() => { if (visible) ask(account, true) })
    return visibility(next => { visible = next })(node)
  }
}

function ask(account: string, demand: Demand) {
  if (!demand) return
  void generation
  if (!session) return
  void changed
  const cycle = demand === 'search' ? untrack(() => refresh) : refresh
  if (asked.get(account) === cycle || (demand === 'search' && asked.has(account))) return
  asked.set(account, cycle)
  pending.add(account)
  if (active.has(account)) { rerun.add(account); return }
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
      let invalidated = false
      for (const account of accounts) {
        active.delete(account)
        if (rerun.delete(account)) { asked.delete(account); invalidated = true }
      }
      if (invalidated) changed++
    }
  }, 30)
}

/** The account's published display name, or null; asks and refreshes it. */
export function nameOf(account: string | undefined, fetch: Demand = true): string | null {
  if (!account || !session) return null
  const entry = known[account]
  ask(account, fetch)
  return entry?.name ?? null
}

/** How a view names an account: its display name, else its short ID. */
export function label(account: string | undefined, fetch: Demand = true): string {
  return nameOf(account, fetch) ?? shortId(account ?? '')
}

/** Only the daemon fetches published images; rows share one request per URL. */
export function avatarOf(account: string | undefined): string | null {
  if (!account || !session) return null
  ask(account, true)
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
