import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

// Exercise both installed transports: AuthClient's signer and the direct
// signer used by the desktop login bridge. No II account or network is used.
const frontend = createRequire(
  new URL("../src/dmsg_frontend/package.json", import.meta.url),
);
const auth = createRequire(frontend.resolve("@icp-sdk/auth/client"));

function setup(t, HeartbeatClient) {
  t.mock.timers.enable({ apis: ["Date", "setTimeout", "setInterval"], now: 0 });
  const listeners = new Set();
  const requests = [];
  let replying = true;
  let established = 0;
  let disconnected = 0;
  const origin = "https://id.ai";
  const signerWindow = {
    postMessage(request) {
      requests.push(request);
      // Slower than the 300 ms establishment polling interval, but well
      // within the actual 2 s disconnect deadline.
      setTimeout(() => {
        if (replying) emit(request.id);
      }, 700);
    },
  };
  function emit(id, source = signerWindow, from = origin) {
    const event = {
      source,
      origin: from,
      data: { jsonrpc: "2.0", id, result: "ready" },
    };
    for (const listener of [...listeners]) listener(event);
  }
  new HeartbeatClient({
    window: {
      addEventListener: (_, listener) => listeners.add(listener),
      removeEventListener: (_, listener) => listeners.delete(listener),
    },
    signerWindow,
    disconnectTimeout: 2000,
    onEstablish: () => established++,
    onDisconnect: () => disconnected++,
    onEstablishTimeout: () =>
      assert.fail("The responsive signer should establish a channel"),
    onStatusChange: () => {},
  });
  return {
    requests,
    emit,
    stopReplies: () => {
      replying = false;
    },
    get established() {
      return established;
    },
    get disconnected() {
      return disconnected;
    },
    advance(ms) {
      for (let elapsed = 0; elapsed < ms; elapsed += 10) t.mock.timers.tick(10);
    },
  };
}

for (const [name, resolve] of [
  ["AuthClient", auth],
  ["desktop login bridge", frontend],
]) {
  const entry = pathToFileURL(resolve.resolve("@icp-sdk/signer/web"));
  const { HeartbeatClient } = await import(
    new URL("./heartbeat/client.js", entry)
  );

  test(`${name}: delayed startup replies do not close a responsive login window`, (t) => {
    const channel = setup(t, HeartbeatClient);
    channel.advance(10000);
    assert.equal(channel.established, 1);
    assert.equal(channel.disconnected, 0);
    assert.ok(
      channel.requests.length <= 13,
      "Late establishment replies must not multiply polling",
    );
  });

  test(`${name}: ignored replies neither create polling loops nor postpone disconnect`, (t) => {
    const channel = setup(t, HeartbeatClient);
    channel.advance(1000);
    assert.equal(channel.established, 1);
    channel.stopReplies();
    const pending = channel.requests.at(-1).id;
    const count = channel.requests.length;
    channel.emit(pending, {});
    channel.emit(pending, undefined, "https://unrelated.example");
    channel.emit("unsolicited");
    channel.emit(channel.requests[0].id); // A late, already-consumed handshake reply.
    channel.advance(400);
    assert.equal(channel.requests.length, count);
    channel.advance(1600);
    assert.equal(channel.disconnected, 1);
    channel.emit(pending);
    channel.advance(1000);
    assert.equal(channel.requests.length, count);
  });
}
