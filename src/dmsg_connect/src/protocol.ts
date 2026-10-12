/** dmsg-connect/1 byte rules (docs/protocol/connect-v1_zh.md); the Rust `dmsg_protocol::connect` twin. */
import { canonical, decodeCanonical, digest, equal, unb64, unbase64, unhex } from './codec.ts'
import { hmacSha256, isP256Spki } from './crypto.ts'
import { ensure } from './errors.ts'

export const PROTOCOL = 'dmsg-connect/1'
export const REQUEST_TYP = 'application/vnd.dmsg.connect-request+cose;v=1'
export const POP_TYP = 'application/vnd.dmsg.connect-pop+cose;v=1'
export const CREATE_METHODS = ['authenticate', 'signDocument', 'signAction', 'checkout']
export const METHODS = [...CREATE_METHODS, 'getOperation', 'openOperation', 'cancelOperation', 'acknowledge']
export const CREATE_TTL_MS = 86_400_000
export const READ_TTL_MS = 120_000
export const KINDS: Record<string, string> = {
  authenticate: 'authentication',
  signDocument: 'document',
  signAction: 'action',
  checkout: 'checkout'
}

export type Environment = 'Local' | 'Staging' | 'Production'

export interface Command {
  method: string
  appId: string
  operationId: string
  publicKey: string
  payload: string | null
  resultDigest: string | null
}

const DIGEST = /^[0-9a-f]{64}$/

/** The closed command shape of §4.3. */
export function parseCommand(value: unknown): Command {
  ensure(value && typeof value === 'object' && !Array.isArray(value), 'INVALID_INPUT')
  const c = value as Command
  ensure(Object.keys(c).sort().join(',') === 'appId,method,operationId,payload,publicKey,resultDigest', 'INVALID_INPUT')
  ensure(METHODS.includes(c.method), 'UNSUPPORTED_PROTOCOL')
  ensure(typeof c.appId === 'string' && /^[a-z0-9-]{1,64}$/.test(c.appId), 'INVALID_INPUT')
  ensure(typeof c.operationId === 'string' && DIGEST.test(c.operationId) && !/^0+$/.test(c.operationId), 'INVALID_INPUT')
  ensure(typeof c.publicKey === 'string' && c.publicKey.length <= 128 && isP256Spki(unbase64(c.publicKey, 128)), 'INVALID_INPUT')
  const create = CREATE_METHODS.includes(c.method)
  ensure(create ? typeof c.payload === 'string' && unbase64(c.payload, 65_536).length >= 0 : c.payload === null, 'INVALID_INPUT')
  ensure(
    c.method === 'acknowledge' ? typeof c.resultDigest === 'string' && DIGEST.test(c.resultDigest) : c.resultDigest === null,
    'INVALID_INPUT'
  )
  return { ...c }
}

export function proofMessage(nonce: Uint8Array, origin: string, connectionId: Uint8Array, command: Command): Uint8Array {
  return digest('dmsg/connect-proof/v1', [PROTOCOL, nonce, origin, connectionId, command])
}

export function operationDigest(origin: string, command: Command): Uint8Array {
  ensure(CREATE_METHODS.includes(command.method), 'INVALID_INPUT')
  return digest('dmsg/connect-operation/v1', [
    origin,
    command.appId,
    command.operationId,
    command.method,
    unbase64(command.publicKey, 128),
    unbase64(command.payload!, 65_536)
  ])
}

export const pairingId = (pub: Uint8Array) => digest('dmsg/connect-pairing-id/v1', pub)

export async function pairingMac(
  secret: Uint8Array,
  environment: Environment,
  account: Uint8Array,
  invite: Uint8Array,
  pub: Uint8Array,
  label: string
): Promise<Uint8Array> {
  return hmacSha256(secret, digest('dmsg/connect-pair/v1', [environment, account, invite, pub, label]))
}

export const requestAad = (account: Uint8Array, pairing: Uint8Array, request: Uint8Array) =>
  canonical(['dmsg/connect-request/v1', account, pairing, request])
export const wrapInfo = (account: Uint8Array, pairing: Uint8Array, request: Uint8Array, device: Uint8Array) =>
  canonical(['dmsg/connect-wrap/v1', account, pairing, request, device])
export const responseInfo = (
  account: Uint8Array,
  pairing: Uint8Array,
  request: Uint8Array,
  version: number,
  device: Uint8Array
) => canonical(['dmsg/connect-response/v1', account, pairing, request, version, device])

export interface Invite {
  v: number
  environment: Environment
  account_id: Uint8Array
  invite_id: Uint8Array
  secret: Uint8Array
}

/** The pairing link's fragment: `base64url(CBOR invite)`. */
export function parseInvite(fragment: string): Invite {
  const invite = decodeCanonical<Invite>(unb64(fragment.replace(/^#/, ''), 4096))
  ensure(
    invite &&
      invite.v === 1 &&
      ['Local', 'Staging', 'Production'].includes(invite.environment) &&
      invite.account_id instanceof Uint8Array &&
      invite.account_id.length === 12 &&
      invite.invite_id instanceof Uint8Array &&
      invite.invite_id.length === 32 &&
      invite.secret instanceof Uint8Array &&
      invite.secret.length === 32 &&
      Object.keys(invite).length === 5,
    'INVALID_INPUT'
  )
  return invite
}

/** A device's reply, after it opened under the device's HPKE key. */
export interface ConnectResponse {
  protocol: string
  request_id: Uint8Array
  version: number
  operation_digest: Uint8Array
  value: Uint8Array
}

export function parseResponse(bytes: Uint8Array, request: Uint8Array, version: number): ConnectResponse {
  const response = decodeCanonical<ConnectResponse>(bytes, 640 * 1024)
  ensure(
    response.protocol === PROTOCOL &&
      response.request_id instanceof Uint8Array &&
      equal(response.request_id, request) &&
      response.version === version &&
      response.operation_digest instanceof Uint8Array &&
      response.value instanceof Uint8Array,
    'INTEGRITY_FAILED'
  )
  return response
}

export const operationIdBytes = (command: Command) => unhex(command.operationId, 32)
