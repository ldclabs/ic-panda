# dmsg_commerce

Generic two-asset subscription checkout and the dMsg account product adapter. See [commerce v2](../../docs/protocol/commerce.md), [public billing types](../dmsg_types/src/integration_billing.rs), [integration registration](../../docs/protocol/integration.md), and generated [Candid](dmsg_commerce.did).

`checkout.rs` and `checkout_model.rs` own generic quotes, two pinned six-decimal assets, per-order funding accounts, ledger/archive validation, durable Apply decisions, original-source refunds and earned merchant settlement. Asset quotes require explicit fresh USD observations and depeg guards. Every transfer freezes its ledger, source, destination, memo, nanosecond creation time and fee. Unknown retains those exact parameters; the recipient can reissue a known rejection with a fresh timestamp, at the original fee or the ledger's expected fee within the original cap.

Funding conservation is checked per order and ledger: confirmed deposits equal retained service obligations, earned balances, refunds, fee reserves, in-flight transfers, completed outflows and network fees. Deduplication is `(ledger, block)`, so equal indexes in the two ledgers are independent. Merchant settlement releases only earned funds; PANDA face value creates no cash income.

`product.rs` implements the dMsg account product using the shared ProductBook CAS and adapter callbacks. `model.rs` retains account resource timelines, catalogs, storage allowances and execution-month entitlements. Generic settlement has no hard-coded dMsg beneficiary. dMsg offers base subscriptions, cash-only upgrades and storage add-ons; upgrades keep the original annual anchor and both upgrades and add-ons are prorated over that full term. PANDA supports base full waivers with immutable original endpoints. Short lease refreshes do not reset held or charged monthly execution units.

Governance registers bounded versioned apps/products and publishes certified configuration. Exact origins, authority/adapter homes, subject schemas, capabilities, signing profiles and merchant identities are explicit. Private checkout/transfer lists are filtered by existing payer/merchant/recipient/adapter/governance permissions. Pause prevents new commitments while allowing reconciliation, refunds, lease refresh and expiry.

Same-schema upgrades retain funds, decisions, replies and certified projections. Obsolete v1 order/refund/change APIs and AuthorizeMembership have been removed, without migration or compatibility branches. Previous v1 throughput/cost samples are not v2 capacity evidence. Test the current implementation with:

```sh
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane commerce:: -- --test-threads=1
```

The actual Wasm suite covers two ledgers, refunds, settlement, Unknown recovery across upgrades, immutable fee caps, separate account/project beneficiaries, independent adapter lost ACK and cash/PANDA CAS, operational access control and conservation. Production configuration and real-money acceptance remain separate.
