# Membership and commerce contracts

English | [简体中文](commerce_zh.md)

This document describes the public implementation, not a deployment or pricing announcement. `membership/1` is product neutral; `dmsg-commerce/1` supplies dMsg resource contracts. The executable Candid in `src/membership`, `src/dmsg_commerce`, `src/dmsg_user`, and `src/dmsg_payment` is the method/type reference. Canonical CBOR is defined by `dmsg_protocol`; [commerce_vectors.json](../../src/dmsg_types/tests/commerce_vectors.json) freezes representative bytes and arithmetic.

## Boundaries

- `membership` owns PANDA qualification, immutable threshold policies, the global `(SNS governance, neuron ID)` occupancy index, short qualification leases, subsidy reservations, and durable product decisions. `ProductConfig` pins the adapter, accepted authority, subject schema, and subject byte length. No dMsg plans or AccountId interpretation is part of this canister.
- `dmsg_commerce` owns cash orders, independent storage add-ons, product contracts, refunds, revenue reserves, and resource projections. Cash activation and the funds classification commit in one message. SNS decisions commit locally and return a stored receipt; retrying the same decision never applies it twice.
- `dmsg_user` owns exact device-approved commercial intents and monthly execution usage. A commercial approval never adds an authentication binding. `dmsg_cose` accepts only its fixed user home; a resource certificate cannot authorize a signature.
- `dmsg_payment` continues to own delivery escrow. Membership orders do not enter the delivery state machine. `dmsg_handle` retains its independent PANDA pricing.

## Encoding and arithmetic

IDs are 32-byte byte strings; dMsg account subjects are the raw 12-byte AccountId. `Beneficiary` includes `product_id`, `authority_canister`, `subject_schema`, and `subject_bytes`. dMsg uses `dmsg` / `dmsg-account-v1`. Principals use the existing protocol representation. Monetary values are u128 (JSON decimal strings); resource counters are u64. Business time is UTC Unix milliseconds. ICRC transfer `created_at_time_ns` remains nanoseconds.

`digest(domain, value)` is SHA-256 of canonical CBOR `(1, domain, value)`. Canonical maps sort encoded keys bytewise. All certified paths below are one raw 32-byte segment:

| Value | Domain and input |
| --- | --- |
| Resource view | `dmsg/commerce/entitlement-key/v1`, Beneficiary |
| Published catalog | `dmsg/commerce/catalog-key/v1`, `"dmsg"` |
| Merchant order | `dmsg/commerce/order-key/v1`, order ID |
| Immutable PANDA policy | `membership/policy-key/v1`, policy version |
| PANDA claim | `membership/claim-key/v1`, claim ID |
| Execution usage | `dmsg/commerce/usage-key/v1`, `(account_id, YYYYMM)` |

Intent and cash order domains are `dmsg/commerce/intent/v1` and `dmsg/commerce/order/v1`. PANDA action and decision domains are `membership/claim-action/v1` and `membership/decision/v1`. A new action requires a new application identity/approval. A short approval can be renewed for the same immutable application while preserving its cooling start. The signed account command and the intent's application deadline are separate.

An annual period ends at the same UTC date/time next year, with February 29 clamped to February 28. All periods are `[start,end)`. Thresholds use arbitrary precision intermediate products and checked u128 results: `ceil(price_cents * R_num * 10^8 / (100 * R_den))`. Cash prices round up; refunds round down. Execution allowance sums `duration_ms * monthly_units` over the complete non-overlapping month timeline and divides once by the month duration. Time before account creation earns no units. At most 64 segments are accepted. A refund or renewal never clears charged or held units.

## Method permissions and state

Governance mutations require the configured SNS governance caller, not `is_controller` and not a caller-supplied proposal number. Ordinary policy publication requires 30 days' notice. Scheduled catalogs and delivery-fee policies are append-only: both their version and effective time must increase. Admission pauses affect new commitments; existing qualification refreshes and refunds remain available. Policy and product records are immutable; a version or product ID cannot be repurposed.

Before admission, governance calls `verify_sns_configuration` / `verify_ledger_configuration`. These check the root's SNS identities and configured ledger decimals, and record the current ledger fee separately from the frozen sales catalog. Deployment must independently pin the production SNS Wasm/Candid and the intended ledger ID; a matching number of decimals does not establish token identity. The implemented ledger adapter is the DFINITY ICRC-3 `1xfer` / `2xfer` block format, including authenticated archive callbacks.

