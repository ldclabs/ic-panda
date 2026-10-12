/**
 * The bridge window's message handling (connect-v1 §4.2–§4.3): the hello
 * from `window.opener`, the registered origin taken from
 * `MessageEvent.origin`, one fresh nonce per command, the page's proof,
 * and results posted only to that origin. No DOM here: the window passes
 * its events and its opener in, and shows what the hooks report.
 */
import { validateOrigin } from 'dmsg-sdk'
import { DmsgError } from 'dmsg-sdk/errors'
import type { Bridge, Connection, Value } from './bridge.ts'
import { base64, hex, random, unbase64 } from './codec.ts'
import { ConnectError, ensure, type ConnectCode } from './errors.ts'
import { CREATE_METHODS, METHODS, PROTOCOL, parseCommand, type Command } from './protocol.ts'

const NONCE_MS = 15_000
const MAX_CHALLENGES = 16
const MAX_FRAME = 150_000
const ID = /^[0-9a-f]{64}$/

/** A window that receives frames: the opener. */
export interface Target {
  readonly closed?: boolean
  postMessage(message: unknown, targetOrigin: string): void
}

export interface PageEvent {
  source: unknown
  origin: string
  data: unknown
}

/** What the window shows. */
export interface PageHooks {
  /** `window.opener`, or null once it is gone. */
  opener(): Target | null
  connected(connection: Connection): void
  /** A hello was refused with `code` (§4.2). */
  refused(origin: string, code: ConnectCode): void
  ended(): void
  /** A method finished; `link` locates its relay request for `dmsg://connect/request`. */
  done(
    method: string,
    outcome: {
      value?: Value
      error?: ConnectCode
      /** For NOT_PAIRED: the account to pair, when the request names one. */
      account?: string
      link?: { account: string; requestId: string } | null
    }
  ): void
  /** Runs before a create method: Safari's click rule (§4.1). */
  beforeCreate(): Promise<void>
}

interface Challenge {
  command: Command
  nonce: Uint8Array
  expiresAt: number
}

interface Live extends Connection {
  source: Target
}

const keys = (value: object) => Object.keys(value).sort().join(',')

/** The error code a page sees for anything thrown while handling a frame. */
export function codeOf(error: unknown): ConnectCode {
  if (error instanceof ConnectError) return error.code
  if (error instanceof DmsgError) return 'INVALID_INPUT'
  return 'UNAVAILABLE'
}

export class BridgePage {
  private connection: Live | null = null
  private handshaking: Target | null = null
  private challenges = new Map<string, Challenge>()
  private readonly bridge: Bridge
  private readonly hooks: PageHooks
  /** Whether this browser meets the baseline (§4.1); otherwise every hello is refused. */
  supported = true

  constructor(bridge: Bridge, hooks: PageHooks) {
    this.bridge = bridge
    this.hooks = hooks
  }

  get current(): Connection | null {
    return this.connection
  }

  /** One `message` event. Anything not from the opener, or malformed, is ignored. */
  async receive(event: PageEvent): Promise<void> {
    const data = event.data as Record<string, unknown>
    if (!data || typeof data !== 'object' || Array.isArray(data) || data.protocol !== PROTOCOL) return
    try {
      if (JSON.stringify(data).length > MAX_FRAME) return
    } catch {
      return
    }
    if (data.type === 'hello') return this.hello(event, data)
    const live = this.connection
    if (!live || event.source !== live.source || event.origin !== live.origin) return
    if (typeof data.requestId !== 'string' || !ID.test(data.requestId)) return
    const requestId = data.requestId
    switch (data.type) {
      case 'capabilities':
        if (keys(data) !== 'protocol,requestId,type') return this.reply(live, requestId, { error: 'INVALID_INPUT' })
        return this.reply(live, requestId, { value: { protocol: PROTOCOL, methods: METHODS } })
      case 'request':
        return this.request(live, requestId, data)
      case 'proof':
        return this.proof(live, requestId, data)
    }
  }

