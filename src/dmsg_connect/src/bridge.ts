/**
 * The bridge page's logic without a window (connect-v1 §4–§6): pairing,
 * the checks before a request leaves the browser, sealing to the account's
 * approving devices, and opening, verifying and pinning their replies.
 *
 * The page's origin comes from the caller, which in a real page is
 * `MessageEvent.origin`; everything else the bridge reads is either signed
 * by the page's session key or certified by the IC. Pairings and
 * operations live in memory here; the page keeps them in IndexedDB.
 */
import { ed25519 } from '@noble/curves/ed25519.js'
import { Principal } from '@icp-sdk/core/principal'
import { Certified, type AppRegistration, type CertifiedAccount, type IcConfig } from './certified.ts'
import { canonical, decodeCanonical, decodeMaps, equal, hex, random, randomId, unb64, unbase64, unhex } from './codec.ts'
import { aesSeal, hpkeOpenAuth, hpkeSeal, p256Pair, p256Verify, rawPublic, resultPair, sign1, spki } from './crypto.ts'
import { ConnectError, ensure, fail } from './errors.ts'
import {
  CREATE_METHODS,
  CREATE_TTL_MS,
  KINDS,
  PROTOCOL,
  READ_TTL_MS,
  REQUEST_TYP,
  operationDigest,
  pairingId,
  pairingMac,
  parseCommand,
  parseInvite,
  parseResponse,
  proofMessage,
  requestAad,
  responseInfo,
  wrapInfo,
  type Command,
  type Environment
} from './protocol.ts'
import { Relay, type Signer } from './relay.ts'

const COMMAND_TYP = 'application/vnd.dmsg.command+cose;v=1'
const GRANT_ACTION = 'dmsg/connect/pairing/grant/v1'
const PAIRING_TTL_MS = 180 * 86_400_000
const MAX_OPEN = 20
const MAX_PER_ORIGIN = 5
/** States after which a device holds the operation. */
const PINNED = ['authorized', 'execution_unknown', 'signed', 'returned', 'failed']

export interface BridgeConfig {
  environment: Environment
  relay: string
  ic: IcConfig
}

export interface Pairing {
  account: Uint8Array
  id: Uint8Array
  key: CryptoKeyPair
  pub: Uint8Array
  label: string
  state: 'candidate' | 'active'
  home?: string
  grant?: string
  expiresAt?: number
}

interface Channel {
  requestId: Uint8Array
  pairing: Pairing
  resultKey: CryptoKeyPair
  seen: number
  device: Uint8Array | null
  expiresAt: number
}

/** An operation value (§4.6). */
export interface Value {
  operationId: string
  kind: string
  state: string
  result?: unknown
  resultDigest?: string
  error?: string
}

/** The relay's view of one request. */
interface Reply {
  state: string
  version: number
  device_id: string | null
  sealed: string | null
  sealed_version: number | null
  acked_digest: string | null
}

interface Operation {
  origin: string
  command: Command
  digest: Uint8Array
  kind: string
  channel: Channel
  delivered: string | null
  last: Value | null
}

export interface Connection {
  connectionId: Uint8Array
  origin: string
  appId: string
}

/** Signs a proof message with the page's session key, as the page does after a challenge. */
export type PageSigner = (message: Uint8Array) => Promise<Uint8Array>

const key = (origin: string, c: Pick<Command, 'appId' | 'operationId'>) => `${origin}\n${c.appId}\n${c.operationId}`

const XID = '0123456789abcdefghijklmnopqrstuv'
/** Canonical Xid text of a 12-byte account ID. */
export function xidText(id: Uint8Array): string {
  ensure(id.length === 12, 'INVALID_INPUT')
  // 96 bits big-endian and four zero bits, in twenty 5-bit groups.
  let bits = 0n
  for (const byte of id) bits = (bits << 8n) | BigInt(byte)
  bits <<= 4n
  let text = ''
  for (let i = 19; i >= 0; i--) text += XID[Number((bits >> BigInt(i * 5)) & 31n)]
  return text
}

