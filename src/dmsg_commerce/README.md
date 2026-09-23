# dmsg_commerce

Merchant subscriptions, independent storage products, SNS product adapter, short resource leases and execution-month timelines. See the [public contract](../../docs/protocol/commerce.md), [types](../dmsg_types/src/billing.rs), and [Candid](dmsg_commerce.did).

Cash uses one configured six-decimal ledger with the DFINITY ICRC-3 transfer/archive adapter. Governance confirms ledger metadata before opening orders. Order subaccounts isolate funds. Reconciliation classifies every deposit and sends refunds to the proven original source. The accounting assertion is:

```
confirmed_in = service_reserve + earned + refundable + fee_reserve
             + outgoing + transferred + network_fees
```

Subscription activation is atomic with the incoming funds decision. Outgoing legs retain their exact parameters across Unknown and upgrades. `collect_revenue` is governance restricted and consumes only released earned balances. Qualification face value never enters cash accounting.

Catalog versions, cash terms, PANDA decisions and their outcomes are fixed. A source entering Closing issues no new leases; refunds wait for the maximum outstanding lease deadline. Old execution charges remain in the user canister. Only configured user homes may request replicated execution entitlement or register an account creation time.

The default plan helper is public, but init takes an explicit catalog. Policy publication requires thirty days' notice. Admission pause does not withdraw active commitments. `refresh_catalog` publishes scheduled transitions for certified consumers; `refresh_entitlement` never creates arbitrary unknown subjects. Existing users can obtain their first Free projection through `dmsg_user.refresh_execution_entitlement`.

Orders are persisted only after read-only user authorization succeeds; concurrent retries consume one daily admission. Public advancement returns minimal `OrderProgress` / `TransferProgress` records. Financial details remain in the authorized queries, including `get_transfer.last_error` for ledger diagnostics. Known rejected transfers can adopt the ledger's reported fee within their frozen ceiling; known fees above the ceiling also block new admission. Storage products have an annual price prorated to the paid base term's endpoint, and survive early base closure independently.

Local configuration and subject records use development stable schema 2 with integer top-level keys. Configuration and bounded rate counters live in heap between messages and are written at initialization and `pre_upgrade`; do not skip that hook. Subject/order/funds records still commit directly to stable memory. Catalogs are cached in version/effective-time order and searched by binary search; `list_catalogs(after_version)` pages through up to 64 versions. Unverifiable SNS refreshes retain a one-minute retry deadline across upgrades without extending the old qualification lease.

## Local cost and upgrade measurements

PocketIC 16, release Wasm, one beneficiary, each order with one verified refundable deposit and one completed outgoing refund. Measurements on 2026-09-23 include the complete upgrade and certification rebuild; they are local samples, not production throughput or million-subject capacity claims.

| Retained orders / deposits / transfers | Upgrade cycles | Unchanged reconciliation cycles | Stable bytes |
| ---: | ---: | ---: | ---: |
| 1 / 1 / 1 | 5,524,199,683 | 7,669,433 | 58,785,792 |
| 100 / 100 / 100 | 13,560,019,050 | 7,860,086 | 58,785,792 |
| 1,000 / 1,000 / 1,000 | 17,894,777,649 | 9,816,851 | 58,785,792 |

Same-schema upgrades preserved accounting and certified order retrieval at all three sizes. An immediate retry after unverifiable SNS qualification cost 8,293,511 commerce cycles, versus 16,758,617 in the reviewed baseline, and no longer called membership or changed the lease revision.

Reproduce after building the Wasm modules:

```sh
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane commerce::regressions::commerce_upgrade_profile -- --ignored --nocapture
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane commerce::regressions::unverifiable_refresh_has_a_persistent_cooldown_and_can_recover -- --nocapture
```

Historical orders, deposits, transfer proofs and decisions remain retained for replay protection. Upgrade still scans transfer history and rebuilds all certified order/subject leaves. The tested 1,000-row sample did not require a cleanup or indexing redesign; larger operating targets must repeat the growth measurement before rollout, including richer subject histories. Do not delete financial deduplication proofs to make an upgrade fit.


## Third-party registration

`register_integration_product` and `register_integration_app` require the configured governance
caller. Products must be registered before referencing apps. Registrations are bounded to 256
products and 64 applications, versioned, and certified. Product subject identity, authorities and
adapter cannot be replaced under an existing ID. App origins, homes, profiles and capabilities
are explicit allowlists; pause updates retain the registration and existing operations.

The pinned user home obtains replicated configuration through `read_integration_configuration`;
public clients use `integration_configuration_certificate`. Both registration maps and certified
leaves survive upgrade. This implements registration and approval infrastructure, not generic v2
cash settlement. Existing merchant v1 runtime behavior is replaced in subsequent work packages.
See [the public integration contract](../../docs/protocol/integration.md).
