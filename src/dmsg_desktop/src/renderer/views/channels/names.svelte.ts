import { call } from '../../rpc'
import { shortId } from '../../shell/session'

/**
 * Display names of accounts, as their own profiles publish them and the
 * daemon checks against their certified devices. A name is asked for the
 * first time a view shows the account, in batches; an account without a
 * profile shows its short ID.
 */
const known = $state<Record<string, string | null>>({})
const asked = new Set<string>()
let waiting: string[] = []
let timer: ReturnType<typeof setTimeout> | null = null

function ask(account: string) {
  if (asked.has(account)) return
  asked.add(account)
  waiting.push(account)
  timer ??= setTimeout(async () => {
    const accounts = waiting
    waiting = []
    timer = null
    try {
      const names = await call<Record<string, string | null>>('channel/names', { accounts })
      for (const account of accounts) known[account] = names[account] ?? null
    } catch {
      // Asked again the next time a view shows them.
      for (const account of accounts) asked.delete(account)
    }
  }, 30)
}

/** The account's published display name, or null; asks for it once. */
export function nameOf(account: string | undefined): string | null {
  if (!account) return null
  if (!(account in known)) ask(account)
  return known[account] ?? null
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