`quote_order` is a non-mutating draft. `open_order` requires the actual payer and the user home's current approval. The opened order has an immutable receive subaccount, price, payer, fee reserve, funding deadline and activation deadline. `check_order_funding` reads the ledger itself. Each block is claimed once; only one qualifying payment activates an order. Underpayments, excess, wrong-source, duplicate and late payments remain refundable to their actual source. `reconcile_order`, `claim_deposit_refund`, `process_transfer` and `reconcile_transfer` can be advanced without the private service. Public advancement returns only `OrderProgress` (order ID/status) or `TransferProgress` (order/transfer IDs/status/replacement ID). Order details/certificates require the payer, corresponding user home or governance; deposit details are also visible to the original depositor, and transfer details to their recipient. No advance response discloses accounts, amounts or authorization intents.

Authorization verification is read-only: `open_order` writes an order and consumes one daily admission only after approval succeeds and the callback rechecks current state. Concurrent/repeated openings return the same order. Authorization attempts have separate per-minute limits of 10 per caller and 200 globally; failures create no permanent orders and do not consume the successful-order quota. Ledger work has its own 400/minute budget, separate from 200/minute resource refreshes. Budgets survive same-schema upgrades.

Cash actions support new annual membership, renewal within 30 days, upgrade preserving the annual endpoint, storage add-ons, and an SNS buyout of the remaining period. The original annual anchor survives upgrades. Refunds cover the first cash base purchase within seven days and cancellation of an unstarted renewal. A refund of that first period also closes its cash upgrades; each contributing order retains its own original payer and refund leg. Storage products quote an annual price, available only with currently qualified paid membership. The price is rounded up in proportion to the original base term remaining at quotation; the add-on ends at that base term endpoint. Once purchased it remains independently valid if the base contract closes early.

Closing stops issuing leases from the affected sources and records the latest outstanding lease deadline. After that deadline, reconciliation fixes unserved principal, terminates future entitlement and records the refund liability. Network-fee dust remains recorded. An outgoing fee cannot exceed the order's frozen fee reserve; a ledger fee above that ceiling blocks the transfer and, when already known, new order admission. A known rejected leg can use the ledger's recorded `BadFee.expected_fee` within the ceiling without waiting for a new sales catalog. `get_transfer.last_error` retains the typed ledger rejection; ambiguous legs cannot be revised. Outgoing transfers persist exact source, destination, amount, fee, memo and nanosecond timestamp before calling the ledger. Unknown results reuse those parameters or reconcile a matching block. Only a known rejected transfer can be revised, and superseded legs cannot be dispatched or reconciled. Revenue collection releases only earned service reserves and pays the configured treasury.

A PANDA claim authenticates the economic principal and verifies the account approval through the registered adapter. It accepts an individually controlled neuron: ConfigureDissolveState, ManagePrincipals, Disburse and Split are required; other principals may have only Vote. Known insufficient stake/control/lock is ineligible; unknown permissions, unrecognized state or a failed call are unverifiable. Effective stake is `max(cached_neuron_stake_e8s - neuron_fees_e8s, 0)`; maturity is excluded. A dissolving neuron is accepted if its fixed unlock timestamp covers the contract endpoint.

New claims cool for at least 65 minutes, beginning with the first verified eligible funded observation. Activation rechecks the approval, principal, stake and final interval. Short leases are anchored at observation start and never exceed one hour. Applying/Closing decisions persist across awaits and upgrades. A lost product ACK is reconciled using the fixed decision ID; it never releases occupancy merely because a timeout elapsed. `cancel_application` releases an unapplied request under its original actor. `reconcile_claim` advances already prepared decisions and clears expired commitments/applications. The product receipt fixes the benefit interval and commitment deadline. Replaced/bought-out claims use `release_replaced_claim`; their old leases must expire before reuse. Known ineligibility has a seven-day product repair period; unverifiability pauses that clock. Released claims cannot revive. SNS upgrades/replacements end the old interval and start the new interval at the same immutable decision timestamp, even when delivery is delayed; existing lease-release deadlines remain recorded.

## Certified consumption and execution

Queries return `CertifiedBatch` schema 1, with exact leaf bytes. Validate the expected canister, certificate, witness, requested path and leaf. Absence is only usable with a complete absence proof plus a certified Free policy. Unknown/pruned witnesses and failed requests are not Free. Query methods never create or extend leases. `refresh_catalog` publishes the effective scheduled catalog; consumers must refresh at a scheduled transition instead of extending the previous published policy.

