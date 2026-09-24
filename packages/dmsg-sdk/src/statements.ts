import {
  APP_ACTION_PROFILE,
  validateAppAction,
  decodeCanonical as decodeActionWire,
  canonical as actionBytes,
  type AppAction,
} from "./index.ts";
import { ed25519 } from "@noble/curves/ed25519.js";
import { secp256k1 } from "@noble/curves/secp256k1.js";
import { sha256 } from "@noble/hashes/sha2.js";
import {
  canonical,
  decodeBounded,
  decodeCanonical,
  equal,
  unutf8,
  utf8,
} from "./cose-codec.ts";

import { ensure } from "./cose-errors.ts";
export const TEXT_PROFILE = "application/vnd.dmsg.text-statement+cose;v=1";
export const DIGEST_PROFILE = "application/vnd.dmsg.digest-statement+cose;v=1";
export const FILE_STATEMENT_PROFILE =
  "application/vnd.dmsg.file-statement+cose;v=1";
export const FILE_STATEMENT_CONTENT_TYPE = "application/cbor";
export const MAX_FILE_STATEMENT_BYTES = 16384;
export const TEXT_CONTENT_TYPE = "text/plain;charset=utf-8";
export type Algorithm = "Ed25519" | "EcdsaSecp256k1";
export interface DocumentStatement {
  issuer: string;
  subject?: string | undefined;
  issuedAt?: bigint | undefined;
  content:
    | { kind: "app_action"; action: AppAction }
    | { kind: "text"; text: string }
    | {
        kind: "digest";
        sha256: Uint8Array;
        contentType?: string | undefined;
        location?: string | undefined;
      }
    | {
        kind: "file_statement";
        text: string;
        sha256: Uint8Array;
        contentType?: string | undefined;
        location?: string | undefined;
      };
}
export function statementPurpose(content: DocumentStatement["content"]) {
  switch (content.kind) {
    case "app_action":
      return "AppAction";
    case "text":
    case "file_statement":
      return "Statement";
    case "digest":
      return "FileAttestation";
  }
}
export function assertStatement(statement: DocumentStatement) {
  assertUri(statement.issuer);
  if (statement.subject !== undefined) {
    ensure(
      statement.subject.length > 0 &&
        utf8(statement.subject).length <= 8192 &&
        !/[\p{Cc}\uD800-\uDFFF]/u.test(statement.subject),
      "INVALID_INPUT",
    );
    if (statement.subject.includes(":")) assertUri(statement.subject);
  }
  if (statement.issuedAt !== undefined)
    ensure(
      typeof statement.issuedAt === "bigint" &&
        statement.issuedAt >= -0x8000000000000000n &&
        statement.issuedAt <= 0x7fffffffffffffffn,
      "INVALID_INPUT",
    );
  if (statement.content.kind === "app_action") {
    ensure(
      statement.subject === undefined && statement.issuedAt === undefined,
      "UNSUPPORTED_PROTOCOL",
    );
    validateAppAction(statement.content.action);
    return;
  }
  if (
    statement.content.kind === "text" ||
    statement.content.kind === "file_statement"
  )
    ensure(
      statement.content.text.length > 0 &&
        utf8(statement.content.text).length <= 4096 &&
        !/[\uD800-\uDFFF]/u.test(statement.content.text),
      "QUOTA_EXCEEDED",
    );
  if (statement.content.kind !== "text") {
    ensure(
      (statement.content.kind === "digest" ||
        statement.content.kind === "file_statement") &&
        statement.content.sha256 instanceof Uint8Array &&
        statement.content.sha256.length === 32,
      "INVALID_INPUT",
    );
    if (statement.content.contentType !== undefined)
      ensure(
        /^[!#$%&'*+.^_`|~0-9a-z-]+\/[!#$%&'*+.^_`|~0-9a-z-]+(?:; *[!#$%&'*+.^_`|~0-9a-z-]+=(?:[!#$%&'*+.^_`|~0-9a-z-]+|"[^"\r\n]+"))*$/i.test(
          statement.content.contentType,
        ) && utf8(statement.content.contentType).length <= 256,
        "INVALID_INPUT",
      );
    if (statement.content.location !== undefined)
      assertUri(statement.content.location);
  }
}
export function statementBytes(
  statement: DocumentStatement,
  algorithm: Algorithm,
  kid: Uint8Array,
) {
  assertStatement(statement);
  ensure(
    ["Ed25519", "EcdsaSecp256k1"].includes(algorithm) &&
      kid instanceof Uint8Array &&
      kid.length > 0 &&
      kid.length <= 256,
    "UNSUPPORTED_PROTOCOL",
  );
  const claims = new Map<number, unknown>([[1, statement.issuer]]);
  if (statement.subject !== undefined) claims.set(2, statement.subject);
  if (statement.issuedAt !== undefined) claims.set(6, statement.issuedAt);
  const headers = new Map<number, unknown>([
    [1, algorithm === "Ed25519" ? -19 : -47],
    [4, kid],
    [15, claims],
  ]);
  let payload: Uint8Array;
  if (statement.content.kind === "app_action") {
    headers.set(2, [15, 16]);
    headers.set(16, APP_ACTION_PROFILE);
    headers.set(3, "application/cbor");
    payload = actionBytes(statement.content.action);
  } else if (statement.content.kind === "text") {
    headers.set(2, [15, 16]);
    headers.set(16, TEXT_PROFILE);
    headers.set(3, TEXT_CONTENT_TYPE);
    payload = utf8(statement.content.text);
  } else if (statement.content.kind === "file_statement") {
    headers.set(2, [15, 16]);
    headers.set(16, FILE_STATEMENT_PROFILE);
    headers.set(3, FILE_STATEMENT_CONTENT_TYPE);
    const fields = new Map<number, unknown>([
      [1, statement.content.text],
      [2, statement.content.sha256],
    ]);
    if (statement.content.contentType !== undefined)
      fields.set(3, statement.content.contentType);
    if (statement.content.location !== undefined)
      fields.set(4, statement.content.location);
    payload = canonical(fields);
  } else {
    headers.set(2, [15, 16, 258]);
    headers.set(16, DIGEST_PROFILE);
    headers.set(258, -16);
    if (statement.content.contentType !== undefined)
      headers.set(259, statement.content.contentType);
    if (statement.content.location !== undefined)
      headers.set(260, statement.content.location);
    payload = statement.content.sha256;
  }
  const protectedBytes = canonical(headers);
  const toBeSigned = canonical([
    "Signature1",
    protectedBytes,
    new Uint8Array(),
    payload,
  ]);
  ensure(toBeSigned.length <= 65536, "QUOTA_EXCEEDED");
  return { protectedBytes, payload: Uint8Array.from(payload), toBeSigned };
}
export interface Artifact {
  cose_sign1: Uint8Array | number[];
  cose_key: Uint8Array | number[];
}
const isBytes = (value: unknown): value is Uint8Array =>
  value instanceof Uint8Array;
