/** Closed deterministic RFC 8949 subset used by the public integration contracts. */
export type CborValue =
  | null
  | boolean
  | bigint
  | string
  | Uint8Array
  | CborValue[]
  | { [key: string]: CborValue }
const utf8 = new TextEncoder()
const U64 = (1n << 64n) - 1n
const U128 = (1n << 128n) - 1n

export function concat(...parts: Uint8Array[]): Uint8Array {
  const result = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let offset = 0
  for (const part of parts) {
    result.set(part, offset)
    offset += part.length
  }
  return result
}

function head(major: number, value: bigint): Uint8Array {
  if (value < 0n || value > U64) throw new Error('Invalid CBOR head')
  if (value < 24n) return Uint8Array.of((major << 5) | Number(value))
  const size =
    value <= 255n ? 1 : value <= 65535n ? 2 : value <= 4294967295n ? 4 : 8
  const result = new Uint8Array(size + 1)
  result[0] = (major << 5) | { 1: 24, 2: 25, 4: 26, 8: 27 }[size]!
  for (let i = size; i > 0; i--) {
    result[i] = Number(value & 255n)
    value >>= 8n
  }
  return result
}

function compare(a: Uint8Array, b: Uint8Array): number {
  for (let i = 0; i < Math.min(a.length, b.length); i++)
    if (a[i] !== b[i]) return a[i]! - b[i]!
  return a.length - b.length
}

/** No floats, undefined, JS Number, prototypes, negative integers or implicit coercion. */
export function canonical(value: unknown, depth = 0): Uint8Array {
  if (depth > 32) throw new Error('CBOR nesting limit')
  if (value === null) return Uint8Array.of(0xf6)
  if (typeof value === 'boolean') return Uint8Array.of(value ? 0xf5 : 0xf4)
  if (typeof value === 'bigint') {
    if (value < 0n || value > U128) throw new Error('Expected u128')
    if (value <= U64) return head(0, value)
    const bytes: number[] = []
    for (let n = value; n; n >>= 8n) bytes.unshift(Number(n & 255n))
    return concat(
      Uint8Array.of(0xc2),
      head(2, BigInt(bytes.length)),
      Uint8Array.from(bytes)
    )
  }
  if (typeof value === 'string') {
    if (new TextDecoder().decode(utf8.encode(value)) !== value)
      throw new Error('Invalid Unicode')
    const bytes = utf8.encode(value)
    return concat(head(3, BigInt(bytes.length)), bytes)
  }
  if (value instanceof Uint8Array)
    return concat(head(2, BigInt(value.length)), value)
  if (Array.isArray(value))
    return concat(
      head(4, BigInt(value.length)),
      ...value.map((v) => canonical(v, depth + 1))
    )
  if (
    typeof value === 'object' &&
    Object.getPrototypeOf(value) === Object.prototype
  ) {
    const pairs = Object.entries(value).map(
      ([k, v]) => [canonical(k, depth + 1), canonical(v, depth + 1)] as const
    )
    pairs.sort((a, b) => compare(a[0], b[0]))
    return concat(head(5, BigInt(pairs.length)), ...pairs.flat())
  }
  throw new Error('Unsupported CBOR value')
}

/** SHA-256 of exact bytes; caller still validates source and protocol semantics. */
export async function sha256(value: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(
    await crypto.subtle.digest('SHA-256', Uint8Array.from(value).buffer)
  )
}

/** SHA256(CBOR([1, domain, value])) with the exact public framing. */
export async function digest(
  domain: string,
  value: unknown
): Promise<Uint8Array> {
  return sha256(canonical([1n, domain, value]))
}

export function equalBytes(a: Uint8Array, b: Uint8Array): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i])
}

/** Decode only the canonical subset emitted by the public contract encoder. */
export function decodeCanonical(bytes: Uint8Array): CborValue {
  if (bytes.length > 65_536) throw new Error('CBOR size limit')
  let offset = 0
  const take = (n: number) => {
    if (offset + n > bytes.length) throw new Error('Truncated CBOR')
    const value = bytes.slice(offset, offset + n)
    offset += n
    return value
  }
  function read(depth: number): CborValue {
    if (depth > 32) throw new Error('CBOR nesting limit')
    const initial = take(1)[0]!,
      major = initial >> 5,
      ai = initial & 31
    if (major === 7) {
      if (ai === 20) return false
      if (ai === 21) return true
      if (ai === 22) return null
      throw new Error('Unsupported CBOR simple value')
    }
    let n = BigInt(ai)
    if (ai >= 24) {
      const count = { 24: 1, 25: 2, 26: 4, 27: 8 }[ai]
      if (!count) throw new Error('Indefinite CBOR')
      n = 0n
      for (const byte of take(count)) n = (n << 8n) | BigInt(byte)
    }
    if (major === 0) return n
    if (major === 6) {
      if (n !== 2n) throw new Error('Unsupported CBOR tag')
      const magnitude = read(depth + 1)
      if (
        !(magnitude instanceof Uint8Array) ||
        magnitude.length < 9 ||
        magnitude.length > 16 ||
        magnitude[0] === 0
      )
        throw new Error('Invalid u128')
      let value = 0n
      for (const byte of magnitude) value = (value << 8n) | BigInt(byte)
      return value
    }
    if (n > BigInt(bytes.length)) throw new Error('CBOR length limit')
    const count = Number(n)
    if (major === 2) return take(count)
    if (major === 3)
      return new TextDecoder('utf-8', { fatal: true }).decode(take(count))
    if (major === 4) return Array.from({ length: count }, () => read(depth + 1))
    if (major === 5) {
      const value: Record<string, CborValue> = {}
      for (let i = 0; i < count; i++) {
        const key = read(depth + 1)
        if (typeof key !== 'string' || Object.hasOwn(value, key))
          throw new Error('Invalid map key')
        Object.defineProperty(value, key, {
          value: read(depth + 1),
          enumerable: true,
          configurable: true,
          writable: true
        })
      }
      return value
    }
    throw new Error('Unsupported CBOR major type')
  }
  const value = read(0)
  if (offset !== bytes.length || !equalBytes(canonical(value), bytes))
    throw new Error('Noncanonical CBOR')
  return value
}
