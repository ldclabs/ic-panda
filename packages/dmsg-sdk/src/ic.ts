import {
  Certificate,
  Cbor,
  flatten_forks,
  lookupResultToBuffer,
  reconstruct,
  type HashTree,
} from "@icp-sdk/core/agent";
import { Principal } from "@icp-sdk/core/principal";
import type {
  AuthenticationRequest,
  AuthenticationResult,
} from "./contracts.ts";
import { canonical, concat, decodeCanonical, equalBytes } from "./encoding.ts";
import { AUTH_TTL_MS, requireValid, validateShape } from "./validation.ts";

/** Canonical CBOR proof view. Convert generated Candid option/principal wrappers at the boundary. */
export interface CertifiedBatch {
  schema: bigint;
  canister: Uint8Array;
  certificate: Uint8Array;
  entries: { key: Uint8Array; value: Uint8Array | null; witness: Uint8Array }[];
}

export function authenticationKey(
  account: Uint8Array,
  operation: Uint8Array,
): Uint8Array {
  validateShape("AccountId", account);
  validateShape("Hash", operation);
  return concat(
    new TextEncoder().encode("authentication/v1/"),
    account,
    operation,
  );
}

function certificateTime(time: Uint8Array): bigint {
  requireValid(time.length > 0 && time.length <= 10, "certificate time");
  let n = 0n;
  for (let i = 0; i < time.length; i++) {
    const byte = time[i]!;
    requireValid(i !== 9 || byte <= 1, "certificate time overflow");
    requireValid(
      i === time.length - 1
        ? (byte & 128) === 0 && (i === 0 || byte !== 0)
        : (byte & 128) !== 0,
      "certificate time encoding",
    );
    n |= BigInt(byte & 127) << BigInt(7 * i);
  }
  return n / 1_000_000n;
}

/** Verify a dedicated dMsg authentication proof against an independently stored product challenge.
 * No root discovery, URL fetch or configuration from the proof is permitted.
 * The product still checks session-key possession and consumes its challenge once.
 */
export async function verifyAuthenticationCertificate(
  batch: CertifiedBatch,
  expected: AuthenticationRequest,
  trustedHome: Uint8Array,
  trustedIcRootDer: Uint8Array,
  nowMs: bigint,
): Promise<AuthenticationResult> {
  validateShape("AuthenticationRequest", expected);
  requireValid(
    expected.version === 1n &&
      batch.entries.length === 1 &&
      batch.entries[0]?.value,
    "proof shape",
  );
  const result = decodeCanonical(
    batch.entries[0]!.value!,
  ) as unknown as AuthenticationResult;
  validateShape("AuthenticationResult", result);
  const { at } = await verifyCertifiedLeaf(
    batch,
    authenticationKey(result.account_id, expected.operation_id),
    trustedHome,
    trustedIcRootDer,
    nowMs,
  );
  requireValid(
    result.version === 1n &&
      equalBytes(canonical(result.request), canonical(expected)) &&
      equalBytes(result.home_user, trustedHome),
    "request binding",
  );
  requireValid(
    expected.issued_at_ms <= result.approved_at_ms &&
      result.approved_at_ms <= at &&
      nowMs < result.expires_at_ms &&
      result.expires_at_ms <= expected.expires_at_ms &&
      expected.expires_at_ms - expected.issued_at_ms <= AUTH_TTL_MS,
    "proof lifetime",
  );
  return result;
}

/** Verify a single exact leaf with independently pinned trust and freshness. */
export async function verifyCertifiedLeaf(
  batch: CertifiedBatch,
  path: Uint8Array,
  trustedHome: Uint8Array,
  trustedIcRootDer: Uint8Array,
  nowMs: bigint,
): Promise<{ value: Uint8Array; at: bigint }> {
  requireValid(
    batch.schema === 1n &&
      equalBytes(batch.canister, trustedHome) &&
      batch.entries.length === 1 &&
      batch.certificate.length <= 65_536 &&
      trustedIcRootDer.length <= 256,
    "proof shape",
  );
  const entry = batch.entries[0]!;
  requireValid(entry.value && entry.witness.length <= 65_536, "proof bounds");
  requireValid(equalBytes(entry.key, path), "authentication path");
  const certificate = await Certificate.create({
    certificate: Uint8Array.from(batch.certificate),
    rootKey: Uint8Array.from(trustedIcRootDer),
    principal: { canisterId: Principal.fromUint8Array(trustedHome) },
    disableTimeVerification: true,
  });
  const time = lookupResultToBuffer(certificate.lookup_path(["time"]));
  requireValid(time, "missing time");
  const at = certificateTime(time);
  requireValid(at <= nowMs && nowMs - at <= 60_000n, "certificate freshness");
  const root = lookupResultToBuffer(
    certificate.lookup_path(["canister", trustedHome, "certified_data"]),
  );
  const witness = Cbor.decode<HashTree>(entry.witness);
  requireValid(
    root && equalBytes(root, await reconstruct(witness)),
    "witness root",
  );
  // Disclosed labels in this service have one raw segment. Avoid SDK find_label's
  // byte-order issue; no absence proof is accepted for authentication.
  const matches = flatten_forks(witness).filter(
    (node) => node[0] === 2 && equalBytes(node[1], path),
  );
  requireValid(matches.length === 1, "exact witness path");
  const node = matches[0]!;
  requireValid(
    node[0] === 2 && node[2][0] === 3 && equalBytes(node[2][1], entry.value),
    "witness value",
  );
  return { value: entry.value, at };
}
