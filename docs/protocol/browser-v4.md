# dMsg browser bridge v4

The installed MV3 extension accepts only `dmsg-extension/4` ports. It checks the
actual Chrome sender against the build allowlist, requires the active top-level
document, and independently checks the commerce app registration before accepting
a new operation. The browser JSON envelope is not Rust Serde JSON or a COSE file.

## Implemented capabilities

`capabilities` returns `authenticate`, `signDocument`, `signAction`, `checkout`,
`getOperation`, `openOperation`, `cancelOperation` and `acknowledge`. Capability
admission still requires the registered app and its exact origin/profile.
The three document profile v1 byte formats are unchanged.

Actions and checkout explicitly bind the expected signing/approving account.
A different open workspace is refused before enqueueing. `checkout` carries a
canonical `CheckoutRequest`; the extension independently verifies product and
asset configurations and obtains an authoritative cash or PANDA quote before
showing the full terms. Wallet identity is connected separately from the dMsg
account. A new device approval needs FormalApprove; formal signatures also need
confirmed recovery material. No private signing key is exported.

Authentication chooses the current registered dMsg workspace account. The page
does not obtain that identity until the user approves the exact request and the
product verifies the dedicated certificate. Formal document requests explicitly
name their expected account and issuer, and use the registered document profiles.

## Product session and exact frames

The product supplies a nonextractable P-256 session key, its 91-byte SPKI DER,
and a signing callback. TokenList uses its existing temporary account challenge
key. The product persists that key and challenge before opening dMsg so a new
document can recover the same operation. Neither dMsg device keys nor a dMsg
wallet/IC delegation leave the extension.

Each RPC has a random 32-byte lowercase-hex `requestId`. This correlation ID is
distinct from the stable application `operationId`. Request frames contain:

```text
{ protocol: "dmsg-extension/4", requestId, type: "request", command }

command = {
  method, appId, operationId, publicKey, payload, resultDigest
}
```

The six command keys are mandatory and closed. `publicKey` is canonical standard
base64 SPKI. `operationId` is nonzero lowercase hex, exactly 32 bytes. For creation,
`payload` is canonical base64 and `resultDigest` is null. For read/open/cancel both
are null; for ACK only `resultDigest` is a 32-byte lowercase-hex digest.

- `authenticate`: payload is the canonical CBOR `AuthenticationRequest`; its app,
  origin, operation ID and session-key SHA-256 must exactly match the verified
  browser envelope. Being another allowed origin of the same app is insufficient.
- `signDocument`: payload is UTF-8 JSON for the closed `SignatureRequest` with
  `protocol = dmsg-extension/4`, `method = signature.request`, `requestId`,
  `accountId`, `nonce`, `expiresAt` and `statement`. The statement uses the existing
  text, digest or file-statement fields. Signed `issuedAt` is seconds; execution
  `expiresAt` is milliseconds. Both use checked decimal strings.

The extension returns a challenge with the same correlation ID, a random
32-byte hex nonce, actual origin and actual document ID. The page signs:

```text
SHA256(CBOR([1, "dmsg/browser-proof/v1", [
  "dmsg-extension/4", nonce_bytes, actual_origin, actual_document_id, command
]]))
```

This 32-byte message is signed using ECDSA P-256/SHA-256. The signature is the
64-byte IEEE P1363 form, sent as canonical base64 in a `type = proof` response.
The nonce is per connection, consumed once, expires after 15 seconds and is
discarded on disconnect. The SDK RPC deadline is 30 seconds; at most 16 pending
challenges are allowed per connection. Chrome reports a port disconnect only to
the other end, so the SDK's own `disconnect()` also rejects its pending calls
with `DISCONNECTED`. Payload bytes are bounded to 65,536.

Capabilities have no private data and need no session proof. All operation
reads, reopenings, cancellations and ACKs require a fresh proof. The actual
origin/document comes from Chrome, not a page-supplied field.

## Durable operations and approval

The operation commitment is:

