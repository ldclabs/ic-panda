import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  CONNECT_PROTOCOL,
  base64,
  connectDmsg,
  connectOperationDigest,
  connectProofMessage,
  hex,
  parseConnectCommand,
  type BridgeWindow,
  type ConnectHost,
} from "../src/connect.ts";
import { canonical } from "../src/encoding.ts";

const v = JSON.parse(
  readFileSync(
    new URL("../../../tests/vectors/client/connect-v1.json", import.meta.url),
    "utf8",
  ),
);
const BRIDGE = "https://connect.test";
const ORIGIN = "https://app.test";

async function session() {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign", "verify"],
  );
  const publicKeyDer = new Uint8Array(
    await crypto.subtle.exportKey("spki", pair.publicKey),
  );
  return {
    publicKeyDer,
    sign: async (message: Uint8Array) =>
      new Uint8Array(
        await crypto.subtle.sign(
          { name: "ECDSA", hash: "SHA-256" },
          pair.privateKey,
          Uint8Array.from(message).buffer,
        ),
      ),
    verify: async (message: Uint8Array, signature: Uint8Array) =>
      crypto.subtle.verify(
        { name: "ECDSA", hash: "SHA-256" },
        pair.publicKey,
        Uint8Array.from(signature).buffer,
        Uint8Array.from(message).buffer,
      ),
  };
}

/** A page with one popup; `sent` records what the page posted to it. */
function fakeHost(blocked = false) {
  const listeners = new Set<(e: any) => void>();
  const sent: { message: any; target: string }[] = [];
  const popup: BridgeWindow & { closed: boolean } = {
    closed: false,
    postMessage(message, target) {
      sent.push({ message, target });
    },
    close() {
      popup.closed = true;
    },
  };
  const host: ConnectHost & {
    deliver(data: unknown, source?: unknown, origin?: string): void;
    opened: string[];
  } = {
    origin: ORIGIN,
    opened: [],
    open(url) {
      host.opened.push(url);
      return blocked ? null : popup;
    },
    addEventListener: (_, fn) => listeners.add(fn),
    removeEventListener: (_, fn) => listeners.delete(fn),
    deliver(data, source = popup, origin = BRIDGE) {
      for (const fn of listeners) fn({ data, source, origin });
    },
  };
  return { host, popup, sent };
}

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

test("reproduces the connect-v1 proof message and operation digest", () => {
  const command = parseConnectCommand(v.command);
  assert.equal(hex(canonical(command)), v.command_cbor_hex);
  assert.equal(
    hex(
      connectProofMessage(
        command,
        v.proof.nonce_hex,
        v.proof.origin,
        v.proof.connection_id_hex,
      ),
    ),
    v.proof.message_hex,
  );
  assert.equal(
    hex(connectOperationDigest(command, v.proof.origin)),
    v.operation_digest_hex,
  );
  for (const bad of [
    { ...v.command, extra: 1 },
    { ...v.command, operationId: "00".repeat(32) },
    { ...v.command, payload: null },
    { ...v.command, resultDigest: "11".repeat(32) },
  ])
    assert.throws(() => parseConnectCommand(bad));
  assert.throws(() => parseConnectCommand({ ...v.command, method: "steal" }), {
    code: "UNSUPPORTED_PROTOCOL",
  });
});

test("a blocked popup fails every call", async () => {
  const { host } = fakeHost(true);
  const client = connectDmsg({
    appId: "token-list",
    session: await session(),
    bridgeOrigin: BRIDGE,
    host,
  });
  await assert.rejects(client.ready, { code: "POPUP_BLOCKED" });
  await assert.rejects(client.getOperation("01".repeat(32)), {
    code: "POPUP_BLOCKED",
  });
  assert.equal(host.opened[0], `${BRIDGE}/v1/`);
});

