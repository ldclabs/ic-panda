# dMsg Canisters: Public Interfaces and Reference Implementation

English | [简体中文](dmsg_canisters_zh.md)

This directory documents the actual implementation. For public protocol specifications and language-agnostic byte rules, see [protocol/README.md](protocol/README.md); for the statement CDDL grammar, see [statements.cddl](protocol/statements.cddl). For locating internal design foundations, see [AGENTS.md](../AGENTS.md).

## Commercial Interface Additions

Added the shared `membership` canister (PANDA qualification, exclusivity, durable product decisions) and `dmsg_commerce` (cash orders, contracts, refunds, resource leases). For the concrete protocol, permissions, lifecycle, and validation boundaries, see [commerce.md](protocol/commerce.md). `dmsg_user` adds explicit commercial approvals, immutable account creation timestamps, and an independent monthly UTC execution ledger; `dmsg_cose` isolates formal signatures from safety operation budgets; `delivery` adopts versioned rate policies and v2 quote/receipt domains. Legacy handle pricing is unaffected.

The implementation separates shared membership from product commerce, for six canister roles in total. Commerce UI is wired and locally tested; TokenList project and independent account adapters are implemented; production wallet/SNS acceptance remains separate; local tests do not constitute production readiness.

COSE schema 8 separates in-flight calls, returned unknown outcomes, and terminal results. Unknown results keep their request and commercial hold without blocking cleanup of later terminal records. Unsent management calls return a failure with zero cost and release their budget reservation. Cleanup discards encoded result bodies without decoding them; initialization and upgrades cache fixed signing prefixes. The entry point authenticates the fixed user home. Public Candid, derivation paths, and key identity are unchanged. `cycles_cost_upper_bound` remains a conservative management-call bound, not an actual bill. See the [COSE implementation and measurements](../src/dmsg_cose/README.md).

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
| dmsg_directory | Certified HTTP publication of Agent Delegation principal documents | [README](../src/dmsg_directory/README.md) / [Candid](../src/dmsg_directory/dmsg_directory.did) |

The extension's cloud wire contract is documented in [cloud_zh.md](protocol/cloud_zh.md). P0 adds device commands, HTTP PoP, complete device evidence, and a real MV3/PocketIC/workerd profile interoperability probe. A1 connects account/device/recovery and root initialization; see [the account/root contract](protocol/account_root_zh.md). A2 now covers content synchronization, conflicts/tombstones and complete ciphertext export. Earlier companion descriptions must be checked against their explicit version amendments.

## Current Contracts

- Statement v3 design implements two document profiles v1, with execution approval domain `dmsg/execute/v3`; exact versions of other independent domains are specified in the protocol docs and test vectors. Incompatible with earlier experimental encodings.
- `sign` outputs `SignedArtifact { cose_sign1, cose_key }`: RFC 9052 COSE_Sign1 and public-only COSE_Key. ICP key origin is stored separately in the result's `key` description; querying the public key cannot itself serve as proof of identity or authorization.
- Internal account `AccountId` directly reuses `ic_auth_types::Xid` (12 bytes). The user canister uses a shared `XidGenerator` for synchronous atomic issuance; initialization adds a fixed `issuer_namespace`, key derivation version is 2, and fresh development instances are used.
- `get_execution_receipt` provides a certified execution leaf binding request ID, to-be-signed bytes, public key, and signature; request metadata no longer enters portable Statements.
- `get_account` returns `AccountInfo`, and payment queries return `EscrowInfo`. Internal budgets, deduplication windows, and ID allocators do not enter these views.
- `dmsg_types` contains no stable storage, certified trees, or network calls. Each canister manages its own `store.rs`, `StableCell`, and typed `StableBTreeMap`; user and COSE execution records are stored independently.
- The stable layout across all canisters employs an independent compact representation: struct fields are stored with explicit CBOR integer map keys, omitting sparse optional fields; scalars, tuples, and raw byte indices retain their original encodings. This representation exists solely within `dmsg_runtime::stable_types` and private `stable_codec.rs` in each canister; it does not alter public CBOR, signature digests, certified leaves, or Candid in `dmsg_types`. `dmsg_user` schema 7 further adopts a bounded execution retention index so normal account operations do not scan historical execution payloads; benchmark measurements and capacity limits are detailed in its README.
- Text signing signs raw UTF-8; digest signing signs RFC 9995 Hash Envelopes; issuer/subject use standard CWT text semantics; kid is variable-length; BIP340 endpoints have been removed. The browser message contract is `dmsg-extension/4`.
- Paid delivery `Quote` / `AdmissionReceipt` reside in public `profiles::delivery`; they are not base types required by every signature implementation.
- `dmsg_payment` returns `Pending` for duplicate requests during billing and lookup; internal refund/fee revisions do not repeatedly update unchanged certified leaves. Bounded configurations and budgets update in the heap and are written to `StableCell` during initialization and `pre_upgrade`, so upgrades must not skip this hook; fund records are written directly to stable tables. Cycles benchmarks and certified tree rebuild capacity boundaries are documented in [payment README](../src/dmsg_payment/README.md).

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

