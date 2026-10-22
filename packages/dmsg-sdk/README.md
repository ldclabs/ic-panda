# dmsg-sdk 0.4.0

A standalone ESM client for dMsg authentication, document/action signing and
subscription checkout. The package imports no Svelte, client database,
private relay or wallet store. It ships JavaScript and TypeScript declarations.

## Contracts

| Surface | Version | Responsibility |
| --- | --- | --- |
| Web connection | `dmsg-connect/1` | The bridge page (connect.dmsg.net) relays the page's browser-reported origin and P-256 key possession to the user's dMsg devices; durable operation recovery |
| Authentication and application approval | 1 | Challenge/account/device/purpose binding |
| Application action | `dmsg-app-action/1` | Commands of the app's registered schema, original files, intended receiver and actor |
| Subscription commerce | 2 | Authoritative USD bill, one cash asset or full PANDA waiver, original Apply receipt |
| Document statements | existing three v1 profiles | Their original signing bytes remain unchanged |

All wire integers, including versions, are `bigint`. Principals, hashes and
account IDs are `Uint8Array`. `canonical` rejects Number, floats, undefined,
invalid Unicode and unsupported objects. `validateShape` rejects extra fields,
unknown variants, unbounded arrays and out-of-range integers. Every check throws
`DmsgError` with the Rust protocol code (`INVALID_INPUT`, `FORBIDDEN`,
`EXPIRED`, `INTEGRITY_FAILED`, ...); digests and commitments are synchronous,
and browser client methods report failures as rejected promises. Generated CDDL and
TypeScript shapes come from the Rust contracts; regenerate with
`python3 scripts/generate-integration-sdk.py`.

```ts
import { connectDmsg } from 'dmsg-sdk/connect'

button.onclick = async () => {
  // Call it synchronously in the user gesture: it opens the bridge popup.
  const client = connectDmsg({ appId, session })
  await client.ready
  await client.authenticate(authenticationRequest)
  await client.signDocument(operationId, documentRequest) // protocol "dmsg-connect/1"
  await client.signAction(preparedAction)
  await client.checkout(checkoutRequest)
  // Reconnect with the same persisted P-256 key and original operation ID.
  const operation = await client.getOperation(operationId)
  await client.openOperation(operationId)
}
```

The page must not send `Cross-Origin-Opener-Policy: same-origin` (it cuts the
popup's opener) and should forbid being framed (`frame-ancestors`). A blocked
popup fails with `POPUP_BLOCKED`, an unanswered handshake with
`BRIDGE_UNAVAILABLE` after 15 seconds, a closed popup with `DISCONNECTED`, and
a call without answer with `TRANSPORT_TIMEOUT` after 30 seconds. Closing the
popup does not cancel operations; open it again in a new gesture.
`bridgeOrigin` overrides `https://connect.dmsg.net` for Local and Staging.
Operations wait as `awaiting_user` until a device of the account approves
them; a browser that is not paired with the account gets `NOT_PAIRED`, and the
person connects it from dMsg (settings, "Connect a browser").

`session` supplies a public SPKI DER key and a `sign(Uint8Array)` method. Keep the
nonexportable private key in product-owned IndexedDB. Persist the original
request **before** dispatch; refresh and timeout must not create a second
operation. Never export dMsg identity, recovery or wallet keys to a product.
The [durable browser example](../../examples/dmsg-browser-client.ts) shows this
boundary. It deliberately leaves product authentication and receipt consumption
to the integrating product.

`checkoutRequest` contains a product-authoritative `BillingOffer`, the expected
dMsg approving account, explicit product approval where required, and `Cash` or
`Panda`. dMsg chooses the wallet and a registered cash ledger, or a
specific neuron. PANDA covers the whole fee with zero customer cash principal;
a successfully applied commitment, including a future renewal, cannot exit
before its original expiry. Native SNS control is not transferred to dMsg.

A browser result is a notification. The product's backend/adapter must verify
and consume the original service decision and record its own delivery receipt.
It must not grant rights from a redirect, screenshot, wallet response, browser
`state`, unsigned JSON or a Worker projection.

## Independent verification

- `dmsg-sdk/ic`: verify IC root, fixed home, certificate time, witness and exact
  authentication challenge or certified leaf; `verifyRecordedLeaf` checks a leaf
  kept as evidence at any certificate time.
- `dmsg-sdk/statements`: verify the original COSE signature and content profile.
- `dmsg-sdk/bundle`: verify an attestation bundle (`dmsg-attestation/1`) the dMsg
  desktop exports, check by check (`verifyAttestationBundle`): the statement
  against the original file's bytes when given, the device signature, the
  execution receipt's certificate by one of the deployment's user homes, and the
  receipt's binding to the artifact, key, account and request. A failed check is
  reported, not thrown; the caller supplies the trusted homes (the handle
  registry's `get_handle_config`) and IC root key. The device's present status is
  not checked.
- `dmsg-sdk/codec`: the bounded canonical CBOR, base64url and hex codec used by
  those statements. `dmsg-sdk/errors` (also re-exported by `dmsg-sdk/connect`)
  provides the coded `DmsgError`.
- Main export: verify action/checkout shapes, actions against the registered
  schema (`validateActionCommand`, `actionSchemaHash`), file commitments,
  exact conversion and one-rounding PANDA arithmetic; convert against **generated
  Candid types** with `fromCandid` / `toCandid`.

These checks are separate from product roles, current account linkage, live SNS
qualification and product delivery. Missing original file bytes mean content
was not checked. TokenList archives the original signature, key descriptor,
execution certificate, product intent and historical account-binding evidence.

## Build and verify

Node >=22.18 and TypeScript 7.0.2 are supported. `dmsg-sdk/connect` needs
`window.open`, `postMessage`, WebCrypto P-256 and `URL.canParse`; the bridge
page itself needs Chrome/Edge 133, Firefox 130 or Safari 17 (WebCrypto X25519).

```sh
pnpm install --frozen-lockfile
pnpm --dir packages/dmsg-sdk build
node --test packages/dmsg-sdk/tests/*.test.ts
pnpm --dir packages/dmsg-sdk pack --pack-destination /tmp/dmsg-release
```

Tests compare Rust vectors with an independent TypeScript encoder/verifier.
Set `DMSG_EXTERNAL_FIXTURE` to the real PocketIC authentication proof for the
additional live-Wasm verification test. Building/packing does not publish npm
or deploy any canister. Pin the coordinated source/Wasm snapshot when deploying.

See [integration](../../docs/protocol/integration.md),
[dmsg-connect/1](../../docs/protocol/connect-v1_zh.md),
[app actions](../../docs/protocol/app-action.md),
[commerce v2](../../docs/protocol/commerce.md), and the
[independent account adapter](../../examples/dmsg-account-product/README.md).

## Principal documents

`dmsg-sdk/principal` uses the official `agent-protocols` `PrincipalDocument`.
`profileTarget`, `accountHome` and `verifyHandle` resolve root account/handle
URLs. `verifyPrincipalDocument` verifies the directory's exact JSON at
`["documents", account_id]` against a pinned IC root, directory and canonical
principal origin, with a 60-second certificate window. The HTTP JSON and
human page consume the same bytes. Schema validation alone is not authentication.
The SDK rejects duplicate JSON keys, wrong IDs, tampered leaves and stale proofs.
`extra["dmsg.link_images"]` retains HTTPS artwork by link URL.

The retired cloud Profile verifier and `/profile` export have been removed.
Agent Profile (`agent-profile/1.0`) is a separate protocol and is unchanged.
