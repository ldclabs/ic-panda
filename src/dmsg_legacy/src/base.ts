import { decode, encode } from 'cborg'
import { sha256 } from '@noble/hashes/sha2.js'
import { Principal } from '@icp-sdk/core/principal'

export const MAX_ARCHIVE = 256 * 1024 * 1024
export const OSS_CHUNK = 256 * 1024
export type Failure =
  | 'network'
  | 'permission'
  | 'missing'
  | 'missing_key'
  | 'version'
  | 'corrupt'
  | 'limit'
  | 'cancelled'
export class LegacyError extends Error {
  constructor(
    readonly code: Failure,
    message: string
  ) {
    super(message)
  }
}
export function requireLegacy(value: unknown, code: Failure, message: string): asserts value {
  if (!value) throw new LegacyError(code, message)
}
export const digest = (value: Uint8Array) =>
  Array.from(sha256(value), (n) => n.toString(16).padStart(2, '0')).join('')
export function binary(value: Uint8Array | number[]): Uint8Array<ArrayBuffer> {
  requireLegacy(
    value instanceof Uint8Array ||
      (Array.isArray(value) && value.every((n) => Number.isInteger(n) && n >= 0 && n <= 255)),
    'corrupt',
    'Invalid byte string'
  )
  return Uint8Array.from(value)
}
export function principal(value: string | Principal) {
  const text = typeof value === 'string' ? value : value.toText()
  requireLegacy(
    Principal.fromText(text).toText() === text,
    'corrupt',
    'Noncanonical Principal'
  )
  return text
}
// An explicit tagged tree retains Candid integer, byte, Principal and map types.
// It is a local observation encoding, not a canister certificate or author signature.
export function observation(value: unknown, depth = 0): unknown {
  requireLegacy(depth < 48, 'limit', 'Observation nesting limit')
  if (value === null || ['string', 'boolean'].includes(typeof value)) return value
  if (typeof value === 'number') {
    requireLegacy(Number.isSafeInteger(value), 'corrupt', 'Invalid integer')
    return value
  }
  if (typeof value === 'bigint') return { integer: value.toString() }
  if (value instanceof Uint8Array) return { bytes: value }
  if (value instanceof Uint32Array) return Array.from(value)
  if (value && typeof (value as Principal).toText === 'function')
    return { principal: principal(value as Principal) }
  if (Array.isArray(value)) return value.map((v) => observation(v, depth + 1))
  if (value instanceof Map)
    return {
      map: Array.from(value, ([k, v]) => [
        observation(k, depth + 1),
        observation(v, depth + 1)
      ])
    }
  requireLegacy(
    value && typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype,
    'corrupt',
    'Unsupported observation type'
  )
  return Object.fromEntries(
    Object.entries(value).map(([k, v]) => [k, observation(v, depth + 1)])
  )
}
export function pack(value: unknown): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(encode(value))
}
export function unpack(value: Uint8Array, max = MAX_ARCHIVE): unknown {
  requireLegacy(value.length <= max, 'limit', 'Archive limit exceeded')
  let offset = 0,
    nodes = 0
  function scan(depth: number) {
    requireLegacy(
      depth <= 32 && ++nodes <= 2000000 && offset < value.length,
      'limit',
      'CBOR structure limit'
    )
    const first = value[offset++]!,
      major = first >> 5,
      info = first & 31
    requireLegacy(
      info < 28 && major !== 6,
      'version',
      'Unsupported CBOR tag or indefinite encoding'
    )
    let size = BigInt(info)
    if (info >= 24) {
      const count = 2 ** (info - 24)
      requireLegacy(offset + count <= value.length, 'corrupt', 'Truncated CBOR length')
      size = 0n
      for (let i = 0; i < count; i++) size = (size << 8n) | BigInt(value[offset++]!)
    }
    if (major === 2 || major === 3) {
      requireLegacy(size <= BigInt(value.length - offset), 'corrupt', 'Truncated CBOR string')
      offset += Number(size)
    } else if (major === 4 || major === 5) {
      requireLegacy(size <= 100000n, 'limit', 'CBOR collection limit')
      for (let i = 0; i < Number(size) * (major === 5 ? 2 : 1); i++) scan(depth + 1)
    } else if (major === 7)
      requireLegacy([20, 21, 22].includes(info), 'version', 'Unsupported CBOR simple value')
  }
  scan(0)
  requireLegacy(offset === value.length, 'corrupt', 'Trailing CBOR bytes')
  try {
    return decode(value, {
      rejectDuplicateMapKeys: true,
      allowIndefinite: false,
      allowNaN: false,
      allowInfinity: false
    })
  } catch {
    throw new LegacyError('corrupt', 'Invalid archive CBOR')
  }
}
export function readObservation(bytes: Uint8Array): any {
  function restore(value: any): any {
    if (Array.isArray(value)) return value.map(restore)
    if (value && typeof value === 'object' && !(value instanceof Uint8Array)) {
      const keys = Object.keys(value)
      if (keys.length === 1) {
        if (value.bytes instanceof Uint8Array) return value.bytes
        if (typeof value.integer === 'string') return BigInt(value.integer)
        if (typeof value.principal === 'string') return Principal.fromText(value.principal)
        if (Array.isArray(value.map))
          return new Map(value.map.map(([k, v]: any[]) => [restore(k), restore(v)]))
      }
      return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, restore(v)]))
    }
    return value
  }
  return restore(unpack(bytes))
}
export interface SourceObject {
  key: string
  kind:
    | 'identity'
    | 'profile'
    | 'channel'
    | 'message'
    | 'file'
    | 'local'
    | 'setting'
    | 'entitlement'
    | 'avatar'
  bytes: Uint8Array
  digest: string
  trust: 'query_observation' | 'local_cache' | 'derived_archive'
  observedAt: number
}
export interface Gap {
  source: string
  code: Failure
  detail: string
}
export interface Inventory {
  format: 'dmsg-legacy-inventory/1'
  principal: string
  messageCanister: string
  mode: 'Local' | 'ECDH' | 'VetKey'
  snapshot: 'pre_migration'
  objects: SourceObject[]
  gaps: Gap[]
  calls: { canister: string; method: string; at: number; outcome: 'ok' | Failure }[]
}