export class Bridge {
  readonly config: BridgeConfig
  readonly relay: Relay
  readonly certified: Certified
  readonly pairings = new Map<string, Pairing>()
  readonly operations = new Map<string, Operation>()
  private reads = new Map<string, Channel[]>()

  constructor(config: BridgeConfig) {
    this.config = config
    this.relay = new Relay(config.relay)
    this.certified = new Certified(config.ic)
  }

  private signer(pairing: Pairing): Signer {
    return { account: pairing.account, pairingId: pairing.id, key: pairing.key.privateKey }
  }

  /** §5.2: read the link the device opened, make a pairing key, submit the candidate. */
  async pair(fragment: string, label: string): Promise<{ account: string; pairingId: string }> {
    const invite = parseInvite(fragment)
    ensure(invite.environment === this.config.environment, 'FORBIDDEN', 'The link is from another environment')
    ensure(label.length > 0 && new TextEncoder().encode(label).length <= 64, 'INVALID_INPUT')
    const keyPair = await p256Pair()
    const pub = await spki(keyPair.publicKey)
    const pairing: Pairing = { account: invite.account_id, id: pairingId(pub), key: keyPair, pub, label, state: 'candidate' }
    const mac = await pairingMac(invite.secret, invite.environment, invite.account_id, invite.invite_id, pub, label)
    await this.relay.call(this.signer(pairing), 'POST', '/v1/connect/pairings', {
      account_id: invite.account_id,
      invite_id: invite.invite_id,
      pairing_pub: pub,
      label,
      mac
    })
    this.pairings.set(hex(invite.account_id), pairing)
    return { account: xidText(invite.account_id), pairingId: hex(pairing.id) }
  }

  /** Poll until the device grants the candidate, then verify the grant. */
  async awaitGrant(account: Uint8Array, timeoutMs: number): Promise<Pairing> {
    const pairing = this.pairings.get(hex(account))
    ensure(pairing, 'NOT_PAIRED')
    const deadline = Date.now() + timeoutMs
    for (;;) {
      const status = await this.relay.call<{ state: string; grant: string | null }>(
        this.signer(pairing),
        'GET',
        `/v1/connect/pairings/${hex(pairing.id)}`
      )
      if (status.state === 'active' && status.grant) {
        await this.acceptGrant(pairing, status.grant)
        return pairing
      }
      ensure(status.state === 'pending', 'NOT_PAIRED', `pairing ${status.state}`)
      ensure(Date.now() < deadline, 'DEVICE_UNAVAILABLE')
      await new Promise((r) => setTimeout(r, 500))
    }
  }

