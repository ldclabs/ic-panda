/**
 * Attestation bundles (`dmsg-attestation/1`): a device-signed statement and
 * the execution receipt a dMsg user home certified for it, as the desktop
 * client exports them. A recipient checks a bundle without dMsg, check by
 * check (PRD S05): the device's signature, the receipt's certificate by a
 * user home of the deployment, the receipt's binding to the artifact, its
 * key, account and request, and the statement against the original file.
 * A failed check is reported, not thrown; a device revoked since never
 * undoes a signature that verifies.
 */
import { Principal } from "@icp-sdk/core/principal";
import type { ExecutionReceipt } from "./contracts.ts";
import { DmsgError, ensure } from "./cose-errors.ts";
import {
  concat,
  decodeCanonical,
  equalBytes,
  sha256,
  unb64,
  unhex,
  utf8,
} from "./encoding.ts";
import { type CertifiedBatch, verifyRecordedLeaf } from "./ic.ts";
import { type DocumentStatement, verifyDocumentArtifact } from "./statements.ts";
import { validateShape } from "./validation.ts";

/** The format a bundle names. */
export const BUNDLE_FORMAT = "dmsg-attestation/1";

/** An attestation bundle as exported; byte fields are base64url. */
export interface AttestationBundle {
  format: string;
  /** The signing account, as its 20-character ID. */
  account: string;
  /** The user home that certified the receipt, as its Principal text. */
  home: string;
  /** The execution ID, hex. */
  request_id: string;
  /** Tagged COSE_Sign1. */
  cose_sign1: string;
  /** Public COSE_Key of the device. */
  cose_key: string;
  /** Canonical CBOR of the certified receipt batch. */
  receipt: string;
}

/** What {@link verifyAttestationBundle} found, check by check. */
export interface BundleReport {
  /** Every check passed and the content does not contradict the file. */
  valid: boolean;
  /** The checks that failed: `signature`, `evidence`, `binding`, `content`. */
  failed: string[];
  /** `matches_file`, `mismatch`, `text`, `digest_only`, `action`, or `unknown` while the signature does not verify. */
  content:
    | "matches_file"
    | "mismatch"
    | "text"
    | "digest_only"
    | "action"
    | "unknown";
  /** The device's signature over the statement verifies. */
  signature: boolean;
  /** A trusted user home certified the receipt. */
  evidence: boolean;
  /** The receipt names this artifact, its key, account and request. */
  binding: boolean;
  /** The statement, once its signature verifies. */
  statement?: DocumentStatement;
  account: string;
  home: string;
  /** RFC 9679 thumbprint of the key that signed, once the signature verifies. */
  keyFingerprint?: Uint8Array;
  deviceId?: Uint8Array;
  securityEpoch?: bigint;
  approvedAt?: bigint;
  origin?: string;
  /** The receipt certificate's time, Unix milliseconds. */
  certifiedAt?: bigint;
}

const XID = "0123456789abcdefghijklmnopqrstuv";

/** The 12 bytes of an account ID in its canonical 20-character text. */
export function accountBytes(text: string): Uint8Array {
  ensure(typeof text === "string" && /^[0-9a-v]{19}[0g]$/.test(text), "INVALID_INPUT");
  const bytes = new Uint8Array(12);
  let bits = 0,
    acc = 0,
    at = 0;
  for (const char of text) {
    acc = (acc << 5) | XID.indexOf(char);
    bits += 5;
    if (bits >= 8 && at < 12) {
      bits -= 8;
      bytes[at++] = (acc >> bits) & 0xff;
    }
  }
  ensure((acc & ((1 << bits) - 1)) === 0, "INVALID_INPUT");
  return bytes;
}

/** The certified-tree key of an execution receipt. */
export function executionReceiptKey(account: Uint8Array, request: Uint8Array): Uint8Array {
  ensure(account.length === 12 && request.length === 32, "INVALID_INPUT");
  return concat(utf8("execution/"), account, request);
}

