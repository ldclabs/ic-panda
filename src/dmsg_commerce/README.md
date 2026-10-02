# dmsg_commerce

Generic two-asset subscription checkout and the dMsg account product adapter. See [commerce v2](../../docs/protocol/commerce.md), [public billing types](../dmsg_types/src/integration_billing.rs), [integration registration](../../docs/protocol/integration.md), and generated [Candid](dmsg_commerce.did).

`checkout.rs` handles settlement calls, `checkout_model.rs` owns pure money/reply transitions, `checkout_store.rs` owns durable records, price history, reader indexes and certification, and `calls.rs` coalesces live callbacks. Together they provide two pinned six-decimal assets, per-order funding accounts, ledger/archive validation, durable Apply decisions, original-source refunds and earned merchant settlement. Asset quotes require explicit fresh USD observations and depeg guards. Every accepted price snapshot must exactly match authority-published stable history; an account approval cannot authenticate a caller-invented conversion price. Every transfer freezes its ledger, source, destination, memo, nanosecond creation time and fee. A live transfer/reconciliation/revision guard excludes overlapping callbacks; upgrades discard only the guard. Unknown retains those exact parameters; the recipient can reissue a known rejection with a fresh timestamp, at the original fee or the ledger's expected fee within the original cap.

Funding conservation is checked per order and ledger: confirmed deposits equal retained service obligations, earned balances, refunds, fee reserves, in-flight transfers, completed outflows and network fees. Deduplication is `(ledger, block)`, so equal indexes in the two ledgers are independent. Merchant settlement releases only earned funds; PANDA face value creates no cash income.

`product.rs` implements the dMsg account product using the shared ProductBook CAS and adapter callbacks. `model.rs` retains account resource timelines, catalogs, storage allowances and execution-month entitlements. Generic settlement has no hard-coded dMsg beneficiary. dMsg offers base subscriptions, cash-only upgrades and storage add-ons; upgrades keep the original annual anchor and both upgrades and add-ons are prorated over that full term. PANDA supports base full waivers with immutable original endpoints. Short lease refreshes do not reset held or charged monthly execution units. Leases end at scheduled catalog boundaries; Free monthly units integrate the actually effective catalog intervals. Subject records retain only current-month/live/future resource history and the last service end. The published optional stop/add-on lease fields remain part of the resource contract consumed by existing services.

Governance registers bounded versioned apps/products and publishes certified configuration. Exact origins, authority/adapter homes, subject schemas, capabilities, signing profiles and merchant identities are explicit. Private checkout/transfer lists are filtered by existing payer/merchant/recipient/adapter/governance permissions. Cash admission pause preserves reconciliation, refunds, lease refresh and expiry. Funding/product/outgoing dispatches are bounded per caller and globally; purely local refund allocation and guarded duplicate calls do not spend that allowance.

Development stable schema 5 uses 1 MiB memory-manager buckets and integer-keyed local order/transfer/asset records. Same-schema upgrades retain funds, published price history, decisions, replies, reader indexes and live certified projections. Obsolete v1 order/refund/change APIs and AuthorizeMembership have been removed, without migration or compatibility branches. Previous v1 throughput/cost samples are not v2 capacity evidence. Test the current implementation with:

```sh
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane commerce:: -- --test-threads=1
```

The actual Wasm suite covers two ledgers, refunds, settlement, Unknown recovery across upgrades, immutable fee caps, separate account/project beneficiaries, independent adapter lost ACK and cash/PANDA CAS, operational access control and conservation. Production configuration and real-money acceptance remain separate.

## History and recovery

`checkout_deposits(order_id, after, take)` returns up to 128 deposits and a real continuation cursor. Order and transfer pages use payer/merchant/adapter/governance/recipient indexes, including archived history, without scanning other readers' records.

`sweep_checkout_history` archives at most 32 settled orders and 32 succeeded/superseded transfers. Orders retain hot records for 30 days after the later of their last money transition and service/activation end; transfer retention begins at its terminal transition. Unfinished transfers, unknown delivery and outstanding money are never archived. Cold orders discard duplicate authorization/decision bodies, retaining an input digest, complete quote, final receipt, funding reference and ledger balances. Direct reads and reader indexes remain available; cold certificate endpoints return `ResultExpired`. Late verified deposits restore the same live order and original-source refund path. Deduplication identities are never recycled. Admission caps are 100,000 hot orders and 1,000,000 total order identities, independent of recovery.

The native history sample can be run explicitly:

```sh
cargo test --locked -p dmsg_commerce checkout_history_profile -- --ignored --nocapture
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane commerce::commerce_review::commerce_cost_profile -- --ignored --exact --nocapture
```

The Wasm comparison uses the same host test and fresh instances for each build. Set `DMSG_WASM_DIR` to a directory containing the baseline commerce Wasm and the same other canisters, and set `DMSG_COMMERCE_BASELINE=1` only for the old build (which has no archive endpoint). Do not upgrade schema 4 data into schema 5.

## Measured boundaries (2026-10-03)

A native debug sample on this machine measured 100 sparse-reader pages at 15.58 ms with 1,000 records and 14.66 ms with 10,000 records. The prior 512-row filtering algorithm took 3,783 ms and 3,836 ms respectively per 100 scans, and may require multiple scans to find the reader's records. Hot certification rebuild at 10,000 records took 6,526 ms; after those settled records were archived, the tree rebuild took 44 µs because cold records are not traversed. A sample rejected order shrank from 2,625 to 1,531 encoded bytes after redundant authorization removal. These are native microbenchmarks, not Wasm throughput/cycles or production capacity evidence.

The actual Wasm regressions cover forged prices, a valid earlier observation across price publication and upgrade, delayed rejection/retry races, interrupted transfers across upgrades, merchant access, per-caller budgets, catalog activation, incoming pagination, bounded cold history, exact replay/conflict behavior, and late refunds. Real asset acceptance, private cloud end-to-end checks and production capacity remain separate gates.

The PocketIC Wasm sample compares baseline `8082df5` with this change using 32 opened-and-cancelled orders, the same other canisters and the same host binary:

| Commerce-only measurement | Baseline | This change |
| --- | ---: | ---: |
| Stable memory after 32 orders | 83,951,616 B | 14,745,600 B |
| Unrelated-reader replicated query, 32 orders | 16,086,227 cycles | 8,927,600 cycles |
| Quote/authorization/open scenario, 32nd order | 120,463,064 cycles | 122,493,284 cycles |
| Same-schema upgrade, 32 hot orders | 7,811,210,552 cycles | 8,009,290,953 cycles |
| Same-schema upgrade after archiving those 32 orders | not available | 7,971,986,001 cycles |

This sample reduces allocated stable memory by 82.4% and unrelated-reader query cost by 44.5%. Added price authentication, indexes and recovery accounting increase the opening scenario by 1.7%; the larger module increases this small hot-upgrade sample by 2.5%. Archive reduces tree work as history grows, but does not establish lower total upgrade cycles at only 32 orders. Cycle figures exclude the user, ledger and other canisters, and native timing does not predict production throughput.
