# Third-party integration contract

Status (2026-09-24): operational development implementation. Dedicated authentication,
closed application signing, browser v4 checkout, two-asset settlement, irreversible
PANDA commitments and both account/project adapters are connected. TokenList and
an independent account product exercise the public contract. Production identities,
R/budgets, deployment and real-fund acceptance remain explicit release steps.
The old commercial endpoints and membership change/buyout paths have been removed;
there is no v2-to-v1 conversion. See [commerce 2](commerce.md) for runtime methods.

## Versions and ownership

| Surface | Version |
| --- | --- |
| External registration and application approval | `dmsg-integration/1` |
| Authentication certified leaf | `application/vnd.dmsg.authentication+cbor;v=1` |
| Typed application action profile | `application/vnd.dmsg.app-action+cose;v=1` |
| Generic checkout and irrevocable membership | `dmsg-commerce/2`, `membership/2` |
| Browser bridge | `dmsg-extension/4` |

The three existing document profiles keep their v1 bytes. Authentication is a
dedicated approved IC-certified leaf, not a signed statement, current security
snapshot, JWT, wallet delegation or product role. Verification must authenticate
the IC root, expected user home, certificate time, witness path and exact bytes
before calling `match_authentication_result`.

`dmsg_types::integration` owns the public Rust types; `dmsg_protocol::integration`
owns validation and commitments. `packages/dmsg-sdk` exports the independent
TypeScript encoder. [integration.cddl](integration.cddl) specifies the canonical
record shapes. CDDL and TypeScript shapes are generated from the same public
types; independent encoder tests prevent sharing the same serialization bug.

## Identity and admission

An application pins exact canonical origins, authentication receiver, user and
COSE homes, the fixed action authority, capabilities, closed signing profiles and product IDs. A product pins its quote authority,
beneficiary authority, adapter, subject schema/size, merchant account, ledgers,
terms and subsidy budget namespace. A paused registration stops **new** approvals
and commitments; it must not stop reconciliation, refunds or expiry processing.

Applications and products are governance registered. Configuration updates
increment their revision; existing IDs cannot change environment or subject
identity semantics. An app-origin update does not retroactively rewrite accepted
orders. Canister principals never come from a web page's suggested callback.

The approving dMsg AccountId, beneficiary, economic actor and ICRC payer are
distinct fields. A dMsg account can approve a project subscription only after
the registered product checks its own permission. Raw quote/approval values and
type validation are not evidence of that permission.

Initial beneficiary schemas are `tokenlist-project-v1` (8-byte big-endian
ProjectId), `dmsg-account-v1` (12-byte Xid) and `sample-account-v1` (12 opaque
fixture bytes). Length never determines the schema. Sample principals and IDs
are fixtures, not deployed identities.

## Canonical bytes

Records are definite-length CBOR maps with the exact text field names in CDDL.
Every field is present; an optional account subaccount is null or 32 bytes.
Serde externally tagged enums use a text string for unit variants and a
single-entry map for variants with fields. Principals, AccountId and hashes are
raw byte strings. Arrays preserve their declared order, and allowlists reject
duplicates. Map keys use RFC 8949 core deterministic bytewise order. Unknown
fields, versions, variants, noncanonical encodings and out-of-range integers
are rejected. No Unicode or text normalization is applied.

All nonnegative integers up to u64 use shortest ordinary CBOR integers; u128
values above u64 use tag 2 and a shortest big-endian byte string. No floats or
negative integers occur. SDK values use bigint, never JS Number. A JSON bridge
must convert those values through checked decimal strings, with explicit byte
encoding; ordinary Rust Serde JSON is not that bridge.

Commitments are `SHA256(CBOR([1, domain, complete_value]))`:

| Value | Exact domain |
| --- | --- |
| BillingOffer | `dmsg/commerce/offer/v2` |
| AuthenticationRequest | `dmsg/authentication/request/v1` |
| ApplicationApproval | `dmsg/application/approval/v1` |
| PandaQuote | `dmsg/commerce/panda-quote/v2` |
| CashQuote | `dmsg/commerce/cash-quote/v2` |
| ProductDecision | `dmsg/commerce/decision/v2` |

Authentication certificate path is one raw segment:
`b"authentication/v1/" || account_id[12] || operation_id[32]`.
Device signatures continue to use the existing device-approval framing with a
purpose-specific command domain and current security epoch/sequence. None of
these hashes is itself a signature or authorization.

