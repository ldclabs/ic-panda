# membership

Product-neutral PANDA SNS qualification, global neuron occupancy, fixed policy snapshots and durable adapter decisions. No product storage quotas or cash ledger accounting live here. See the [public commerce contract](../../docs/protocol/commerce.md), [types](../dmsg_types/src/membership.rs) and [Candid](membership.did).

Configure one shared instance, its SNS root/governance/PANDA ledger, registered adapters, policies and explicit subsidy budget. Governance must verify the SNS configuration before admitting claims. Governance calls use the fixed SNS principal. The minimal adapter exposes `authorize_membership_intent`, `authorize_membership_close`, `apply_membership_decision`, and `get_membership_decision`.

Initial account authorization is read-only: failed approval creates no Claim and consumes no application slot or subsidy reservation. After approval, the callback rechecks current policy, occupancy, capacity and budget before storing state and reading SNS. Prepared decisions reconcile by their immutable ID. Occupancy includes uncompleted product application/close commitments; a timeout does not prove absence of benefit. New applications require a fresh account approval after cooling. Refresh is on demand, bounded globally, and unavailable to arbitrary unrelated callers. An upgrade rebuilds certification from stable claim records.

Claim storage schema 2 uses dedicated CBOR representations with integer field keys throughout requests, policies, views, decisions and receipts, including enum payload fields. Absent optional fields are omitted. Config uses development schema 2 and no longer duplicates the initial policies; public protocol bytes and certification remain separate from storage encoding. Unit-test samples shrink from 1,199 to 526 bytes for a checking claim and from 2,465 to 1,260 bytes for an active claim with a decision and receipt, compared with the previous schema 1 encoding. These are serialized record sizes, excluding stable-map and certification overhead. Earlier development state is not migrated. Same-schema upgrades retain claims, counters, deadlines and certification.

The SNS Candid projection is pinned to DFINITY IC commit `2967c1cc9ba88fd85f196a09d05ea941bbabe830`, governance proto and neuron semantics. Production Wasm/Candid identity and wallet ingress compatibility require separate deployment verification. The test SNS is explicitly non-production.

## Admission and recovery

- Authorization attempts are limited to 10 per economic actor and 200 globally per minute. SNS qualification and existing-claim refresh have independent 200/minute budgets. Authorized new applications still obey the configured hourly, retained-Claim and subsidy limits. Terminal claims remain retained for operation identity; `max_claims` is a retained-record limit, not an active-member limit.
- Admission pause is rechecked after callbacks and before preparing a new Apply decision. Existing leases can refresh and previously prepared decisions can reconcile while admission is paused.
- A pending application with a known ineligible SNS observation is released, even after cooling began. Restored stake requires a new application and a new cooling interval. Unverifiable observations remain retryable. Active-claim repair semantics remain in the product adapter.
- `sweep_expired_claims()` releases at most 32 due claims from a stable deadline index and returns the number processed. Anyone may call it; authorized admission also runs one batch. Repeat when a full batch is returned. Pending approval deadlines, active commitment endpoints and acknowledged close fences can be swept; unacknowledged Apply/Close decisions cannot expire merely by time.
- Release advances the callback generation, clears the busy flag, releases the budget once and removes live occupancy/pending/deadline entries. Repeating a close on a Released claim returns the same terminal view. A definitively rejected Close can be reauthorized as a new immutable decision; retries of a pending Close keep its original ID.
- The first delivery sends Apply directly. Recovery queries the original receipt before any resend; a stored receipt requires no product call. The test adapter can commit a decision and lose its ACK to verify both Apply and Close recovery across membership upgrades.

## Storage and verification

Fixed configuration is cached and written only when changed. Hot rate/budget counters commit in heap at ordinary message boundaries and are persisted by `pre_upgrade`; upgrades must not skip that hook. Stable memory uses 16-page (1 MiB) allocation buckets. Internal lock/generation updates do not rewrite the certified leaf or unchanged pending/deadline indexes. `save` owns the public revision and updates the caller's Claim so responses do not need another read.

Policies use an ordered per-product/benefit effective-time index rebuilt during upgrade. A version is immutable and two policies for the same product/benefit cannot share an effective timestamp. Initial and subsequently registered products use the same identity/schema validation, and initial product IDs must be unique.

The PocketIC regression module is `tests/dmsg_integration/tests/control_plane/membership.rs`. It covers unauthorized capacity pressure, concurrent admission, bounded expiry, budget/attempt retention across upgrades, cooling invalidation, pause during an SNS callback, terminal replay, policy selection and lost decision ACKs. The test SNS also supplies a deliberately permissive adapter only for fault tests; it must never be used as a production authority.

Run the optional small-sample cycles/upgrade profile with:

```sh
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane membership_cycles_profile -- --ignored --nocapture --test-threads=1
```

The profile uses 0/8/32 cancelled historical claims plus one active claim and checks state after upgrade. It measures membership canister cycles only. It is not a capacity validation for the configured maximum of one million retained claims; certification still rebuilds across all retained claims during upgrade.

On 2026-09-23, PocketIC 16.0.0 with the repository release profile and locked dependencies compared the membership source at `e5850f4` against this change. All other canisters used the same Wasm files. Results below are millions of cycles, shown as before → after:

| Cancelled history | Admission | Activation | SNS refresh | Cached refresh | Upgrade |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 27.94 → 27.19 | 46.41 → 39.89 | 21.57 → 20.31 | 8.96 → 8.97 | 3410.84 → 3527.37 |
| 8 | 29.85 → 28.18 | 47.73 → 40.81 | 22.13 → 20.68 | 8.98 → 9.00 | 3420.04 → 3536.52 |
| 32 | 30.91 → 28.97 | 48.76 → 41.33 | 22.65 → 20.96 | 8.99 → 9.01 | 3451.03 → 3567.51 |

Stable allocation in each sample fell from 40.0625 MiB to 7.0625 MiB despite the added deadline/counter storage. Activation uses 14–15% fewer cycles and SNS refresh 6–8% fewer; cached refresh changes by less than 0.2%. Upgrade cycles increase by about 3.4%, so these optimizations do not establish cheaper upgrades or large-scale capacity. The regression and profile runs verify same-schema upgrade retention; they do not migrate the baseline instance into the new development schema.
