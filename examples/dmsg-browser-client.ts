/** Product-owned durable dMsg session (dmsg-connect/1). Compile with dmsg-sdk 0.4.0 and DOM libs. */
import { canonical, decodeCanonical, type CheckoutRequest } from "dmsg-sdk";
import {
  connectDmsg,
  hex,
  DmsgError,
  type ConnectOperation,
  type ConnectSession,
} from "dmsg-sdk/connect";
interface Saved {
  scope: string;
  appId: string;
  key: CryptoKeyPair;
  publicKeyDer: Uint8Array;
  requestCbor: Uint8Array;
  state: "prepared" | "sent";
}
async function database() {
  return new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open("my-product-dmsg", 1);
    request.onupgradeneeded = () =>
      request.result.createObjectStore("operations", { keyPath: "scope" });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
async function load(scope: string): Promise<Saved | undefined> {
  const db = await database();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("operations"),
      request = tx.objectStore("operations").get(scope);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
    tx.oncomplete = () => db.close();
  });
}
async function save(value: Saved) {
  const db = await database();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction("operations", "readwrite");
    tx.objectStore("operations").put(value);
    tx.oncomplete = () => {
      db.close();
      resolve();
    };
    tx.onabort = tx.onerror = () => {
      db.close();
      reject(tx.error);
    };
  });
}
/** The saved operation of `scope`, or a new one with a non-extractable key. */
async function prepare(scope: string, request: CheckoutRequest) {
  const stored = await load(scope);
  if (stored) return stored;
  const key = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign", "verify"],
  );
  const value: Saved = {
    scope,
    appId: request.offer.app_id,
    key,
    publicKeyDer: new Uint8Array(
      await crypto.subtle.exportKey("spki", key.publicKey),
    ),
    requestCbor: canonical(request),
    state: "prepared",
  };
  await save(value);
  return value;
}
const session = (value: Saved): ConnectSession => ({
  publicKeyDer: value.publicKeyDer,
  sign: async (message) =>
    new Uint8Array(
      await crypto.subtle.sign(
        { name: "ECDSA", hash: "SHA-256" },
        value.key.privateKey,
        Uint8Array.from(message),
      ),
    ),
});
/**
 * Call from a click handler: the bridge popup opens before anything is
 * awaited. `request` must come from the authenticated product backend,
 * with its actual approval.
 */
export function checkout(
  scope: string,
  request: CheckoutRequest,
): Promise<ConnectOperation> {
  const stored = prepare(scope, request);
  const client = connectDmsg({
    appId: request.offer.app_id,
    session: stored.then(session),
  });
  return (async () => {
    const saved = await stored;
    // Recover the saved terms even when the caller's backend now offers something else.
    const original = decodeCanonical(
      saved.requestCbor,
    ) as unknown as CheckoutRequest;
    const id = hex(original.offer.operation_id);
    try {
      await client.getOperation(id);
      return await client.openOperation(id);
    } catch (error) {
      if (
        !(error instanceof DmsgError) ||
        !["NOT_FOUND", "DEVICE_UNAVAILABLE"].includes(error.code)
      )
        throw error;
    }
    if (saved.state !== "prepared")
      throw new Error(
        "Outcome unknown: preserve this operation and reconnect the original account.",
      );
    saved.state = "sent";
    await save(saved);
    try {
      return await client.checkout(original);
    } catch (error) {
      // Nothing was created: the person may pair this browser and try again.
      if (
        error instanceof DmsgError &&
        ["NOT_PAIRED", "ACCOUNT_MISMATCH", "POPUP_BLOCKED"].includes(error.code)
      ) {
        saved.state = "prepared";
        await save(saved);
      }
      throw error;
    }
  })();
}
/** Call from a click handler, like `checkout`. */
export function status(
  scope: string,
  appId: string,
): Promise<ConnectOperation> {
  const stored = load(scope).then((value) => {
    if (!value) throw new Error("No saved operation");
    return value;
  });
  const client = connectDmsg({ appId, session: stored.then(session) });
  return stored.then((saved) => {
    const request = decodeCanonical(
      saved.requestCbor,
    ) as unknown as CheckoutRequest;
    return client.getOperation(hex(request.offer.operation_id));
  });
}
// After notification, ask your authenticated backend for the contract and bounded
// entitlement. Only its durable Apply receipt can establish product delivery.
