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
  verifyProfileSecurity,
  verifyPublicProfile,
} from "../src/profile.ts";
import { b64, canonical, hash, unb64, utf8 } from "../src/cose-codec.ts";
import { verifyCertifiedPath } from "../src/ic.ts";
import * as f from "./profile-fixture.ts";

const security = {
  account: f.account,
  issuer: f.issuer,
  epoch: 4n,
  devices: f.devices,
  certifiedAt: f.now,
};

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

test("account evidence binds the full device map, home and exact account", async () => {
  const batch = await f.securityBatch();
  const verified = await verifyProfileSecurity(
    batch,
    f.account,
    f.home,
    f.rootKey,
    f.devices,
    f.now,
  );
  assert.equal(verified!.issuer, f.issuer);
  assert.equal(
    verifyPublicProfile(f.record, verified!).profile.display_name,
    f.profile.display_name,
  );
  const changed = new Map([[f.deviceId, { ...f.device, next_sequence: 2 }]]);
  await assert.rejects(
    verifyProfileSecurity(batch, f.account, f.home, f.rootKey, changed, f.now),
  );
  await assert.rejects(
    verifyProfileSecurity(
      batch,
      f.account,
      f.registry,
      f.rootKey,
      f.devices,
      f.now,
    ),
  );
  await assert.rejects(
    verifyProfileSecurity(
      batch,
      accountText(new Uint8Array(12).fill(5)),
      f.home,
      f.rootKey,
      f.devices,
      f.now,
    ),
  );
  await assert.rejects(
    verifyProfileSecurity(
      batch,
      f.account,
      f.home,
      f.rootKey,
      f.devices,
      f.now + 60001n,
    ),
  );
});

test("the existing Rust cloud vector verifies; expired submission deadlines do not undo it", () => {
  const v = JSON.parse(
    readFileSync(
      new URL("../../../tests/vectors/client/cloud-v1.json", import.meta.url),
      "utf8",
    ),
  );
  const d = {
    ...f.device,
    input: {
      ...f.device.input,
      signing_pub: Uint8Array.from(Buffer.from(v.public_key_hex, "hex")),
    },
  };
  const record = {
    signed: v.command,
    payload: v.profile,
    version: 1,
    hash: hash(unb64(v.command.cose_sign1)),
    signer: { device_id: v.context.deviceId },
  };
  assert.deepEqual(
    verifyPublicProfile(record, {
      ...security,
      issuer: v.context.issuer,
      devices: new Map([[f.deviceId, d]]),
    }).profile,
    v.profile,
  );
});

test("tampered signatures, projections, hashes, issuer, action, epoch and device are refused", () => {
  const encoded = unb64(f.record.signed.cose_sign1);
  encoded[encoded.length - 1] ^= 1;
  for (const changed of [
    { ...f.record, signed: { cose_sign1: b64(encoded) } },
    { ...f.record, payload: { ...f.profile, bio: "injected" } },
    { ...f.record, version: 2 },
    { ...f.record, hash: "00".repeat(32) },
    { ...f.record, signer: { device_id: "00".repeat(32) } },
    f.signedRecord({ ...f.command, action: "dmsg/channel/v1" }),
    f.signedRecord({ ...f.command, security_epoch: 5 }),
    f.signedRecord(f.command, "https://attacker.test/"),
    f.signedRecord({
      ...f.command,
      payload: { ...f.profile, links: ["javascript:alert(1)"] },
    }),
  ])
    assert.throws(() => verifyPublicProfile(changed, security));
  assert.throws(() =>
    verifyPublicProfile(f.record, { ...security, devices: new Map() }),
  );
});

test("a retained revoked device is reported as a historical signer", () => {
  const devices = new Map([[f.deviceId, { ...f.device, revoked_at: 123 }]]);
  assert.equal(
    verifyPublicProfile(f.record, { ...security, devices }).deviceActive,
    false,
  );
});

test("noncanonical COSE outer encoding cannot change the signed record hash", () => {
  const bytes = unb64(f.record.signed.cose_sign1);
  const noncanonical = Uint8Array.from([0xd2, 0x98, 4, ...bytes.slice(2)]);
  assert.throws(() =>
    verifyPublicProfile(
      {
        ...f.record,
        signed: { cose_sign1: b64(noncanonical) },
        hash: hash(noncanonical),
      },
      security,
    ),
  );
});