  /** §5.3: the grant is a device command of an active FormalApprove device of the account, for this key. */
  async acceptGrant(pairing: Pairing, grant: string): Promise<void> {
    const cose = unb64(grant, 262_144)
    ensure(cose[0] === 0xd2, 'INTEGRITY_FAILED')
    const parts = decodeMaps(cose.subarray(1)) as unknown[]
    ensure(Array.isArray(parts) && parts.length === 4, 'INTEGRITY_FAILED')
    const [protectedBytes, , payload, signature] = parts as [Uint8Array, unknown, Uint8Array, Uint8Array]
    const headers = decodeMaps(protectedBytes) as Map<number, unknown>
    ensure(headers instanceof Map && headers.get(1) === -19 && headers.get(16) === COMMAND_TYP, 'INTEGRITY_FAILED')
    const device = headers.get(4) as Uint8Array
    const issuer = (headers.get(15) as Map<number, string>)?.get(1)
    const body = decodeCanonical<{ protocol: string; action: string; payload: Record<string, unknown> }>(payload)
    ensure(body.protocol === 'dmsg-cloud/1' && body.action === GRANT_ACTION, 'INTEGRITY_FAILED')
    const g = body.payload as {
      pairing_id: Uint8Array
      pairing_pub: Uint8Array
      label: string
      home: Uint8Array
      granted_at: number
      expires_at: number
    }
    const home = Principal.fromUint8Array(g.home).toText()
    const account = await this.certified.account(home, pairing.account)
    const signer = account.devices.find((d) => equal(d.device_id, device))
    ensure(
      signer?.active &&
        signer.capabilities.includes('FormalApprove') &&
        issuer === account.issuer &&
        ed25519.verify(
          signature,
          canonical(['Signature1', protectedBytes, new Uint8Array(), payload]),
          signer.signing_pub,
          { zip215: false }
        ),
      'INTEGRITY_FAILED',
      'Grant not signed by an approving device'
    )
    ensure(
      equal(g.pairing_id, pairing.id) &&
        equal(g.pairing_pub, pairing.pub) &&
        g.label === pairing.label &&
        g.granted_at < g.expires_at &&
        g.expires_at - g.granted_at <= PAIRING_TTL_MS &&
        Date.now() < g.expires_at,
      'INTEGRITY_FAILED',
      'Grant for another key'
    )
    pairing.state = 'active'
    pairing.home = home
    pairing.grant = grant
    pairing.expiresAt = g.expires_at
  }

  /** §4.2: the hello's origin must be registered for the app, live, in this environment. */
  async connect(origin: string, appId: string): Promise<Connection> {
    await this.registration(appId, origin)
    return { connectionId: randomId(), origin, appId }
  }

  private async registration(appId: string, origin: string): Promise<AppRegistration> {
    const app = await this.certified.application(appId)
    ensure(app.environment === this.config.environment, 'APP_NOT_REGISTERED')
    ensure(app.origins.includes(origin), 'APP_NOT_REGISTERED')
    ensure(!app.paused, 'APP_PAUSED')
    return app
  }

  /** One page command: a fresh nonce, the page's proof over it, then the method. */
  async call(connection: Connection, input: Command, sign: PageSigner): Promise<Value> {
    const command = parseCommand(input)
    ensure(command.appId === connection.appId, 'FORBIDDEN')
    const nonce = random(32)
    const message = proofMessage(nonce, connection.origin, connection.connectionId, command)
    const signature = await sign(message)
    ensure(await p256Verify(unbase64(command.publicKey, 128), message, signature), 'FORBIDDEN')
    if (CREATE_METHODS.includes(command.method)) return this.create(connection, command, nonce, signature)
    const op = this.operations.get(key(connection.origin, command))
    if (op) ensure(op.command.publicKey === command.publicKey, 'NOT_FOUND')
    switch (command.method) {
      case 'getOperation':
        return op ? this.refresh(op, 0) : this.recover(connection, command, nonce, signature, 'read')
      case 'openOperation':
        return this.recover(connection, command, nonce, signature, 'open')
      case 'cancelOperation': {
        ensure(op, 'NOT_FOUND')
        await this.relay.call(this.signer(op.channel.pairing), 'POST', `/v1/connect/requests/${hex(op.channel.requestId)}/cancel`)
        return this.value(op, { state: 'cancelled' })
      }
      default: {
        ensure(op && op.delivered && op.delivered === command.resultDigest, 'INTEGRITY_FAILED')
        await this.relay.call(this.signer(op.channel.pairing), 'POST', `/v1/connect/requests/${hex(op.channel.requestId)}/ack`, {
          result_digest: unhex(command.resultDigest!, 32)
        })
        return { ...(op.last as Value), state: 'returned' }
      }
    }
  }

  private value(op: Operation, update: Partial<Value>): Value {
    return { operationId: op.command.operationId, kind: op.kind, state: 'awaiting_user', ...op.last, ...update }
  }