Resource projections distinguish business revision from lease revision. Refreshing a lease does not invalidate a paid order's business CAS. Resource leaves contain no neuron ID or payer identity. Unknown SNS qualification issues no reusable fallback lease. Known invalid qualification grants only the Free resources and preserves the repair information. Independent storage add-ons survive base membership termination. Unverifiable refreshes have a persistent one-minute retry cooldown; returning the prior expired view does not extend qualification.

`get_execution_entitlement` is a replicated response restricted to the corresponding configured user home; it needs no query certificate and never calls back into the user canister. The user supplies its immutable account creation time, recomputes the complete month's allowance, rejects revision rollback and reserves units together with the execution sequence. `ExecutionGrant.commerce` binds the reservation ID, month, units, policy versions and expiry; the COSE grant digest is `dmsg/cose-execution/v3`. Formal statement and approval bytes remain unchanged. Explicit `refresh_execution_entitlement` fetches current business terms even while an older local lease remains valid, preserving held and charged units. Completed and expired historical outputs consume their original units; recorded COSE failures release units; Unknown retains the reservation. Transport non-delivery and COSE application rejections without a stored terminal result retain the grant, sequence and reservation for same-request reconciliation, including closing an expired sequence. Root derivation uses protected budgets instead of commercial units. COSE reserves 20% of the deployment execution/cycles ceiling from formal signatures for safety operations while retaining the total hard limit.

Public errors add `MembershipStale`, `MembershipIneligible`, and `MembershipClosing`. Continue using `VersionConflict`, `IdempotencyConflict`, `QuotaExceeded`, `Pending`, `Expired`, `FeeBlocked`, and `ExecutionUnknown` according to the recorded operation. Never create another payment to resolve an unknown result.

## Delivery profile 2

`Quote` adds `fee_policy_version`. Its service fee must equal `max(ceil(recipient_net * rate_bps / 10000), minimum_atomic)` under the effective policy. The net amount is unchanged. Quote and AdmissionReceipt domains become `dmsg/quote/v2` and `dmsg/admission-receipt/v2`; receipt `protocol` is 2. Existing opened orders keep their fixed fee and settlement/refund decision. Private clients must explicitly adopt this version before using these development canisters.

Delivery payment separately exposes controller-only `set_ledger_fee` for the expected network fee within the deployment ceiling. It does not change the governance service-fee policy or rewrite accepted quotes/prepared transfers. Default configuration certificates select the policy effective at query time; historical versions remain queryable. Transfer records include a bounded `last_failure` diagnostic without weakening Unknown handling.

## Verification and deployment limits

`make test-dmsg` covers native tests, Clippy, all six production Wasm modules, Candid extraction comparisons, independent Rust/JavaScript vectors and PocketIC tests with fault-injecting ledger/SNS fixtures. The fixtures are not production SNS or wallet acceptance evidence. No production R is hardcoded; 5000 PANDA/USD appears only in tests. Product policies and production economic-principal ingress connections require deployment verification. This repository does not implement TokenList's adapter, private resource accounting, a cross-chain payment adapter, or an automatic debit mandate.

`list_catalogs(after_version)` returns up to 64 versions strictly after the optional cursor. Catalog lookup uses a bounded ordered cache; weight changes compare against the preceding scheduled version and take effect only at a UTC month boundary.

Commerce uses development stable schema 2 for its local configuration/subject records and the revised merchant transfer format. Configuration and rate budgets are held in memory and persisted at initialization and `pre_upgrade`; upgrades must not skip that hook. The current growth profile and its limits are documented in [the commerce README](../../src/dmsg_commerce/README.md). Development stable schemas are new-instance schemas; same-schema upgrades retain state. These changes do not migrate legacy production services or roots. The shared service enforces only registered consumers; TokenList must switch to that same deployed authority before cross-product exclusivity can be advertised.

The pre-commerce public baseline review (`65a2635` → `b319e21`) found that changes in `user.rs`, `cose.rs`, `payment.rs` and `profiles/delivery.rs` were comments/formatting/trailing commas. The protocol README also clarified shared Xid allocation and historical execution proof freshness. This implementation deliberately changes the commercial and delivery contracts listed above; it is not a hash-only re-pin of that baseline.

Generate an exact handoff snapshot after building and testing:

```sh
python3 scripts/export-commerce-snapshot.py --output /tmp/commerce-source.json --fixtures /tmp/dmsg-commerce-fixtures
```

The snapshot explicitly identifies a working tree and its base commit. It does not assert a new release commit or private-service compatibility. The receiving repository must review the changed contracts and vectors before updating its own source pin.
