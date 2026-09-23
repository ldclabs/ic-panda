# @dmsg/sdk 0.2.0

A standalone ESM client for dMsg authentication, document/action signing and
subscription checkout. The package imports no Svelte, extension database,
private relay or wallet store. It ships JavaScript and TypeScript declarations.

## Contracts

| Surface | Version | Responsibility |
| --- | --- | --- |
| Browser bridge | `dmsg-extension/4` | Exact origin/document and P-256 key possession; durable operation recovery |
| Authentication and application approval | 1 | Challenge/account/device/purpose binding |
| Application action | `dmsg-app-action/1` | Closed commands, original files, intended receiver and actor |
| Subscription commerce | 2 | Authoritative USD bill, one cash asset or full PANDA waiver, original Apply receipt |
| Document statements | existing three v1 profiles | Their original signing bytes remain unchanged |

All wire integers, including versions, are `bigint`. Principals, hashes and
account IDs are `Uint8Array`. `canonical` rejects Number, floats, undefined,
invalid Unicode and unsupported objects. `validateShape` rejects extra fields,
unknown variants, unbounded arrays and out-of-range integers. Generated CDDL and
TypeScript shapes come from the Rust contracts; regenerate with
`python3 scripts/generate-integration-sdk.py`.

```ts
import { connectDmsg } from '@dmsg/sdk/browser'

const client = connectDmsg({ extensionId, appId, session })
await client.authenticate(authenticationRequest)
await client.signDocument(operationId, documentRequest)
await client.signAction(preparedAction)
await client.checkout(checkoutRequest)
// Reconnect with the same persisted P-256 key and original operation ID.
const operation = await client.getOperation(operationId)
await client.openOperation(operationId)
```

`session` supplies a public SPKI DER key and a `sign(Uint8Array)` method. Keep the
nonexportable private key in product-owned IndexedDB. Persist the original
request **before** dispatch; refresh and timeout must not create a second
operation. Never export dMsg identity, recovery or wallet keys to a product.
The [durable browser example](../../examples/dmsg-browser-client.ts) shows this
boundary. It deliberately leaves product authentication and receipt consumption
to the integrating product.

`checkoutRequest` contains a product-authoritative `BillingOffer`, the expected
dMsg approving account, explicit product approval where required, and `Cash` or
`Panda`. The extension chooses the wallet and a registered cash ledger, or a
specific neuron. PANDA covers the whole fee with zero customer cash principal;
a successfully applied commitment, including a future renewal, cannot exit
before its original expiry. Native SNS control is not transferred to dMsg.

A browser result is a notification. The product's backend/adapter must verify
and consume the original service decision and record its own delivery receipt.
It must not grant rights from a redirect, screenshot, wallet response, browser
`state`, unsigned JSON or a Worker projection.

## Independent verification

- `@dmsg/sdk/ic`: verify IC root, fixed home, certificate time, witness and exact
  authentication challenge or certified leaf.
- `@dmsg/sdk/statements`: verify the original COSE signature and content profile.
- Main export: verify closed action/checkout shapes, file/input commitments,
  exact conversion and one-rounding PANDA arithmetic; convert against **generated
  Candid types** with `fromCandid` / `toCandid`.

These checks are separate from product roles, current account linkage, live SNS
qualification and product delivery. Missing original file bytes mean content
was not checked. TokenList archives the original signature, key descriptor,
execution certificate, product intent and historical account-binding evidence.

## Build and verify

Node >=22.18 and TypeScript 6.0.3 are supported. The browser path needs Chrome MV3
external ports, WebCrypto P-256 and structured cloning of nonexportable keys.

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
[browser v4](../../docs/protocol/browser-v4.md),
[app actions](../../docs/protocol/app-action.md),
[commerce v2](../../docs/protocol/commerce.md), and the
[independent account adapter](../../examples/dmsg-account-product/README.md).