  /** §4.5: every check before a create request leaves the browser. */
  private async create(connection: Connection, command: Command, nonce: Uint8Array, signature: Uint8Array): Promise<Value> {
    const app = await this.registration(command.appId, connection.origin)
    const capability = { authenticate: 'Authenticate', signDocument: 'SignDocument', signAction: 'SignAction', checkout: 'Checkout' }[
      command.method
    ]!
    ensure(app.capabilities.includes(capability), 'FORBIDDEN')
    const payload = unbase64(command.payload!, 65_536)
    const { account, deadline } = this.binding(app, connection, command, payload)
    const digest = operationDigest(connection.origin, command)
    const existing = this.operations.get(key(connection.origin, command))
    if (existing) {
      ensure(equal(existing.digest, digest), 'IDEMPOTENCY_CONFLICT')
      return this.value(existing, {})
    }
    const pairing = account
      ? this.pairings.get(hex(account))
      : [...this.pairings.values()].filter((p) => p.state === 'active').at(0)
    ensure(pairing?.state === 'active' && Date.now() < (pairing.expiresAt ?? 0), 'NOT_PAIRED')
    const open = [...this.operations.values()].filter(
      (o) => o.channel.pairing === pairing && !['signed', 'returned', 'rejected', 'cancelled', 'expired', 'failed'].includes(o.last?.state ?? 'awaiting_user')
    )
    ensure(open.length < MAX_OPEN && open.filter((o) => o.origin === connection.origin).length < MAX_PER_ORIGIN, 'QUOTA_EXCEEDED')
    const now = Date.now()
    const expiresAt = Math.min(deadline, now + CREATE_TTL_MS)
    ensure(now < expiresAt, 'EXPIRED')
    const channel = await this.send(pairing, connection, command, nonce, signature, 'create', expiresAt)
    const op: Operation = {
      origin: connection.origin,
      command,
      digest,
      kind: KINDS[command.method]!,
      channel,
      delivered: null,
      last: null
    }
    this.operations.set(key(connection.origin, command), op)
    return this.value(op, {})
  }