Stable layout versions are maintained by each canister's `store.rs`, tested using new instances, and do not read development state from previous schemas. `dmsg_handle` schema 7 uses `StableLog` for events and holds name and account locks only while a registration charge is in flight or unknown, so requests that have not started charging never reserve names. Frozen legacy records are plain lookups rechecked by claims and stay out of the heap certification tree; only the snapshot commitment and active names are certified. It keeps fee maintenance for new registrations and small allocation buckets; cycles comparisons and capacity limits are documented in its [README](../src/dmsg_handle/README.md). Integer keys and representative sample bytes are pinned by round-trip, size threshold, `StableBTreeMap` allocation, and SHA-256 golden tests. Execution recovery after code upgrades on the same schema is covered by PocketIC. Certified trees continue using public protocol encodings and are reconstructed from stable records, so compact stable representations do not affect certified responses.

Production deployment requires creating canister IDs first, then configuring references using their respective Init arguments. Every user home and handle/cose/payment/directory must share one fixed environment and `issuer_namespace`. Services route each account to the home whose allocator fingerprint its ID carries; a new home is registered with each service's `admin_add_user_home`, and fingerprints may not collide. `dmsg_cose` is initialized by a controller or governance and verifies production keys and fingerprints; it rejects execution if public keys are not ready and cannot fall back to a test root. Production ledger/archiving, extended full approval flows, private service protocols, capacity limits, and audits require independent acceptance.

Signed artifacts may carry an opaque RFC 9921 CTT token at unprotected header 270; verification only bounds its size and does not check its trust. There is no TSA network client, CMS/X.509 trust validation, full evidence package archiving, or `anchor_snapshot` endpoint; successful regular signing does not imply an authoritative timestamp has been obtained. Channels, profiles, general grants, messages, and file bodies are not stored in these canisters, nor are periodic checkpoints written.

## Client integration increment (2026-09-22)

`SetDeviceCapabilities` uses an explicit administrator approval, preserves device keys/roles, protects the last root administrator and requires the corresponding security/root transition. The payment service now certifies public routing, signer and fee-policy records. Certification queries depend on IC batch time to avoid stale replica-cache certificates without weakening freshness checks.

Local MV3/PocketIC/workerd probes cover signing, commercial/SNS clients, paid delivery, channels, historical grants, device revocation and legacy/shared migration. They use synthetic identities, ledger funds and historical material; production origins, real legacy accounts and real-money acceptance remain separate.

The shared runtime stores compact representations directly, avoiding the extra domain-object clone before map writes. Commercial reservations and fee policies also use integer keys. That increment used user schema 6 and payment schema 5; later versions are recorded by each service and the increments below. Certified batches enforce the 256 KiB limit on the complete successful Candid response, including the certificate and envelope.

2026-09-23 payment uses schema 6: authorization attempts and successful admissions have separate budgets; default certified queries select the currently effective fee policy. Controllers maintain the expected network fee within the fixed ceiling; accepted quotes and prepared transfers remain frozen. Budget changes do not recertify configuration, historical policies use compact encoding, and stable allocation uses 1 MiB buckets. Transfers expose bounded `last_failure` diagnostics. See the payment README for capacity and cycles measurements.

## 2026-09-29 Agent Delegation Additions

