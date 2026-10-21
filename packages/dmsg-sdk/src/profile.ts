/** Public account profiles: certified identity/devices and signed cloud content.
 * Network destinations and the IC root are supplied by the application, never
 * by a profile response. A valid signature does not establish the latest version.
 */
import { ed25519 } from "@noble/curves/ed25519.js";
import { accountBytes } from "./bundle.ts";
import { ensure } from "./cose-errors.ts";
import {
  canonical,
  decodeBounded,
  decodeCanonical,
  digest,
  equal,
  hash,
  hex,
  unb64,
  utf8,
} from "./cose-codec.ts";
import { verifyCertifiedPath, type CertifiedBatch } from "./ic.ts";

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

export interface ProfileDevice {
  input: {
    device_id: Uint8Array;
    signing_pub: Uint8Array;
    hpke_pub: Uint8Array;
    role: string;
    capabilities: string[];
  };
  added_at: number | bigint;
  added_by: Uint8Array | null;
  revoked_at: number | bigint | null;
  next_sequence: number | bigint;
}

export interface ProfileSecurity {
  account: string;
  issuer: string;
  epoch: bigint;
  devices: Map<Uint8Array, ProfileDevice>;
  certifiedAt: bigint;
}

/** Exact device records, including revoked devices and sequence counters. */
export async function verifyProfileSecurity(
  batch: CertifiedBatch,
  account: string,
  home: Uint8Array,
  rootKey: Uint8Array,
  devices: Map<Uint8Array, ProfileDevice>,
  now: bigint,
): Promise<ProfileSecurity | null> {
  const id = accountBytes(account);
  const { value, at } = await verifyCertifiedPath(
    batch,
    id,
    [id],
    home,
    rootKey,
    now,
  );
  if (value === null) return null;
  const snapshot = decodeCanonical<Record<string, unknown>>(value);
  ensure(
    snapshot.schema === 4 &&
      snapshot.account_id instanceof Uint8Array &&
      equal(snapshot.account_id, id) &&
      snapshot.home_user instanceof Uint8Array &&
      equal(snapshot.home_user, home) &&
      typeof snapshot.issuer === "string" &&
      snapshot.issuer.length > 0 &&
      snapshot.devices_root instanceof Uint8Array &&
      equal(snapshot.devices_root, digest("dmsg/devices/v1", devices)),
    "INTEGRITY_FAILED",
  );
  const ids = new Set<string>();
  for (const [key, device] of devices) {
    ensure(
      key.length === 32 &&
        !ids.has(hex(key)) &&
        equal(key, device.input.device_id) &&
        device.input.signing_pub.length === 32,
      "INTEGRITY_FAILED",
    );
    ids.add(hex(key));
  }
  return {
    account,
    issuer: snapshot.issuer,
    epoch: integer(snapshot.security_epoch),
    devices,
    certifiedAt: at,
  };
}

export interface PublicProfile {
  version: number;
  prev_hash: string | null;
  display_name: string;
  bio: string;
  avatar_upload: string | null;
  links: string[];
}

export interface VerifiedProfile {
  profile: PublicProfile;
  hash: string;
  deviceId: string;
  deviceActive: boolean;
  certifiedAt: bigint;
}

function integer(value: unknown): bigint {
  ensure(
    (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) ||
      (typeof value === "bigint" &&
        value >= 0n &&
        value <= 0xffffffffffffffffn),
    "INTEGRITY_FAILED",
  );
  return BigInt(value as number | bigint);
}

const hashText = (v: unknown): v is string =>
  typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
function profileShape(p: PublicProfile): void {
  ensure(
    !!p &&
      typeof p === "object" &&
      Object.keys(p).sort().join(",") ===
        "avatar_upload,bio,display_name,links,prev_hash,version",
    "INTEGRITY_FAILED",
  );
  ensure(
    typeof p.version === "number" &&
      Number.isSafeInteger(p.version) &&
      p.version > 0 &&
      (p.version === 1 ? p.prev_hash === null : hashText(p.prev_hash)) &&
      typeof p.display_name === "string" &&
      [...p.display_name].length <= 80 &&
      !/[\p{Cc}]/u.test(p.display_name) &&
      typeof p.bio === "string" &&
      [...p.bio].length <= 2000 &&
      !/[\p{Cc}]/u.test(p.bio.replaceAll("\n", "")) &&
      (p.avatar_upload === null || hashText(p.avatar_upload)) &&
      Array.isArray(p.links) &&
      p.links.length <= 10,
    "INTEGRITY_FAILED",
  );
  for (const link of p.links) {
    ensure(
      typeof link === "string" &&
        link.length <= 2048 &&
        !/[\p{Cc}]/u.test(link) &&
        new URL(link).protocol === "https:",
      "INTEGRITY_FAILED",
    );
  }
}

