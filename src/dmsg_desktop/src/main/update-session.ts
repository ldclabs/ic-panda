/** One serialized updater, with cancellation and install eligibility owned by the current channel. */
import type { UpdateChannel, UpdateState } from '../shared/contract.ts'
import type { DesktopRelease } from './release-feed.ts'

interface Options {
  enabled: boolean
  channel: UpdateChannel
  find: (channel: UpdateChannel, signal: AbortSignal) => Promise<DesktopRelease | null>
  download?: (release: DesktopRelease, signal: AbortSignal) => Promise<void>
  changed: () => void
}

export class UpdateSession {
  state: UpdateState
  channel: UpdateChannel
  applying = false
  private generation = 0
  private abort?: AbortController
  private candidate: DesktopRelease | null = null
  private work: Promise<void> = Promise.resolve()
  private pending?: { generation: number; result: Promise<UpdateState> }
  private options: Options

  constructor(options: Options) {
    this.options = options
    this.channel = options.channel
    this.state = { kind: options.enabled ? 'idle' : 'off' }
  }

  /** Persist the choice before calling this; failed persistence leaves the running channel alone. */
  changeChannel(channel: UpdateChannel) {
    if (this.applying) throw new Error('An update is being installed.')
    if (channel === this.channel) return
    this.channel = channel
    this.generation++
    this.abort?.abort()
    this.candidate = null
    this.set({ kind: this.options.enabled ? 'idle' : 'off' })
    void this.check()
  }

  check(): Promise<UpdateState> {
    if (this.applying || ['off', 'ready'].includes(this.state.kind)) return Promise.resolve(this.state)
    if (this.pending?.generation === this.generation) return this.pending.result
    const generation = this.generation
    this.set({ kind: 'checking' })
    // A cancelled check/download must settle before reusing the native updater for another feed.
    this.work = this.work.then(() => this.run(generation))
    const result = this.work.then(() => {
      if (this.pending?.generation === generation) this.pending = undefined
      return this.state
    })
    this.pending = { generation, result }
    return result
  }

  private async run(generation: number) {
    if (generation !== this.generation) return
    const abort = new AbortController()
    this.abort = abort
    const current = () => generation === this.generation && !abort.signal.aborted
    try {
      const release = await this.options.find(this.channel, abort.signal)
      if (!current()) return
      this.candidate = release
      if (!release) return this.set({ kind: 'latest' })
      if (!this.options.download) return this.set({ kind: 'available', version: release.version })
      this.set({ kind: 'downloading', version: release.version })
      await this.options.download(release, abort.signal)
      if (current()) this.set({ kind: 'ready', version: release.version })
    } catch (error) {
      if (current()) {
        this.candidate = null
        this.set({ kind: 'error', message: (error instanceof Error ? error.message : String(error)).split('\n')[0] })
      }
    }
  }

  /** Freeze channel changes only while the person-requested installation waits for idle and exits. */
  beginApply(): DesktopRelease | null {
    if (this.applying || !['ready', 'available'].includes(this.state.kind) || !this.candidate) return null
    if (this.channel === 'stable' && this.candidate.channel !== 'stable') return null
    this.applying = true
    this.options.changed()
    return this.candidate
  }

  endApply() {
    this.applying = false
    this.options.changed()
  }

  installFailed(error: Error) {
    this.applying = false
    this.candidate = null
    this.set({ kind: 'error', message: error.message.split('\n')[0] })
  }

  private set(state: UpdateState) {
    this.state = state
    this.options.changed()
  }
}