Added `dmsg_directory`, which publishes principal documents as described in [agent_zh.md](protocol/agent_zh.md). User schema 8 adds a principal table (memory 8) and `principal_updated_at`; `SecuritySnapshot` is schema 3. Account commands add principal enablement and hosted-controller registration, retirement, compromise and renaming; new methods are `register_controller`, `sign_agent_event`, `publish_principal` and `get_principal`. The default `SensitivePolicy` and `SetPolicy` allow `AgentController` (at most four purposes). COSE adds `KeyPurpose::AgentController`, `ExecutionKind::AgentEvent` and `ExecutionOutput::AgentSignature`; agent events count against the formal-signature allowance and budgets and have no certified execution receipt.

`dmsg_protocol` depends on crates.io `agent-protocols =0.10.0` (`default-features = false`) for strict I-JSON, JCS and Agent Delegation validation. In a local release build the user Wasm grew from 3,771,117 to 4,244,709 bytes and COSE from 2,165,748 to 2,412,506 bytes; the directory is 1,496,953 bytes (before ic-wasm shrink). Directory capacity and upgrade rebuild cost are not measured, and home migration is not implemented.

## 2026-10-02 User Review Fixes

The `dmsg_user` development layout moves to schema 9; `SecuritySnapshot` stays at schema 3. `complete_recovery(account_id, request_id)` binds completion to the original request and keeps the latest completion receipt; when the 1024-entry binding table is full, expired bindings are reclaimed. Fixed-service callers, third-party approval capacity and agent-event authorization are checked before awaits and rechecked after callbacks. The new public `prune_executions(account_id)` examines at most the 64-entry retention index, removes expired terminal results and returns the count, keeping pending executions, replay guards and historical settlement. `get_execution_receipt` can return a certified absence proof, and an exact replay of a cleaned execution request returns `ResultExpired`.

Execution authorization is split into a read-only precheck and a synchronous commit that reuse immutable parsing results; controller registration no longer clones the whole account up front. The certification tree keeps the batch rebuild, which measured lower memory use, and publishes the root once; the per-leaf streaming candidate was not kept. See the [user README](../src/dmsg_user/README.md) for boundaries, regressions and same-configuration cycles comparisons. Development interfaces and extension bindings were updated together and old experimental layouts are not read; production deployment and large-scale capacity still require independent acceptance.

## 2026-10-02 Handle Review Fixes

Schema 7 uses an in-memory call guard to distinguish live charges from `Charging` operations left by an upgrade. Orphaned operations retry the original ledger arguments; reconciliation shares the guard and retries preserve prior uncertainty. Claims and transfers retain exact request receipts, so later ownership changes do not alter a claim replay. Read-only configuration paths borrow decoded state, while mutations still persist synchronously; operation indexes use fixed 32-byte keys. Public Candid and certified leaves are unchanged. Recovery, evidence validation, concurrency regressions and active-name capacity measurements are described in the [handle README](../src/dmsg_handle/README.md).

## 2026-10-02 Payment Review Fixes

Payment uses schema 8: separate bounded caller/global budgets for authorization, ledger reads and payouts; settlement reserves cover approved fee ceilings. Refunds combine up to 32 same-source deposits and released reserve, with deposit pagination and refund previews. Retained-order capacity only stops new admissions; deduplication keys remain and zero open counters are removed. See the [payment README](../src/dmsg_payment/README.md) for measurements and real-asset validation limits.

## 2026-10-02 Directory Review Fixes

Directory schema 2 persists the document SHA-256 digest, reuses it for publication and HTTP queries, moves the loaded response body and streams stable records into the certification tree on upgrade. Home and directory share a conservative 64 KiB document budget with space reserved for later retirement, compromise and maximum names; oversized registrations fail before the home commits. Public configuration URLs are bounded, and HTTP routing follows the certification library's path normalization while canonical principal resolution remains exact. Public Candid is unchanged; development schema 1 is rejected.

Targeted unit/PocketIC regressions and the reproducible [directory profile](../src/dmsg_directory/README.md) cover these changes. In a local 35,360-byte document sample, replicated HTTP-query cycles fall by 31.87%; for 130 records, post-upgrade Wasm memory falls from 6,356,992 to 1,966,080 bytes and upgrade cycles by 15.14%. These samples do not establish maximum capacity or gateway throughput. The complete dMsg validation script was not run for this increment.


## 2026-10-03 commerce review fixes