  /** The payload binding of §4.5; returns the target account, if the method names one, and the deadline. */
  private binding(app: AppRegistration, connection: Connection, command: Command, payload: Uint8Array) {
    switch (command.method) {
      case 'signDocument': {
        const request = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload))
        ensure(
          request.protocol === PROTOCOL && request.method === 'signature.request' && request.requestId === command.operationId,
          'INTEGRITY_FAILED'
        )
        const profile = { text: 'TextStatementV1', digest: 'DigestStatementV1', file_statement: 'FileStatementV1' }[
          request.statement?.content?.kind as string
        ]
        ensure(profile && app.profiles.includes(profile), 'FORBIDDEN')
        const account = this.accountOf(request.accountId)
        return { account, deadline: Number(request.expiresAt) }
      }
      case 'signAction': {
        const action = decodeCanonical<Record<string, unknown>>(payload)
        ensure(app.profiles.includes('AppActionV1'), 'FORBIDDEN')
        ensure(
          action.origin === connection.origin &&
            action.app_id === command.appId &&
            hex(action.operation_id as Uint8Array) === command.operationId,
          'INTEGRITY_FAILED'
        )
        ensure(BigInt(action.app_config_version as number) === BigInt(app.config_version), 'POLICY_STALE')
        return { account: action.signing_account as Uint8Array, deadline: Number(action.expires_at_ms) }
      }
      case 'authenticate': {
        const request = decodeCanonical<Record<string, unknown>>(payload)
        ensure(BigInt(request.app_config_version as number) === BigInt(app.config_version), 'POLICY_STALE')
        ensure(
          request.origin === connection.origin &&
            request.app_id === command.appId &&
            hex(request.operation_id as Uint8Array) === command.operationId,
          'INTEGRITY_FAILED'
        )
        return { account: null, deadline: Number(request.expires_at_ms) }
      }
      default: {
        const checkout = decodeCanonical<Record<string, any>>(payload)
        return { account: checkout.approving_account as Uint8Array, deadline: Number(checkout.offer?.accept_by_ms) }
      }
    }
  }

  private accountOf(text: string): Uint8Array {
    for (const pairing of this.pairings.values()) if (xidText(pairing.account) === text) return pairing.account
    fail('NOT_PAIRED')
  }

  /** Seal a request to the account's approving devices and post it. */
  private async send(
    pairing: Pairing,
    connection: Connection,
    command: Command,
    nonce: Uint8Array,
    signature: Uint8Array,
    kind: 'create' | 'read' | 'open',
    expiresAt: number
  ): Promise<Channel> {
    const account = await this.certified.account(pairing.home!, pairing.account)
    const recipients = this.approvers(account)
    ensure(recipients.length, 'NO_APPROVING_DEVICE')
    const resultKey = await resultPair()
    const requestId = randomId()
    const now = Date.now()
    const request = {
      protocol: PROTOCOL,
      environment: this.config.environment,
      account_id: pairing.account,
      pairing_id: pairing.id,
      request_id: requestId,
      kind,
      origin: connection.origin,
      connection_id: connection.connectionId,
      nonce,
      command,
      page_signature: signature,
      result_key: await rawPublic(resultKey.publicKey),
      created_at: now,
      expires_at: expiresAt
    }
    const cose = await sign1(pairing.key.privateKey, pairing.id, REQUEST_TYP, canonical(request))
    const cek = random(32)
    const iv = random(12)
    const envelope = {
      v: 1,
      recipients: await Promise.all(
        recipients.map(async (d) => {
          const wrapped = await hpkeSeal(d.hpke_pub, cek, wrapInfo(pairing.account, pairing.id, requestId, d.device_id))
          return { device_id: d.device_id, enc: wrapped.enc, wrapped: wrapped.ciphertext }
        })
      ),
      nonce: iv,
      ciphertext: await aesSeal(cek, iv, cose, requestAad(pairing.account, pairing.id, requestId))
    }
    await this.relay.call(this.signer(pairing), 'POST', '/v1/connect/requests', {
      request_id: requestId,
      kind,
      expires_at: expiresAt,
      envelope
    })
    return { requestId, pairing, resultKey, seen: 0, device: null, expiresAt }
  }

  private approvers(account: CertifiedAccount) {
    return account.devices.filter((d) => d.active && d.capabilities.includes('FormalApprove'))
  }

  /** Read the newest reply of a channel, waiting up to `wait` ms for a new one. */
  private async poll(channel: Channel, wait: number) {
    return this.relay.call<Reply>(
      this.signer(channel.pairing),
      'GET',
      `/v1/connect/requests/${hex(channel.requestId)}?after=${channel.seen}&wait=${wait}`
    )
  }

  /**
   * §6.4: open a reply with HPKE auth mode under the device's certified
   * HPKE key, check its request, version and operation, and, once a device
   * holds the operation, accept only that device. Anything else is dropped.
   */
  private async open(channel: Channel, reply: Reply, digest: Uint8Array | null): Promise<Value | null> {
    const version = reply.sealed_version ?? 0
    if (!reply.sealed || !reply.device_id || version <= channel.seen) return null
    const device = unhex(reply.device_id, 32)
    if (channel.device && !equal(channel.device, device)) fail('INTEGRITY_FAILED', 'Reply from a device that does not hold the operation')
    const account = await this.certified.account(channel.pairing.home!, channel.pairing.account)
    const certified = account.devices.find((d) => d.active && equal(d.device_id, device))
    ensure(certified, 'INTEGRITY_FAILED', 'Reply from outside the certified devices')
    const sealed = decodeCanonical<{ enc: Uint8Array; ciphertext: Uint8Array }>(unb64(reply.sealed, 640 * 1024))
    const plain = await hpkeOpenAuth(
      channel.resultKey,
      certified.hpke_pub,
      sealed.enc,
      sealed.ciphertext,
      responseInfo(channel.pairing.account, channel.pairing.id, channel.requestId, version, device)
    )
    const response = parseResponse(plain, channel.requestId, version)
    if (digest) ensure(equal(response.operation_digest, digest), 'INTEGRITY_FAILED')
    const value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(response.value)) as Value
    channel.seen = version
    if (PINNED.includes(value.state)) channel.device = device
    return value
  }

  /** The current state of an operation with a local record. */
  async refresh(op: Operation, wait: number): Promise<Value> {
    const reply = await this.poll(op.channel, wait)
    let value: Value | null = null
    try {
      value = await this.open(op.channel, reply, op.digest)
    } catch (error) {
      if (!(error instanceof ConnectError) || error.code !== 'INTEGRITY_FAILED') throw error
      // A forged reply is dropped; the state stays what verified replies said.
    }
    if (value) {
      ensure(value.operationId === op.command.operationId, 'INTEGRITY_FAILED')
      op.last = value
    }
    const last = op.last
    if (!last || !PINNED.includes(last.state) || reply.state === 'device_revoked' || reply.state === 'abandoned') {
      const mapped = { cancelled: 'cancelled', expired: 'expired', device_revoked: 'failed', abandoned: 'execution_unknown', result_expired: 'result_expired' }[reply.state]
      if (mapped && !(last && ['signed', 'returned', 'rejected', 'failed'].includes(last.state))) {
        op.last = this.value(op, { state: mapped, ...(reply.state === 'device_revoked' ? { error: 'DEVICE_REVOKED' } : {}) })
      }
    }
    const current = op.last ?? this.value(op, {})
    if (current.state === 'signed' && current.resultDigest) {
      op.delivered = current.resultDigest
      if (reply.acked_digest === current.resultDigest) return { ...current, state: 'returned' }
    }
    return current
  }

  /**
   * §4.4 without a local record: a `read` (or `open`) request to every
   * paired account, then one long poll. A sealed snapshot answers; all
   * `unknown` is NOT_FOUND; silence is DEVICE_UNAVAILABLE. Read requests
   * are reused for two minutes.
   */
  private async recover(connection: Connection, command: Command, nonce: Uint8Array, signature: Uint8Array, kind: 'read' | 'open'): Promise<Value> {
    const k = `${kind}\n${key(connection.origin, command)}`
    let channels = (this.reads.get(k) ?? []).filter((c) => Date.now() < c.expiresAt)
    if (!channels.length) {
      const pairings = [...this.pairings.values()].filter((p) => p.state === 'active')
      ensure(pairings.length, 'NOT_PAIRED')
      channels = await Promise.all(
        pairings.map((p) => this.send(p, connection, command, nonce, signature, kind, Date.now() + READ_TTL_MS))
      )
      this.reads.set(k, channels)
    }
    const deadline = Date.now() + 20_000
    const replies = await Promise.all(
      channels.map(async (channel) => {
        // One account may answer `unknown` from one device while another device still looks.
        for (;;) {
          const reply = await this.poll(channel, Math.max(0, deadline - Date.now()))
          const value = await this.open(channel, reply, null).catch(() => null)
          if (value || ['unknown', 'closed', 'expired', 'result_expired'].includes(reply.state) || Date.now() >= deadline)
            return { reply, value }
          channel.seen = Math.max(channel.seen, reply.version)
        }
      })
    )
    const answered = replies.find((r) => r.value)
    if (answered) {
      const value = answered.value!
      ensure(value.operationId === command.operationId, 'INTEGRITY_FAILED')
      return value
    }
    ensure(!replies.every((r) => r.reply.state === 'unknown'), 'NOT_FOUND')
    fail('DEVICE_UNAVAILABLE')
  }

  /** Drop the local record of an operation, as cleared site data would. */
  forget(origin: string, appId: string, operationId: string): boolean {
    return this.operations.delete(key(origin, { appId, operationId }))
  }
}
