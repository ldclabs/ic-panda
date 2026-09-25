import { sha256 as sha256Bytes } from "@noble/hashes/sha2.js";
import { DmsgError, ensure } from "./cose-errors.ts";

/** Closed deterministic RFC 8949 subset used by the public integration contracts. */
export type CborValue =
  | null
  | boolean
  | bigint
  | string
  | Uint8Array
  | CborValue[]
  | { [key: string]: CborValue };
const encoder = new TextEncoder();
// Preserve leading U+FEFF as part of the signed string.
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const U64 = (1n << 64n) - 1n;
const U128 = (1n << 128n) - 1n;

export const utf8 = (text: string): Uint8Array<ArrayBuffer> =>
  encoder.encode(text);

/** False for lone UTF-16 surrogates, which have no UTF-8 encoding. */
export const isWellFormed = (text: string) => !/[\uD800-\uDFFF]/u.test(text);

export function unutf8(bytes: Uint8Array): string {
  try {
    return decoder.decode(bytes);
  } catch {
    throw new DmsgError("INVALID_INPUT");
  }
}

export const hex = (bytes: Uint8Array) =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

export function unhex(text: string): Uint8Array<ArrayBuffer> {
  ensure(
    typeof text === "string" && /^(?:[0-9a-f]{2})+$/.test(text),
    "INVALID_INPUT",
  );
  return Uint8Array.from(text.match(/../g)!, (b) => parseInt(b, 16));
}

/** Standard padded base64. */
export function base64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 8192)
    binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(binary);
}

/** Unpadded base64url. */
export const b64 = (bytes: Uint8Array) =>
  base64(bytes).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");

// The round trip accepts only the canonical spelling of the requested alphabet.
function decode64(
  text: string,
  max: number,
  url: boolean,
): Uint8Array<ArrayBuffer> {
  ensure(
    typeof text === "string" && text.length <= Math.ceil(max / 3) * 4,
    "QUOTA_EXCEEDED",
  );
  let binary: string;
  try {
    binary = atob(url ? text.replaceAll("-", "+").replaceAll("_", "/") : text);
  } catch {
    throw new DmsgError("INVALID_INPUT");
  }
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
  ensure(
    bytes.length <= max && (url ? b64(bytes) : base64(bytes)) === text,
    "INVALID_INPUT",
  );
  return bytes;
}

export const unbase64 = (text: string, max = 65_536) =>
  decode64(text, max, false);

export const unb64 = (text: string, max = 2 * 1024 * 1024) =>
  decode64(text, max, true);

