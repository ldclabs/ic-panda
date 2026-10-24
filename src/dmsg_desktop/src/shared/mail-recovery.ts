/** Recover retained mailbox metadata page by page; the daemon persists each verified page. */
export async function recoverMailboxes(
  hosts: string[],
  list: (input: { host: string; limit: number; cursor?: string }) => Promise<{ next_cursor?: string }>,
  active: () => boolean = () => true
): Promise<boolean> {
  for (const host of new Set(hosts.map((host) => host.trim()).filter(Boolean))) {
    const seen = new Set<string>()
    let cursor: string | undefined
    let pages = 0
    do {
      if (!active()) return false
      if (++pages > 100) throw new Error('Mailbox inventory exceeded the page limit')
      const page = await list({ host, limit: 100, ...(cursor ? { cursor } : {}) })
      cursor = page.next_cursor
      if (cursor !== undefined) {
        if (!cursor || seen.has(cursor)) throw new Error('Mailbox inventory cursor did not advance')
        seen.add(cursor)
      }
    } while (cursor)
  }
  return active()
}