  /** §4.2: the first valid hello from the opener; a new hello ends the old connection. */
  private async hello(event: PageEvent, data: Record<string, unknown>) {
    const opener = this.hooks.opener()
    if (!opener || event.source !== opener || this.handshaking === opener) return
    if (keys(data) !== 'appId,protocol,type' || typeof data.appId !== 'string' || !/^[a-z0-9-]{1,64}$/.test(data.appId)) return
    try {
      validateOrigin(event.origin, this.bridge.config.environment)
    } catch {
      return
    }
    this.end()
    this.handshaking = opener
    try {
      ensure(this.supported, 'UNSUPPORTED_BROWSER')
      const connection = await this.bridge.connect(event.origin, data.appId)
      if (this.hooks.opener() !== opener) return
      this.connection = { ...connection, source: opener }
      opener.postMessage(
        {
          protocol: PROTOCOL,
          type: 'ready',
          connectionId: hex(connection.connectionId),
          origin: connection.origin,
          appId: connection.appId,
          methods: METHODS
        },
        connection.origin
      )
      this.hooks.connected(connection)
    } catch (error) {
      const code = codeOf(error)
      opener.postMessage({ protocol: PROTOCOL, type: 'rejected', error: code }, event.origin)
      this.hooks.refused(event.origin, code)
    } finally {
      this.handshaking = null
    }
  }

  private request(live: Live, requestId: string, data: Record<string, unknown>) {
    if (keys(data) !== 'command,protocol,requestId,type') return this.reply(live, requestId, { error: 'INVALID_INPUT' })
    if (this.challenges.has(requestId)) return this.reply(live, requestId, { error: 'INVALID_INPUT' })
    const now = Date.now()
    for (const [id, c] of this.challenges) if (c.expiresAt <= now) this.challenges.delete(id)
    if (this.challenges.size >= MAX_CHALLENGES) return this.reply(live, requestId, { error: 'QUOTA_EXCEEDED' })
    let command: Command
    try {
      command = parseCommand(data.command)
      ensure(command.appId === live.appId, 'FORBIDDEN')
    } catch (error) {
      return this.reply(live, requestId, { error: codeOf(error) })
    }
    const nonce = random(32)
    this.challenges.set(requestId, { command, nonce, expiresAt: now + NONCE_MS })
    live.source.postMessage(
      { protocol: PROTOCOL, type: 'challenge', requestId, nonce: hex(nonce), origin: live.origin, connectionId: hex(live.connectionId) },
      live.origin
    )
  }

  /** §4.3: each nonce answers one proof, within 15 seconds, on this connection only. */
  private async proof(live: Live, requestId: string, data: Record<string, unknown>) {
    const challenge = this.challenges.get(requestId)
    this.challenges.delete(requestId)
    if (keys(data) !== 'protocol,requestId,signature,type') return this.reply(live, requestId, { error: 'INVALID_INPUT' })
    if (!challenge) return this.reply(live, requestId, { error: 'FORBIDDEN' })
    if (Date.now() >= challenge.expiresAt) return this.reply(live, requestId, { error: 'EXPIRED' })
    const { command } = challenge
    try {
      const signature = unbase64(data.signature as string, 64)
      ensure(signature.length === 64, 'INVALID_INPUT')
      const create = CREATE_METHODS.includes(command.method)
      if (create) await this.hooks.beforeCreate()
      const value = await this.bridge.call(live, command, challenge.nonce, signature)
      const link = this.bridge.request(live.origin, live.appId, command.operationId)
      this.hooks.done(command.method, { value, link })
      this.reply(live, requestId, { value })
    } catch (error) {
      const code = codeOf(error)
      const account = code === 'NOT_PAIRED' && error instanceof ConnectError && error.message !== code ? error.message : undefined
      this.hooks.done(command.method, { error: code, ...(account ? { account } : {}) })
      this.reply(live, requestId, { error: code })
    }
  }

  private reply(live: Live, requestId: string, outcome: { value?: unknown; error?: ConnectCode }) {
    // A reply to a connection that ended is dropped; the page asks again.
    if (this.connection !== live) return
    live.source.postMessage(
      outcome.error
        ? { protocol: PROTOCOL, type: 'result', requestId, ok: false, error: outcome.error }
        : { protocol: PROTOCOL, type: 'result', requestId, ok: true, value: outcome.value },
      live.origin
    )
  }

  /** Every second: the connection ends when its opener is gone or replaced (§4.2). */
  tick(): void {
    const live = this.connection
    if (!live) return
    const opener = this.hooks.opener()
    if (!opener || opener !== live.source || opener.closed) this.end()
  }

  private end() {
    if (!this.connection) return
    this.connection = null
    this.challenges.clear()
    this.hooks.ended()
  }
}

