export interface Outgoing {
  message_id: string
  draft?: Record<string, unknown> & { text?: string; reply_to?: { seq: number } }
  file?: { name: string; size: number; source?: string }
  sending?: boolean
  queued?: boolean
  error?: string
  created_at?: number
}

export interface Delivery extends Outgoing {
  channel: string
  state: 'queued' | 'sending' | 'failed' | 'confirmed'
  message?: any
}

/** An older send waits for the person, who may no longer mean it. */
const RECOVER_MS = 24 * 60 * 60 * 1000
/** A connection that keeps failing is not asked again on every event. */
const RESUME_GAP_MS = 30_000

/** Session-owned sends. Navigation releases views, not their delivery queue. */
export class MessageDeliveries {
  revision = 0
  private generation = 0
  private rows: Delivery[] = []
  private jobs = new Map<string, Promise<void>>()
  private tails = new Map<string, { key: string; task: Promise<void> }>()
  private attempts = new Map<string, { item: Outgoing; prepare: () => Promise<unknown>; work: () => Promise<{ message: any }>; at: number; born: number }>()
  private adopted = new Set<string>()
  private changed: (rows: Delivery[]) => void
  private ready: () => boolean

  constructor(changed: (rows: Delivery[]) => void, ready: () => boolean = () => true) {
    this.changed = changed
    this.ready = ready
  }

  submit(channel: string, item: Outgoing, prepare: () => Promise<unknown>, work: () => Promise<{ message: any }>): Promise<void> {
    const key = `${channel}:${item.message_id}`
    const running = this.jobs.get(key)
    if (running) return running
    const generation = this.generation, previous = this.tails.get(channel)?.task
    const now = Date.now()
    this.attempts.set(key, { item, prepare, work, at: now, born: this.attempts.get(key)?.born ?? item.created_at ?? now })
    const pending: Delivery = { ...item, channel, state: 'queued', queued: true, sending: false, error: undefined }
    this.put(pending)
    // Every intent is journaled immediately, including messages waiting for
    // a prior network request. A rejection is consumed here, not left as an
    // unhandled promise while the preceding message is still running.
    const preparation = this.ready()
      ? prepare().then(() => ({ error: undefined }), error => ({ error }))
      : Promise.resolve({ error: { code: 'UNAVAILABLE' } })
    const task: Promise<void> = (async () => {
      await previous
      if (generation !== this.generation) return
      try {
        const prepared = await preparation
        if (generation !== this.generation) return
        if (prepared.error) throw prepared.error
        if (!this.ready()) throw { code: 'UNAVAILABLE' }
        this.put({ ...pending, state: 'sending', queued: false, sending: true })
        const result = await work()
        if (generation !== this.generation) return
        if (!result?.message || typeof result.message.seq !== 'number') throw { code: 'INTEGRITY_FAILED' }
        this.attempts.delete(key)
        this.put({ ...pending, state: 'confirmed', queued: false, sending: false, message: result.message })
        const completed = this.rows.filter(row => row.state === 'confirmed')
        if (completed.length > 100) {
          const retained = new Set(completed.slice(-100))
          this.rows = this.rows.filter(row => row.state !== 'confirmed' || retained.has(row))
          this.publish()
        }
      } catch (error) {
        if (generation !== this.generation) return
        const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : ''
        this.put({ ...pending, state: 'failed', queued: false, sending: false, error: /^[A-Z_]+$/.test(code) ? code : 'UNAVAILABLE' })
      } finally {
        if (generation === this.generation) {
          this.jobs.delete(key)
          if (this.tails.get(channel)?.key === key) this.tails.delete(channel)
        }
      }
    })()
    this.jobs.set(key, task)
    this.tails.set(channel, { key, task })
    return task
  }

  /** The connection returned: failed sends without a verdict go again under their own IDs. */
  resume(now = Date.now()) {
    if (!this.ready()) return
    for (const row of this.rows) {
      const key = `${row.channel}:${row.message_id}`, attempt = this.attempts.get(key)
      if (row.state !== 'failed' || !attempt || !uncertainSend(row)) continue
      if (now - attempt.born >= RECOVER_MS || now - attempt.at < RESUME_GAP_MS) continue
      void this.submit(row.channel, attempt.item, attempt.prepare, attempt.work)
    }
  }

  /** A send left in the outbox by an earlier run is picked up once, when its channel opens. */
  adopt(channel: string, item: Outgoing, now = Date.now()) {
    const key = `${channel}:${item.message_id}`
    if (!this.ready() || this.adopted.has(key) || this.attempts.has(key) || this.jobs.has(key)) return false
    this.adopted.add(key)
    return uncertainSend(item) && item.created_at !== undefined && now - item.created_at < RECOVER_MS
  }

  /** Only snapshots started after confirmation may replace the ACK's rows. */
  loaded(channel: string, sequences: number[], since: number) {
    if (since !== this.revision) return
    const found = new Set(sequences)
    const rows = this.rows.filter(row => row.channel !== channel || row.state !== 'confirmed' || !found.has(row.message?.seq))
    if (rows.length !== this.rows.length) { this.rows = rows; this.publish() }
  }

  discard(channel: string, id: string) {
    if (this.jobs.has(`${channel}:${id}`)) return
    this.attempts.delete(`${channel}:${id}`)
    this.rows = this.rows.filter(row => row.channel !== channel || row.message_id !== id)
    this.publish()
  }

  reset() {
    this.generation++
    this.jobs.clear(); this.tails.clear(); this.attempts.clear(); this.adopted.clear(); this.rows = []
    this.publish()
  }

  private put(row: Delivery) {
    const index = this.rows.findIndex(old => old.channel === row.channel && old.message_id === row.message_id)
    this.rows = index < 0 ? [...this.rows, row] : this.rows.map((old, i) => i === index ? row : old)
    this.publish()
  }

  private publish() { this.revision++; this.changed([...this.rows]) }
}

export function uncertainSend(item: Outgoing) {
  return !item.error || ['EXECUTION_UNKNOWN', 'UNAVAILABLE', 'NETWORK_UNAVAILABLE', 'CANCELLED'].includes(item.error)
}
