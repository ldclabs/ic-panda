import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  accountBytes,
  accountHome,
  accountText,
  handlePath,
  profileTarget,
  verifyHandle,
  verifyPrincipalDocument,
} from "../src/principal.ts";
import { b64, canonical, hash, unb64, utf8 } from "../src/cose-codec.ts";
import { verifyCertifiedPath } from "../src/ic.ts";
import * as f from "./principal-fixture.ts";

test("root-path IDs and handles are disjoint; routing uses the account allocator", () => {
  assert.equal(accountText(accountBytes(f.account)), f.account);
  assert.deepEqual(profileTarget(f.account), { account: f.account });
  assert.deepEqual(profileTarget("ALICE_19"), { handle: "alice_19" });
  for (const target of [
    "_alice",
    "ali-ce",
    "../alice",
    "a".repeat(20),
    "A".repeat(20),
    " Alice",
    "中文",
  ])
    assert.throws(() => profileTarget(target));
  assert.deepEqual(
    accountHome(f.account, "Local", f.namespace, [f.home]),
    f.home,
  );
  assert.equal(
    accountHome(f.account, "Production", f.namespace, [f.home]),
    null,
  );
});

test("real BLS-certified bucketed handle resolves and follows a transfer", async () => {
  assert.equal(
    await verifyHandle(
      await f.handleBatch(),
      f.handle,
      f.registry,
      f.rootKey,
      f.now,
    ),
    f.account,
  );
  const next = new Uint8Array(12).fill(9);
  assert.equal(
    await verifyHandle(
      await f.handleBatch(next),
      f.handle,
      f.registry,
      f.rootKey,
      f.now,
    ),
    accountText(next),
  );
  const absent = await f.certified(f.registry, utf8(f.handle), null);
  assert.equal(
    await verifyHandle(absent, f.handle, f.registry, f.rootKey, f.now),
    null,
  );
  const batch = await f.handleBatch();
  for (const at of [f.now - 1n, f.now + 60001n])
    await assert.rejects(
      verifyHandle(batch, f.handle, f.registry, f.rootKey, at),
    );
  const root = f.rootKey.slice();
  root[root.length - 1] ^= 1;
  await assert.rejects(verifyHandle(batch, f.handle, f.registry, root, f.now));
  await assert.rejects(verifyHandle(batch, f.handle, f.home, f.rootKey, f.now));
  await assert.rejects(
    verifyHandle(batch, "bob", f.registry, f.rootKey, f.now),
  );
  const tampered = structuredClone(batch);
  tampered.entries[0].value![0] ^= 1;
  await assert.rejects(
    verifyHandle(tampered, f.handle, f.registry, f.rootKey, f.now),
  );
});

test("absence requires a known empty range, including nested bucket paths", async () => {
  const path = handlePath(f.handle),
    key = utf8(f.handle),
    zero = new Uint8Array(32);
  const unknown = await f.certified(f.registry, key, null, path, f.now, [
    4,
    zero,
  ]);
  await assert.rejects(
    verifyHandle(unknown, f.handle, f.registry, f.rootKey, f.now),
  );
  // Bytes 2 and 10 must compare numerically, not as JS array strings.
  const tree = [
    1,
    [2, Uint8Array.of(2), [3, Uint8Array.of(1)]],
    [2, Uint8Array.of(10), [3, Uint8Array.of(1)]],
  ] as const;
  const missing = await f.certified(
    f.home,
    Uint8Array.of(9),
    null,
    [],
    f.now,
    tree as any,
  );
  assert.equal(
    (
      await verifyCertifiedPath(
        missing,
        Uint8Array.of(9),
        [Uint8Array.of(9)],
        f.home,
        f.rootKey,
        f.now,
      )
    ).value,
    null,
  );
  const pruned = await f.certified(f.home, Uint8Array.of(9), null, [], f.now, [
    1,
    [4, zero],
    [2, Uint8Array.of(10), [3, Uint8Array.of(1)]],
  ]);
  await assert.rejects(
    verifyCertifiedPath(
      pruned,
      Uint8Array.of(9),
      [Uint8Array.of(9)],
      f.home,
      f.rootKey,
      f.now,
    ),
  );
});

test("the certified document preserves the official fields and legacy link artwork", async () => {
  const document = await verifyPrincipalDocument(
    await f.documentBatch(),
    f.account,
    f.directory,
    f.principalOrigin,
    f.rootKey,
    f.now,
  );
  assert.deepEqual(document, f.document);
});

test("the document binds exact bytes, directory, account, canonical ID and freshness", async () => {
  const proof = await f.documentBatch();
  const check = (
    batch = proof,
    directory = f.directory,
    origin = f.principalOrigin,
    now = f.now,
  ) =>
    verifyPrincipalDocument(
      batch,
      f.account,
      directory,
      origin,
      f.rootKey,
      now,
    );
  await assert.rejects(check(proof, f.home));
  await assert.rejects(check(proof, f.directory, "https://other.test"));
  await assert.rejects(
    check(proof, f.directory, f.principalOrigin, f.now + 60001n),
  );
  const tampered = structuredClone(proof);
  tampered.entries[0].value![5] ^= 1;
  await assert.rejects(check(tampered));
  const invalid = { ...f.document, id: "https://evil.test/" + f.account };
  await assert.rejects(check(await f.documentBatch(invalid)));
  const duplicate = utf8(
    JSON.stringify(f.document).replace(
      '"name":"Alice"',
      '"name":"Alice","name":"Mallory"',
    ),
  );
  await assert.rejects(
    check(
      await f.certified(f.directory, f.accountId, duplicate, [
        utf8("documents"),
        f.accountId,
      ]),
    ),
  );
  const missing = await f.certified(f.directory, f.accountId, null);
  assert.equal(await check(missing), null);
});

test(
  "browser SDK independently verifies the real directory Wasm document",
  { skip: !process.env.DMSG_PRINCIPAL_FIXTURE },
  async () => {
    const sample = JSON.parse(
      readFileSync(process.env.DMSG_PRINCIPAL_FIXTURE!, "utf8"),
    );
    const { Principal } = await import("@icp-sdk/core/principal");
    const { decodeCanonical, unhex } = await import("../src/cose-codec.ts");
    const batch = decodeCanonical<any>(unhex(sample.batch_hex));
    batch.schema = BigInt(batch.schema);
    const document = await verifyPrincipalDocument(
      batch,
      sample.account,
      Principal.fromText(sample.directory).toUint8Array(),
      sample.origin,
      unhex(sample.root_hex),
      BigInt(sample.now_ms),
    );
    assert.deepEqual(document, sample.document);
    assert.ok(document?.extra?.["dmsg.link_images"]);
  },
);
