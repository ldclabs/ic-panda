import { validateAppAction } from "./action.ts";
import type {
  AppAction,
  AuthenticationRequest,
  CheckoutRequest,
} from "./contracts.ts";
import { DmsgError, ensure } from "./cose-errors.ts";
import {
  base64,
  canonical,
  digest,
  equalBytes,
  hex,
  sha256,
  unbase64,
  unhex,
  utf8,
} from "./encoding.ts";
import { validateIdentifier, validateShape } from "./validation.ts";

export { DmsgError } from "./cose-errors.ts";
export { base64, hex, unbase64, unhex } from "./encoding.ts";

/** dmsg-connect/1: an application page talks to the dMsg bridge page in a popup. */
export const CONNECT_PROTOCOL = "dmsg-connect/1";
export const BRIDGE_ORIGIN = "https://connect.dmsg.net";

export interface ConnectSession {
  publicKeyDer: Uint8Array;
  sign(message: Uint8Array): Promise<Uint8Array>;
}
export type ConnectMethod =
  | "authenticate"
  | "signDocument"
  | "signAction"
  | "checkout"
  | "getOperation"
  | "openOperation"
  | "cancelOperation"
  | "acknowledge";
export interface ConnectCommand {
  method: ConnectMethod;
  appId: string;
  operationId: string;
  publicKey: string;
  payload: string | null;
  resultDigest: string | null;
}
export type OperationState =
  | "awaiting_user"
  | "authorized"
  | "execution_unknown"
  | "signed"
  | "returned"
  | "rejected"
  | "cancelled"
  | "expired"
  | "failed"
  | "result_expired";
export interface ConnectOperation {
  operationId: string;
  kind: "authentication" | "document" | "action" | "checkout";
  state: OperationState;
  result?: {
    proof?: string;
    artifact?: unknown;
    receipt?: unknown;
    keyDescriptorCbor?: string;
    executionCertificateCbor?: string;
    executionId?: string;
    [key: string]: unknown;
  };
  resultDigest?: string;
  error?: string;
}

// Methods that create an operation carry its payload.
const CREATE_METHODS: string[] = [
  "authenticate",
  "signDocument",
  "signAction",
  "checkout",
];
const METHODS = [
  ...CREATE_METHODS,
  "getOperation",
  "openOperation",
  "cancelOperation",
  "acknowledge",
];
const DIGEST = /^[0-9a-f]{64}$/;
const HELLO_MS = 100;
const HANDSHAKE_MS = 15_000;
const CALL_MS = 30_000;
const MAX_PENDING = 16;

/** The closed command shape of connect-v1 §4.3. */
export function parseConnectCommand(value: unknown): ConnectCommand {
  ensure(
    value && typeof value === "object" && !Array.isArray(value),
    "INVALID_INPUT",
  );
  const c = value as ConnectCommand;
  ensure(
    Object.keys(c).sort().join(",") ===
      "appId,method,operationId,payload,publicKey,resultDigest",
    "INVALID_INPUT",
  );
  ensure(METHODS.includes(c.method), "UNSUPPORTED_PROTOCOL");
  validateIdentifier(c.appId);
  ensure(
    typeof c.operationId === "string" &&
      DIGEST.test(c.operationId) &&
      !/^0+$/.test(c.operationId),
    "INVALID_INPUT",
  );
  ensure(
    typeof c.publicKey === "string" && unbase64(c.publicKey, 128).length === 91,
    "INVALID_INPUT",
  );
  const create = CREATE_METHODS.includes(c.method);
  ensure(
    create ? typeof c.payload === "string" : c.payload === null,
    "INVALID_INPUT",
  );
  if (create) unbase64(c.payload!);
  ensure(
    c.method === "acknowledge"
      ? typeof c.resultDigest === "string" && DIGEST.test(c.resultDigest)
      : c.resultDigest === null,
    "INVALID_INPUT",
  );
  return structuredClone(c);
}

/** What the session key signs after the bridge's challenge (§4.3). */
export function connectProofMessage(
  command: ConnectCommand,
  nonce: string,
  origin: string,
  connectionId: string,
): Uint8Array {
  ensure(
    typeof nonce === "string" &&
      DIGEST.test(nonce) &&
      typeof connectionId === "string" &&
      DIGEST.test(connectionId),
    "INVALID_INPUT",
  );
  return digest("dmsg/connect-proof/v1", [
    CONNECT_PROTOCOL,
    unhex(nonce),
    origin,
    unhex(connectionId),
    command,
  ]);
}

