/** Product-owned durable bridge session. Compile with @dmsg/sdk 0.2.0 and DOM libs. */
import { canonical, decodeCanonical, type CheckoutRequest } from "@dmsg/sdk";
import {
  connectDmsg,
  hex,
  DmsgBrowserError,
  type BrowserOperation,
} from "@dmsg/sdk/browser";
interface Saved {
  scope: string;
  extensionId: string;
  appId: string;
  key: CryptoKeyPair;
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
async function connection(value: Saved) {
  return connectDmsg({
    extensionId: value.extensionId,
    appId: value.appId,
    session: {
      publicKeyDer: new Uint8Array(
        await crypto.subtle.exportKey("spki", value.key.publicKey),
      ),
      sign: async (message) =>
        new Uint8Array(
          await crypto.subtle.sign(
            { name: "ECDSA", hash: "SHA-256" },
            value.key.privateKey,
            Uint8Array.from(message),
          ),
        ),
    },
  });
}
/** request must come from the authenticated product backend, with its actual approval. */
export async function checkout(
  scope: string,
  extensionId: string,
  request: CheckoutRequest,
): Promise<BrowserOperation> {
  let stored = await load(scope);
  if (!stored) {
    const key = await crypto.subtle.generateKey(
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["sign", "verify"],
    );
    stored = {
      scope,
      extensionId,
      appId: request.offer.app_id,
      key,
      requestCbor: canonical(request),
      state: "prepared",
    };
    await save(stored);
  }
  // Recover the saved terms even when the caller's backend now offers something else.
  const original = decodeCanonical(
    stored.requestCbor,
  ) as unknown as CheckoutRequest;
  const id = hex(original.offer.operation_id),
    client = await connection(stored);
  try {
    try {
      await client.getOperation(id);
      return await client.openOperation(id);
    } catch (error) {
      if (
        !(error instanceof DmsgBrowserError) ||
        !["NOT_FOUND", "FORBIDDEN"].includes(error.code)
      )
        throw error;
    }
    if (stored.state !== "prepared")
      throw new Error(
        "Outcome unknown: preserve this operation and reconnect the original account.",
      );
    stored.state = "sent";
    await save(stored);
    try {
      return await client.checkout(original);
    } catch (error) {
      if (
        error instanceof DmsgBrowserError &&
        ["LOCKED", "ACCOUNT_MISMATCH"].includes(error.code)
      ) {
        stored.state = "prepared";
        await save(stored);
      }
      throw error;
    }
  } finally {
    client.disconnect();
  }
}
export async function status(scope: string) {
  const stored = await load(scope);
  if (!stored) throw new Error("No saved operation");
  const request = decodeCanonical(
    stored.requestCbor,
  ) as unknown as CheckoutRequest;
  const client = await connection(stored);
  try {
    return await client.getOperation(hex(request.offer.operation_id));
  } finally {
    client.disconnect();
  }
}
// After notification, ask your authenticated backend for the contract and bounded
// entitlement. Only its durable Apply receipt can establish product delivery.
