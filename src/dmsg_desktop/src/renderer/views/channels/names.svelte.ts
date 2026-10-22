import { call } from '../../rpc'
import { shortId } from '../../shell/session'

/**
 * Display names of accounts, as their own certified PrincipalDocuments
 * publish them. A name is asked for the first time a view shows the account,
 * in batches; a found name is refreshed after ten minutes and a missing one
 * after one minute. An account without a document shows its short ID.
 */
const NAME_TTL = 600_000
const MISSING_TTL = 60_000
const known = $state<Record<string, { name: string | null; at: number }>>({})
const asked = new Set<string>()
let waiting: string[] = []
let timer: ReturnType<typeof setTimeout> | null = null

function ask(account: string) {
  if (asked.has(account)) return
  asked.add(account)
  waiting.push(account)
  timer ??= setTimeout(async () => {
    const all = waiting
    waiting = []
    timer = null
    // The daemon reads at most 100 accounts a call.
    for (let i = 0; i < all.length; i += 100) {
      const accounts = all.slice(i, i + 100)
      try {
        const names = await call<Record<string, string | null>>('channel/names', { accounts })
        for (const account of accounts) {
          // A failed read is omitted so the next view asks for it again; a
          // returned name stays until its TTL expires.
          if (account in names) {
            known[account] = { name: names[account] ?? null, at: Date.now() }
          }
          asked.delete(account)
        }
      } catch {
        // Asked again the next time a view shows them.
        for (const account of accounts) asked.delete(account)
      }
    }
  }, 30)
}

/** The account's published display name, or null; asks and refreshes it. */
export function nameOf(account: string | undefined): string | null {
  if (!account) return null
  const entry = known[account]
  const ttl = entry?.name === null ? MISSING_TTL : NAME_TTL
  if (!entry || (Date.now() - entry.at >= ttl && !asked.has(account))) {
    if (entry) asked.delete(account)
    ask(account)
  }
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
