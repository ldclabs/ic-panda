/**
 * The desktop's connection to its `dmsg` daemon: the owner credential from
 * `dmsg app-token`, then JSON-RPC and events over `/ws/app/v1`. The daemon
 * holds every key; the desktop never starts a second one.
 */
import { EventEmitter } from 'node:events'
import { execFile, spawn } from 'node:child_process'
import { promisify } from 'node:util'
import WebSocket from 'ws'
import type { DaemonEvent } from '../shared/contract'

const run = promisify(execFile)

type Waiting = { resolve(value: unknown): void; reject(error: Error): void; timer: NodeJS.Timeout }

export class DaemonError extends Error {
  readonly code: string
  constructor(code: string, message: string) {
    super(message)
    this.code = code
  }
}

export interface DaemonOptions {
  /** The `dmsg` binary. */
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
    const ask = async () => {
      const { stdout } = await run(this.options.binary, ['--home', this.options.home, 'app-token'], {
        timeout: 15_000
      })
      return JSON.parse(stdout.trim().split('\n').pop()!) as { url: string; token: string }
    }
    try {
      return await ask()
    } catch {
      const child = spawn(this.options.binary, ['--home', this.options.home, 'daemon'], {
        detached: true,
        stdio: 'ignore'
      })
      child.unref()
      for (let i = 0; i < 30; i++) {
        await new Promise((r) => setTimeout(r, 500))
        try {
          return await ask()
        } catch {
          /* still starting */
        }
      }
      throw new DaemonError('DAEMON_STOPPED', 'The dmsg daemon did not start')
    }
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
      this.emit('state')
      await this.call('app/initialize')
    } catch (error) {
      this.lost(error instanceof Error ? error.message : String(error))
    }
  }

  private lost(reason: string) {
    if (this.socket) this.socket.removeAllListeners()
    this.socket = null
    const was = this.connected
    this.connected = false
    this.lastError = reason
    for (const [, w] of this.waiting) {
      clearTimeout(w.timer)
      w.reject(new DaemonError('UNAVAILABLE', reason))
    }
    this.waiting.clear()
    if (was) this.emit('state')
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

  close() {
    this.closed = true
    clearTimeout(this.retry)
    this.socket?.close()
  }
}
