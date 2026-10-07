import { verifyDocumentArtifact, statementBytes } from "../src/statements.ts";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
import { canonical, decodeCanonical, digest } from "../src/encoding.ts";
import {
  actionSchemaHash,
  validateAppAction,
  validateActionAdmission,
  validateActionCommand,
  appActionDigest,
} from "../src/action.ts";
import { validateActionSchema, validateApp } from "../src/validation.ts";
import type {
  ActionSchema,
  ActionValue,
  AppAction,
  AppRegistration,
  FieldType,
} from "../src/contracts.ts";
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
const actionApp = (): AppRegistration => value("action_app");
const schema = (): ActionSchema => actionApp().action_schema!;
const review = (): AppAction => value("app_action_1")[2];

test("sample actions validate independently and match Rust commitments", () => {
  const app = actionApp();
  validateApp(app);
  for (let i = 0; i < 4; i++) {
    const action = value(`app_action_${i}`)[2] as AppAction;
    validateAppAction(action);
    validateActionCommand(action, app.action_schema!);
    validateActionAdmission(action, app, action.issued_at_ms);
    assert.deepEqual(action.schema_hash, actionSchemaHash(app.action_schema!));
    assert.deepEqual(
      appActionDigest(action),
      digest("dmsg/app-action/v1", action),
    );
  }
});

test("action file display names retain leading U+FEFF through the wire", () => {
  const action = fixture();
  action.files[0]!.display_name = "﻿report.pdf";
  validateAppAction(action);
  const decoded = decodeCanonical(canonical(action)) as unknown as AppAction;
  validateAppAction(decoded);
  assert.deepEqual(decoded, action);
});

test("action receiver, origin, manifest, schema, command and intent are inseparable", () => {
  const action = fixture(),
    original = appActionDigest(action);
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
      a.actor[0] = a.actor[0]! ^ 1;
    },
    (a) => {
      a.intent_hash[0] = a.intent_hash[0]! ^ 1;
    },
    (a) => {
      a.schema_hash[0] = a.schema_hash[0]! ^ 1;
    },
    (a) => {
      a.command = value("app_action_3")[2].command;
    },
    (a) => {
      a.command.args[0]!.value = { Nat: 43n };
    },
  ];
  for (const change of changes) {
    const other = structuredClone(action);
    change(other);
    validateAppAction(other);
    assert.notDeepEqual(appActionDigest(other), original);
  }
});

test("commands conform exactly to the schema they sign", () => {
  validateActionCommand(review(), schema());
  const other = review();
  other.schema_hash[0] = other.schema_hash[0]! ^ 1;
  assert.throws(
    () => validateActionCommand(other, schema()),
    /INTEGRITY_FAILED/,
  );
  const unknown = review();
  unknown.command.name = "TransferAssets";
  assert.throws(
    () => validateActionCommand(unknown, schema()),
    /UNSUPPORTED_PROTOCOL/,
  );
  const set = (i: number, v: ActionValue) => (a: AppAction) => {
    a.command.args[i]!.value = v;
  };
  for (const change of [
    (a: AppAction) => void a.command.args.pop(),
    (a: AppAction) => void a.command.args.push(a.command.args[0]!),
    // Same-typed arguments cannot be swapped silently.
    (a: AppAction) => {
      const [x, y] = [a.command.args[1]!, a.command.args[2]!];
      a.command.args[1] = y;
      a.command.args[2] = x;
    },
    set(0, { Nat: 0n }),
    set(2, { Nat: 0x1_0000_0000n }),
    set(0, { Text: "42" }),
    set(3, { Choice: "Escalated" }),
    set(3, "Null"),
    set(5, { Text: "x".repeat(4097) }),
    set(4, {
      List: [
        {
          Record: [
            { name: "locator", value: { Text: "07/\ntotal_supply" } },
            { name: "detail", value: { Text: "x" } },
            { name: "blocking", value: { Bool: true } },
          ],
        },
      ],
    }),
  ]) {
    const a = review();
    change(a);
    assert.throws(() => validateActionCommand(a, schema()), /INVALID_INPUT/);
  }
  const transition = value("app_action_2")[2] as AppAction;
  transition.command.args[4]!.value = "Null";
  validateActionCommand(transition, schema());
  transition.command.args[2]!.value = { Hash: new Uint8Array(32) };
  assert.throws(() => validateActionCommand(transition, schema()));
});