## Time and resource bounds

Business timestamps are u64 Unix milliseconds. COSE iat remains seconds; ICRC
created_at_time and IC certificate time remain nanoseconds. Comparisons use
half-open intervals: equality with expiry is already expired.

| Bound | Maximum / rule |
| --- | --- |
| Authentication request / proof | 5 minutes, proof cannot exceed request |
| Derived product session | 24 hours before fresh authentication |
| Fresh authentication certificate | 60 seconds; no future certificate accepted |
| Unaccepted offer | 15 minutes from issuance |
| Cash funding | 30 minutes from acceptance |
| Initial cash activation | 24 hours, bounded by contract end |
| PANDA application | 24 hours, bounded by contract end |
| New PANDA cooling | At least 65 minutes; application must outlive cooling |
| Qualification lease | At most one hour, truncated to the contract end |
| Rate policy notice | At least 30 days |
| Per-account pending operations | 32 |
| Successful external approvals | 60 per account per UTC hour |
| App origins / homes / products | At most 16 of each |
| Generic encoded input | At most 65,536 bytes |

The user issuer enforces the per-account pending and hourly approval limits.
It requires an active, unfrozen account and a nonrevoked device with FormalApprove;
RootManage and vault access are not granted. Both device sequence and security
epoch are checked again after the commerce configuration call. Expired records
are removed in bounded batches of 64 accounts by prune_external_approvals. A timeout of
an accepted operation never authorizes releasing an unknown Apply.

## Cash and PANDA

The authoritative BillingOffer prices the **entire exact interval** in USD micro
units. Free offers cannot consume a PANDA claim. Services fetch/verify the offer
from the registered product authority, not from a browser's amount or expiry.
Acceptance freezes the offer; cooling does not extend its expiry or reprice it.

PANDA stake is exactly
`ceil(amount_usd_micros * r_num * 100_000_000 / (1_000_000 * r_den))`.
Intermediate values are arbitrary precision; only the final result must fit
u128. The service computes it using an immutable effective rate policy. The
expiry is a lock coverage and commitment boundary, not another proration factor.
The subsidy obligation is USD service value, never cash revenue or merchant cash.
Only full waiver is supported: no discount, partial payment or buyout.

After product Apply succeeds, the original contract expiry is immutable. Benefit
termination does not release occupancy. No user, product, pause, unlink or
ordinary governance action may shorten it. Pre-Apply applications can cancel;
unknown decisions retain their identity/reservations until receipt reconciliation.
Renewals create a separately approved consecutive term. SNS owners retain their
native control; this contract does not claim physical custody or prohibit SNS
Split. Stateful enforcement is the membership work package.

Production cash assets are pinned to ckUSDT `cngnf-vqaaa-aaaar-qag4q-cai` and
ckUSDC `xevnm-gaaaa-aaaar-qafnq-cai`. Their metadata/fees and archive behavior must
still be verified before deployment. Local fixtures have different explicit
principals. Every order commits to one ledger, payer, deposit, amount, conversion
basis, fee ceiling/reserve and deadlines. Accounting and block deduplication are
scoped by ledger; merchant reserves are also scoped by merchant.

## Delivery and recovery

Persist the complete ProductDecision before invoking a product adapter. Apply
authenticates the registered service and atomically writes business state and
the complete terminal receipt. A repeated ID returns that receipt; the same ID
with different bytes fails. Both known application and known rejection are
durable. Unknown is the absence of an authenticated terminal receipt, not a new
decision outcome that permits reissuing or refunding.

The receipt commits to the decision hash and adapter. An Applied receipt retains
the exact offer end. Product authorization and current role/version checks
remain separate from funds verification, dMsg account approval and signature
mathematics.

## Reproduce the contract checks

```sh
cargo test --locked -p dmsg_types --test integration_contract
cargo run --locked -p dmsg_types --example integration_vectors > /tmp/integration-vectors.json
cmp /tmp/integration-vectors.json src/dmsg_types/tests/integration_vectors.json
python3 scripts/generate-integration-sdk.py --check
node --test packages/dmsg-sdk/tests/contracts.test.ts
```

The vectors include project and account subjects, two distinct fixture ledgers,
authentication, exact approval, cash/PANDA quotes, a durable decision, and u128
overflow boundaries. There are no real external identities, real transfers or
production rate defaults in these tests.

The browser transport and recovery contract is specified in [browser-v4](browser-v4.md).