test("handshake, challenge and proof bind this origin and connection", async () => {
  const { host, sent } = fakeHost();
  const key = await session();
  const client = connectDmsg({
    appId: "token-list",
    session: key,
    bridgeOrigin: BRIDGE,
    host,
  });
  await tick(250);
  const hellos = sent.filter((s) => s.message.type === "hello");
  assert.ok(hellos.length >= 2, "hello repeats until ready");
  assert.ok(sent.every((s) => s.target === BRIDGE));
  assert.deepEqual(hellos[0]!.message, {
    protocol: CONNECT_PROTOCOL,
    type: "hello",
    appId: "token-list",
  });
  const connectionId = "55".repeat(32);
  const ready = {
    protocol: CONNECT_PROTOCOL,
    type: "ready",
    connectionId,
    origin: ORIGIN,
    appId: "token-list",
    methods: ["getOperation"],
  };
  // Another window or origin cannot answer for the bridge.
  host.deliver(ready, {}, BRIDGE);
  host.deliver(ready, undefined, "https://evil.test");
  assert.equal(client.open, false);
  host.deliver(ready);
  await client.ready;
  assert.equal(client.open, true);
  const count = sent.length;
  await tick(250);
  assert.equal(sent.length, count, "no hello after ready");

  const operationId = "01".repeat(32);
  const result = client.getOperation(operationId);
  await tick();
  const request = sent.at(-1)!.message;
  assert.equal(request.type, "request");
  assert.equal(request.command.publicKey, base64(key.publicKeyDer));
  const nonce = "44".repeat(32);
  // A challenge for another origin or connection is never signed.
  host.deliver({
    protocol: CONNECT_PROTOCOL,
    type: "challenge",
    requestId: request.requestId,
    nonce,
    origin: "https://evil.test",
    connectionId,
  });
  await assert.rejects(result, { code: "SOURCE_MISMATCH" });

  const second = client.getOperation(operationId);
  await tick();
  const again = sent.at(-1)!.message;
  host.deliver({
    protocol: CONNECT_PROTOCOL,
    type: "challenge",
    requestId: again.requestId,
    nonce,
    origin: ORIGIN,
    connectionId,
  });
  await tick(20);
  const proof = sent.at(-1)!.message;
  assert.equal(proof.type, "proof");
  assert.ok(
    await key.verify(
      connectProofMessage(again.command, nonce, ORIGIN, connectionId),
      Uint8Array.from(Buffer.from(proof.signature, "base64")),
    ),
  );
  const value = { operationId, kind: "document", state: "signed" };
  host.deliver({
    protocol: CONNECT_PROTOCOL,
    type: "result",
    requestId: again.requestId,
    ok: true,
    value,
  });
  assert.deepEqual(await second, value);

  const third = client.cancelOperation(operationId);
  await tick();
  host.deliver({
    protocol: CONNECT_PROTOCOL,
    type: "result",
    requestId: sent.at(-1)!.message.requestId,
    ok: false,
    error: "NOT_CANCELLABLE",
  });
  await assert.rejects(third, { code: "NOT_CANCELLABLE" });
  client.close();
});

test("rejected handshakes, wrong ready frames and closed popups end the connection", async () => {
  {
    const { host } = fakeHost();
    const client = connectDmsg({
      appId: "token-list",
      session: await session(),
      bridgeOrigin: BRIDGE,
      host,
    });
    host.deliver({
      protocol: CONNECT_PROTOCOL,
      type: "rejected",
      error: "APP_NOT_REGISTERED",
    });
    await assert.rejects(client.ready, { code: "APP_NOT_REGISTERED" });
  }
  {
    const { host } = fakeHost();
    const client = connectDmsg({
      appId: "token-list",
      session: await session(),
      bridgeOrigin: BRIDGE,
      host,
    });
    host.deliver({
      protocol: CONNECT_PROTOCOL,
      type: "ready",
      connectionId: "55".repeat(32),
      origin: "https://other.test",
      appId: "token-list",
      methods: [],
    });
    await assert.rejects(client.ready, { code: "SOURCE_MISMATCH" });
  }
  {
    const { host, popup } = fakeHost();
    const client = connectDmsg({
      appId: "token-list",
      session: await session(),
      bridgeOrigin: BRIDGE,
      host,
    });
    host.deliver({
      protocol: CONNECT_PROTOCOL,
      type: "ready",
      connectionId: "55".repeat(32),
      origin: ORIGIN,
      appId: "token-list",
      methods: [],
    });
    await client.ready;
    const call = client.getOperation("01".repeat(32));
    await tick();
    popup.closed = true;
    await assert.rejects(call, { code: "DISCONNECTED" });
    await assert.rejects(client.getOperation("01".repeat(32)), {
      code: "DISCONNECTED",
    });
  }
});

test("a bridge that never answers is unavailable after 15 seconds", async () => {
  mock.timers.enable({ apis: ["setInterval", "setTimeout", "Date"] });
  try {
    const { host } = fakeHost();
    const client = connectDmsg({
      appId: "token-list",
      session: await session(),
      bridgeOrigin: BRIDGE,
      host,
    });
    mock.timers.tick(15_200);
    await assert.rejects(client.ready, { code: "BRIDGE_UNAVAILABLE" });
  } finally {
    mock.timers.reset();
  }
});

test("requests must bind this page before they reach the bridge", async () => {
  const { host } = fakeHost();
  const client = connectDmsg({
    appId: "token-list",
    session: await session(),
    bridgeOrigin: BRIDGE,
    host,
  });
  await assert.rejects(
    client.signDocument("01".repeat(32), {
      protocol: "dmsg-extension/4",
      requestId: "01".repeat(32),
    }),
    { code: "REQUEST_BINDING" },
  );
  await assert.rejects(
    client.signDocument("01".repeat(32), {
      protocol: CONNECT_PROTOCOL,
      requestId: "02".repeat(32),
    }),
    { code: "REQUEST_BINDING" },
  );
  client.close();
});