export function concat(...parts: Uint8Array[]): Uint8Array {
  const result = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

export function equalBytes(a: Uint8Array, b: Uint8Array): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

function head(major: number, value: bigint): Uint8Array {
  ensure(value >= 0n && value <= U64, "INVALID_INPUT");
  if (value < 24n) return Uint8Array.of((major << 5) | Number(value));
  const size =
    value <= 255n ? 1 : value <= 65535n ? 2 : value <= 4294967295n ? 4 : 8;
  const result = new Uint8Array(size + 1);
  result[0] = (major << 5) | { 1: 24, 2: 25, 4: 26, 8: 27 }[size]!;
  for (let i = size; i > 0; i--) {
    result[i] = Number(value & 255n);
    value >>= 8n;
  }
  return result;
}

function compare(a: Uint8Array, b: Uint8Array): number {
  for (let i = 0; i < Math.min(a.length, b.length); i++)
    if (a[i] !== b[i]) return a[i]! - b[i]!;
  return a.length - b.length;
}

/** No floats, undefined, JS Number, prototypes, negative integers or implicit coercion. */
export function canonical(value: unknown, depth = 0): Uint8Array {
  ensure(depth <= 32, "QUOTA_EXCEEDED");
  if (value === null) return Uint8Array.of(0xf6);
  if (typeof value === "boolean") return Uint8Array.of(value ? 0xf5 : 0xf4);
  if (typeof value === "bigint") {
    ensure(value >= 0n && value <= U128, "INVALID_INPUT");
    if (value <= U64) return head(0, value);
    const bytes: number[] = [];
    for (let n = value; n; n >>= 8n) bytes.unshift(Number(n & 255n));
    return concat(
      Uint8Array.of(0xc2),
      head(2, BigInt(bytes.length)),
      Uint8Array.from(bytes),
    );
  }
  if (typeof value === "string") {
    ensure(isWellFormed(value), "INVALID_INPUT");
    const bytes = utf8(value);
    return concat(head(3, BigInt(bytes.length)), bytes);
  }
  if (value instanceof Uint8Array)
    return concat(head(2, BigInt(value.length)), value);
  if (Array.isArray(value))
    return concat(
      head(4, BigInt(value.length)),
      ...value.map((v) => canonical(v, depth + 1)),
    );
  if (
    typeof value === "object" &&
    Object.getPrototypeOf(value) === Object.prototype
  ) {
    const pairs = Object.entries(value).map(
      ([k, v]) => [canonical(k, depth + 1), canonical(v, depth + 1)] as const,
    );
    pairs.sort((a, b) => compare(a[0], b[0]));
    return concat(head(5, BigInt(pairs.length)), ...pairs.flat());
  }
  throw new DmsgError("INVALID_INPUT");
}

/** SHA-256 of exact bytes; caller still validates source and protocol semantics. */
export function sha256(value: Uint8Array): Uint8Array {
  return sha256Bytes(value);
}

/** SHA256(CBOR([1, domain, value])) with the exact public framing. */
export function digest(domain: string, value: unknown): Uint8Array {
  return sha256(canonical([1n, domain, value]));
}

/** Decode only the canonical subset emitted by the public contract encoder. */
export function decodeCanonical(bytes: Uint8Array): CborValue {
  ensure(bytes.length <= 65_536, "QUOTA_EXCEEDED");
  let offset = 0;
  const take = (n: number) => {
    ensure(offset + n <= bytes.length, "INTEGRITY_FAILED");
    const value = bytes.slice(offset, offset + n);
    offset += n;
    return value;
  };
  function read(depth: number): CborValue {
    ensure(depth <= 32, "QUOTA_EXCEEDED");
    const initial = take(1)[0]!,
      major = initial >> 5,
      ai = initial & 31;
    if (major === 7) {
      if (ai === 20) return false;
      if (ai === 21) return true;
      if (ai === 22) return null;
      throw new DmsgError("UNSUPPORTED_PROTOCOL");
    }
    let n = BigInt(ai);
    if (ai >= 24) {
      const count = { 24: 1, 25: 2, 26: 4, 27: 8 }[ai];
      ensure(count, "UNSUPPORTED_PROTOCOL");
      n = 0n;
      for (const byte of take(count)) n = (n << 8n) | BigInt(byte);
    }
    if (major === 0) return n;
    if (major === 6) {
      ensure(n === 2n, "UNSUPPORTED_PROTOCOL");
      const magnitude = read(depth + 1);
      ensure(
        magnitude instanceof Uint8Array &&
          magnitude.length >= 9 &&
          magnitude.length <= 16 &&
          magnitude[0] !== 0,
        "INTEGRITY_FAILED",
      );
      let value = 0n;
      for (const byte of magnitude) value = (value << 8n) | BigInt(byte);
      return value;
    }
    ensure(n <= BigInt(bytes.length), "INTEGRITY_FAILED");
    const count = Number(n);
    if (major === 2) return take(count);
    if (major === 3) return unutf8(take(count));
    if (major === 4)
      return Array.from({ length: count }, () => read(depth + 1));
    if (major === 5) {
      const value: Record<string, CborValue> = {};
      for (let i = 0; i < count; i++) {
        const key = read(depth + 1);
        ensure(
          typeof key === "string" && !Object.hasOwn(value, key),
          "INTEGRITY_FAILED",
        );
        Object.defineProperty(value, key, {
          value: read(depth + 1),
          enumerable: true,
          configurable: true,
          writable: true,
        });
      }
      return value;
    }
    throw new DmsgError("UNSUPPORTED_PROTOCOL");
  }
  const value = read(0);
  ensure(
    offset === bytes.length && equalBytes(canonical(value), bytes),
    "INTEGRITY_FAILED",
  );
  return value;
}
