import { test } from "node:test";
import assert from "node:assert/strict";
import {
  base64,
  browserProofMessage,
  browserOperationDigest,
  connectDmsg,
  hex,
  parseBrowserCommand,
  verifyBrowserProof,
  type BrowserCommand,
  type BrowserPort,
} from "../src/browser.ts";
import { canonical, sha256 } from "../src/encoding.ts";
import { EXTENSION_PROTOCOL } from "../src/validation.ts";

async function session() {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign", "verify"],
  );
  return {
    publicKeyDer: new Uint8Array(
      await crypto.subtle.exportKey("spki", pair.publicKey),
    ),
    sign: async (message: Uint8Array) =>
      new Uint8Array(
        await crypto.subtle.sign(
          { name: "ECDSA", hash: "SHA-256" },
          pair.privateKey,
          Uint8Array.from(message).buffer,
        ),
      ),
  };
}
const source = {
  origin: "https://product.test",
  documentId: "document-a",
  tabId: 1,
  frameId: 0,
};

test("fresh connection proofs bind the exact command, source document, nonce and original key", async () => {
  const key = await session(),
    stranger = await session();
  const command: BrowserCommand = {
    appId: "product",
    operationId: "01".repeat(32),
    publicKey: base64(key.publicKeyDer),
    method: "getOperation",
    payload: null,
    resultDigest: null,
  };
  const nonce = "02".repeat(32),
    signature = base64(
      await key.sign(await browserProofMessage(command, nonce, source)),
    );
  await verifyBrowserProof(command, nonce, source, signature);
  for (const altered of [
    { ...command, operationId: "03".repeat(32) },
    { ...command, method: "cancelOperation" as const },
    { ...command, publicKey: base64(stranger.publicKeyDer) },
  ])
    await assert.rejects(verifyBrowserProof(altered, nonce, source, signature));
  await assert.rejects(
    verifyBrowserProof(command, "04".repeat(32), source, signature),
  );
  await assert.rejects(
    verifyBrowserProof(
      command,
      nonce,
      { ...source, documentId: "document-b" },
      signature,
    ),
  );
  await assert.rejects(
    verifyBrowserProof(
      command,
      nonce,
      { ...source, origin: "https://evil.test" },
      signature,
    ),
  );
  assert.throws(() => parseBrowserCommand({ ...command, extra: true }));
  assert.throws(() => parseBrowserCommand({ ...command, payload: "AA==" }));
});

test("public SDK completes the proof handshake, correlates calls and preserves operation IDs after reconnect", async () => {
  const key = await session();
  const request = {
    version: 1n,
    environment: "Local" as const,
    app_id: "product",
    app_config_version: 1n,
    origin: source.origin,
    receiver: new Uint8Array([3, 1]),
    challenge_hash: new Uint8Array(32).fill(5),
    session_key_hash: await sha256(key.publicKeyDer),
    purpose: "Login" as const,
    nonce: new Uint8Array(32).fill(6),
    operation_id: new Uint8Array(32).fill(7),
    issued_at_ms: BigInt(Date.now()),
    expires_at_ms: BigInt(Date.now()) + 300000n,
  };
  const seen: string[] = [];
  function connect(_extension: string, options: { name: string }): BrowserPort {
    assert.equal(options.name, EXTENSION_PROTOCOL);
    let receive: (value: any) => void, disconnected: () => void;
    const pending = new Map<string, BrowserCommand>();
    return {
      onMessage: {
        addListener(fn) {
          receive = fn;
        },
      },
      onDisconnect: {
        addListener(fn) {
          disconnected = fn;
        },
      },
      disconnect() {
        disconnected();
      },
      postMessage(value: any) {
        queueMicrotask(() => {
          void (async () => {
            if (value.type === "capabilities") {
              receive({
                protocol: EXTENSION_PROTOCOL,
                type: "result",
                requestId: value.requestId,
                ok: true,
                value: {
                  protocol: EXTENSION_PROTOCOL,
                  methods: ["authenticate"],
                },
              });
              return;
            }
            if (value.type === "request") {
              const command = parseBrowserCommand(value.command);
              pending.set(value.requestId, command);
              receive({
                protocol: EXTENSION_PROTOCOL,
                type: "challenge",
                requestId: value.requestId,
                nonce: "08".repeat(32),
                ...source,
              });
            } else {
              const command = pending.get(value.requestId)!;
              pending.delete(value.requestId);
              await verifyBrowserProof(
                command,
                "08".repeat(32),
                source,
                value.signature,
              );
              seen.push(command.operationId);
              receive({
                protocol: EXTENSION_PROTOCOL,
                type: "result",
                requestId: value.requestId,
                ok: true,
                value: {
                  operationId: command.operationId,
                  kind: "authentication",
                  state: "awaiting_user",
                },
              });
            }
          })().catch((e) => {
            throw e;
          });
        });
      },
    };
  }
  let client = connectDmsg({
    extensionId: "a".repeat(32),
    appId: "product",
    origin: source.origin,
    session: key,
    connect,
  });
  assert.deepEqual((await client.capabilities()).methods, ["authenticate"]);
  const created = await client.authenticate(request);
  client.disconnect();
  client = connectDmsg({
    extensionId: "a".repeat(32),
    appId: "product",
    origin: source.origin,
    session: key,
    connect,
  });
  assert.equal(
    (await client.getOperation(created.operationId)).operationId,
    created.operationId,
  );
  assert.deepEqual(seen, [
    hex(request.operation_id),
    hex(request.operation_id),
  ]);
  assert.throws(() => client.checkout(undefined as never), /record/);
  const { readFileSync } = await import("node:fs");
  const { decodeCanonical } = await import("../src/encoding.ts");
  const vectors = JSON.parse(
    readFileSync(
      new URL(
        "../../../src/dmsg_types/tests/commerce_vectors.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const quote = (
    decodeCanonical(
      Buffer.from(
        vectors.find((v: any) => v.name === "checkout_terms_v2").cbor_hex,
        "hex",
      ),
    ) as any
  )[2];
  quote.offer.app_id = "product";
  const payment = await client.checkout({
    offer: quote.offer,
    approving_account: new Uint8Array(12).fill(1),
    product_approval: null,
    method: "Cash",
  });
  assert.equal(payment.operationId, hex(quote.offer.operation_id));
  const document = await client.signDocument("09".repeat(32), {
    requestId: "09".repeat(32),
    statement: { content: { kind: "text", text: "hello" } },
  });
  assert.equal(document.operationId, "09".repeat(32));
  client.disconnect();
});

test("operation commitment is stable across documents but changes with content or key", async () => {
  const key = await session();
  const command: BrowserCommand = {
    method: "authenticate",
    appId: "product",
    operationId: "01".repeat(32),
    publicKey: base64(key.publicKeyDer),
    payload: base64(canonical({ amount: 1n })),
    resultDigest: null,
  };
  const hash = hex(await browserOperationDigest(command, source.origin));
  assert.notEqual(
    hex(
      await browserOperationDigest(
        { ...command, payload: base64(canonical({ amount: 2n })) },
        source.origin,
      ),
    ),
    hash,
  );
  assert.notEqual(
    hex(await browserOperationDigest(command, "https://elsewhere.test")),
    hash,
  );
});