test("registrations carry a bounded schema exactly with SignAction", () => {
  const app = actionApp();
  assert.throws(() => validateApp({ ...app, action_schema: null }));
  assert.throws(() =>
    validateApp({
      ...app,
      capabilities: ["Authenticate"],
      profiles: [],
    }),
  );
  const s = schema();
  const nat: FieldType = { Nat: { min: 0n, max: 9n } };
  const list = (item: FieldType): FieldType => ({
    List: { item, max_items: 1n },
  });
  const one = (ty: FieldType): ActionSchema => ({
    version: 1n,
    commands: [
      {
        name: "A",
        title: [{ locale: "en", text: "A" }],
        fields: [{ name: "a", label: [{ locale: "en", text: "a" }], ty }],
      },
    ],
  });
  validateActionSchema(s);
  validateActionSchema(one(list(list(nat))));
  for (const bad of [
    { ...s, version: 2n },
    { ...s, commands: [] },
    { ...s, commands: [s.commands[0]!, s.commands[0]!] },
    one(list(list(list(nat)))),
    one({ Nat: { min: 2n, max: 1n } }),
    one({ Text: { max_bytes: 0n, multiline: false } }),
    one({ Choice: { options: [] } }),
    one({ Record: { fields: [] } }),
    one({ Optional: { item: { Optional: { item: nat } } } }),
    {
      ...one(nat),
      commands: [
        { ...one(nat).commands[0]!, title: [{ locale: "en", text: "a\nb" }] },
      ],
    },
  ])
    assert.throws(() => validateActionSchema(bad as ActionSchema));
});

test("unknown semantics and stale registration cannot enter action admission", () => {
  const action = fixture();
  const app = actionApp();
  validateActionAdmission(action, app, action.issued_at_ms);
  const deep: ActionValue = { List: [{ List: [{ List: [{ Nat: 1n }] }] }] };
  for (const other of [
    { ...action, version: 2n },
    {
      ...action,
      command: {
        name: "Transfer",
        args: [{ name: "payload", value: { Bytes: new Uint8Array([1]) } }],
      },
    },
    { ...action, command: { ...action.command, hidden_semantics: true } },
    { ...action, command: { ...action.command, name: "not a name" } },
    {
      ...action,
      command: { ...action.command, args: [{ name: "a", value: deep }] },
    },
    { ...action, display_summary: "approve everything" },
    { ...action, files: [...action.files, ...action.files] },
    { ...action, files: [{ ...action.files[0], file_id: "😀" }] },
    { ...action, expires_at_ms: action.expires_at_ms + 1n },
    { ...action, receiver: new Uint8Array([4]) },
    { ...action, actor: new Uint8Array(12) },
    { ...action, files: [{ ...action.files[0], revision: 0n }] },
  ])
    assert.throws(() => validateAppAction(other as AppAction));
  assert.throws(() =>
    validateActionAdmission(action, app, action.expires_at_ms),
  );
  for (const stale of [
    { ...app, paused: true },
    { ...app, config_version: 2n },
    { ...app, action_schema: { ...schema(), commands: schema().commands.slice(1) } },
    value("app") as AppRegistration,
  ])
    assert.throws(() =>
      validateActionAdmission(action, stale, action.issued_at_ms),
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
      verified.keyFingerprint,
    ).toBeSigned,
  );
  assert.equal(verified.checks.content, "not_checked");
  const changed = structuredClone(artifact);
  changed.cose_sign1[changed.cose_sign1.length - 1] ^= 1;
  assert.throws(() => verifyDocumentArtifact(changed));
});
