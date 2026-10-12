/**
 * The headless bridge of the desktop tests: JSON lines in, JSON lines out.
 * It plays the application page (session keys, proofs) and the bridge
 * page (src/bridge.ts) without a browser. Test-only: it trusts the origin
 * its driver names, which a real bridge page takes from MessageEvent.origin.
 *
 *   {"id":1,"cmd":"init","config":{...}}       -> {"id":1,"ok":true,"value":...}
 *   {"id":2,"cmd":"pair","url":"...","label":"..."}
 */
import { createInterface } from 'node:readline'
import { Bridge, xidText, type Connection } from '../src/bridge.ts'
import { p256 } from '@noble/curves/nist.js'
import { b64, base64, hex, randomId, unhex, utf8 } from '../src/codec.ts'
import { p256Pair, p256Sign, spki } from '../src/crypto.ts'
import { ConnectError } from '../src/errors.ts'
import type { Command } from '../src/protocol.ts'

let bridge: Bridge | null = null
const connections = new Map<string, Connection>()
const sessions = new Map<string, CryptoKeyPair>()

/** The page's session key of an operation; a seed lets two pages share one, as one app does after a reload. */
async function session(operationId: string, seed?: string) {
  let pair = sessions.get(operationId)
  if (!pair && seed) {
    const d = unhex(seed, 32)
    const point = p256.getPublicKey(d, false)
    const jwk = { kty: 'EC', crv: 'P-256', x: b64(point.subarray(1, 33)), y: b64(point.subarray(33)) }
    pair = {
      privateKey: await crypto.subtle.importKey('jwk', { ...jwk, d: b64(d) }, { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign']),
      publicKey: await crypto.subtle.importKey('jwk', jwk, { name: 'ECDSA', namedCurve: 'P-256' }, true, ['verify'])
    }
    sessions.set(operationId, pair)
  }
  if (!pair) {
    pair = await p256Pair()
    sessions.set(operationId, pair)
  }
  return { pair, publicKey: base64(await spki(pair.publicKey)) }
}

async function command(connection: Connection, method: string, operationId: string, payload: string | null, resultDigest: string | null = null, seed?: string) {
  const { pair, publicKey } = await session(operationId, seed)
  const c: Command = { method, appId: connection.appId, operationId, publicKey, payload, resultDigest }
  return bridge!.call(connection, c, (message) => p256Sign(pair.privateKey, message))
}

async function run(input: Record<string, any>): Promise<unknown> {
  switch (input.cmd) {
    case 'init': {
      const c = input.config
      bridge = new Bridge({
        environment: c.environment,
        relay: c.relay,
        ic: { host: c.ic_host, rootKey: unhex(c.root_key_hex), handle: c.handle, commerce: c.commerce }
      })
      return { ready: true }
    }
    case 'pair': {
      const fragment = new URL(input.url).hash.slice(1)
      return bridge!.pair(fragment, input.label)
    }
    case 'await_grant': {
      const pairing = [...bridge!.pairings.values()].find((p) => xidText(p.account) === input.account)
      if (!pairing) throw new ConnectError('NOT_PAIRED')
      await bridge!.awaitGrant(pairing.account, input.timeout_ms ?? 30_000)
      return { state: 'active', pairing_id: hex(pairing.id) }
    }
    case 'connect': {
      const connection = await bridge!.connect(input.origin, input.app_id)
      const id = hex(connection.connectionId)
      connections.set(id, connection)
      return { connection: id }
    }
    case 'sign_document': {
      const connection = connections.get(input.connection)!
      const operationId = input.operation_id ?? hex(randomId())
      const request = {
        protocol: 'dmsg-connect/1',
        method: 'signature.request',
        requestId: operationId,
        accountId: input.account,
        nonce: input.nonce ?? hex(randomId()),
        expiresAt: String(input.expires_at ?? Date.now() + (input.ttl_ms ?? 600_000)),
        statement: { issuer: input.issuer, content: { kind: 'text', text: input.text } }
      }
      const value = await command(connection, 'signDocument', operationId, base64(utf8(JSON.stringify(request))), null, input.page_seed)
      return { ...value, operationId }
    }
    case 'get':
    case 'open':
    case 'cancel': {
      const connection = connections.get(input.connection)!
      const method = { get: 'getOperation', open: 'openOperation', cancel: 'cancelOperation' }[input.cmd as 'get']
      if (input.cmd === 'get' && input.wait_ms) {
        const deadline = Date.now() + input.wait_ms
        for (;;) {
          const value = await command(connection, method, input.operation_id, null)
          if (!input.until || input.until.includes(value.state) || Date.now() > deadline) return value
          await new Promise((r) => setTimeout(r, 500))
        }
      }
      return command(connection, method, input.operation_id, null)
    }
    case 'ack': {
      const connection = connections.get(input.connection)!
      const op = [...bridge!.operations.values()].find((o) => o.command.operationId === input.operation_id)
      if (!op?.delivered) throw new ConnectError('NOT_FOUND')
      return command(connection, 'acknowledge', input.operation_id, null, op.delivered)
    }
    case 'forget': {
      const connection = connections.get(input.connection)!
      return { forgotten: bridge!.forget(connection.origin, connection.appId, input.operation_id) }
    }
    default:
      throw new ConnectError('UNSUPPORTED_PROTOCOL')
  }
}

const lines = createInterface({ input: process.stdin })
for await (const line of lines) {
  if (!line.trim()) continue
  let input: Record<string, any> = {}
  try {
    input = JSON.parse(line)
    const value = await run(input)
    process.stdout.write(JSON.stringify({ id: input.id, ok: true, value }) + '\n')
  } catch (error) {
    const code = error instanceof ConnectError ? error.code : 'UNAVAILABLE'
    process.stdout.write(
      JSON.stringify({ id: input.id, ok: false, error: code, message: error instanceof Error ? error.message : String(error) }) + '\n'
    )
  }
}