/** The operation commitment devices check (§4.3); unchanged from browser-v4 but for the domain. */
export function connectOperationDigest(
  command: ConnectCommand,
  origin: string,
): Uint8Array {
  ensure(CREATE_METHODS.includes(command.method), "INVALID_INPUT");
  return digest("dmsg/connect-operation/v1", [
    origin,
    command.appId,
    command.operationId,
    command.method,
    unbase64(command.publicKey),
    unbase64(command.payload!),
  ]);
}

/** The popup as the page sees it. */
export interface BridgeWindow {
  readonly closed: boolean;
  postMessage(message: unknown, targetOrigin: string): void;
  close(): void;
}

/** The page's window; tests replace it. */
export interface ConnectHost {
  readonly origin: string;
  open(url: string, name: string, features: string): BridgeWindow | null;
  addEventListener(
    type: "message",
    listener: (event: {
      source: unknown;
      origin: string;
      data: unknown;
    }) => void,
  ): void;
  removeEventListener(
    type: "message",
    listener: (event: {
      source: unknown;
      origin: string;
      data: unknown;
    }) => void,
  ): void;
}

/**
 * Open the dMsg bridge page and connect this page's session key to it.
 * Call it synchronously in a user gesture: it opens the popup at once. The
 * session may still be loading (a promise), e.g. from IndexedDB. The
 * session key stays with the product; no dMsg identity is exported.
 */