/** Verify exact COSE bytes, the account issuer and the relay's JSON projection.
 * Revoked devices can still prove historical statements; callers display that
 * state separately. An expired submission deadline does not undo a signature.
 */
export function verifyPublicProfile(
  value: unknown,
  security: ProfileSecurity,
): VerifiedProfile {
  const response = value as {
    signed: { cose_sign1: string };
    payload: PublicProfile;
    version: number;
    hash: string;
    signer: { device_id: string };
  };
  const encoded = unb64(response.signed.cose_sign1, 65536);
  ensure(encoded[0] === 0xd2, "INTEGRITY_FAILED");
  const fields = decodeBounded(encoded.subarray(1), 65536);
  ensure(Array.isArray(fields) && fields.length === 4, "INTEGRITY_FAILED");
  ensure(equal(canonical(fields), encoded.subarray(1)), "INTEGRITY_FAILED");
  const [protectedBytes, unsigned, payload, signature] = fields;
  ensure(
    protectedBytes instanceof Uint8Array &&
      payload instanceof Uint8Array &&
      signature instanceof Uint8Array &&
      signature.length === 64 &&
      unsigned instanceof Map &&
      unsigned.size === 0,
    "INTEGRITY_FAILED",
  );
  const headers = decodeBounded(protectedBytes);
  ensure(equal(canonical(headers), protectedBytes), "INTEGRITY_FAILED");
  ensure(
    headers instanceof Map &&
      headers.size === 6 &&
      headers.get(1) === -19 &&
      headers.get(3) === "application/cbor" &&
      headers.get(16) === "application/vnd.dmsg.command+cose;v=1",
    "UNSUPPORTED_PROTOCOL",
  );
  const crit = headers.get(2),
    claims = headers.get(15),
    kid = headers.get(4);
  ensure(
    Array.isArray(crit) &&
      crit.length === 2 &&
      crit[0] === 15 &&
      crit[1] === 16 &&
      claims instanceof Map &&
      claims.size === 1 &&
      claims.get(1) === security.issuer &&
      kid instanceof Uint8Array &&
      kid.length === 32,
    "INTEGRITY_FAILED",
  );
  const device = [...security.devices].find(([id]) => equal(id, kid))?.[1];
  ensure(
    device &&
      ed25519.verify(
        signature,
        canonical(["Signature1", protectedBytes, new Uint8Array(), payload]),
        device.input.signing_pub,
        { zip215: false },
      ),
    "INTEGRITY_FAILED",
  );
  const command = decodeCanonical<{
    protocol: string;
    action: string;
    payload: PublicProfile;
    security_epoch: number | bigint;
    request_id: string;
    deadline: number | bigint;
  }>(payload, 65536);
  ensure(
    Object.keys(command).sort().join(",") ===
      "action,deadline,payload,protocol,request_id,security_epoch" &&
      command.protocol === "dmsg-cloud/1" &&
      command.action === "dmsg/profile/v1" &&
      hashText(command.request_id) &&
      integer(command.security_epoch) <= security.epoch &&
      integer(command.deadline) > 0n,
    "INTEGRITY_FAILED",
  );
  profileShape(command.payload);
  profileShape(response.payload);
  ensure(
    equal(canonical(command.payload), canonical(response.payload)) &&
      response.version === command.payload.version &&
      response.hash === hash(encoded) &&
      response.signer.device_id === hex(kid),
    "INTEGRITY_FAILED",
  );
  return {
    profile: command.payload,
    hash: response.hash,
    deviceId: hex(kid),
    deviceActive: device.revoked_at === null,
    certifiedAt: security.certifiedAt,
  };
}
