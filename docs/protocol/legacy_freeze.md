# Legacy service freeze and snapshot contract

English | [简体中文](legacy_freeze_zh.md)

First written 2026-09-22; reorganized 2026-10-24. This document describes the legacy-service freeze and evidence contract. The client's built-in source channels, name identity and cutover are in [production.toml](../../src/dmsg_client/production.toml), which lists no OSS bucket. Actual deployment status requires inventory evidence and cannot be inferred from this document or source code.

## Messages, profiles, channels and name identity

The four legacy canisters store freeze state in separate stable memory 200, preserving their original business storage layouts. `admin_legacy_drain(cutover_id)` enters Draining; ordinary and administrator business entry points reject new writes. Already-started asynchronous operations have persistent tickets and recheck the current phase, boot and time after cross-canister returns. Original business arguments are retained for reconciliation of unknown outcomes and are readable only by the controller.

Transport or response-decoding failure is not treated as proof that nothing happened: unknown tickets survive the parent request's return. Active calls cannot be forcibly cleared. Completed calls with unknown results and calls interrupted by upgrades require the controller to call `admin_legacy_resolve(ticket,evidence_digest)` with actual reconciliation material. This digest records an administrative reconciliation, not an automatically generated ledger proof. Inventory and ledger evidence must also be retained independently.

`admin_legacy_seal(cutover_id)` enters ReadOnly only from Draining after all tickets are resolved and baseline checks are complete. The first upgrade from an old module without ticket tracking requires `admin_legacy_acknowledge_baseline(evidence_digest)`; new tracking cannot retrospectively prove the absence of pre-upgrade calls. There is no interface to reopen legacy business. Once frozen, ordinary upgrade configuration arguments to the same module cannot change its business configuration.

Existing reads, original `my_iv`, channel download tokens, name-identity `sign_in/get_delegation` and necessary name-delegation revocation remain available. Frozen name-identity roles are stored separately in stable memory 201; later login times or narrower delegations do not rewrite them. Snapshot support does not overwrite existing authentication and name-event certified_data trees.

## Independently verifiable paginated snapshots

`legacy_snapshot(scope,after)` is a bounded update. The caller retains its `request_status` IC certificate and original request ID. `@dmsg/legacy`'s `collectSnapshot` / `verifySnapshotProof` verify the trusted IC root, target canister, request ID for the method/arguments/caller, certified reply, ReadOnly state, scope, cursor, count and rolling digest. Historical certificates are evidence of the frozen state, not current authority.

Scopes include public names/frozen roles and profiles, channels and messages readable under the original caller's permissions. Each page contains at most 64 records and 128 KiB; missing message slots are explicit None values. Names and other ordered collections have fixed ordering; original profile HashMaps are converted to ordered representations. Each row preserves original Candid bytes; source version, cutover and scope are committed. The name-event tip is additional context and cannot replace the complete name-map digest.

## Boundaries of the proposed COSE and OSS extensions

The COSE candidate starts from the deployed-use v0.9.3 baseline and adds `admin_legacy_drain_namespace` / `admin_legacy_seal_namespace`. It closes data/permission writes and new business signatures only for the specified namespace; asynchronous business signatures recheck freeze state before returning. Original settings, keyId, ownership, derivation paths, roots and context remain unchanged, and ECDH/vetKD recovery stays available. Freeze retains setting/historical-payload digests. Global root configuration still used by legacy recovery must not be silently changed.

The OSS candidate starts from v1.2.3 and adds `admin_legacy_drain_folder` / `admin_legacy_seal_folder`. Root folder 0 is not an implicit freeze-entire-bucket argument. Storage entry points reject writes, moves and deletions for the folder and its descendants, as well as moving/deleting an ancestor containing a frozen folder. Existing write tokens do not bypass this restriction; other folders retain their permissions.

Under original read permissions, `legacy_folder_manifest` returns sealed-file metadata, each actual 256 KiB transport fragment's digest and length, missing fragments, the actual ciphertext digest and completeness flag. It does not label OSS fragments as independently authenticated AEAD chunks. Authorized clients still decrypt the whole file. Original request evidence containing tokens belongs in encrypted material, not public attachments or logs.

These are proposed freeze capabilities of dependency services, not proof of enablement in current client configuration. No frozen OSS bucket is built in, so frozen comparisons of archives with attachments may remain incomplete. Ordinary query observations must not replace certified freeze proofs.

## Operational material and verification

`src/dmsg_legacy/scripts/legacy-freeze-plan.mjs` generates staged, module-hash-bound Candid controller payloads from the target inventory, completed baseline-check file and cutover ID. It does not sign, submit, upgrade or stop writes. Deployment, control authority, in-flight operations and evidence must be checked before use. A generated plan does not establish production execution; execution records must be retained separately.

The four old canisters are controlled only by SNS root; SNS governance executes freeze steps as generic functions. Each `admin_legacy_*` has a `validate_admin_legacy_*` query with identical arguments: it simulates the same transition on a copy without writing and returns proposal text. If in-flight writes prevent sealing, the error lists operation numbers, methods, start times and whether they were interrupted or have unknown results, but not callers or arguments. `admin_legacy_pending` remains restricted to controller and governance.

`legacy-freeze-inputs.mjs` drafts those two inputs from the `legacy-inventory.mjs` inventory and canister release artifacts. Targets bind release-module SHA-256 hashes checked against the release's `.txt` files. Baseline `unresolved` entries list each source's outstanding upgrade and in-flight checks; the planner refuses to generate a plan until these are cleared and `evidence_ref` is filled. COSE and OSS targets are excluded. `legacy-name-proofs.mjs` uses a one-time Ed25519 identity to collect and verify `Names` and `Authorities` snapshot pages, producing input for name import; it calls only `legacy_snapshot`.

Local tests use Wasm matched to public releases in I0: old IVs, profiles and messages survive upgrades of all four services; real cross-canister waits block premature sealing; Chrome checks multipage snapshots and tampering cases. Additional tests compare COSE ECDH/VetKey/wrapping before and after, block old OSS write tokens and confirm other folders remain writable. Simulated ledgers create test data only; no real funds are involved.

Synthetic tests cannot replace authorized real-user recovery in all three modes, production controller/in-flight inventory, independent release checks or exit acceptance. The old minter is not wholly disabled: freezing source business prevents new PoL events, while historical entitlements, settlement and whether minting stops remain separate governance decisions.

`Attestation { digest, expires_at }` is an explicit migration-approval scope: only in ReadOnly and for a non-anonymous caller, it returns that caller, a 32-byte intent digest and a deadline no more than seven days away. It changes no old account, channel, ACL or key. The client must display the complete plan corresponding to the digest and obtain a user click; the service must independently verify the frozen ACL, never interpreting any caller's proof as administrator authority. Calls and certification still use `legacy_snapshot` ingress proofs.
