# Legacy service freeze and snapshot contract

English | [简体中文](legacy_freeze_zh.md)

This document describes the legacy-service freeze and evidence contract. The client's built-in source channels, name identity and cutover are in [production.toml](../../src/dmsg_client/production.toml), which lists no OSS bucket. Actual deployment status requires inventory evidence and cannot be inferred from this document or source code.

## Messages, profiles, channels and name identity

The four legacy canisters store freeze state in separate stable memory 200, preserving their original business storage layouts. `admin_legacy_drain(cutover_id)` enters Draining; ordinary and administrator business entry points reject new writes. Already-started asynchronous operations have persistent tickets and recheck the current phase, boot and time after cross-canister returns. Original business arguments are retained for reconciliation of unknown outcomes and are readable only by the controller.

Transport or response-decoding failure is not treated as proof that nothing happened: unknown tickets survive the parent request's return. Active calls cannot be forcibly cleared. Completed calls with unknown results and calls interrupted by upgrades require the controller to call `admin_legacy_resolve(ticket,evidence_digest)` with actual reconciliation material. This digest records an administrative reconciliation, not an automatically generated ledger proof. Inventory and ledger evidence must also be retained independently.

`admin_legacy_seal(cutover_id)` enters ReadOnly only from Draining after all tickets are resolved and baseline checks are complete. The first upgrade from an old module without ticket tracking requires `admin_legacy_acknowledge_baseline(evidence_digest)`; new tracking cannot retrospectively prove the absence of pre-upgrade calls. There is no interface to reopen legacy business. Once frozen, ordinary upgrade configuration arguments to the same module cannot change its business configuration.

Existing reads, original `my_iv`, channel download tokens, name-identity `sign_in/get_delegation` and necessary name-delegation revocation remain available. Frozen name-identity roles are stored separately in stable memory 201; later login times or narrower delegations do not rewrite them. Snapshot support does not overwrite existing authentication and name-event certified_data trees.

## Independently verifiable paginated snapshots

`legacy_snapshot(scope,after)` is a bounded update. The caller retains its `request_status` IC certificate and original request ID. `@dmsg/legacy`'s `collectSnapshot` / `verifySnapshotProof` verify the trusted IC root, target canister, request ID for the method/arguments/caller, certified reply, ReadOnly state, scope, cursor, count and rolling digest. Historical certificates are evidence of the frozen state, not current authority.

Scopes include public names/frozen roles and profiles, channels and messages readable under the original caller's permissions. Each page contains at most 64 records and 128 KiB; missing message slots are explicit None values. Names and other ordered collections have fixed ordering; original profile HashMaps are converted to ordered representations. Each row preserves original Candid bytes; source version, cutover and scope are committed. The name-event tip is additional context and cannot replace the complete name-map digest.

## External COSE and OSS services

COSE namespace and OSS folder freeze capabilities belong to their own repositories and deployment versions; they are not established by this repository. No OSS bucket is built into the client configuration, so archive attachments may lack frozen comparison evidence. Ordinary queries are not certified freeze proofs, and legacy recovery still depends on the original derivation paths, roots and context.

When reading `legacy_folder_manifest`, the client verifies metadata, lengths and hashes of actual 256 KiB transport fragments, missing fragments and the complete ciphertext hash. Transport fragments are not independent AEAD blocks; authorized clients decrypt the complete file. Original evidence containing tokens belongs in controlled encrypted storage.

## Operational material and verification

`src/dmsg_legacy/scripts/legacy-freeze-plan.mjs` generates staged, module-hash-bound Candid controller payloads from the target inventory, completed baseline-check file and cutover ID. It does not sign, submit, upgrade or stop writes. Deployment, control authority, in-flight operations and evidence must be checked before use. A generated plan does not establish production execution; execution records must be retained separately.

Management entry points check controllers or the configured governance identity; actual controllers require inventory verification. SNS-managed instances can execute freeze steps through governance generic functions. Each `admin_legacy_*` has a `validate_admin_legacy_*` query with identical arguments: it simulates the same transition on a copy without writing and returns proposal text. If in-flight writes prevent sealing, the error lists operation numbers, methods, start times and whether they were interrupted or have unknown results, but not callers or arguments. `admin_legacy_pending` remains restricted to controller and governance.

`legacy-freeze-inputs.mjs` drafts those two inputs from the `legacy-inventory.mjs` inventory and canister release artifacts. Targets bind release-module SHA-256 hashes checked against the release's `.txt` files. Baseline `unresolved` entries list each source's outstanding upgrade and in-flight checks; the planner refuses to generate a plan until these are cleared and `evidence_ref` is filled. COSE and OSS targets are excluded. `legacy-name-proofs.mjs` uses a one-time Ed25519 identity to collect and verify `Names` and `Authorities` snapshot pages, producing input for name import; it calls only `legacy_snapshot`.

Test entry points are [legacy_fixture](../../tests/dmsg_integration/tests/legacy_fixture.rs), [dependency regressions](../../tests/dmsg_integration/tests/legacy_dependencies.rs) and [snapshot-gateway tests](../../src/dmsg_legacy/tests/snapshot-gateway.test.ts). They cover retained state, in-flight cross-canister calls, snapshot pagination and tampering rejection. This documentation review did not rerun these integration tests.

Synthetic tests cannot replace authorized real-user recovery in all three modes, production controller/in-flight inventory, independent release checks or exit acceptance. The old minter is not wholly disabled: freezing source business prevents new PoL events, while historical entitlements, settlement and whether minting stops remain separate governance decisions.

`Attestation { digest, expires_at }` is an explicit migration-approval scope: only in ReadOnly and for a non-anonymous caller, it returns that caller, a 32-byte intent digest and a deadline no more than seven days away. It changes no old account, channel, ACL or key. The client must display the complete plan corresponding to the digest and obtain a user click; the service must independently verify the frozen ACL, never interpreting any caller's proof as administrator authority. Calls and certification still use `legacy_snapshot` ingress proofs.
