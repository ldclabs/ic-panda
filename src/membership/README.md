# membership

Product-neutral PANDA qualification, global neuron occupancy and durable product decisions. Public contracts are [commerce v2](../../docs/protocol/commerce.md), [integration membership types](../dmsg_types/src/integration_membership.rs), and the generated [Candid](membership.did).

Products supply authoritative USD bills and deadlines. Governance supplies an explicit rational PANDA rate and USD subsidy budget. The threshold is calculated with one final ceiling; it is a full subscription waiver only. The original SNS root/governance/ledger and reviewed governance module are pinned and rechecked after callbacks. Changing the reviewed module pin invalidates verification without clearing commitments.

Application authorization separates the dMsg approving account, product beneficiary and economic wallet actor. Product adapters verify their own current account or project authority. Quotes alone reserve nothing; accepted applications freeze the bill, policy, threshold and budget. First actual eligible observation starts at least 65 minutes of cooling, followed by fresh approval for the same application.

Apply success fixes `committed_until` to the original contract end, including future renewals. There is no early Close, Replace, Upgrade, Buyout or consumer-release API. Ineligibility, unlinking, product closure and terminated rights retain occupancy and budget until that end. SNS native control remains with the user. One neuron may support at most the current and contiguous next term for the same beneficiary, never two products.

Qualification leases last at most one hour and never exceed the contract. Ineligible observations stop new leases; the seven-day repair clock pauses during Unverifiable observations, without extending an old lease. Terminated rights cannot revive. Cancellation only precedes prepared Apply; unknown Apply is reconciled through its original decision/receipt and is never expired as an unused request.

`sweep_panda_commitments` releases bounded due records once, preserving a committed next-term reference. `panda_operations` is a bounded, permission-filtered operational query. Admission pause still allows original-operation recovery, qualification checks and expiry. Stable state and certified leaves survive same-schema upgrades; development v1 schemas are not migrated.

Actual Wasm regression cases live in [commerce.rs](../../tests/dmsg_integration/tests/control_plane/commerce.rs). They include cooling/reapproval, no early exit, cross-product exclusivity, contiguous terms, lost Apply ACK, module-pin changes during SNS reads, terminated rights and budget/occupancy release. The [account adapter](../../examples/dmsg-account-product/README.md) uses the same public product protocol as TokenList. Local SNS, ledger funds and identities do not establish production wallet compatibility. Previous v1 cost profiles do not describe this implementation.
