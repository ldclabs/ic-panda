# dMsg Canisters: Public Interfaces and Reference Implementation

English | [简体中文](dmsg_canisters_zh.md)

This directory documents the actual implementation. For public protocol specifications and language-agnostic byte rules, see [protocol/README.md](protocol/README.md); for the statement CDDL grammar, see [statements.cddl](protocol/statements.cddl). For locating internal design foundations, see [AGENTS.md](../AGENTS.md).

## Commercial Interface Additions

Added the shared `membership` canister (PANDA qualification, exclusivity, durable product decisions) and `dmsg_commerce` (cash orders, contracts, refunds, resource leases). For the concrete protocol, permissions, lifecycle, and validation boundaries, see [commerce.md](protocol/commerce.md). `dmsg_user` adds explicit commercial approvals, immutable account creation timestamps, and an independent monthly UTC execution ledger; `dmsg_cose` isolates formal signatures from safety operation budgets; `delivery` adopts versioned rate policies and v2 quote/receipt domains. Legacy handle pricing is unaffected.

The implementation separates shared membership from product commerce, for six canister roles in total. Commerce UI is wired and locally tested; TokenList adapters and production wallet/SNS acceptance remain separate deliveries; local tests do not constitute production readiness.

## Sub-crates

| Package | Responsibilities | Documentation |
| --- | --- | --- |
| dmsg_types | Public data contracts, including base signatures, ICP interfaces, and optional application protocols | [README](../src/dmsg_types/README.md) |
| dmsg_protocol | Deterministic encoding, approval construction, standard COSE verification, and CTT digests | [README](../src/dmsg_protocol/README.md) |
| dmsg_runtime | Shared stable records, certified trees, ledger helpers, and call utilities for reference implementations | [README](../src/dmsg_runtime/README.md) |
| dmsg_user | Identities, authentications, devices, recovery, root commitments, and execution approvals | [README](../src/dmsg_user/README.md) / [Candid](../src/dmsg_user/dmsg_user.did) |
| dmsg_cose | Standard signature artifacts, restricted vetKD, key origins, and execution state | [README](../src/dmsg_cose/README.md) / [Candid](../src/dmsg_cose/dmsg_cose.did) |
| dmsg_handle | Name ownership, imports, pricing, and transfers | [README](../src/dmsg_handle/README.md) / [Candid](../src/dmsg_handle/dmsg_handle.did) |
| membership | PANDA SNS qualification, cross-product occupancy, and durable entitlement decisions | [README](../src/membership/README.md) / [Candid](../src/membership/membership.did) |
| dmsg_commerce | Tier plans, cash orders, refunds, and certified resource entitlements | [README](../src/dmsg_commerce/README.md) / [Candid](../src/dmsg_commerce/dmsg_commerce.did) |
| dmsg_payment | Fixed-term escrow, deposit verification, mutually exclusive fund decisions, and disbursements | [README](../src/dmsg_payment/README.md) / [Candid](../src/dmsg_payment/dmsg_payment.did) |

The extension's cloud wire contract is documented in [cloud_zh.md](protocol/cloud_zh.md). P0 adds device commands, HTTP PoP, complete device evidence, and a real MV3/PocketIC/workerd profile interoperability probe. A1 connects account/device/recovery and root initialization; see [the account/root contract](protocol/account_root_zh.md). A2 now covers content synchronization, conflicts/tombstones and complete ciphertext export. Earlier companion descriptions must be checked against their explicit version amendments.

## Current Contracts

- Statement v3 design implements two document profiles v1, with execution approval domain `dmsg/execute/v3`; exact versions of other independent domains are specified in the protocol docs and test vectors. Incompatible with earlier experimental encodings.
- `sign` outputs `SignedArtifact { cose_sign1, cose_key }`: RFC 9052 COSE_Sign1 and public-only COSE_Key. ICP key origin is stored separately in the result's `key` description; querying the public key cannot itself serve as proof of identity or authorization.
- Internal account `AccountId` directly reuses `ic_auth_types::Xid` (12 bytes). The user canister uses a shared `XidGenerator` for synchronous atomic issuance; initialization adds a fixed `issuer_namespace`, key derivation version is 2, and fresh development instances are used.
- `get_execution_receipt` provides a certified execution leaf binding request ID, to-be-signed bytes, public key, and signature; request metadata no longer enters portable Statements.
- `get_account` returns `AccountInfo`, and payment queries return `EscrowInfo`. Internal budgets, deduplication windows, and ID allocators do not enter these views.
- `dmsg_types` contains no stable storage, certified trees, or network calls. Each canister manages its own `store.rs`, `StableCell`, and typed `StableBTreeMap`; user and COSE execution records are stored independently.
- The stable layout across all canisters employs an independent compact representation: struct fields are stored with explicit CBOR integer map keys, omitting sparse optional fields; scalars, tuples, and raw byte indices retain their original encodings. This representation exists solely within `dmsg_runtime::stable_types` and private `stable_codec.rs` in each canister; it does not alter public CBOR, signature digests, certified leaves, or Candid in `dmsg_types`. `dmsg_user` schema 5 further adopts a bounded execution retention index so normal account operations do not scan historical execution payloads; benchmark measurements and capacity limits are detailed in its README.
- Text signing signs raw UTF-8; digest signing signs RFC 9995 Hash Envelopes; issuer/subject use standard CWT text semantics; kid is variable-length; BIP340 endpoints have been removed. The browser message contract is `dmsg-extension/3`.
- Paid delivery `Quote` / `AdmissionReceipt` reside in public `profiles::delivery`; they are not base types required by every signature implementation.
- `dmsg_payment` returns `Pending` for duplicate requests during billing and lookup; internal refund/fee revisions do not repeatedly update unchanged certified leaves. Fixed-size configurations and budgets update in the heap and are written to `StableCell` during initialization and `pre_upgrade`, so upgrades must not skip this hook; fund records are written directly to stable tables. Cycles benchmarks and certified tree rebuild capacity boundaries are documented in [payment README](../src/dmsg_payment/README.md).