Commerce development schema 5 shares a live guard across transfer dispatch, reconciliation and revision, while recovering interrupted `InFlight` legs with their frozen arguments. Checkout admission exactly matches authority-published historical price snapshots. Catalog boundaries end old resource leases, and Free execution units follow the effective policy timeline. Published resource fields remain intact; subject working history drops completed earlier months and redundant call state.

Merchant access, reader-indexed order/transfer pages, complete deposit cursor pagination and per-caller recovery budgets are implemented. `checkout_store.rs` owns stable records and certification, and the pure model classifies ledger replies. One-MiB memory buckets and compact order/transfer records reduce storage overhead. Bounded terminal archival keeps the input digest, quote, balances, receipt, reader indexes and deposit deduplication; late deposits restore the original refund route, while unknown results and money obligations remain live. See the [commerce README](../src/dmsg_commerce/README.md) for reproducible validation and measured limits.

## 2026-10-06 handle governance and revenue collection

Handle development schema 8 adds a fixed SNS `governance` principal to `HandleInit`. Snapshot import and sealing, `update_ledger_fee` and the new `admin_collect_token` accept either a controller or governance. Each administrative method has a `validate_*` query with the same arguments that dry-runs it against current state and renders the proposal text, so it can be registered as an SNS generic-function validator. `import_legacy_handles` now takes the snapshot position `offset`, so a batch executed out of order is rejected instead of leaving a gap. `admin_collect_token` collects registration revenue from the handle canister's default account. Names of 7–20 bytes now cost 100 PANDA, so the whole price table matches the live legacy `ic_message` prices. The certification tree now holds only name leaves, and the unused `snapshot_certified` is removed; measured upgrade instructions raise the per-instance active-name limit from 100,000 to 150,000, about 55% of the 300B upgrade limit. Deployment, governance and migration steps are in the [handle README](../src/dmsg_handle/README.md).

## 2026-10-06 handle: ten million names and multiple user homes

Handle development schema 9 lets the single global name registry hold ten million names. The certification tree now lives in stable memory: names fall into 2^20 buckets by `handle_bucket`, the bucket bits form a binary label trie, a write rehashes one bucket and one path, and an upgrade only republishes the root. A name's proof path is now its 20 bucket-bit labels followed by the name. `HandleInit` replaces the single `home_user` with `environment`, `issuer_namespace` and an append-only `user_homes`; each authorization check goes to the user home named by the account ID's allocator fingerprint, and a transfer across homes checks each side at its own home. `admin_add_user_home` and its validator are new. Stable memory uses 8 MiB buckets and can address 256 GiB. Measured from 1,000 to ten million names, an upgrade takes about 1.15 million instructions and a 64-name certified response stays under 81 KB; the active-name limit rises to ten million. Data and procedures are in the [handle README](../src/dmsg_handle/README.md).

## 2026-10-06 multiple user homes and SNS governance

Like handle, cose, directory and payment now route by the account ID's allocator fingerprint. `CoseInit` replaces `initial_home_user` with an append-only `user_homes`, and `execute`/`get_execution` accept only the home that allocated the account. `PaymentInit` replaces `home_user` with `environment`, `issuer_namespace` and `user_homes`; opening an escrow sends `verify_payment_offer` to the recipient account's home, and the certified configuration leaf moves to schema 2 with `user_homes`. Commerce allows up to 64 user homes; membership takes user homes from commerce app registrations and needs no change. A new `dmsg_user` shard only needs registering with each service and a new app-registration version from `register_integration_app` that lists it.

User, cose and directory gain a fixed `governance`. Administrative methods of all seven canisters (user, handle, cose, directory, payment, commerce, membership) accept controllers and governance, and each has a same-argument `validate_*` query that runs the method's checks against current state and renders the proposal text, so it can be registered as an SNS generic-function validator. New administrative methods are `admin_add_user_home` on each routing service, `admin_set_account_limits` (account cap and daily new-account quota) on user, `admin_set_daily_budget` on cose and `admin_set_custom_domains` on directory; upgrades of these two canisters no longer read arguments, and configuration changes only through administrative methods. Cose `prune_executions` becomes public maintenance, like the user canister's. Stable layouts: user schema 10, cose schema 9, directory schema 3, payment schema 9.

The frontend still connects to one `dmsg_user`. Client routing by account fingerprint, authentication-principal uniqueness across homes and migration of existing accounts are not implemented. See each canister README for the verification scope.
