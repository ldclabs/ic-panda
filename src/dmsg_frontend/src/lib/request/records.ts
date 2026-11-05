// What this browser sent through a link, as far as it can tell: the relay keeps
// each record's status, and its name and size in a receipt only the link opens.

import type { MineEntry } from './api'
import { openReceipt, type Receipt } from './seal'

export type RecordState =
  'uploading' | 'delivered' | 'saved' | 'withdrawn' | 'replaced' | 'removed'
export type SentRecord = {
  uploadId: string
  state: RecordState
  storedAt: number
  bytes: number
  receipt: Receipt | null
}

/** Open the receipts and tell a file replaced from one withdrawn. */
export async function readRecords(
  entries: MineEntry[],
  key: Uint8Array,
  requestId: string
): Promise<SentRecord[]> {
  const opened = await Promise.all(
    entries.map((entry) =>
      entry.receipt
        ? openReceipt(key, requestId, entry.upload_id, entry.receipt).catch(
            () => null
          )
        : null
    )
  )
  const replaced = new Set(
    opened.flatMap((r) => (r?.replaces ? [r.replaces] : []))
  )
  return entries
    .map((entry, index) => ({
      uploadId: entry.upload_id,
      state:
        entry.status === 'withdrawn' && replaced.has(entry.upload_id)
          ? ('replaced' as const)
          : entry.status,
      storedAt: entry.stored_at,
      bytes: entry.bytes,
      receipt: opened[index] ?? null
    }))
    .sort((a, b) => b.storedAt - a.storedAt)
}