## Build and Verification

Requires Rust stable, wasm32-unknown-unknown target, candid-extractor 0.1.6, didc, and Node.js; PocketIC server and test crates are pinned to 16.0.0. Dependencies are governed by `Cargo.lock`.

```sh
rustup target add wasm32-unknown-unknown
make build-dmsg
pnpm --dir src/dmsg_app bindings
POCKET_IC_BIN=/path/to/pocket-ic make test-dmsg
pnpm --dir src/dmsg_app check
pnpm --dir src/dmsg_app test
```

`test-dmsg` verifies Rust tests, Clippy, Wasm compilation, Candid consistency, independent Rust/JavaScript protocol vectors, and PocketIC invocations. The test ledger supports fault injection and public minting for testing purposes only and is not in `dfx.json`. See [Integration Test Guide](../tests/dmsg_integration/README.md) for details.

## State and Guarantees

The canister types maintain their respective authorities: account approval, handle ownership, fixed-key execution, and fund finality. Account approvals commit locally before executing across canisters; administrative calls persist execution state prior to external calls. Unknown outcomes query the original request; they cannot automatically issue new requests for re-signing or refresh timestamps on unknown transfers. Device revocation, recovery disputes, root CAS, replay protection after result cleanup, mutual exclusion between settlement and refunds, and conservation of funds continue to be enforced by local state machines.

Stable layout versions are maintained by each canister's `store.rs`, tested using new instances, and do not read development state from previous schemas. `dmsg_handle` schema 4 uses `StableLog` for events, fixed-byte name locks, and principal active operation sets, while narrowing stable memory allocation buckets; cycles comparisons and capacity limits are documented in its [README](../src/dmsg_handle/README.md). Integer keys and representative sample bytes are pinned by round-trip, size threshold, `StableBTreeMap` allocation, and SHA-256 golden tests. Execution recovery after code upgrades on the same schema is covered by PocketIC. Certified trees continue using public protocol encodings and are reconstructed from stable records, so compact stable representations do not affect certified responses.

Production deployment requires creating canister IDs first, then configuring references using their respective Init arguments. `issuer_namespace` in `user` and `cose` must match and remain fixed; only a single fixed user home is currently supported. Future multi-home issuance requires registration and collision prevention for allocator fingerprints. `dmsg_cose` is initialized by its controller and verifies production keys and fingerprints; it rejects execution if public keys are not ready and cannot fall back to a test root. Production ledger/archiving, extended full approval flows, private service protocols, capacity limits, and audits require independent acceptance.

The current TSA touchpoints are RFC 9921 CTT message imprint computation and an explicit unverified token assembly helper. There is no TSA network client, CMS/X.509 trust validation, full evidence package archiving, or `anchor_snapshot` endpoint; successful regular signing does not imply an authoritative timestamp has been obtained. Channels, profiles, general grants, messages, and file bodies are not stored in these canisters, nor are periodic checkpoints written.

## Client integration increment (2026-09-22)

`SetDeviceCapabilities` uses an explicit administrator approval, preserves device keys/roles, protects the last root administrator and requires the corresponding security/root transition. The payment service now certifies public routing, signer and fee-policy records. Certification queries depend on IC batch time to avoid stale replica-cache certificates without weakening freshness checks.

Local MV3/PocketIC/workerd probes cover signing, commercial/SNS clients, paid delivery, channels, historical grants, device revocation and legacy/shared migration. They use synthetic identities, ledger funds and historical material; production origins, real legacy accounts and real-money acceptance remain separate.
