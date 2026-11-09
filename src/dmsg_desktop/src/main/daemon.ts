/**
 * The desktop's connection to its `dmsg` daemon: `dmsg start` (which starts
 * the daemon unless one runs), the owner credential from `dmsg app-token`,
 * then JSON-RPC and events over `/ws/app/v1`. The daemon holds every key;
 * the desktop never starts a second one. Its calls, many of them reads on
 * events, are not activity for the idle lock; only `active()` is.
 */
import { EventEmitter } from 'node:events'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import WebSocket from 'ws'
import type { DaemonEvent } from '../shared/contract'
import { DaemonError } from './error.ts'
import { ReadRecovery } from './recovery.ts'
import { requireRuntimeVersion } from './runtime.ts'
import { isRead, requestKey } from '../shared/requests.ts'

const run = promisify(execFile)

/** Why a step failed: what a `dmsg` command wrote to stderr, else the error's message. */
export function reasonOf(error: unknown): string {
  const stderr = (error as { stderr?: unknown } | null)?.stderr
  return (typeof stderr === 'string' && stderr.trim()) || (error instanceof Error ? error.message : String(error))
}

/**
 * The code a window shows for a failed call. A reviewed read may publish
 * idempotent security evidence before fetching data, but cannot create a
 * business operation to duplicate. Only reads recover in the background, so
 * a write whose own reads failed asks to be tried again, not to be waited for.
 */
function settled(read: boolean, code: string): string {
  if (read) return code === 'EXECUTION_UNKNOWN' ? 'NETWORK_UNAVAILABLE' : code
  return code === 'NETWORK_UNAVAILABLE' ? 'UNAVAILABLE' : code
}

type Waiting = { method: string; read: boolean; resolve(value: unknown): void; reject(error: Error): void; timer?: NodeJS.Timeout }

type ConnectionAttempt = { task: Promise<void>; socket: WebSocket | null }
type Connection =
  | { kind: 'stopped' }
  | { kind: 'connecting' | 'initializing'; attempt: ConnectionAttempt }
  | { kind: 'ready'; socket: WebSocket }
  | { kind: 'waiting'; timer: NodeJS.Timeout }

/**
 * Library work that lasts as long as its bytes: a folder imported, a file
 * written out, the whole library checked. It has no time limit; a daemon
 * that goes away still fails it.
 */
const UNTIMED = new Set(['files/import', 'files/export', 'files/open', 'files/verify', 'files/cloud_fetch', 'files/cloud_verify', 'files/backup_run'])

export interface DaemonOptions {
  /** The `dmsg` binary; `runtime.ts` sets the installed one. */
  binary: string
  /** Packaged desktop version; do not attach to an older daemon still draining its work. */
  minimumVersion?: string
  /** The dMsg home. */
  home: string
  /** Test only: a daemon URL and credential instead of `dmsg app-token`. */
  fixed?: { url: string; token: string }
  /** Bounded test timings; production reads and writes use separate budgets. */
  timing?: { read?: number; write?: number; recovery?: number }
}

export class Daemon extends EventEmitter {
  readonly options: DaemonOptions
  lastError: string | undefined
  private connection: Connection = { kind: 'stopped' }
  private waiting = new Map<number, Waiting>()
  private next = 0
  private delay = 1000
  private inFlight = new Map<string, Promise<unknown>>()
  readonly recovery: ReadRecovery

  get connected() { return this.connection.kind === 'ready' }

  private get socket(): WebSocket | null {
    const state = this.connection
    return state.kind === 'ready' ? state.socket
      : state.kind === 'initializing' ? state.attempt.socket : null
  }

  private owns(attempt: ConnectionAttempt) {
    const state = this.connection
    return (state.kind === 'connecting' || state.kind === 'initializing') && state.attempt === attempt
  }

  constructor(options: DaemonOptions) {
    super()
    this.options = options
    this.recovery = new ReadRecovery(
      (method, params) => this.requestOnce(method, params),
      () => this.emit('state'),
      () => this.emit('event', { type: 'resync' }),
      options.timing?.recovery,
    )
  }

  /** The daemon's address and owner credential, starting the daemon if it is not running. */
  private async credential(): Promise<{ url: string; token: string }> {
    if (this.options.fixed) return this.options.fixed
    const home = ['--home', this.options.home]
    // Returns once the daemon answers; a bad config.toml fails here.
    const started = await run(this.options.binary, [...home, '--json', 'start'], { timeout: 45_000 })
    if (this.options.minimumVersion) {
      const status = JSON.parse(started.stdout.trim().split('\n').pop()!) as { version?: string }
      requireRuntimeVersion(status.version, this.options.minimumVersion)
    }
    const { stdout } = await run(this.options.binary, [...home, 'app-token'], { timeout: 15_000 })
    return JSON.parse(stdout.trim().split('\n').pop()!) as { url: string; token: string }
  }

  /** Connect now; a try already under way, such as the backoff's, is the same try. */
  connect(): Promise<void> {
    const state = this.connection
    if (state.kind === 'ready') return Promise.resolve()
    if (state.kind === 'connecting' || state.kind === 'initializing') return state.attempt.task
    if (state.kind === 'waiting') clearTimeout(state.timer)
    const task = Promise.resolve().then(() => this.tryConnect(attempt))
    const attempt: ConnectionAttempt = { task, socket: null }
    this.connection = { kind: 'connecting', attempt }
    return task
  }

