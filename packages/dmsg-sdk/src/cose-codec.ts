import { encode, decode, Tagged } from "cborg";
import { DmsgError, ensure } from "./cose-errors.ts";
import { equalBytes, hex, isWellFormed, sha256 } from "./encoding.ts";

export {
  utf8,
  unutf8,
  hex,
  unhex,
  b64,
  unb64,
  equalBytes as equal,
} from "./encoding.ts";
export const random = (size = 32) =>
  crypto.getRandomValues(new Uint8Array(size));
export const id = () => hex(random());
export const bytes = (value: Uint8Array): Uint8Array<ArrayBuffer> =>
  new Uint8Array(value);
export const hash = (value: Uint8Array) => hex(sha256(value));

function validate(value: unknown, depth = 0, budget = { left: 50000 }): void {
  ensure(depth <= 24 && --budget.left >= 0, "QUOTA_EXCEEDED");
  if (typeof value === "number")
    ensure(Number.isSafeInteger(value), "INVALID_INPUT");
  else if (typeof value === "bigint")
    ensure(
      value >= -0x8000000000000000n && value <= 0xffffffffffffffffn,
      "INVALID_INPUT",
    );
  else if (value instanceof Tagged)
    ensure(
      value.tag === 2 &&
        value.value instanceof Uint8Array &&
        value.value.length > 8 &&
        value.value.length <= 16 &&
        value.value[0] !== 0,
      "INVALID_INPUT",
    );
  else if (Array.isArray(value))
    value.forEach((v) => validate(v, depth + 1, budget));
  else if (value instanceof Map)
    value.forEach((v, k) => {
      validate(k, depth + 1, budget);
      validate(v, depth + 1, budget);
    });
  else if (typeof value === "string")
    ensure(isWellFormed(value), "INVALID_INPUT");
  else if (
    value instanceof Uint8Array ||
    value === null ||
    typeof value === "boolean"
  )
    return;
  else if (
    typeof value === "object" &&
    Object.getPrototypeOf(value) === Object.prototype
  )
    Object.entries(value).forEach(([key, v]) => {
      validate(key, depth + 1, budget);
      validate(v, depth + 1, budget);
    });
  else throw new DmsgError("INVALID_INPUT");
}
export function canonical(value: unknown): Uint8Array<ArrayBuffer> {
  validate(value);
  return bytes(encode(value));
}

// Scan before recursive decoding: bounds allocation, nesting, indefinite lengths
// and tag handling even for a hostile recovery file.
function scan(data: Uint8Array): void {
  let offset = 0,
    nodes = 0;
  function item(depth: number) {
    ensure(
      depth <= 24 && ++nodes <= 50000 && offset < data.length,
      "INVALID_INPUT",
    );
    const first = data[offset++]!,
      major = first >> 5,
      info = first & 31;
    ensure((major !== 6 || info === 2) && info < 28, "UNSUPPORTED_PROTOCOL");
    let size = BigInt(info);
    if (info >= 24) {
      const count = 2 ** (info - 24);
      ensure(offset + count <= data.length, "INTEGRITY_FAILED");
      size = 0n;
      for (let i = 0; i < count; i++)
        size = (size << 8n) | BigInt(data[offset++]!);
    }
    if (major === 6) item(depth + 1);
    else if (major === 2 || major === 3) {
      ensure(size <= BigInt(data.length - offset), "INTEGRITY_FAILED");
      offset += Number(size);
    } else if (major === 4 || major === 5) {
      ensure(size <= 25000n, "QUOTA_EXCEEDED");
      for (let i = 0; i < Number(size) * (major === 5 ? 2 : 1); i++)
        item(depth + 1);
    } else if (major === 7)
      ensure([20, 21, 22].includes(info), "UNSUPPORTED_PROTOCOL");
  }
  item(0);
  ensure(offset === data.length, "INTEGRITY_FAILED");
}
export function decodeBounded(data: Uint8Array, max = 262144): unknown {
  ensure(data.length <= max && data.length > 0, "QUOTA_EXCEEDED");
  scan(data);
  const value = decode(data, {
    allowIndefinite: false,
    allowUndefined: false,
    allowNaN: false,
    allowInfinity: false,
    rejectDuplicateMapKeys: true,
    allowBigInt: true,
    useMaps: true,
    tags: Tagged.preserve(2),
  });
  validate(value);
  return value;
}
export function decodeCanonical<T = unknown>(
  data: Uint8Array,
  max = 262144,
): T {
  const value = decodeBounded(data, max);
  ensure(equalBytes(canonical(value), data), "INTEGRITY_FAILED");
  // COSE integer-key maps stay maps; domain records become safe objects.
  function records(v: unknown): unknown {
    if (v instanceof Map) {
      if (
        [...v.keys()].every(
          (key) =>
            typeof key === "string" &&
            !["__proto__", "constructor", "prototype"].includes(key),
        )
      )
        return Object.fromEntries(
          [...v].map(([k, value]) => [k, records(value)]),
        );
      return new Map([...v].map(([k, value]) => [k, records(value)]));
    }
    return Array.isArray(v) ? v.map(records) : v;
  }
  return records(value) as T;
}
export const digest = (domain: string, value: unknown) =>
  sha256(canonical([1, domain, value]));
