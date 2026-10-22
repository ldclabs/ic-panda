/** One public identity document for machine and human readers. */
import { accountBytes } from "./bundle.ts";
import { ensure } from "./cose-errors.ts";
import { decodeCanonical, digest, equal, utf8 } from "./cose-codec.ts";
import { verifyCertifiedPath, type CertifiedBatch } from "./ic.ts";
import {
  validatePrincipalDocument,
  validatePrincipalResolution,
  type PrincipalDocument,
} from "agent-protocols/delegation";
import { parseStrictJson } from "agent-protocols/identity";
export type {
  PrincipalDocument,
  PrincipalLink,
} from "agent-protocols/delegation";
export { accountBytes };

export function accountText(bytes: Uint8Array): string {
  ensure(bytes.length === 12, "INVALID_INPUT");
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  value <<= 4n;
  return Array.from(
    { length: 20 },
    (_, i) =>
      "0123456789abcdefghijklmnopqrstuv"[
        Number((value >> BigInt((19 - i) * 5)) & 31n)
      ],
  ).join("");
}

/** Handles have at most 19 ASCII characters; account IDs have exactly 20. */
export function profileTarget(
  text: string,
): { account: string } | { handle: string } {
  if (text.length === 20) {
    accountBytes(text);
    return { account: text };
  }
  ensure(/^[a-zA-Z0-9][a-zA-Z0-9_]{0,18}$/.test(text), "INVALID_INPUT");
  return { handle: text.toLowerCase() };
}

export function handlePath(handle: string): Uint8Array[] {
  const target = profileTarget(handle);
  ensure("handle" in target && target.handle === handle, "INVALID_INPUT");
  const bits = digest("dmsg/handle-bucket/v1", handle);
  return [
    ...Array.from({ length: 20 }, (_, i) =>
      Uint8Array.of((bits[Math.floor(i / 8)]! >> (7 - (i % 8))) & 1),
    ),
    utf8(handle),
  ];
}

export async function verifyHandle(
  batch: CertifiedBatch,
  handle: string,
  registry: Uint8Array,
  rootKey: Uint8Array,
  now: bigint,
): Promise<string | null> {
  const { value } = await verifyCertifiedPath(
    batch,
    utf8(handle),
    handlePath(handle),
    registry,
    rootKey,
    now,
  );
  if (value === null) return null;
  const record = decodeCanonical<{
    handle: string;
    version: number | bigint;
    owner_account: Uint8Array;
  }>(value);
  ensure(
    record.handle === handle &&
      integer(record.version) > 0n &&
      record.owner_account instanceof Uint8Array,
    "INTEGRITY_FAILED",
  );
  return accountText(record.owner_account);
}

/** Select the registered home named by the account allocator fingerprint. */
export function accountHome(
  account: string,
  environment: string,
  namespace: string,
  homes: Uint8Array[],
): Uint8Array | null {
  const id = accountBytes(account);
  ensure(
    ["Local", "Staging", "Production"].includes(environment) &&
      homes.length > 0 &&
      homes.length <= 64,
    "INVALID_INPUT",
  );
  const matches = homes.filter((home) =>
    equal(
      id.slice(4, 9),
      digest("dmsg/account-id-generator/v1", [
        "dmsg",
        environment,
        namespace,
        home,
      ]).slice(0, 5),
    ),
  );
  ensure(matches.length <= 1, "INTEGRITY_FAILED");
  return matches[0] ?? null;
}

function integer(value: unknown): bigint {
  ensure(
    (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) ||
      (typeof value === "bigint" && value >= 0n),
    "INTEGRITY_FAILED",
  );
  return BigInt(value as number | bigint);
}

/** Verify the directory's exact PrincipalDocument JSON under its pinned IC root.
 * These are the same bytes served at document.id, not a relay projection.
 */
export async function verifyPrincipalDocument(
  batch: CertifiedBatch,
  account: string,
  directory: Uint8Array,
  principalOrigin: string,
  rootKey: Uint8Array,
  now: bigint,
): Promise<PrincipalDocument | null> {
  const id = accountBytes(account);
  const { value } = await verifyCertifiedPath(
    batch,
    id,
    [utf8("documents"), id],
    directory,
    rootKey,
    now,
  );
  if (value === null) return null;
  const document = parseStrictJson(
    new TextDecoder("utf-8", { fatal: true }).decode(value),
  ) as PrincipalDocument;
  validatePrincipalDocument(document);
  validatePrincipalResolution(document, `${principalOrigin}/${account}`);
  ensure(
    document.name === undefined || typeof document.name === "string",
    "INTEGRITY_FAILED",
  );
  ensure(
    document.description === undefined ||
      typeof document.description === "string",
    "INTEGRITY_FAILED",
  );
  ensure(
    document.links === undefined ||
      (Array.isArray(document.links) &&
        document.links.every(
          (link) =>
            typeof link.name === "string" &&
            typeof link.rel === "string" &&
            typeof link.url === "string" &&
            new URL(link.url).protocol === "https:",
        )),
    "INTEGRITY_FAILED",
  );
  return document;
}