  private async tryConnect(attempt: ConnectionAttempt): Promise<void> {
    try {
      const { url, token } = await this.credential()
      if (!this.owns(attempt)) return
      const ws = url.replace(/^http/, 'ws') + '/ws/app/v1'
      const socket = new WebSocket(ws, { handshakeTimeout: 10_000, headers: { authorization: `Bearer ${token}` } })
      attempt.socket = socket
      await new Promise<void>((resolve, reject) => {
        socket.once('open', () => resolve())
        socket.once('error', reject)
        socket.once('close', () => reject(new Error('connection closed')))
      })
      if (!this.owns(attempt)) { socket.terminate(); return }
      this.connection = { kind: 'initializing', attempt }
      socket.on('message', (data) => this.receive(String(data)))
      socket.on('close', () => { if (this.socket === socket) this.lost('connection closed') })
      socket.on('error', (error) => { if (this.socket === socket) this.lost(error.message) })
      // No business call can use this socket before initialization finishes.
      await this.requestOnce('app/initialize', { activity: 'reported' })
      if (!this.owns(attempt)) return
      this.connection = { kind: 'ready', socket }
      this.lastError = undefined
      this.delay = 1000
      // Windows hear of the connection once their reads no longer count as a person's activity.
      this.emit('state')
      this.emit('event', { type: 'resync' })
    } catch (error) {
      if (this.owns(attempt)) this.lost(reasonOf(error))
    }
  }

  /** Release the current attempt/socket before another one can take its place. */
  private stopConnection() {
    const state = this.connection
    this.connection = { kind: 'stopped' }
    if (state.kind === 'waiting') clearTimeout(state.timer)
    if (state.kind === 'connecting') {
      // Keep the handshake listeners: termination must settle its promise too.
      state.attempt.socket?.terminate()
    } else if (state.kind === 'initializing' || state.kind === 'ready') {
      const socket = state.kind === 'ready' ? state.socket : state.attempt.socket
      socket?.removeAllListeners().on('error', () => {}).terminate()
    }
    this.inFlight.clear()
    this.recovery.reset()
    for (const [, w] of this.waiting) {
      clearTimeout(w.timer)
      w.reject(new DaemonError(w.read ? 'DAEMON_DISCONNECTED' : 'EXECUTION_UNKNOWN', 'daemon connection lost'))
    }
    this.waiting.clear()
  }

  private lost(reason: string) {
    const was = this.connected
    const changed = this.lastError !== reason
    this.stopConnection()
    this.lastError = reason
    this.connection = { kind: 'waiting', timer: setTimeout(() => void this.connect(), this.delay) }
    this.delay = Math.min(this.delay * 2, 30_000)
    if (was || changed) this.emit('state')
  }

  private receive(text: string) {
    let message: { id?: number; result?: unknown; error?: { message: string; data?: { code?: string } }; method?: string; params?: DaemonEvent }
    try {
      message = JSON.parse(text)
    } catch {
      return
    }
    if (message.method === 'event' && message.params) {
      if (message.params.type === 'session/changed') {
        this.recovery.reset()
        this.inFlight.clear()
        for (const [id, waiting] of this.waiting) {
          // Login completion and socket initialization describe the daemon,
          // not account content; an unlock is expected while they run.
          if (waiting.read && !['session/login_result', 'app/initialize'].includes(waiting.method)) {
            clearTimeout(waiting.timer)
            this.waiting.delete(id)
            waiting.reject(new DaemonError('LOCKED', 'session changed'))
          }
        }
      }
      this.emit('event', message.params)
      return
    }
    const waiting = message.id === undefined ? undefined : this.waiting.get(message.id)
    if (!waiting) return
    this.waiting.delete(message.id!)
    clearTimeout(waiting.timer)
    if (message.error) waiting.reject(new DaemonError(settled(waiting.read, message.error.data?.code ?? 'UNAVAILABLE'), message.error.message))
    else waiting.resolve(message.result)
  }

  /** Coalesce concurrent identical calls, including double-clicked writes.
   * Only safe failed reads enter background recovery. Never replay a write. */
  call(method: string, params: unknown = null): Promise<unknown> {
    const key = requestKey(method, params)
    const existing = this.inFlight.get(key)
    if (existing) return existing
    const generation = this.recovery.generation
    const work = this.requestOnce(method, params).catch((error: DaemonError) => {
      if (this.connected && error.code === 'NETWORK_UNAVAILABLE') this.recovery.add(method, params, generation)
      throw error
    }).finally(() => {
      if (this.inFlight.get(key) === work) this.inFlight.delete(key)
    })
    this.inFlight.set(key, work)
    return work
  }

  /** One RPC only. A timed-out or disconnected write has an unknown result;
   * its daemon journal remains responsible for reconciliation. */
  private requestOnce(method: string, params: unknown = null): Promise<unknown> {
    const socket = this.socket
    if (!socket || (!this.connected && method !== 'app/initialize')) return Promise.reject(new DaemonError('DAEMON_DISCONNECTED', 'daemon disconnected'))
    const id = ++this.next
    // Keep only classification while awaiting a reply, not a write's plaintext
    // parameters for the entire timeout (including after the account locks).
    const read = isRead(method, params)
    return new Promise((resolve, reject) => {
      const timer = UNTIMED.has(method)
        ? undefined
        : setTimeout(() => {
            this.waiting.delete(id)
            reject(new DaemonError(read ? 'NETWORK_UNAVAILABLE' : 'EXECUTION_UNKNOWN', 'daemon response timed out'))
          }, read ? this.options.timing?.read ?? 45_000 : this.options.timing?.write ?? 180_000)
      this.waiting.set(id, { method, read, resolve, reject, timer })
      socket.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }))
    })
  }

  /** A person used a window. */
  active() {
    this.call('app/activity').catch(() => {})
  }

  close() {
    const was = this.connected
    this.stopConnection()
    if (was) this.emit('state')
  }
}
