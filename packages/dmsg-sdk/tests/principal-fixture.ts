// Synthetic test-only IC authority for PrincipalDocument proof tests.
import { readFileSync } from "node:fs";
import { bls12_381 } from "@noble/curves/bls12-381.js";
import { reconstruct, type HashTree } from "@icp-sdk/core/agent";
import { canonical, digest, utf8 } from "../src/cose-codec.ts";
import {
  accountText,
  handlePath,
  type PrincipalDocument,
} from "../src/principal.ts";
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
export const directory = Uint8Array.of(3, 1);
export const principalOrigin = "https://id.dmsg.test";
export const document: PrincipalDocument = {
  id: `${principalOrigin}/${account}`,
  type: "person",
  protocol: "agent-delegation/1.0",
  name: "Alice",
  description: "A public identity for people and agents.",
  aliases: [`https://dmsg.test/${account}`],
  links: [{ name: "Website", url: "https://example.com/", rel: "related" }],
  controllers: [],
  updated_at: Number(now),
  extra: {
    "dmsg.link_images": {
      "https://example.com/": "https://example.com/image.png",
    },
  },
};
export const documentBytes = (value = document) => utf8(JSON.stringify(value));
export const documentBatch = (value = document) =>
  certified(directory, accountId, documentBytes(value), [
    utf8("documents"),
    accountId,
  ]);
export const handleBatch = (owner = accountId) =>
  certified(
    registry,
    utf8(handle),
    canonical({ handle, version: 1, owner_account: owner }),
    handlePath(handle),
  );
