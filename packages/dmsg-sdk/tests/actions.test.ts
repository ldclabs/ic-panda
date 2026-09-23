import { verifyDocumentArtifact, statementBytes } from "../src/statements.ts";
import { actionInputHash } from "../src/action.ts";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
import { canonical, decodeCanonical, digest } from "../src/encoding.ts";
import {
  validateAppAction,
  validateActionAdmission,
  appActionDigest,
} from "../src/action.ts";
import type { AppAction, AppRegistration } from "../src/contracts.ts";
const vectors = JSON.parse(
  readFileSync(
    new URL(
      "../../../src/dmsg_types/tests/integration_vectors.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
const value = (name: string): any =>
  decodeCanonical(
    Uint8Array.from(
      Buffer.from(vectors.find((v: any) => v.name === name).cbor_hex, "hex"),
    ),
  );
const fixture = (): AppAction => value("app_action_0")[2];

test("four closed action variants validate independently and match Rust commitments", async () => {
  for (let i = 0; i < 4; i++) {
    const action = value(`app_action_${i}`)[2] as AppAction;
    validateAppAction(action);
    assert.deepEqual(
      await appActionDigest(action),
      await digest("dmsg/app-action/v1", action),
    );
  }
});

test("action receiver, origin, manifest, display data and intent are inseparable", async () => {
  const action = fixture(),
    original = await appActionDigest(action);
  const changes: ((a: AppAction) => void)[] = [
    (a) => {
      a.origin = "https://other.test";
    },
    (a) => {
      a.receiver = new Uint8Array([99, 1]);
    },
    (a) => {
      a.files[0]!.display_name = "changed.cbor";
    },
    (a) => {
      a.files[0]!.revision++;
    },
    (a) => {
      a.files[0]!.sha256[0] = a.files[0]!.sha256[0]! ^ 1;
    },
    (a) => {
      a.expires_at_ms--;
    },
    (a) => {
      a.actor_id[0] = a.actor_id[0]! ^ 1;
    },
    (a) => {
      a.intent_hash[0] = a.intent_hash[0]! ^ 1;
    },
    (a) => {
      a.command = value("app_action_3")[2].command;
    },
  ];
  for (const change of changes) {
    const other = structuredClone(action);
    change(other);
    other.input_hash = actionInputHash(other.command);
    validateAppAction(other);
    assert.notDeepEqual(await appActionDigest(other), original);
  }
});

test("unknown semantics and stale registration cannot enter action admission", () => {
  const action = fixture();
  const app = value("app") as AppRegistration;
  validateActionAdmission(action, app, action.issued_at_ms);
  for (const other of [
    { ...action, version: 2n },
    { ...action, command: { TransferAssets: { amount: 1n } } },
    { ...action, command: { ...action.command, hidden_semantics: true } },
    { ...action, display_summary: "approve everything" },
    { ...action, files: [...action.files, ...action.files] },
    { ...action, files: [{ ...action.files[0], file_id: "😀" }] },
    { ...action, expires_at_ms: action.expires_at_ms + 1n },
    { ...action, receiver: new Uint8Array([4]) },
    { ...action, files: [{ ...action.files[0], revision: 0n }] },
  ])
    assert.throws(() => validateAppAction(other as AppAction));
  assert.throws(() =>
    validateActionAdmission(action, app, action.expires_at_ms),
  );
  assert.throws(() =>
    validateActionAdmission(
      action,
      { ...app, paused: true },
      action.issued_at_ms,
    ),
  );
  assert.throws(() =>
    validateActionAdmission(
      action,
      { ...app, config_version: 2n },
      action.issued_at_ms,
    ),
  );
  assert.throws(() =>
    validateActionAdmission(
      action,
      { ...app, capabilities: ["Authenticate"], profiles: [] },
      action.issued_at_ms,
    ),
  );
  assert.deepEqual(decodeCanonical(canonical(action)), action);
});

test("independent SDK verifies the Rust COSE action and exact signing bytes", () => {
  const artifact = value("app_action_artifact"),
    request = value("app_action_request");
  const verified = verifyDocumentArtifact(artifact);
  assert.equal(verified.statement.content.kind, "app_action");
  assert.deepEqual(verified.statement, {
    issuer: request.issuer,
    subject: undefined,
    issuedAt: undefined,
    content: { kind: "app_action", action: request.action },
  });
  assert.deepEqual(
    verified.toBeSigned,
    statementBytes(
      {
        issuer: request.issuer,
        content: { kind: "app_action", action: request.action },
      },
      "Ed25519",
      request.key.kid,
    ).toBeSigned,
  );
  assert.equal(verified.checks.content, "not_checked");
  const changed = structuredClone(artifact);
  changed.cose_sign1[changed.cose_sign1.length - 1] ^= 1;
  assert.throws(() => verifyDocumentArtifact(changed));
});
