import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { secp256k1 } from "@noble/curves/secp256k1.js";
import { canonical, decodeCanonical, unhex } from "../src/cose-codec.ts";
import { verifyDocumentArtifact } from "../src/statements.ts";

// Rust's es256k_artifacts_are_low_s_and_reject_high_s_malleations test
// reconstructs and checks both artifacts from a fixed test key.
const fixture = JSON.parse(
  readFileSync(
    new URL(
      "../../../src/dmsg_protocol/tests/fixtures/es256k.json",
      import.meta.url,
    ),
    "utf8",
  ),
);

for (const compressed of [false, true]) {
  test(`ES256K matches Rust low-S rules with ${compressed ? "compressed" : "full"} coordinates`, () => {
    const key = decodeCanonical(unhex(fixture.cose_key)) as Map<number, unknown>;
    const x = key.get(-2) as Uint8Array;
    const y = key.get(-3) as Uint8Array;
    const publicKey = Uint8Array.from([4, ...x, ...y]);
    if (compressed) key.set(-3, (y[31]! & 1) === 1);
    const cose_key = canonical(key);
    const low = verifyDocumentArtifact({
      cose_key,
      cose_sign1: unhex(fixture.low_s),
    });
    assert.equal(low.checks.signature, "verified");

    const highBytes = unhex(fixture.high_s);
    const [protectedBytes, , payload, signature] = decodeCanonical(
      highBytes.subarray(1),
    ) as [Uint8Array, Map<unknown, unknown>, Uint8Array, Uint8Array];
    const toBeSigned = canonical([
      "Signature1",
      protectedBytes,
      new Uint8Array(),
      payload,
    ]);
    assert.deepEqual(toBeSigned, low.toBeSigned);
    // This is a mathematically valid n-s variant, not a corrupt signature.
    assert.equal(
      secp256k1.verify(signature, toBeSigned, publicKey, {
        prehash: true,
        lowS: false,
      }),
      true,
    );
    assert.throws(
      () => verifyDocumentArtifact({ cose_key, cose_sign1: highBytes }),
      { code: "INTEGRITY_FAILED" },
    );
  });
}
