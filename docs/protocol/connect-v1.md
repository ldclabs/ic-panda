# dmsg-connect/1: web application connection protocol

English | [简体中文](connect-v1_zh.md)

Drafted 2026-10-12, reviewed against public source 2026-10-24. Relay, device (`dmsg_protocol::connect`, `dmsg_client::connect`), bridge (`src/dmsg_connect`, asset canister dmsg_connect) and page SDK (`dmsg-sdk/connect`) are implemented. Production domains/artifacts/relay configuration require separate [release verification](../../src/dmsg_connect/README.md#release); this document does not establish the live version. It replaces retired dmsg-extension/4 (section 9). dMsg has no browser extension: applications use `https://connect.dmsg.net` to request approval on desktop, and later mobile, devices. [connect-v1 vectors](../../tests/vectors/client/connect-v1.json) define exact bytes.

Unchanged: three document profiles and action signing bytes, device execution approvals/certified receipts ([public protocol](README.md)), AuthenticationRequest, AppAction, CheckoutRequest, commerce contracts and AppRegistration in dmsg_commerce. Only page/device transport, origin verification and operation recovery change.

## 1. Roles and flow

| Role | Description | Holds |
| --- | --- | --- |
| Application page | Registered dmsg_commerce application at origin O | Non-exportable P-256 page-session key, stored by the application |
| Bridge | `https://connect.dmsg.net/v1/`, independent ICP asset canister containing bridge code only, no login/II | Non-exportable P-256 pairing key and non-exportable X25519 result-unsealing key per operation |
| Relay | dmsg-cloud API, called API_ORIGIN | Pairings, request mailbox, sealed replies; sees metadata/ciphertext only |
| Device | Account desktop/mobile device with FormalApprove | Device Ed25519 and X25519 HPKE keys; sole party able to approve/sign |

```mermaid
sequenceDiagram
  participant P as Application page (origin O)
  participant B as Bridge connect.dmsg.net
  participant R as Relay API_ORIGIN
  participant D as dMsg device
  P->>B: window.open，hello（postMessage）
  B->>B: Record event.origin = O; check registration
  B-->>P: ready { connectionId }
  P->>B: request { command }
  B-->>P: challenge { nonce, origin, connectionId }
  P->>B: proof signed by session key
  B->>B: Verify signature and payload bindings
  B->>R: Pairing-key-signed request sealed to devices
  B-->>P: result { state: awaiting_user }
  R-->>D: WS hint; alternatively user opens dMsg via dmsg://
  D->>R: Fetch request
  D->>D: Open; verify pairing, page signature and registration; ask user
  D->>R: Authorize by CAS, pin device, execute, return bridge-sealed result
  P->>B: getOperation
  B->>R: Read and open result
  B-->>P: result { state: signed, result }
  P->>B: acknowledge
```

## 2. Trust model

### 2.1 Origin provenance

Browsers supply MessageEvent.origin; pages cannot declare it themselves. The bridge accepts only window.opener messages and replies only to that origin using postMessage targetOrigin.

The device receives a pairing-key-signed origin claim by the bridge. It can verify the paired bridge's claim, not browser origin independently. The retired extension relayed Chrome sender information; the bridge now relays browser event.origin with the same trust-chain shape. Origin in device signatures/receipts is client-checked provenance, not cryptographic origin proof.

### 2.2 Consequences of compromise

| Compromised party | Can | Cannot |
| --- | --- | --- |
| Application or impersonating site | Request using its actual origin | Claim another origin; unregistered origins fail at both bridge and device |
| Bridge code/domain | Claim any origin in a paired browser, even while no application is in use because pairing keys stay available; read requests/results passing through it | Approve/sign/pay without device human confirmation, read the vault or act from an unpaired browser |
| Relay | Observe section 2.4 metadata, withhold/delay/drop messages and ignore page cancellation when arbitrating authorization races; device still requires approval | Read/alter sealed signed requests/results, forge device-signed pairing, approve for users or forge X25519-authenticated replies (6.4) |

The bridge is an additional trusted web surface. Deployment requires:

- Independent canister/subdomain with bridge code only, absent from all II alternative origins, so compromise cannot request II delegation.
- Headers: never `Cross-Origin-Opener-Policy: same-origin`, which severs the bridge's opener; use unsafe-none and no COEP. CSP is `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src <IC gateway> <API_ORIGIN>; frame-ancestors 'none'; base-uri 'none'; form-action 'none'`; `X-Frame-Options: DENY` and `Referrer-Policy: no-referrer`. Override conflicting asset-canister defaults.
- No third-party scripts or rendered user content; reproducible builds with published asset hashes.
- Approval identifies origin as browser-reported through connect.dmsg.net and shows pairing label/request time, allowing detection of unsolicited requests.
- Product documentation discloses this trust boundary.

### 2.3 Phishing-resistant pairing

A pairing-key holder can assert origins, so pairing must establish generation on real connect.dmsg.net. Only devices initiate: generate a one-time 32-byte secret and open `https://connect.dmsg.net/v1/pair#…` with it in the fragment, unseen by servers and readable only at the real origin. The bridge HMACs its new public key with that secret; the device checks before granting. Phishing sites lack the secret and cannot enroll their keys. Pages can only ask users to open dMsg, not initiate invitations.

### 2.4 Relay-visible metadata

The relay sees no origin, app, method or payload. It sees account, pairing ID/label (visible browser name), request ID/kind (create/read/open), recipient devices, size/time/coarse state and the authorizing/responding device. Hints in 7.5 also expose continued online presence of locked devices.

## 3. Encoding

- Page frames are structured-cloneable plain objects with closed field sets; extra/missing keys fail. IDs/nonces/digests use lowercase hex; keys/payloads/signatures canonical standard base64; large integers decimal strings. JSON-serialized frames are at most 150,000 bytes.
- Relay bodies/signing objects use RFC 8949 core deterministic CBOR, keys ordered length-first then bytewise, binaries as bstr. URL IDs use lowercase hex, AccountId canonical Xid text, time Unix milliseconds.
- `digest(domain, value) = SHA256(CBOR([1, domain, value]))`, as in other dMsg contracts.
- Environment uses dmsg_types::Environment Serde text: Local, Staging or Production.
- Origins follow dmsg_protocol::validate_origin: exact HTTPS origin, at most 256 bytes; only Local permits loopback HTTP.

## 4. Page and bridge

### 4.1 Opening the window

Applications open the bridge synchronously in a user gesture handler, such as click:

```js
const popup = window.open('https://connect.dmsg.net/v1/', 'dmsg-connect', 'popup,width=440,height=680')
```

- Applications cannot use COOP same-origin, which severs opener; use same-origin-allow-popups or omit it.
- Run as a top-level document. Bridges cannot determine whether opener is an iframe; applications should prevent embedding with frame-ancestors.
- Blocked popups return POPUP_BLOCKED. Closing one returns DISCONNECTED for unfinished calls, without cancelling created operations.
- **Browser baseline:** Chrome/Edge 133, Firefox 130, Safari 17 or later, based on WebCrypto non-exportable X25519 support. There is no pure-JS fallback; older browsers show unsupported and return UNSUPPORTED_BROWSER.
- **Safari storage protection:** storage may be removed after 7 days without first-party interaction. Bridges record the latest popup click; in Safari after 5 days, create methods require a Continue click inside the popup before proceeding.

### 4.2 Handshake

1. Every 100 ms the page sends `{protocol: "dmsg-connect/1", type: "hello", appId}` to the popup, targetOrigin fixed to https://connect.dmsg.net, until ready or 15-second timeout.
2. Bridge accepts the first hello with `event.source === window.opener`, valid section-3 origin and valid appId. Read certified registration from dmsg_commerce and require origin in origins, matching build environment and unpaused app.
3. Generate a 32-byte connectionId and reply `{protocol, type: "ready", connectionId, origin, appId, methods}` to that origin. Registration failure returns `{protocol, type: "rejected", error}` and stops processing.
4. Ignore other origins/sources once connected; check opener every second and end if closed/inaccessible. Navigation to another origin prevents delivery through targetOrigin.

One popup serves one connection at a time. Reuse it; a new hello ends the old connection.

### 4.3 Command frames and proofs

The command has exactly six mandatory keys:

```text
command = { method, appId, operationId, publicKey, payload, resultDigest }
```

- appId matches the handshake.
- operationId is nonzero 32-byte lowercase hex.
- publicKey is standard base64 of 91-byte P-256 SPKI DER.
- Create payloads are standard base64 decoding to at most 65,536 bytes; other methods use null.
- Only acknowledge has a 32-byte-hex resultDigest; others use null.

Frames:

```text
page → bridge   { protocol, type: "request", requestId, command }
bridge → page   { protocol, type: "challenge", requestId, nonce, origin, connectionId }
page → bridge   { protocol, type: "proof", requestId, signature }
bridge → page   { protocol, type: "result", requestId, ok: true, value }
                { protocol, type: "result", requestId, ok: false, error }
page → bridge   { protocol, type: "capabilities", requestId }   // value is { protocol, methods }
```

requestId is a random 32-byte hex value generated by the page for correlating this connection's frames only. Before signing, require challenge.origin == location.origin and connectionId equal to the handshake value, then sign:

```text
proof_message = digest("dmsg/connect-proof/v1", [
  "dmsg-connect/1", nonce_bytes, origin, connection_id_bytes, command
])
```

Encode command's JSON values as the section-3 canonical CBOR map of six text keys with text/null values, without base64/hex decoding. Sign the 32-byte message using ECDSA P-256/SHA-256, 64-byte IEEE P1363, sent as standard base64.

Each request has a 32-byte single-use nonce, expiring in 15 seconds or on disconnect. A connection permits 16 pending proofs. Every command except capabilities needs a new proof.

Operation commitment:

```text
operation_digest = digest("dmsg/connect-operation/v1", [
  origin, appId, operationId, method, public_key_der, payload_bytes
])
```

operationId enters the digest as text; public_key_der and payload_bytes are decoded bytes.

### 4.4 Methods

| Method | Payload | Bridge behavior | Device offline |
| --- | --- | --- | --- |
| authenticate | Canonical CBOR AuthenticationRequest | Create per 4.5 | Create normally as awaiting_user |
| signDocument | UTF-8 JSON SignatureRequest | Same | Same |
| signAction | Canonical CBOR AppAction | Same | Same |
| checkout | Canonical CBOR CheckoutRequest | Same | Same |
| getOperation | null | Read/open latest sealed response if local record exists, otherwise send read below | Existing local record unaffected; otherwise DEVICE_UNAVAILABLE |
| openOperation | null | Send open to redisplay for continuation/reconciliation and return state | Return current state; redisplay when device returns |
| cancelOperation | null | Change relay pending to cancelled | Unaffected |
| acknowledge | null; resultDigest required | Digest must match a result actually delivered by this bridge's local record, then acknowledge | Unaffected |

Non-create methods require the original session public key, checked against bridge records or by the device.

**read/open without a local record:** send one per paired account, then long-poll once with wait=20000. Open/return the first device-sealed response; unknown (7.4) means that account lacks the operation. No reply within 20 seconds returns DEVICE_UNAVAILABLE. Reuse read requests within their 2-minute lifetime in the same window; getOperation retries only poll again. Reads are not in IndexedDB, so reopening creates new ones. All accounts returning unknown yields NOT_FOUND.

### 4.5 Creation rules

Bridge checks before submission; devices repeat every check in 6.3:

1. **Capability:** registration permits Authenticate/SignDocument/SignAction/Checkout, and document profile is in profiles.
2. **Payload bindings:**
   - authenticate: validateAuthentication succeeds, including current app_config_version; origin/app_id/operation_id match the connection/command, session_key_hash is SHA-256(publicKey DER), and environment/expiry are valid.
   - signDocument: closed SignatureRequest with protocol dmsg-connect/1, method signature.request, requestId == operationId, plus accountId/nonce/expiresAt/statement. Statement uses text/digest/file-statement fields. issuedAt is seconds, expiresAt milliseconds, both checked decimal strings.
   - signAction: validateActionAdmission checks registered action_authority/action_schema; origin/app_id/operation_id match and expiry is valid.
   - checkout: `offer.app_id` and `offer.operation_id` match; `offer.accept_by_ms` is in the future, product is in `product_ids`, and method is in `offer.allowed_settlement_methods`.
3. **Target:** signDocument uses accountId, signAction signing_account, checkout approving_account. Browser needs an active pairing for it, otherwise NOT_PAIRED plus instructions to connect this browser in dMsg. authenticate specifies no account: select the only pairing, ask in popup if multiple, or NOT_PAIRED if none. No FormalApprove device yields NO_APPROVING_DEVICE.
4. **Idempotency:** look up (origin, appId, operationId) locally; same operation_digest returns existing operation without resubmission, otherwise IDEMPOTENCY_CONFLICT.
5. **Limits:** at most 20 unfinished operations per pairing and 5 per origin, otherwise QUOTA_EXCEEDED. Devices also impose aggregate limits across browsers (6.3).

On success, generate request_id and a non-exportable X25519 result key, persist the local record including private CryptoKey in IndexedDB, submit create (section 6), and return `{operationId, kind, state: "awaiting_user"}`.

Creation retries, including after pairing, use the same operationId and identical payload.

### 4.6 Operation values and states

Result value shape:

```text
{ operationId, kind: "authentication" | "document" | "action" | "checkout",
  state, result?, resultDigest?, error? }
```

| State | Meaning |
| --- | --- |
| awaiting_user | Submitted, not device-authorized; includes offline/undecided |
| authorized | Device authorized and started execution; no longer cancellable |
| execution_unknown | Device reconciles original execution ID without re-signing, or relay received no terminal state long after authorization (7.4 abandoned) |
| signed | Result available with result/resultDigest |
| returned | Page acknowledged the result digest |
| rejected | User or pre-display device refusal, with error |
| cancelled | Page cancelled before authorization |
| expired | Authorization deadline passed |
| failed | Definite execution failure with error, or pinned-device revocation with DEVICE_REVOKED |
| result_expired | Retention ended and result was deleted |

Authentication result is its dedicated certificate: signed means ready, not document signing or application-session creation. Documents/actions return COSE, execution ID and certified receipt. Checkout returns `{orderId, checkoutCbor}` progress: lowercase order/claim ID and standard-base64 deterministic CBOR of CheckoutView/PandaClaimView read after payment/application. Applications verify independently: authentication uses trusted IC root/home and saved challenge before consuming it; documents/actions verify artifact, receipt and business authority; checkout queries the application's authoritative contract, never treating bridge state as settlement/authorization proof. `resultDigest = SHA256(CBOR(result))`. Lost acknowledgement may reread the same result; acknowledgement authorizes no new signing/payment.

### 4.7 Error codes

When ok=false, error is one of these stable codes, also used by rejected/failed value.error:

| Code | Meaning |
| --- | --- |
| INVALID_INPUT | Invalid frame/command |
| UNSUPPORTED_PROTOCOL | Unsupported version/method |
| UNSUPPORTED_BROWSER | Below section 4.1 baseline |
| FORBIDDEN | Invalid proof/origin/source, disallowed capability/profile/payment method, failed action admission or policy purpose |
| APP_NOT_REGISTERED | Origin absent from registration |
| APP_PAUSED | New operations paused |
| POLICY_STALE | Payload app_config_version differs from registration |
| INTEGRITY_FAILED | Payload origin/appId/operationId/session-key mismatch or unopenable/mismatched response |
| ACCOUNT_MISMATCH | Payload account differs from pairing |
| NOT_PAIRED | No active browser pairing for account |
| NO_APPROVING_DEVICE | No FormalApprove device |
| LOCKED | sensitive_policy.frozen |
| IDEMPOTENCY_CONFLICT | Same operationId with changed content |
| NOT_FOUND | Operation absent or belongs to another session key |
| NOT_CANCELLABLE | Authorized or terminal operation |
| QUOTA_EXCEEDED | Proof/pending/relay/device limits exceeded |
| EXPIRED | Nonce/request expired |
| DEVICE_UNAVAILABLE | Device did not respond by deadline; retryable |
| DEVICE_REVOKED | Pinned execution device revoked |
| UNAVAILABLE | Temporary relay/IC failure; retryable |

SDK also emits POPUP_BLOCKED, BRIDGE_UNAVAILABLE for handshake timeout, DISCONNECTED and TRANSPORT_TIMEOUT for calls over 30 seconds.

## 5. Browser pairing

### 5.1 Material

- **Pairing key:** WebCrypto non-exportable P-256 ECDSA (`extractable: false`), stored as CryptoKey in connect.dmsg.net IndexedDB; pairing_pub is 91-byte SPKI DER.
- **Pairing ID:** `pairing_id = digest("dmsg/connect-pairing-id/v1", pairing_pub)`.
- **Label:** browser/system name such as Chrome · macOS, 1–64 UTF-8 bytes, shown on device.
- One browser can hold a pairing per account; each pairing belongs to one account only.

### 5.2 Ceremony

```mermaid
sequenceDiagram
  participant U as User
  participant D as Unlocked dMsg device
  participant R as Relay
  participant B as connect.dmsg.net in browser
  U->>D: Settings → Connect browser
  D->>D: Generate invite_id and 32-byte secret
  D->>R: Invite via dmsg/connect/invite/v1
  D->>B: Open https://connect.dmsg.net/v1/pair#<invitation> in system browser
  B->>B: Read/clear fragment; check environment; generate pairing key
  B->>R: Submit {pairing_pub, label, mac} with self-signed PoP
  D->>R: Read candidate
  D->>D: Check MAC with secret; show label for user confirmation
  D->>R: Grant via dmsg/connect/pairing/grant/v1
  B->>R: Query pairing and obtain grant
  B->>B: Verify grant against certified devices; save pairing
```

- **Invitation:** fragment is `base64url(CBOR {v: 1, environment, account_id, invite_id, secret})`. Environment must match the build, otherwise reject as another environment. Relay API_ORIGIN is build-pinned, never taken from the link. invite_id/secret are random 32 bytes each; invitations last 10 minutes and complete once.
- **Candidate MAC:** `mac = HMAC-SHA256(secret, digest("dmsg/connect-pair/v1", [environment, account_id, invite_id, pairing_pub, label]))`. Relay accepts at most 3 candidates per invitation and rate-limits IPs (7.6). Devices accept correct MACs only and require label confirmation.
- **Copy link:** devices may let users open the same link in another browser. Until expiry it conveys pairing ability, so device confirmation remains mandatory.
- **Web side:** unpaired bridge/apps may only ask to connect this browser in dMsg, optionally opening `dmsg://connect/pair?account=<xid>` (section 8); they cannot create invitations.
- **Storage loss:** private browsing or site-data clearing loses pairing and requires re-pairing. Safari protection is in 4.1.

### 5.3 Grant and verification

Devices issue dmsg/connect/pairing/grant/v1 commands, using the [cloud envelope](cloud.md), with payload:

```text
{ invite_id, pairing_id, pairing_pub, label, home, granted_at, expires_at }
```

The grant is the command's COSE_Sign1 itself, retained/returned unchanged by the relay. home is the account's user-home Principal as CBOR bstr; pairing_pub is bstr. `expires_at - granted_at` is at most 180 days. The issuing device must remain unrevoked with FormalApprove, which the relay also checks for this command.

Pairing is valid only with a valid grant signature, an issuing device still present/unrevoked/FormalApprove in the current certified set, no expiry/revocation, home in registry user_homes and certified home state confirming ownership of account_id. Bridge/device verify independently rather than trusting relay status. Issuer-device revocation invalidates all its pairings and relay reads return 410.

Pairings do not renew. After expiry, report NOT_PAIRED and repeat section 5.2.

Any FormalApprove device may issue `dmsg/connect/pairing/revoke/v1 {pairing_id}`. Device settings list label, issuer device, creation and last-use time.

## 6. Requests and replies

### 6.1 ConnectRequest

The bridge constructs and pairing-key-signs tagged COSE_Sign1. Protected headers are exactly canonical CBOR `{1: -7, 3: "application/cbor", 4: pairing_id, 16: "application/vnd.dmsg.connect-request+cose;v=1"}`, with empty unprotected headers/external AAD and 64-byte r‖s signature.

```text
ConnectRequest = {
  protocol: "dmsg-connect/1",
  environment: tstr,             ; section 3
  account_id: bstr .size 12,
  pairing_id: bstr .size 32,
  request_id: bstr .size 32,
  kind: "create" / "read" / "open",
  origin: tstr,                  ; origin checked by the bridge
  connection_id: bstr .size 32,
  nonce: bstr .size 32,
  command: { method, appId, operationId, publicKey, payload, resultDigest },
  page_signature: bstr .size 64,
  result_key: bstr .size 32,     ; X25519 public key for sealing device replies
  created_at: uint,
  expires_at: uint
}
```

Create expires_at uses payload expiry (authentication/action expires_at_ms, signature deadline or checkout accept_by_ms), at most 24 hours after created_at. Read/open last 2 minutes. Outer relay kind/expires_at must match.

### 6.2 Sealing

Only account devices can read requests. Bridge reads certified devices from the home as in [account evidence](cloud.md), selecting every unrevoked FormalApprove device:

```text
cek      = 32 random bytes
envelope = {
  v: 1,
  recipients: [{ device_id: bstr .size 32, enc: bstr .size 32, wrapped: bstr }],
  nonce: bstr .size 12,
  ciphertext: bstr        ; AES-256-GCM(cek, nonce, COSE_Sign1(ConnectRequest), aad)
}
aad      = CBOR(["dmsg/connect-request/v1", account_id, pairing_id, request_id])
wrapped  = HPKE Base (X25519 / HKDF-SHA256 / AES-256-GCM) sealing cek,
           info = aad = CBOR(["dmsg/connect-wrap/v1", account_id, pairing_id, request_id, device_id])
```

recipients[].device_id is relay-visible for unknown-completion checks in 7.4; at most 16 recipient devices per request.

### 6.3 Device processing

Locked devices fetch/store original sealed envelopes without opening. After unlock:

1. Unwrap cek with the device HPKE key, decrypt and parse COSE.
2. Obtain/verify grant under 5.3 and request signature under pairing_pub. Match account_id/pairing_id/request_id/kind/expires_at to relay metadata, environment to local environment, created_at at most 60 seconds ahead of local time (as with HTTP PoP), and unexpired request.
3. Verify page_signature with command.publicKey over proof_message(nonce, origin, connection_id, command).
4. Repeat all 4.5 checks using fresh certified registration (60 seconds). Mismatched app_config_version returns POLICY_STALE.
5. Check sensitive policy, also enforced by canisters: frozen returns LOCKED; signDocument/signAction KeyPurpose outside allowed_purposes returns FORBIDDEN. Canisters enforce daily_executions at execution, surfaced as failed if exceeded.
6. Look up (origin, appId, operationId). Different digest replies rejected/IDEMPOTENCY_CONFLICT. Same digest creates no new approval: register an additional delivery channel and immediately return current state, `respond.state = "open"` if nonterminal. Every later state change writes a reply to each still-open request, delivering results to every submitting bridge. Additional channels do not call authorize.
7. At most 20 unfinished local approvals and 5 per origin.

Any failure replies rejected with error and `respond.state = "closed"` without prompting. Otherwise enqueue approval without relay viewing-status registration. Show origin, app name/version, pairing label, account, purpose, expiry, request time and inspectable original payload. Document requests check app version/profile; authentication recomputes the saved payload commitment. Disclose that origin was reported by the browser through connect.dmsg.net.

After approval but before authorize, signAction preflights product authority through user-home inspect_app_action; failure replies rejected without consuming device sequence. Checkout needs a quote before approval: a person chooses cash asset or PANDA neurons, the device obtains/verifies commerce/membership terms, and the approval digest commits them. Unquoted checkout cannot be approved. Afterwards the device wallet pays cash or submits PANDA application; results follow 4.6.

After user approval, journal approval/request encrypted before authorize. Relay accepts only pending and pins that device; only it may thereafter reply/execute/reconcile. Other devices observing authorized with another pin mark handled elsewhere and hide it. Failed authorization due to cancel/expiry/another device abandons the journal without execution. Persist parameters/signatures before submission; reconcile unknowns by original execution ID without re-signing. Expired results never automatically create new approval.

User refusal directly replies rejected.

Read/open: a device with the operation and no different pinned owner returns a current snapshot; open also redisplays it. Devices without a record or pinned elsewhere reply `respond.state = "unknown"` without ciphertext. After authorization only the pinned device has authoritative state; others must not answer with stale snapshots.

### 6.4 Replies

Each state change writes a new sealed-response version; the relay keeps only the highest:

```text
ConnectResponse = {
  protocol: "dmsg-connect/1",
  request_id: bstr .size 32,
  version: uint,                  ; strictly increasing from 1 per request
  operation_digest: bstr .size 32,
  value: bstr                     ; UTF-8 JSON value from 4.6
}
sealed = CBOR { enc: bstr .size 32, ciphertext: bstr }
         HPKE Auth (X25519 / HKDF-SHA256 / AES-256-GCM) seals CBOR(ConnectResponse):
         skS = responding device X25519 private key (certified hpke_pub), pkR = result_key,
         info = aad = CBOR(["dmsg/connect-response/v1", account_id, pairing_id, request_id, version, device_id])
```

Bridge reads device_id/sealed/sealed_version, finds the unrevoked device's hpke_pub in fresh certified state, opens with Auth at sealed_version, then matches request_id, version equal to sealed_version and greater than previously seen, and operation_digest. Only then deliver value; any failure discards with INTEGRITY_FAILED. After a create reply reaches authorized or later, pin that device_id and accept only its replies. Auth authenticity does not depend solely on result-key secrecy: relay/others lacking the device HPKE key cannot forge replies.

Read replies are one-time current snapshots. Open redisplays and returns current state.

### 6.5 Cancellation and acknowledgement

- **Cancel:** relay cancel changes pending to cancelled, making later authorize fail. Authorized/ended requests return NOT_CANCELLABLE. Cancelling an additional channel (6.3 step 6) affects only that channel, not the operation.
- **Acknowledge:** bridge submits ack {result_digest}; relay records it. Device sync checks against its produced result before marking returned. Once relay records matching acknowledgement, bridge reports returned.

## 7. Relay interfaces

### 7.1 Bridge authentication

Each bridge request carries X-Dmsg-Connect-Pop: unpadded-base64url tagged COSE_Sign1, headers as 6.1 except typ `application/vnd.dmsg.connect-pop+cose;v=1`. Payload is canonical CBOR:

```text
{ protocol: "dmsg-connect/1", account_id: bstr .size 12, pairing_id: bstr .size 32,
  request_id: bstr .size 32, deadline: uint, audience: tstr, method: tstr, target: tstr,
  body_digest: bstr .size 32 }
```

account_id chooses the account mailbox, since bridge URLs omit it. Other rules match [device HTTP PoP](cloud.md): API-origin audience, canonical path/query target, actual-body SHA-256, and `now < deadline ≤ now + 60000`. HTTP request_id is random per request and relay rejects replay until expiry. Pairing-candidate PoP verifies under body pairing_pub because pairing is not active yet.

Bodies are canonical CBOR with `Content-Type: application/cbor` and bstr binaries. Replies are JSON `{ok: true, data}` or `{ok: false, error: {code, message, retryable}}`, with lowercase-hex IDs/digests and unpadded-base64url sealed/grant/envelopes. Revoked/expired/ineligible-issuer pairings return 410 NOT_PAIRED; noncancellable requests return 409 NOT_CANCELLABLE.

CORS for /v1/connect/* permits only `Origin: https://connect.dmsg.net`, separately configured for Local. Preflight OPTIONS returns `Access-Control-Allow-Headers: Content-Type, X-Dmsg-Connect-Pop`, `Access-Control-Allow-Methods: GET, POST` and `Access-Control-Max-Age: 600`. CORS constrains browsers only; PoP provides authority.

### 7.2 Bridge routes

| Route | Body and response | Notes |
| --- | --- | --- |
| POST /v1/connect/pairings | `{account_id, invite_id, pairing_pub, label, mac}` | Self-signed candidate PoP; ≤3 per invitation |
| GET /v1/connect/pairings/:pairing | `{state, account_id, grant?}` | pending/active/revoked/expired; active returns original grant COSE |
| POST /v1/connect/requests | `{request_id, kind, expires_at, envelope}` → `{seq}` | Submit sealed request; retain envelope.recipients[].device_id |
| GET /v1/connect/requests/:request?wait=&after= | `{state, version, device_id, sealed, sealed_version, acked_digest}` | Only creating pairing reads; state follows 7.4 or result_expired after retention. Version counts all replies including unknown; sealed is latest sealed reply with its own sealed_version and writing device_id. Long-poll wait ≤20000 ms returns on version > after or state change |
| POST /v1/connect/requests/:request/cancel | — | Section 6.5 |
| POST /v1/connect/requests/:request/ack | `{result_digest}` | Section 6.5 |

### 7.3 Device routes

Routes use device HTTP PoP and device commands for writes, with bstr binary payloads. Any unrevoked account device may read. Grant/revoke/authorize/respond require FormalApprove, checked per action in the command pipeline.

| Route | Device command action | Notes |
| --- | --- | --- |
| POST /v1/accounts/:account/connect/invites | dmsg/connect/invite/v1 | `{invite_id, expires_at}`, ≤10 minutes, ≤4 concurrent/account |
| GET /v1/accounts/:account/connect/invites/:invite | — | Candidates `{pairing_id, pairing_pub, label, mac, submitted_at}` |
| POST /v1/accounts/:account/connect/pairings | dmsg/connect/pairing/grant/v1 | Grant from 5.3; key/label match candidate; invitation consumed |
| `POST /v1/accounts/:account/connect/pairings/:pairing/revoke` | `dmsg/connect/pairing/revoke/v1` | `{pairing_id}` |
| GET /v1/accounts/:account/connect/pairings | — | Pairings, grants, issuer devices and last-use time |
| GET /v1/accounts/:account/connect/requests?after=&limit= | — | Mailbox envelope/state/version/pinned device/ack digest. Each change advances seq; after=last greatest seq reveals state changes |
| GET /v1/accounts/:account/connect/requests/:request | — | Single request, same fields |
| POST /v1/accounts/:account/connect/requests/:request/authorize | dmsg/connect/authorize/v1 | `{request_id, operation_digest}` CAS per 6.3; EXPIRED if expired, 409 VERSION_CONFLICT if authorized/cancelled/ended. Same device/digest replay returns original result |
| POST /v1/accounts/:account/connect/requests/:request/respond | dmsg/connect/respond/v1 | `{request_id, version, state, sealed_digest}`, state open/closed/unknown. Body sealed beside cose_sign1 is ≤512 KiB; command commits SHA-256 only. Unknown has no reply and null sealed_digest |

Respond requires version=current+1, otherwise 409 VERSION_CONFLICT and reread/retry. Authorized requests accept only the pinned device; pending/read/open accept any recipient. Unknown is only for read/open.

### 7.4 Relay states

```text
create:
  pending ──authorize（CAS）──▶ authorized ──respond(closed)──▶ closed
     │                            ├──pinned device revoked──▶ device_revoked
     │                            └──no terminal result 7 days after expires_at──▶ abandoned
     ├──respond(closed)──▶ closed          (pre-display rejection or duplicate-create terminal snapshot)
     ├──cancel──▶ cancelled
     └──expiry before authorization──▶ expired

read / open:
  pending ──respond(closed)──▶ closed
     ├──all recipients respond(unknown)──▶ unknown
     └──expiry──▶ expired
```

- respond(open) advances version/replaces sealed without changing relay state; used for authorized/execution_unknown intermediates and additional-channel snapshots.
- Authorized requests do not expire and cannot be cancelled.
- Read/open need no authorization.
- Relay understands only these states; fine page states stay sealed. Terminal relay states without a reply map as follows: cancelled→cancelled, expired→expired, device_revoked→failed + DEVICE_REVOKED, abandoned→execution_unknown, unknown→getOperation NOT_FOUND.

### 7.5 Device hint channel

A locked device without a retained signing key cannot open PoP connections. Hints still tell it that work is waiting:

- While holding a signing key (unlocked or background-online), generate a random 32-byte token and register SHA-256 using `dmsg/hint/token/create/v1 {token_hash, expires_at}` at POST /v1/accounts/:account/hint-tokens, valid ≤30 days. Store token in OS credentials, or plaintext workspace metadata if unavailable because hints carry no content. Rotate with less than 7 days left; ≤2 active tokens/device permit overlap. `dmsg/hint/token/revoke/v1 {token_hash}` at POST /v1/accounts/:account/hint-tokens/revoke revokes it; device revocation invalidates all tokens.
- Upgrade GET /v1/hints/ws?account=<xid> with `Authorization: Bearer <base64url token>`. Server sends `{type: "hint", account, kind}`; channel hints add resource=channel ID and event=message/control. Devices ping at least every 30 seconds; relay pong does not wake sleeping connections; reconnect after 60 seconds without messages. Kinds: connect, mcp-event for [MCP Events delivery](cloud.md), channel for messages (≤1 per 5 seconds/channel) or every control/member/epoch/rotation change, and inbox when a free letter is admitted or a paid letter funded and visible. No request ID, sender, origin or content; channel IDs name existing memberships. Connect hints target FormalApprove-device tokens only. Unlocked devices fetch named channels, rereading shortly after coalesced message hints. Locked devices notify only message hints, never control hints. No per-channel WebSocket exists.
- Hints only wake checks; reading still requires PoP. Mobile APNs/FCM carrying the same data is deferred to the mobile phase.

### 7.6 Limits and retention

| Item | Limit |
| --- | --- |
| Unfinished invitations/account | 4 |
| Pairing candidates | 3/invitation; 10/source IP/minute |
| Active pairings/account | 32 |
| Unfinished requests/pairing | 20 |
| Hint tokens/device | 2 |
| Request envelope | 192 KiB |
| Sealed reply | 512 KiB |
| Pending create | ≤24 hours, bounded by payload expiry |
| Read/open | 2 minutes |
| Authorized without terminal result | 7 days after expires_at, then abandoned |
| Ended requests/replies | 7 days, then result_expired |
| Bridge submissions | 60/pairing/minute |

## 8. Opening the device

Desktop registers the dmsg: scheme:

| URL | Effect |
| --- | --- |
| dmsg://connect/pair?account=<xid> | Open Connect browser; omit account when unknown, as for authenticate |
| dmsg://connect/request?account=<xid>&request=<hex> | Open approval center at the request |

Any site can trigger these links, so they only navigate. Devices execute nothing from the link and still read/verify via PoP. After submission, the bridge offers Confirm in dMsg using the second link and notes that users can also use the approval center or dmsg approvals. Platforms without the scheme, such as CLI-only Linux, cannot use the button. Mobile universal/app links under https://connect.dmsg.net/app/… are deferred.

## 9. Differences from dmsg-extension/4

dmsg-extension/4 retired with the extension; its deleted contract remains in Git history. Migration comparison:

| Area | dmsg-extension/4 | dmsg-connect/1 |
| --- | --- | --- |
| Transport | chrome.runtime.connect port | Popup/postMessage, bridge→relay→device |
| Origin | Chrome sender: origin/documentId/tab/frame | Bridge-signed MessageEvent.origin; no documentId |
| Proof | dmsg/browser-proof/v1 with documentId | dmsg/connect-proof/v1 with connectionId |
| Operation digest | dmsg/browser-operation/v1 | dmsg/connect-operation/v1, unchanged fields |
| Approval | Separate extension window | Desktop and later mobile approval center |
| Records | Extension local storage | Device authoritative, bridge local index, relay sealed replies |
| Browsers | Chromium only | Chrome/Edge, Firefox and Safari meeting 4.1 |
| `SignatureRequest.protocol` | `dmsg-extension/4` | `dmsg-connect/1` |
| Added | — | Pairing; NOT_PAIRED, NO_APPROVING_DEVICE, DEVICE_UNAVAILABLE, DEVICE_REVOKED, NOT_CANCELLABLE, POLICY_STALE, LOCKED, UNSUPPORTED_BROWSER; value.error |
| Removed | — | requiresUnlock; locked devices appear as awaiting_user |

## 10. SDK

dmsg-sdk/connect replaces removed dmsg-sdk/browser:

```ts
import { connectDmsg } from 'dmsg-sdk/connect'

button.onclick = async () => {
  // Call synchronously in a user gesture: window.open runs immediately.
  const dmsg = connectDmsg({ appId, session: { publicKeyDer, sign } })
  await dmsg.ready
  const operation = await dmsg.signDocument(operationId, request)
}
```

Methods: capabilities, authenticate, signDocument, signAction, checkout, getOperation, openOperation, cancelOperation and acknowledge; plus ready, close(), read-only open (popup still open) and methods advertised in ready. Session may be a Promise because popups must open synchronously while IndexedDB key loading is asynchronous. After user closure, call connectDmsg again in a new gesture. bridgeOrigin override is only for Local/Staging. Persist session keys, challenges and operationId, and recover after refresh with the same key.

## 11. Reserved capabilities

Outside /1:

- requestDelegation/registerController, the phase-3 Agent Protocols browser entry points.
- Application reads under content grants through cloud proxy-file/proxy-read.
- Redirect mode using registered callback URIs when embedded browsers block popups.
- Mobile universal links and push.
- Pairing renewal.

New methods may stay in /1 and be advertised through capabilities if existing frames/digests/state semantics stay unchanged. Changes to existing bytes require a protocol-version upgrade.

## 12. Verification requirements

- **Vectors:** tests/vectors/client/connect-v1.json covers proof_message, operation_digest, pairing_id, invitations/MAC, ConnectRequest CBOR/COSE, envelope AAD/HPKE info, ConnectResponse/Auth sealing and Connect PoP. Rust generates them; bridge, SDK and relay independently compare bytes.
- **Negative cases:** wrong hello source/origin, replies after opener navigation, nonce replay/expiry, altered command/payload/connectionId, unregistered/paused/disallowed capabilities, stale app_config_version, payment/action admission refusals, frozen/disallowed account purposes, wrong invitation/request environment, bad pairing MAC, replaced pairing_pub/grant, revoked issuer, replaced/replayed envelopes/replies, unpinned/uncertified responder, cancel/authorize races, concurrent device authorization, post-authorization crash recovery, pinned-device revocation, record-loss reads with found/missing/all-unknown devices, duplicate-create delivery to every additional channel, wrong acknowledgement digest and all limits.
- **E2E:** real Chromium/Firefox/WebKit app and bridge pages, daemon, local workerd relay and PocketIC, covering pairing, four create methods, offline creation, cross-device approval, cancellation, acknowledgement and result expiry.
