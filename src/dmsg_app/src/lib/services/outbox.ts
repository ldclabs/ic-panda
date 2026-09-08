import type { OutboxJob } from '../models'
import { ensure } from '../errors'

export interface Receipt {
  sequence: number
  digest: string
}
export interface OutboxStore {
  list(): Promise<OutboxJob[]>
  put(job: OutboxJob): Promise<void>
}
export interface OutboxTransport {
  status(id: string): Promise<Receipt | 'not_found' | 'unknown'>
  publish(job: Readonly<OutboxJob>): Promise<Receipt>
}
/** Persistent phase precedes network I/O; retry sends the exact stored frame.
 * No encryption or all-powerful session credential belongs in this runner. */
export async function reconcileOutbox(
  store: OutboxStore,
  transport: OutboxTransport,
  now = Date.now()
) {
  for (const original of (await store.list()).slice(0, 25)) {
    if (
      !['queued', 'sending', 'unknown'].includes(original.state) ||
      original.nextAttempt > now
    )
      continue
    const job = { ...original }
    try {
      if (job.state === 'sending' || job.state === 'unknown') {
        const previous = await transport.status(job.id)
        if (previous === 'unknown') {
          job.state = 'unknown'
          job.nextAttempt = now + 60000
          await store.put(job)
          continue
        }
        if (previous !== 'not_found') {
          accept(job, previous)
          await store.put(job)
          continue
        }
      }
      job.state = 'sending'
      job.attempt++
      await store.put(job)
      const receipt = await transport.publish(Object.freeze({ ...job }))
      accept(job, receipt)
      await store.put(job)
    } catch (error) {
      job.state =
        error instanceof Error && error.message === 'INTEGRITY_FAILED' ? 'blocked' : 'unknown'
      job.error = job.state === 'blocked' ? 'INTEGRITY_FAILED' : 'EXECUTION_UNKNOWN'
      job.nextAttempt = now + Math.min(300000, 1000 * 2 ** Math.min(job.attempt, 8))
      await store.put(job)
    }
  }
}
function accept(job: OutboxJob, receipt: Receipt) {
  ensure(
    receipt.digest === job.digest &&
      Number.isSafeInteger(receipt.sequence) &&
      receipt.sequence > 0,
    'INTEGRITY_FAILED'
  )
  job.state = 'stored'
  job.receipt = receipt
  job.error = undefined
}