```text
SHA256(CBOR([1, "dmsg/browser-operation/v1", [
  actual_origin, appId, operationId, method, public_key_der, payload_bytes
]]))
```

The extension stores it with the app, account, original public key and encrypted
payload. It checks the original account again during recovery. A new document
may update the document binding only after proving possession of that key and
matching the original app and origin. Reusing an ID with changed content fails.

The independent confirmation window displays the actual origin, application,
account, purpose and expiry. It fetches certified registration/account state.
Approval parameters and device signature are durably stored before dispatch.
Unknown results use the same request and signature; an expired result never
creates a fresh approval automatically. Document signing rechecks the app version
and profile before approval; authentication also recomputes the saved payload
commitment before signing.

Disconnect or service-worker restart does not cancel the operation. Cancellation
is allowed only while awaiting user approval. A cancelled record cannot become
authorized or unknown through a late callback. Unknown execution records are
excluded from completed-history eviction. Refreshing an acknowledged result does
not turn it back into an unacknowledged result.

`openOperation` reopens an existing record without creating an approval. This
supports unlocking/reconciliation even when new application admission is paused.
Private result delivery still needs the original product key, same account and
an unlocked dMsg page; IDs alone are insufficient.

## Results and verification

Replies distinguish transport errors from operation state:

```text
{ protocol, requestId, type: "result", ok: true, value: BrowserOperation }
{ protocol, requestId, type: "result", ok: false, error: stable_code }
```

Operations report awaiting_user, authorized, execution_unknown, signed, returned,
rejected, cancelled, expired, failed or result_expired. For authentication,
`signed` means the dedicated certificate is ready; it is not a document signature
or proof that the product already created a session. The product independently
verifies the certificate against its trusted IC root/home and stored challenge,
then consumes its own challenge. Document consumers independently verify the
artifact, execution receipt and their own business permissions.

An unlocked page may be needed to read a result (`requiresUnlock`). ACK requires
the digest of a result actually delivered on that connection. ACK loss permits
reading the same result again; it never authorizes another signature or payment.

## Reference integration

Use `connectDmsg` from `@dmsg/sdk/browser` with `extensionId`, `appId` and
`session: { publicKeyDer, sign }`. Use `@dmsg/sdk/ic` for certificate verification.
The product must persist the key, immutable challenge and stable operation ID.
On refresh, use `getOperation`/`openOperation` with the original key. A missing
operation can be created with the same original request; a failed transport must
not cause a new request ID or new approval.

TokenList's `dmsg-login.ts` demonstrates registration, login, linking and step-up
without replacing its local account/session/role model. Its canister verifier
also checks the proof independently; browser validation is not its trust root.

Real browser tests use an isolated extension build, a temporary product origin
and a temporary Chrome profile. Real-Wasm authentication handoff is tested
separately in TokenList's `tests/dmsg-authentication`. Production identity origins,
and the combined browser/product/canister matrix, remain explicit release work.

## Action and checkout recovery

`signAction` uses a separate purpose and `sign_app_action` operation. It retains
the original Candid approval/journal and exports the original COSE, full key
descriptor, execution ID and IC certificate. A fresh certificate can be read for
an existing completed execution without signing again. Signed-but-unsubmitted
results remain exportable; signing alone does not prove product delivery.

Checkout persists its complete quote and original device approval before dispatch,
and the wallet persists its exact ledger transfer before payment. Read the service
operation before replaying an old approval: its short approval may have expired
while the accepted order remains recoverable. The post-cooling PANDA approval
uses the same original application and retains earlier approval history. An
Applying/Active result is reconciled rather than silently starting another claim.
The locally constructed approval deadline leaves a minute of margin within the
service's five-minute maximum to accommodate its bounded certificate-time skew.

Private result retrieval still requires the same product P-256 key, app, actual
origin and dMsg workspace account. `checkoutCbor` is a progress notification;
products query their own authoritative contract after it, rather than trusting
browser state as settlement evidence. Cancellation of a browser request works
only before authorization. Accepted cash/PANDA operations use their service state
machines; an applied PANDA commitment has no early-exit control.
