// Synthetic test-only authority. Never a production IC root or account.
import { readFileSync } from "node:fs";
import { bls12_381 } from "@noble/curves/bls12-381.js";
import { ed25519 } from "@noble/curves/ed25519.js";
import { reconstruct, type HashTree } from "@icp-sdk/core/agent";
import { canonical, digest, utf8, b64, hash, hex } from "../src/cose-codec.ts";
import {
  accountText,
  handlePath,
  type ProfileDevice,
  type PublicProfile,
} from "../src/profile.ts";
import type { CertifiedBatch } from "../src/ic.ts";

const auth = JSON.parse(
  readFileSync(
    new URL(
      "../../../src/dmsg_types/tests/authentication_vector.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
const bls = bls12_381.shortSignatures,
  secret = new Uint8Array(32).fill(1);
const prefix = Buffer.from(auth.root_der_hex, "hex").subarray(0, -96);
export const rootKey = Uint8Array.from([
  ...prefix,
  ...bls.getPublicKey(secret).toBytes(),
]);
export const now = 1800000000000n;
export const home = Uint8Array.of(1, 1),
  registry = Uint8Array.of(2, 1);
export const namespace = "https://dmsg.test/";
export const accountId = new Uint8Array(12).fill(1);
accountId.set(
  digest("dmsg/account-id-generator/v1", [
    "dmsg",
    "Local",
    namespace,
    home,
  ]).slice(0, 5),
  4,
);
export const account = accountText(accountId),
  issuer = namespace + account,
  handle = "alice";
export const seed = new Uint8Array(32).fill(7),
  deviceId = new Uint8Array(32).fill(7);
export const device: ProfileDevice = {
  input: {
    device_id: deviceId,
    signing_pub: ed25519.getPublicKey(seed),
    hpke_pub: new Uint8Array(32).fill(8),
    role: "Administrator",
    capabilities: ["ContentSign"],
  },
  added_at: 1,
  added_by: null,
  revoked_at: null,
  next_sequence: 1,
};
export const devices = new Map([[deviceId, device]]);
export const snapshot = {
  schema: 4,
  account_id: accountId,
  home_user: home,
  home_cose: home,
  issuer,
  security_epoch: 4,
  devices_root: digest("dmsg/devices/v1", devices),
};
export const profile: PublicProfile = {
  version: 1,
  prev_hash: null,
  display_name: "Alice · Public profile",
  bio: "A signed public profile.\nFor people and agents.",
  avatar_upload: null,
  links: ["https://example.com/"],
};
export const command = {
  protocol: "dmsg-cloud/1",
  action: "dmsg/profile/v1",
  security_epoch: 4,
  request_id: "09".repeat(32),
  deadline: Number(now) - 60000,
  payload: profile,
};
export function signedRecord(body = command, owner = issuer) {
  const headers = canonical(
    new Map<number, unknown>([
      [1, -19],
      [2, [15, 16]],
      [3, "application/cbor"],
      [4, deviceId],
      [15, new Map([[1, owner]])],
      [16, "application/vnd.dmsg.command+cose;v=1"],
    ]),
  );
  const payload = canonical(body);
  const signature = ed25519.sign(
    canonical(["Signature1", headers, new Uint8Array(), payload]),
    seed,
  );
  const encoded = Uint8Array.from([
    0xd2,
    ...canonical([headers, new Map(), payload, signature]),
  ]);
  return {
    signed: { cose_sign1: b64(encoded) },
    payload: body.payload,
    version: body.payload.version,
    hash: hash(encoded),
    signer: { device_id: hex(deviceId) },
  };
}
export const record = signedRecord();

export async function certified(
  canister: Uint8Array,
  key: Uint8Array,
  value: Uint8Array | null,
  path: Uint8Array[] = [key],
  at = now,
  customTree?: HashTree,
): Promise<CertifiedBatch> {
  let witness: HashTree = value === null ? [0] : [3, value];
  if (value !== null)
    for (const label of [...path].reverse()) witness = [2, label, witness];
  if (customTree) witness = customTree;
  let n = at * 1000000n;
  const time = [];
  do {
    const low = Number(n & 127n);
    n >>= 7n;
    time.push(low | (n ? 128 : 0));
  } while (n);
  const tree: HashTree = [
    1,
    [
      2,
      utf8("canister"),
      [
        2,
        canister,
        [
          2,
          utf8("certified_data"),
          [3, new Uint8Array(await reconstruct(witness))],
        ],
      ],
    ],
    [2, utf8("time"), [3, Uint8Array.from(time)]],
  ];
  const message = Uint8Array.from([
    ...utf8("\x0dic-state-root"),
    ...new Uint8Array(await reconstruct(tree)),
  ]);
  const signature = bls
    .sign(
      bls.hash(message, "BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_"),
      secret,
    )
    .toBytes();
  return {
    schema: 1n,
    canister,
    certificate: canonical({ tree, signature }),
    entries: [{ key, value, witness: canonical(witness) }],
  };
}
export const securityBatch = () =>
  certified(home, accountId, canonical(snapshot));
export const handleBatch = (owner = accountId) =>
  certified(
    registry,
    utf8(handle),
    canonical({ handle, version: 1, owner_account: owner }),
    handlePath(handle),
  );