/** A bundle's shape, as JSON parsed from a file. */
export function parseAttestationBundle(value: unknown): AttestationBundle {
  const b = value as Record<string, unknown>;
  ensure(
    !!b &&
      typeof b === "object" &&
      !Array.isArray(b) &&
      Object.keys(b).every((k) =>
        ["format", "account", "home", "request_id", "cose_sign1", "cose_key", "receipt"].includes(k),
      ) &&
      ["format", "account", "home", "request_id", "cose_sign1", "cose_key", "receipt"].every(
        (k) => typeof b[k] === "string",
      ),
    "INVALID_INPUT",
  );
  ensure(b.format === BUNDLE_FORMAT, "UNSUPPORTED_PROTOCOL");
  return b as unknown as AttestationBundle;
}

/**
 * Verify a bundle as kept evidence against the deployment's user `homes`
 * and the IC root key, and its statement against the original `file` when
 * given. Throws only for a malformed bundle.
 */
export async function verifyAttestationBundle(
  input: AttestationBundle,
  options: { homes: Uint8Array[]; rootKey: Uint8Array; file?: Uint8Array | undefined },
): Promise<BundleReport> {
  const bundle = parseAttestationBundle(input);
  const account = accountBytes(bundle.account);
  let home: Uint8Array;
  try {
    home = Principal.fromText(bundle.home).toUint8Array();
  } catch {
    throw new DmsgError("INVALID_INPUT");
  }
  ensure(/^[0-9a-f]{64}$/.test(bundle.request_id), "INVALID_INPUT");
  const request = unhex(bundle.request_id);
  const artifact = { cose_sign1: unb64(bundle.cose_sign1), cose_key: unb64(bundle.cose_key) };
  const batch = decodeCanonical(unb64(bundle.receipt)) as unknown as CertifiedBatch;
  ensure(!!batch && typeof batch === "object" && Array.isArray(batch.entries), "INVALID_INPUT");

  let verified: ReturnType<typeof verifyDocumentArtifact> | undefined;
  try {
    verified = verifyDocumentArtifact(artifact);
  } catch {
    verified = undefined;
  }
  let receipt: ExecutionReceipt | undefined;
  let certifiedAt: bigint | undefined;
  if (options.homes.some((h) => equalBytes(h, home))) {
    try {
      const found = await verifyRecordedLeaf(batch, executionReceiptKey(account, request), home, options.rootKey);
      const leaf = decodeCanonical(found.value) as unknown as ExecutionReceipt;
      validateShape("ExecutionReceipt", leaf);
      receipt = leaf;
      certifiedAt = found.at;
    } catch {
      receipt = undefined;
    }
  }
  const binding =
    !!verified &&
    !!receipt &&
    receipt.schema === 2n &&
    receipt.issuer === verified.statement.issuer &&
    verified.statement.issuer.endsWith(bundle.account) &&
    equalBytes(receipt.account_id, account) &&
    equalBytes(receipt.request_id, request) &&
    equalBytes(receipt.to_be_signed_digest, sha256(verified.toBeSigned)) &&
    equalBytes(receipt.public_key_fingerprint, verified.keyFingerprint) &&
    equalBytes(receipt.signature_digest, sha256(verified.signature));
  const content = verified?.statement.content;
  const kind: BundleReport["content"] = !content
    ? "unknown"
    : content.kind === "text"
      ? "text"
      : content.kind === "app_action"
        ? "action"
        : options.file === undefined
          ? "digest_only"
          : equalBytes(sha256(options.file), content.sha256)
            ? "matches_file"
            : "mismatch";
  const failed = [
    !verified && "signature",
    !receipt && "evidence",
    !binding && "binding",
    kind === "mismatch" && "content",
  ].filter((f): f is string => !!f);
  return {
    valid: failed.length === 0,
    failed,
    content: kind,
    signature: !!verified,
    evidence: !!receipt,
    binding,
    statement: verified?.statement,
    account: bundle.account,
    home: bundle.home,
    keyFingerprint: verified?.keyFingerprint,
    deviceId: receipt?.device_id,
    securityEpoch: receipt?.security_epoch,
    approvedAt: receipt?.approved_at,
    origin: receipt?.origin,
    certifiedAt,
  };
}