export function connectDmsg(options: {
  appId: string;
  session: ConnectSession | Promise<ConnectSession>;
  bridgeOrigin?: string;
  host?: ConnectHost;
}) {
  validateIdentifier(options.appId);
  const host = options.host ?? (globalThis as unknown as ConnectHost);
  const origin = host.origin ?? globalThis.location?.origin;
  ensure(origin && origin !== "null", "INVALID_ORIGIN");
  const bridgeOrigin = new URL(options.bridgeOrigin ?? BRIDGE_ORIGIN).origin;
  const session = Promise.resolve(options.session);
  session.catch(() => {});
  const popup = host.open(
    `${bridgeOrigin}/v1/`,
    "dmsg-connect",
    "popup,width=440,height=680",
  );
  const pending = new Map<
    string,
    {
      command: ConnectCommand | undefined;
      resolve: (v: any) => void;
      reject: (e: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  let connectionId: string | null = null;
  let methods: string[] = [];
  let closed = false;
  let settle: { resolve: () => void; reject: (e: Error) => void };
  const ready = new Promise<void>((resolve, reject) => {
    settle = { resolve, reject };
  });
  // A rejected `ready` must not surface as unhandled when only calls observe it.
  ready.catch(() => {});
  const timers: ReturnType<typeof setInterval>[] = [];
  const finish = (id: string, value: unknown, error?: Error) => {
    const p = pending.get(id);
    if (!p) return;
    clearTimeout(p.timer);
    pending.delete(id);
    error ? p.reject(error) : p.resolve(value);
  };
  const close = (code = "DISCONNECTED") => {
    if (closed) return;
    closed = true;
    timers.forEach(clearInterval);
    host.removeEventListener("message", listen);
    settle.reject(new DmsgError(code));
    for (const id of [...pending.keys()])
      finish(id, null, new DmsgError("DISCONNECTED"));
  };
  const post = (frame: Record<string, unknown>) =>
    popup!.postMessage({ protocol: CONNECT_PROTOCOL, ...frame }, bridgeOrigin);

  function listen(event: { source: unknown; origin: string; data: unknown }) {
    if (event.source !== popup || event.origin !== bridgeOrigin) return;
    const m = event.data as Record<string, any>;
    if (!m || typeof m !== "object" || m.protocol !== CONNECT_PROTOCOL) return;
    if (m.type === "ready" && connectionId === null) {
      if (
        m.origin !== origin ||
        m.appId !== options.appId ||
        typeof m.connectionId !== "string" ||
        !DIGEST.test(m.connectionId) ||
        !Array.isArray(m.methods)
      )
        return close("SOURCE_MISMATCH");
      connectionId = m.connectionId;
      methods = m.methods;
      clearInterval(timers.shift());
      settle.resolve();
      return;
    }
    if (m.type === "rejected" && connectionId === null)
      return close(typeof m.error === "string" ? m.error : "FORBIDDEN");
    const p = pending.get(m.requestId);
    if (!p || connectionId === null) return;
    if (m.type === "challenge") {
      // Sign only for this page's origin on the connection the bridge opened.
      if (
        !p.command ||
        m.origin !== origin ||
        m.connectionId !== connectionId ||
        typeof m.nonce !== "string" ||
        !DIGEST.test(m.nonce)
      ) {
        finish(m.requestId, null, new DmsgError("SOURCE_MISMATCH"));
        return;
      }
      void (async () => {
        const proof = await (
          await session
        ).sign(connectProofMessage(p.command!, m.nonce, origin, connectionId!));
        if (!closed && pending.get(m.requestId) === p)
          post({
            type: "proof",
            requestId: m.requestId,
            signature: base64(proof),
          });
      })().catch((e) => finish(m.requestId, null, e));
    } else if (m.type === "result") {
      finish(
        m.requestId,
        m.value,
        m.ok ? undefined : new DmsgError(m.error || "UNAVAILABLE"),
      );
    }
  }

  if (!popup) {
    closed = true;
    settle!.reject(new DmsgError("POPUP_BLOCKED"));
  } else {
    host.addEventListener("message", listen);
    const started = Date.now();
    timers.push(
      setInterval(() => {
        if (Date.now() - started > HANDSHAKE_MS)
          return close("BRIDGE_UNAVAILABLE");
        try {
          post({ type: "hello", appId: options.appId });
        } catch {
          // The popup is still loading another origin; the next hello reaches it.
        }
      }, HELLO_MS),
    );
    timers.push(
      setInterval(() => {
        if (popup.closed) close();
      }, 500),
    );
  }

  async function send(command?: ConnectCommand): Promise<any> {
    await ready;
    if (closed) throw new DmsgError("DISCONNECTED");
    if (pending.size >= MAX_PENDING) throw new DmsgError("QUOTA_EXCEEDED");
    const id = hex(crypto.getRandomValues(new Uint8Array(32)));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => finish(id, null, new DmsgError("TRANSPORT_TIMEOUT")),
        CALL_MS,
      );
      pending.set(id, { command, resolve, reject, timer });
      try {
        post(
          command
            ? { type: "request", requestId: id, command }
            : { type: "capabilities", requestId: id },
        );
      } catch (e) {
        finish(id, null, e as Error);
      }
    });
  }
  async function command(
    method: ConnectMethod,
    operationId: string,
    payload: string | null = null,
    resultDigest: string | null = null,
  ): Promise<ConnectOperation> {
    const { publicKeyDer } = await session;
    return send(
      parseConnectCommand({
        method,
        appId: options.appId,
        operationId,
        publicKey: base64(publicKeyDer),
        payload,
        resultDigest,
      }),
    );
  }
  // Every method reports failures as a rejected promise.
  return {
    /** Resolves once the bridge accepted this page's origin for the app. */
    ready,
    /** Whether the popup is still connected. */
    get open() {
      return !closed && connectionId !== null;
    },
    /** The bridge's methods, as announced in its `ready`. */
    get methods() {
      return [...methods];
    },
    capabilities: (): Promise<{ protocol: string; methods: string[] }> =>
      send(),
    authenticate: async (
      request: AuthenticationRequest,
    ): Promise<ConnectOperation> => {
      validateShape("AuthenticationRequest", request);
      ensure(
        request.app_id === options.appId &&
          request.origin === origin &&
          equalBytes(
            request.session_key_hash,
            sha256((await session).publicKeyDer),
          ),
        "REQUEST_BINDING",
      );
      return command(
        "authenticate",
        hex(request.operation_id),
        base64(canonical(request)),
      );
    },
    signDocument: async (
      operationId: string,
      documentRequest: {
        protocol: string;
        requestId: string;
        [k: string]: unknown;
      },
    ): Promise<ConnectOperation> => {
      ensure(
        documentRequest?.protocol === CONNECT_PROTOCOL &&
          documentRequest.requestId === operationId,
        "REQUEST_BINDING",
      );
      return command(
        "signDocument",
        operationId,
        base64(utf8(JSON.stringify(documentRequest))),
      );
    },
    signAction: async (action: AppAction): Promise<ConnectOperation> => {
      validateAppAction(action);
      ensure(
        action.app_id === options.appId && action.origin === origin,
        "REQUEST_BINDING",
      );
      return command(
        "signAction",
        hex(action.operation_id),
        base64(canonical(action)),
      );
    },
    checkout: async (request: CheckoutRequest): Promise<ConnectOperation> => {
      validateShape("CheckoutRequest", request);
      ensure(request.offer.app_id === options.appId, "REQUEST_BINDING");
      return command(
        "checkout",
        hex(request.offer.operation_id),
        base64(canonical(request)),
      );
    },
    getOperation: (id: string) => command("getOperation", id),
    openOperation: (id: string) => command("openOperation", id),
    cancelOperation: (id: string) => command("cancelOperation", id),
    acknowledge: (id: string, resultDigest: string) =>
      command("acknowledge", id, null, resultDigest),
    /** Close the popup; operations already created are not cancelled. */
    close: () => {
      popup?.close();
      close();
    },
  };
}

export type DmsgConnection = ReturnType<typeof connectDmsg>;
