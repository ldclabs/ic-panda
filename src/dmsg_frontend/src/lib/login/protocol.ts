// dmsg-login/1 messages (docs/protocol/login-v1_zh.md): canonical CBOR in a
// base64url URL fragment. The byte rules are fixed by
// tests/vectors/client/login-v1.json, which the Rust client also checks.
import { decode, encode } from 'cborg'

export const DERIVATION_ORIGINS = ['https://dmsg.net', 'https://panda.fans']
export const MIN_TTL_MS = 60_000
export const MAX_TTL_MS = 8 * 60 * 60_000
const SPKI_PREFIX = [
  0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00
]
const KEYS = [
  'derivation_origin',
  'next_url',
  'session_pubkey',
  'state',
  'ttl_ms',
  'v'
]
const CALLBACK =
  /^http:\/\/(?:127\.0\.0\.1|\[::1\]):([1-9][0-9]{0,4})\/auth\/callback$/

export interface LoginRequest {
  v: 1
  state: Uint8Array
  /** Decoded from the fragment's own buffer, so backed by an `ArrayBuffer`. */
  session_pubkey: Uint8Array<ArrayBuffer>
  ttl_ms: number
  derivation_origin: string
  next_url: string
}

export type LoginErrorCode =
  'user_cancelled' | 'ii_error' | 'derivation_origin_not_allowed' | 'timeout'

export function b64(bytes: Uint8Array): string {
  let text = ''
  for (const byte of bytes) text += String.fromCharCode(byte)
  return btoa(text).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

/** Canonical base64url only: no padding, no other alphabet, zero trailing bits. */
export function unb64(text: string): Uint8Array<ArrayBuffer> {
  if (!/^[A-Za-z0-9_-]*$/.test(text) || text.length % 4 === 1)
    throw new Error('base64url')
  const bytes = Uint8Array.from(
    atob(text.replace(/-/g, '+').replace(/_/g, '/')),
    (c) => c.charCodeAt(0)
  )
  if (b64(bytes) !== text) throw new Error('base64url')
  return bytes
}

function equal(a: Uint8Array, b: Uint8Array) {
  return a.length === b.length && a.every((byte, i) => byte === b[i])
}

/** The port of a callback URL, which must be exactly
 * `http://127.0.0.1:<port>/auth/callback` or `http://[::1]:<port>/auth/callback`. */
export function callbackPort(url: string): number {
  const match = CALLBACK.exec(url)
  const port = match ? Number(match[1]) : 0
  if (!port || port > 65535) throw new Error('next_url')
  return port
}

function check(condition: boolean, field: string): asserts condition {
  if (!condition) throw new Error(field)
}

/** Decode and check a request fragment. Throws naming the first bad field;
 * nothing may contact Internet Identity or navigate before this passes. */
export function parseRequest(fragment: string): LoginRequest {
  const bytes = unb64(fragment)
  const value = decode(bytes, {
    strict: true,
    rejectDuplicateMapKeys: true
  }) as Record<string, unknown>
  check(
    value !== null &&
      typeof value === 'object' &&
      Object.keys(value).sort().join() === KEYS.join() &&
      equal(encode(value), bytes),
    'encoding'
  )
  const request = value as unknown as LoginRequest
  check(request.v === 1, 'v')
  check(
    request.state instanceof Uint8Array &&
      request.state.length === 32 &&
      request.state.some((b) => b !== 0),
    'state'
  )
  const key = request.session_pubkey
  check(
    key instanceof Uint8Array &&
      key.length === 44 &&
      SPKI_PREFIX.every((b, i) => key[i] === b),
    'session_pubkey'
  )
  check(
    Number.isSafeInteger(request.ttl_ms) &&
      request.ttl_ms >= MIN_TTL_MS &&
      request.ttl_ms <= MAX_TTL_MS,
    'ttl_ms'
  )
  check(
    DERIVATION_ORIGINS.includes(request.derivation_origin),
    'derivation_origin'
  )
  check(typeof request.next_url === 'string', 'next_url')
  callbackPort(request.next_url)
  return request
}

/** The success fragment: the delegation chain's `toJSON()` text as UTF-8. */
export function encodeResult(state: Uint8Array, delegation: string): string {
  // Copy into this realm's Uint8Array so cborg encodes a byte string.
  const bytes = Uint8Array.from(new TextEncoder().encode(delegation))
  return b64(encode({ v: 1, state, delegation: bytes }))
}

export function encodeError(state: Uint8Array, error: LoginErrorCode): string {
  return b64(encode({ v: 1, state, error }))
}
