/** Canonical CBOR (RFC 8949 core deterministic), hex and base64 as dmsg-connect/1 spells them. */
import { decode, encode } from 'cborg'
import { sha256 as sha256Bytes } from '@noble/hashes/sha2.js'
import { fail } from './errors.ts'

export const utf8 = (text: string) => new TextEncoder().encode(text)

export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

export function unhex(text: string, size?: number): Uint8Array {
  if (typeof text !== 'string' || !/^(?:[0-9a-f]{2})*$/.test(text)) fail('INVALID_INPUT')
  const bytes = Uint8Array.from(text.match(/../g) ?? [], (b) => parseInt(b, 16))
  if (size !== undefined && bytes.length !== size) fail('INVALID_INPUT')
  return bytes
}

/** Standard padded base64. */
export function base64(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i += 8192)
    binary += String.fromCharCode(...bytes.subarray(i, i + 8192))
  return btoa(binary)
}

/** Unpadded base64url. */
export const b64 = (bytes: Uint8Array) =>
  base64(bytes).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '')

function decode64(text: string, url: boolean, max: number): Uint8Array {
  if (typeof text !== 'string' || text.length > Math.ceil(max / 3) * 4 + 4) fail('INVALID_INPUT')
  let binary: string
  try {
    binary = atob(url ? text.replaceAll('-', '+').replaceAll('_', '/') : text)
  } catch {
    fail('INVALID_INPUT')
  }
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0))
  if (bytes.length > max || (url ? b64(bytes) : base64(bytes)) !== text) fail('INVALID_INPUT')
  return bytes
}

/** Canonical standard base64 only. */
export const unbase64 = (text: string, max = 1 << 20) => decode64(text, false, max)
/** Canonical base64url only. */
export const unb64 = (text: string, max = 1 << 20) => decode64(text, true, max)

export function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let at = 0
  for (const part of parts) {
    out.set(part, at)
    at += part.length
  }
  return out
}

export function equal(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false
  let diff = 0
  for (let i = 0; i < a.length; i++) diff |= a[i]! ^ b[i]!
  return diff === 0
}

/** Deterministic encoding: map keys sorted length-first, then bytewise. */
export function canonical(value: unknown): Uint8Array {
  return encode(value)
}

/** Decode and require the exact canonical bytes back; maps with integer keys stay Maps. */
export function decodeCanonical<T = unknown>(bytes: Uint8Array, max = 1 << 20): T {
  if (!bytes.length || bytes.length > max) fail('QUOTA_EXCEEDED')
  let value: unknown
  try {
    value = decode(bytes, {
      allowIndefinite: false,
      allowUndefined: false,
      allowNaN: false,
      allowInfinity: false,
      rejectDuplicateMapKeys: true,
      useMaps: false
    })
  } catch {
    fail('INTEGRITY_FAILED')
  }
  if (!equal(canonical(value), bytes)) fail('INTEGRITY_FAILED')
  return value as T
}

/** Decode keeping every map a Map (COSE headers). */
export function decodeMaps(bytes: Uint8Array): unknown {
  try {
    return decode(bytes, { useMaps: true, rejectDuplicateMapKeys: true, allowIndefinite: false })
  } catch {
    fail('INTEGRITY_FAILED')
  }
}

export const sha256 = (bytes: Uint8Array) => sha256Bytes(bytes)

/** `SHA256(CBOR([1, domain, value]))`. */
export function digest(domain: string, value: unknown): Uint8Array {
  return sha256(canonical([1, domain, value]))
}

export function random(size = 32): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(size))
}

/** A random nonzero 32-byte ID. */
export function randomId(): Uint8Array {
  for (;;) {
    const id = random()
    if (id.some((b) => b !== 0)) return id
  }
}