function map(value: unknown): Map<number, unknown> {
  ensure(value instanceof Map, "INTEGRITY_FAILED");
  return value;
}
function optionalText(value: unknown): string | undefined {
  ensure(value === undefined || typeof value === "string", "INTEGRITY_FAILED");
  return value as string | undefined;
}
function parseFileStatement(payload: Uint8Array): DocumentStatement["content"] {
  const fields = map(decodeCanonical(payload, MAX_FILE_STATEMENT_BYTES));
  ensure(
    [...fields.keys()].every((k) => [1, 2, 3, 4].includes(k)) &&
      typeof fields.get(1) === "string" &&
      isBytes(fields.get(2)),
    "UNSUPPORTED_PROTOCOL",
  );
  return {
    kind: "file_statement",
    text: fields.get(1) as string,
    sha256: fields.get(2) as Uint8Array,
    contentType: optionalText(fields.get(3)),
    location: optionalText(fields.get(4)),
  };
}
/** Crypto/profile validation only. URI, key and timestamp presence are not identity or TSA trust. */
export function verifyDocumentArtifact(
  artifact: Artifact,
  content?: Uint8Array,
) {
  const encoded = Uint8Array.from(artifact.cose_sign1),
    keyBytes = Uint8Array.from(artifact.cose_key);
  ensure(
    encoded.length <= 196608 && encoded[0] === 0xd2 && keyBytes.length <= 2048,
    "INTEGRITY_FAILED",
  );
  const fields = decodeBounded(encoded.subarray(1));
  ensure(Array.isArray(fields) && fields.length === 4, "INTEGRITY_FAILED");
  const [protectedBytes, unsigned, payload, signature] = fields;
  ensure(
    isBytes(protectedBytes) && isBytes(payload) && isBytes(signature),
    "INTEGRITY_FAILED",
  );
  const headers = map(decodeBounded(protectedBytes)),
    unprotected = map(unsigned),
    key = map(decodeBounded(keyBytes));
  const alg = headers.get(1),
    kid = headers.get(4),
    profile = headers.get(16);
  ensure(
    (alg === -19 || alg === -47) &&
      isBytes(kid) &&
      kid.length > 0 &&
      kid.length <= 256,
    "UNSUPPORTED_PROTOCOL",
  );
  const hash = profile === DIGEST_PROFILE,
    fileStatement = profile === FILE_STATEMENT_PROFILE,
    appAction = profile === APP_ACTION_PROFILE;
  ensure(
    hash || fileStatement || appAction || profile === TEXT_PROFILE,
    "UNSUPPORTED_PROTOCOL",
  );
  const allowed = hash
    ? [1, 2, 4, 15, 16, 258, 259, 260]
    : [1, 2, 3, 4, 15, 16];
  ensure(
    [...headers.keys()].every((k) => allowed.includes(k)),
    "UNSUPPORTED_PROTOCOL",
  );
  const critical = headers.get(2),
    expected = hash ? [15, 16, 258] : [15, 16];
  ensure(
    Array.isArray(critical) &&
      critical.length === expected.length &&
      expected.every((k) => critical.includes(k)),
    "UNSUPPORTED_PROTOCOL",
  );
  ensure(
    [...unprotected].every(
      ([k, v]) => k === 270 && isBytes(v) && v.length > 0 && v.length <= 131072,
    ),
    "UNSUPPORTED_PROTOCOL",
  );
  const claims = map(headers.get(15));
  ensure(
    [...claims.keys()].every((k) => [1, 2, 6].includes(k)) &&
      typeof claims.get(1) === "string",
    "UNSUPPORTED_PROTOCOL",
  );
  const at = claims.get(6);
  ensure(
    at === undefined ||
      typeof at === "bigint" ||
      (typeof at === "number" && Number.isSafeInteger(at)),
    "INTEGRITY_FAILED",
  );
  const statement: DocumentStatement = {
    issuer: claims.get(1) as string,
    subject: optionalText(claims.get(2)),
    issuedAt: at === undefined ? undefined : BigInt(at as bigint | number),
    content: appAction
      ? {
          kind: "app_action",
          action: decodeActionWire(payload) as unknown as AppAction,
        }
      : hash
        ? {
            kind: "digest",
            sha256: payload,
            contentType: optionalText(headers.get(259)),
            location: optionalText(headers.get(260)),
          }
        : fileStatement
          ? parseFileStatement(payload)
          : { kind: "text", text: unutf8(payload) },
  };
  ensure(
    hash
      ? headers.get(258) === -16 && payload.length === 32
      : headers.get(3) ===
          (fileStatement || appAction
            ? FILE_STATEMENT_CONTENT_TYPE
            : TEXT_CONTENT_TYPE),
    "UNSUPPORTED_PROTOCOL",
  );
  assertStatement(statement);
  ensure(
    !key.has(-4) &&
      key.get(3) === alg &&
      (!key.has(2) ||
        (isBytes(key.get(2)) && equal(key.get(2) as Uint8Array, kid))),
    "INTEGRITY_FAILED",
  );
  const ops = key.get(4);
  ensure(
    ops === undefined || (Array.isArray(ops) && ops.includes(2)),
    "FORBIDDEN",
  );
  const x = key.get(-2);
  ensure(
    isBytes(x) && x.length === 32 && signature.length === 64,
    "INTEGRITY_FAILED",
  );
  const toBeSigned = canonical([
    "Signature1",
    protectedBytes,
    new Uint8Array(),
    payload,
  ]);
  const required = new Map<number, unknown>([
    [1, key.get(1)],
    [-1, key.get(-1)],
    [-2, x],
  ]);
  if (alg === -19) {
    ensure(
      key.get(1) === 1 &&
        key.get(-1) === 6 &&
        ed25519.verify(signature, toBeSigned, x, { zip215: false }),
      "INTEGRITY_FAILED",
    );
  } else {
    const y = key.get(-3);
    ensure(
      key.get(1) === 2 &&
        key.get(-1) === 8 &&
        (typeof y === "boolean" || (isBytes(y) && y.length === 32)),
      "INTEGRITY_FAILED",
    );
    const publicKey =
      typeof y === "boolean"
        ? Uint8Array.from([y ? 3 : 2, ...x])
        : Uint8Array.from([4, ...x, ...(y as Uint8Array)]);
    ensure(
      secp256k1.verify(signature, toBeSigned, publicKey, {
        prehash: true,
        lowS: true,
      }),
      "INTEGRITY_FAILED",
    );
    // RFC 9679 requires uncompressed coordinates even for a compressed COSE_Key.
    required.set(
      -3,
      secp256k1.Point.fromBytes(publicKey).toBytes(false).subarray(33),
    );
  }
  if (content !== undefined && statement.content.kind !== "app_action")
    ensure(
      statement.content.kind === "text"
        ? equal(content, payload)
        : equal(sha256(content), statement.content.sha256),
      "INTEGRITY_FAILED",
    );
  return {
    statement,
    toBeSigned,
    signature,
    keyFingerprint: sha256(canonical(required)),
    checks: {
      signature: "verified",
      content:
        statement.content.kind === "app_action"
          ? "not_checked"
          : statement.content.kind === "text" || content !== undefined
            ? "verified"
            : "not_provided",
      issuerBinding: "not_checked",
      authorization: "not_checked",
      timestamp: unprotected.has(270) ? "not_checked" : "not_provided",
      currentStatus: "not_checked",
    } as const,
  };
}

function assertUri(value: string) {
  ensure(
    typeof value === "string" &&
      value.length > 0 &&
      value.length <= 8192 &&
      /^[\x21-\x7e]+$/.test(value),
    "INVALID_INPUT",
    "需要规范的绝对 URI。",
  );
  ensure(!/%(?![0-9a-fA-F]{2})/.test(value), "INVALID_INPUT");
  const parsed = new URL(value);
  ensure(
    parsed.href === value && !parsed.username && !parsed.password,
    "INVALID_INPUT",
    "URI 不能在签名时被改写。",
  );
  return value;
}
