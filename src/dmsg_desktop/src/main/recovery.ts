import { isRead, requestKey } from '../shared/requests.ts'

export type RecoveryState = 'idle' | 'recovering' | 'offline'

/** Background recovery of failed reads, bound to one daemon/account session.
 * No write, approval, signing or payment operation can enter this queue. */
export class ReadRecovery {
  state: RecoveryState = 'idle'
  generation = 0
  private failed = new Map<string, { method: string; params: unknown }>()
  private timer: ReturnType<typeof setTimeout> | undefined
  private running = false
  private rounds = 0

  private read: (method: string, params: unknown) => Promise<unknown>
  private changed: () => void
  private recovered: () => void
  private delay: number

  constructor(
    read: (method: string, params: unknown) => Promise<unknown>,
    changed: () => void,
    recovered: () => void,
    delay = 5000,
  ) { this.read = read; this.changed = changed; this.recovered = recovered; this.delay = delay }

  add(method: string, params: unknown, generation: number) {
    if (generation !== this.generation || !isRead(method)) return
    this.failed.set(requestKey(method, params), { method, params })
    this.set(this.rounds ? 'offline' : 'recovering')
    this.schedule()
  }

  reset() {
    this.generation++
    this.failed.clear()
    clearTimeout(this.timer)
    this.timer = undefined
    this.rounds = 0
    this.set('idle')
  }

  private set(state: RecoveryState) {
    if (state !== this.state) { this.state = state; this.changed() }
  }

  private schedule() {
    if (this.timer || this.running || !this.failed.size) return
    this.timer = setTimeout(() => { this.timer = undefined; void this.retry() }, Math.min(this.delay * 2 ** this.rounds, 60_000))
  }

  async retry() {
    if (this.running || !this.failed.size) return
    clearTimeout(this.timer)
    this.timer = undefined
    const generation = this.generation
    this.running = true
    this.set('recovering')
    let recovered = false
    try {
      // Sequential reads keep an outage from causing a retry burst.
      for (const [key, request] of [...this.failed]) {
        if (generation !== this.generation) break
        try {
          await this.read(request.method, request.params)
          if (generation !== this.generation) break
          this.failed.delete(key)
          recovered = true
        } catch (error) {
          if (generation !== this.generation) break
          if ((error as { code?: string }).code !== 'NETWORK_UNAVAILABLE') {
            this.failed.delete(key)
            recovered = true // Reload to show the authoritative refusal.
          }
        }
      }
    } finally {
      this.running = false
      if (generation === this.generation) {
        // Backoff counts consecutive failed rounds; a drained queue starts over.
        this.rounds = this.failed.size ? this.rounds + 1 : 0
        this.set(this.failed.size ? 'offline' : 'idle')
        if (recovered) this.recovered()
      }
      this.schedule()
    }
  }
}
