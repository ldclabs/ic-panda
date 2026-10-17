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
import { DaemonError } from './error'

const run = promisify(execFile)

/** Why a step failed: what a `dmsg` command wrote to stderr, else the error's message. */
export function reasonOf(error: unknown): string {
  const stderr = (error as { stderr?: unknown } | null)?.stderr
  return (typeof stderr === 'string' && stderr.trim()) || (error instanceof Error ? error.message : String(error))
}

type Waiting = { resolve(value: unknown): void; reject(error: Error): void; timer: NodeJS.Timeout }

export interface DaemonOptions {
  /** The `dmsg` binary; `runtime.ts` sets the installed one. */
  binary: string
  /** The dMsg home. */
  home: string
  /** Test only: a daemon URL and credential instead of `dmsg app-token`. */
  fixed?: { url: string; token: string }
}

export class Daemon extends EventEmitter {
  readonly options: DaemonOptions
  connected = false
  lastError: string | undefined
  private socket: WebSocket | null = null
  private waiting = new Map<number, Waiting>()
  private next = 0
  private retry: NodeJS.Timeout | undefined
  private delay = 1000
  private closed = false

  constructor(options: DaemonOptions) {
    super()
    this.options = options
  }

  /** The daemon's address and owner credential, starting the daemon if it is not running. */
  private async credential(): Promise<{ url: string; token: string }> {
    if (this.options.fixed) return this.options.fixed
    const home = ['--home', this.options.home]
    // Returns once the daemon answers; a bad config.toml fails here.
    await run(this.options.binary, [...home, 'start'], { timeout: 45_000 })
    const { stdout } = await run(this.options.binary, [...home, 'app-token'], { timeout: 15_000 })
    return JSON.parse(stdout.trim().split('\n').pop()!) as { url: string; token: string }
  }

  async connect(): Promise<void> {
    this.closed = false
    clearTimeout(this.retry)
    try {
      const { url, token } = await this.credential()
      const ws = url.replace(/^http/, 'ws') + '/ws/app/v1'
      const socket = new WebSocket(ws, { headers: { authorization: `Bearer ${token}` } })
      await new Promise<void>((resolve, reject) => {
        socket.once('open', () => resolve())
        socket.once('error', reject)
      })
      this.socket = socket
      socket.on('message', (data) => this.receive(String(data)))
      socket.on('close', () => this.lost('connection closed'))
      socket.on('error', (error) => this.lost(error.message))
      this.connected = true
      this.lastError = undefined
      this.delay = 1000
      // Windows hear of the connection once their reads no longer count as a person's activity.
      await this.call('app/initialize', { activity: 'reported' })
      this.emit('state')
    } catch (error) {
      this.lost(reasonOf(error))
    }
  }

  private lost(reason: string) {
    // A connection given up on, such as one whose `app/initialize` failed, closes too.
    this.socket?.removeAllListeners().on('error', () => {}).terminate()
    this.socket = null
    const was = this.connected
    const changed = this.lastError !== reason
    this.connected = false
    this.lastError = reason
    for (const [, w] of this.waiting) {
      clearTimeout(w.timer)
      w.reject(new DaemonError('UNAVAILABLE', reason))
    }
    this.waiting.clear()
    if (was || changed) this.emit('state')
    if (!this.closed) {
      clearTimeout(this.retry)
      this.retry = setTimeout(() => void this.connect(), this.delay)
      this.delay = Math.min(this.delay * 2, 30_000)
    }
  }

  private receive(text: string) {
    let message: { id?: number; result?: unknown; error?: { message: string; data?: { code?: string } }; method?: string; params?: DaemonEvent }
    try {
      message = JSON.parse(text)
    } catch {
      return
    }
    if (message.method === 'event' && message.params) {
      this.emit('event', message.params)
      return
    }
    const waiting = message.id === undefined ? undefined : this.waiting.get(message.id)
    if (!waiting) return
    this.waiting.delete(message.id!)
    clearTimeout(waiting.timer)
    if (message.error) waiting.reject(new DaemonError(message.error.data?.code ?? 'UNAVAILABLE', message.error.message))
    else waiting.resolve(message.result)
  }

  /** One method; approvals that execute may take a while. */
  call(method: string, params: unknown = null): Promise<unknown> {
    const socket = this.socket
    if (!socket || !this.connected) return Promise.reject(new DaemonError('UNAVAILABLE', 'Not connected to the dmsg daemon'))
    const id = ++this.next
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id)
        reject(new DaemonError('UNAVAILABLE', `${method} timed out`))
      }, 180_000)
      this.waiting.set(id, { resolve, reject, timer })
      socket.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }))
    })
  }

  /** A person used a window. */
  active() {
    this.call('app/activity').catch(() => {})
  }

  close() {
    this.closed = true
    clearTimeout(this.retry)
    this.socket?.close()
  }
}
