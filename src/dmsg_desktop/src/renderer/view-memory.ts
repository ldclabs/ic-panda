/**
 * What the main window's views want back when they open again, as a person
 * comes back to where they were. IDs and scroll positions stay until the
 * account changes. Plaintext (a reply being written, attachments named) goes as
 * soon as the account locks, with the views that showed it.
 */
const places = new Map<string, unknown>()
const plain = new Map<string, unknown>()
let account: string | null = null
let unlocked = false

/** A status arrived: another account starts with nothing, and only an unlocked one keeps plaintext. */
export function settleMemory(status: { account?: string | null; state?: string } | null) {
  const next = status?.account ?? null
  if (next !== account) {
    places.clear()
    plain.clear()
    account = next
  }
  unlocked = status?.state === 'unlocked'
  if (!unlocked) plain.clear()
}

/** The session changed and the next status is not known yet: plaintext goes now. */
export const forgetPlain = () => {
  unlocked = false
  plain.clear()
}

export const remembered = <T>(key: string): T | undefined => places.get(key) as T | undefined
export const remember = (key: string, value: unknown) => {
  if (value === null || value === undefined) places.delete(key)
  else places.set(key, value)
}

/** Whether a route can be gone back to: not one that asks for something once, as `?open=` does. */
const lasting = (route: string) => !/\?(?!tab=\w+$)/.test(route)

/** Where a view was left, for its sidebar entry and shortcut to come back to. */
export const noteRoute = (view: string, route: string) => {
  if (lasting(route)) places.set(`route:${view}`, route)
}
export const lastRoute = (view: string) => remembered<string>(`route:${view}`) ?? view

export const rememberedPlain = <T>(key: string): T | undefined => plain.get(key) as T | undefined
export const rememberPlain = (key: string, value: unknown) => {
  // Old views tear down after session/changed has already cleared this cache.
  if (unlocked) plain.set(key, value)
}
