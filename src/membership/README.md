# membership

Product-neutral PANDA SNS qualification, global neuron occupancy, fixed policy snapshots and durable adapter decisions. No product storage quotas or cash ledger accounting live here. See the [public commerce contract](../../docs/protocol/commerce.md), [types](../dmsg_types/src/membership.rs) and [Candid](membership.did).

Configure one shared instance, its SNS root/governance/PANDA ledger, registered adapters, policies and explicit subsidy budget. Governance must verify the SNS configuration before admitting claims. Governance calls use the fixed SNS principal. The minimal adapter exposes `authorize_membership_intent`, `authorize_membership_close`, `apply_membership_decision`, and `get_membership_decision`.

State is stored before external calls. Prepared decisions reconcile by their immutable ID. Occupancy includes uncompleted product application/close commitments; a timeout does not prove absence of benefit. New applications require a fresh account approval after cooling. Refresh is on demand, bounded globally, and unavailable to arbitrary unrelated callers. An upgrade rebuilds certification from stable claim records.

Claim storage schema 2 uses dedicated CBOR representations with integer field keys throughout requests, policies, views, decisions and receipts, including enum payload fields. Absent optional fields are omitted. Config keeps its schema 1 envelope; public protocol bytes and certification remain separate from storage encoding. Unit-test samples shrink from 1,199 to 526 bytes for a checking claim and from 2,465 to 1,260 bytes for an active claim with a decision and receipt, compared with the previous schema 1 encoding. These are serialized record sizes, excluding stable-map and certification overhead. Previous development Claim records are not migrated.

The SNS Candid projection is pinned to DFINITY IC commit `2967c1cc9ba88fd85f196a09d05ea941bbabe830`, governance proto and neuron semantics. Production Wasm/Candid identity and wallet ingress compatibility require separate deployment verification. The test SNS is explicitly non-production.
