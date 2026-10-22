/** One public identity document for machine and human readers. */
import canonicalize from "canonicalize";
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

const MAX_DOCUMENT_BYTES = 65_536;
const MAX_METADATA_BYTES = 32_768;
const MAX_URL_BYTES = 2_048;
const CONTROL = /\p{Cc}/u;
const DOCUMENT_FIELDS = new Set([
  "id",
  "type",
  "name",
  "description",
  "avatar_url",
  "aliases",
  "links",
  "protocol",
  "controllers",
  "retired_controllers",
  "delegation_service",
  "updated_at",
  "extra",
]);
const LINK_FIELDS = new Set(["name", "url", "rel"]);

/** JCS bytes of a parsed value, rejecting values JSON cannot represent. */
function canonical(value: unknown): Uint8Array {
  let text: string | undefined;
  try {
    text = canonicalize(value);
  } catch {
    ensure(false, "INTEGRITY_FAILED");
  }
  ensure(typeof text === "string", "INTEGRITY_FAILED");
  return utf8(text);
}

function httpsUrl(value: string): void {
  ensure(
    utf8(value).length <= MAX_URL_BYTES &&
      !/[\u0000-\u001f\u007f"\\]/.test(value),
    "INTEGRITY_FAILED",
  );
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    ensure(false, "INTEGRITY_FAILED");
  }
  ensure(
    parsed.protocol === "https:" &&
      parsed.hostname.length > 0 &&
      parsed.username === "" &&
      parsed.password === "",
    "INTEGRITY_FAILED",
  );
}

function hasControl(value: string): boolean {
  return [...value].some((char) => CONTROL.test(char));
}

function validateDocumentShape(document: PrincipalDocument): void {
  ensure(
    Object.keys(document).every((field) => DOCUMENT_FIELDS.has(field)),
    "INTEGRITY_FAILED",
  );
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
    document.avatar_url === undefined ||
      typeof document.avatar_url === "string",
    "INTEGRITY_FAILED",
  );
  ensure(
    document.links === undefined ||
      (Array.isArray(document.links) &&
        document.links.every(
          (link) =>
            typeof link === "object" &&
            link !== null &&
            Object.keys(link).every((field) => LINK_FIELDS.has(field)) &&
            typeof link.name === "string" &&
            typeof link.rel === "string" &&
            typeof link.url === "string",
        )),
    "INTEGRITY_FAILED",
  );
  ensure(
    document.extra === undefined ||
      (typeof document.extra === "object" &&
        document.extra !== null &&
        !Array.isArray(document.extra)),
    "INTEGRITY_FAILED",
  );
}

/** The same display bounds the user home enforces before publishing. */
function validateDocumentMetadata(document: PrincipalDocument): void {
  const text = (value: string | undefined, max: number, multiline: boolean) => {
    if (value === undefined) return;
    ensure([...value].length <= max, "QUOTA_EXCEEDED");
    ensure(
      ![...value].some(
        (char) => CONTROL.test(char) && !(multiline && char === "\n"),
      ),
      "INTEGRITY_FAILED",
    );
  };
  text(document.name, 80, false);
  text(document.description, 2000, true);
  if (document.avatar_url !== undefined) httpsUrl(document.avatar_url);
  const links = document.links ?? [];
  ensure(links.length <= 100, "QUOTA_EXCEEDED");
  for (const link of links) {
    ensure(
      link.name.trim().length > 0 &&
        [...link.name].length <= 128 &&
        !hasControl(link.name) &&
        link.rel.length > 0 &&
        utf8(link.rel).length <= 64 &&
        !hasControl(link.rel),
      "INTEGRITY_FAILED",
    );
    httpsUrl(link.url);
  }
  const images = document.extra?.["dmsg.link_images"];
  if (images !== undefined) {
    ensure(
      typeof images === "object" && images !== null && !Array.isArray(images),
      "INTEGRITY_FAILED",
    );
    const entries = Object.entries(images);
    ensure(entries.length <= 100, "QUOTA_EXCEEDED");
    for (const [url, image] of entries) {
      ensure(
        links.some((link) => link.url === url) && typeof image === "string",
        "INTEGRITY_FAILED",
      );
      httpsUrl(image);
    }
  }
  const metadata = {
    name: document.name ?? null,
    description: document.description ?? null,
    avatar_url: document.avatar_url ?? null,
    links: document.links ?? [],
    extra: document.extra ?? {},
  };
  ensure(canonical(metadata).length <= MAX_METADATA_BYTES, "QUOTA_EXCEEDED");
}

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
  return (
    (await verifyHandleRecord(batch, handle, registry, rootKey, now))
      ?.account ?? null
  );
}

/** Certified name ownership plus the owner's registered display spelling. */
export async function verifyHandleRecord(
  batch: CertifiedBatch,
  handle: string,
  registry: Uint8Array,
  rootKey: Uint8Array,
  now: bigint,
): Promise<{ account: string; displayHandle?: string } | null> {
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
    display_handle?: string;
    version: number | bigint;
    owner_account: Uint8Array;
  }>(value);
  ensure(
    record.handle === handle &&
      integer(record.version) > 0n &&
      record.owner_account instanceof Uint8Array,
    "INTEGRITY_FAILED",
  );
  if (record.display_handle !== undefined) {
    ensure(
      typeof record.display_handle === "string" &&
        /^[a-zA-Z0-9][a-zA-Z0-9_]{0,18}$/.test(record.display_handle) &&
        record.display_handle.toLowerCase() === handle,
      "INTEGRITY_FAILED",
    );
  }
  return {
    account: accountText(record.owner_account),
    displayHandle: record.display_handle,
  };
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
  ensure(value.length <= MAX_DOCUMENT_BYTES, "QUOTA_EXCEEDED");
  const document = parseStrictJson(
    new TextDecoder("utf-8", { fatal: true }).decode(value),
  ) as PrincipalDocument;
  ensure(equal(canonical(document), value), "INTEGRITY_FAILED");
  validatePrincipalDocument(document);
  validatePrincipalResolution(document, `${principalOrigin}/${account}`);
  validateDocumentShape(document);
  validateDocumentMetadata(document);
  return document;
}
