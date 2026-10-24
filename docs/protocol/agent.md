# dMsg and Agent Delegation: self-held controllers and principal publication

English | [简体中文](agent_zh.md)

This document describes only public canister interfaces and byte rules. The normative protocol is the pinned [agent-protocols](https://github.com/ldclabs/agent-protocols) version: `v0.12.6`, specification source commit `13257c1`, with Rust and TypeScript SDK `agent-protocols 0.12.6`.

## Roles

| Role | Implementation | Responsibility |
| --- | --- | --- |
| Principal host | `dmsg_directory` | Serves the principal document at `principal_origin/<account_id>` through ICP-certified HTTP |
| Authoritative state | `dmsg_user` (the account's home) | Public profile updates and controller registration, retirement, compromise marking and renaming |
| Controller key | Ordinary Ed25519 key entry in the client vault | Readable by root-holding devices; proves possession to the home at registration; signs events locally |
| Delegation service | The document's `delegation_service` origin, the dmsg-cloud relay API origin (`https://api.dmsg.net`) | Accepts Agent Delegation 1.0 events, stores accepted records, and serves credentials and queries through endpoints in its discovery document |

The user home is the linearization point for controller changes: once retirement commits, the key cannot produce accepted new events regardless of directory publication timing. The directory publishes only home-pushed state and does not authorize actions.

## Principals and controllers

- Principal ID is `principal_origin + "/" + canonical Xid text` and never changes. `principal_origin` is fixed in `UserInit` and `DirectoryInit` and cannot be modified.
- Account commands require `dmsg/account/v2` approval by an administrator device with RootManage:
  - `EnablePrincipal { principal_type }` creates and publishes an empty principal. An ordinary user's first `UpdatePrincipal` may also initialize one, without registering a controller first.
  - `UpdatePrincipal { document }` accepts official `PrincipalDocument` UTF-8 JSON, at most 64 KiB. It replaces only `type/name/description/avatar_url/links/extra`; `id` must match the account and `updated_at` must equal the current value, or 0 initially. The service generates `controllers`, `retired_controllers`, `aliases`, service addresses and the new timestamp; forms cannot change them. It requires RootManage administrator-device approval like other account commands.
  - `RegisterController { generation, public_key, name, delegation, proof }`: `proof` is the controller key's Ed25519 signature over `controller_pop_message(home, account, generation, delegation, approval.request_id)` in domain `dmsg/controller-pop/v1`. The home verifies before committing. Generations start at 1, increase consecutively and are never reused. There is no `register_controller` entry point or COSE call.
  - `RetireController { generation }`、`MarkControllerCompromised { generation, invalid_from }`、`RenameController { generation, name }`。
- Principal changes advance `account_version`, not `security_epoch`. Each increments `version` and sets `updated_at = max(now, previous + 1)`; new `valid_from` and retirement `retired_at` use that value.
- Limits: 8 current controllers and 32 lifetime records; retired records are retained. Restricted authority has at most 8 scopes of 64 bytes each and 4 relying parties (HTTPS origins or Agent IDs) of 256 bytes each. Names are 1..64 bytes.
- Home and directory share a conservative 64 KiB document budget, including maximum configured URL lengths, all controllers' future retirement/compromise fields and maximum names. Registrations exceeding it return `QuotaExceeded` before home commit.
- Anyone may call `publish_principal(account_id)` to idempotently push current state to the directory; mutation commands attempt it immediately after local commit. `get_principal` returns `PrincipalInfo` with current state and published version. `published_version < state.version` means the change has not yet taken effect in the published document.

### Controller registration input

The desktop offers a default dMsg signing-key entry point, but the client still generates and stores its private key in the vault; the production directory's `source` is `https://dmsg.net`. Users explicitly select unrestricted authority and approve registration; the default entry point grants no Agent capabilities automatically. Advanced options register custom restricted keys; integrating applications define their scopes, and format validation does not prove application support.

`agent/register` requires either explicit `unrestricted: true` or both nonempty `scopes` and `audiences`, never a mixture; omitted fields no longer mean `*`. CLI equivalents are `--unrestricted` or `--scope` with `--audience`. Before approval, desktop forms and Rust clients reject duplicate/empty entries, whitespace or wildcards in scopes, invalid relying parties and excess size: at most 8 scopes of 64 UTF-8 bytes each, 4 relying parties of 256 UTF-8 bytes each, and a name of at most 64 UTF-8 bytes without control characters. Key rotation uses the same validation and explicit authority selection.

## Local signing

The client (desktop daemon, `src/dmsg_client`'s `agent` module) constructs, signs and submits events locally, without user-home or COSE execution:

1. The controller private key is a vault entry with object ID `SHA256(CBOR(["dmsg/agent-controller/1", account, generation]))`, protected at the content-root level and synced under that root. Compromise handling matches content-root handling: revoke the device, rotate the root and retire the controller.
2. Before signing, verify that the generation is current, the local vault public key matches registration and the device has `FormalApprove` (a client rule). `created_at = max(now, valid_from)` and nonce is the locally recorded `max(last + 1, created_at)`.
3. Sign `SHA3-256(JCS(event))` with Ed25519. Envelope `hash` and `signature` are unpadded base64url. Journal the envelope encrypted before submission; 5xx/408/429 retain `signed` for identical resubmission, while other 4xx responses are terminal and require a new signature.
4. The service uses SDK `validate_delegation_acceptance` against the certified document to check scopes, relying parties and `expires_at` (at most 366 days). Revocation and replacement are checked on acceptance under the ceilings in Delegation section 5: a restricted controller may manage only credentials whose latest grant's scopes and relying parties fit its ceiling; `"*"` may manage all. Credential state does not record which key issued it. A replacement key whose ceiling covers an old credential can replace or revoke that credential. Before creating approval, the client selects the newest current controller covering the grant; for revocation/replacement it uses the service-stored original scope and audiences, and replacement scope cannot exceed them. An explicitly selected generation must also cover them or return `Forbidden`, preventing a later `delegation_ceiling_exceeded` after approval and signing.
5. Delegation IDs are `<account_id>.<suffix>`, with the principal's canonical account text and a random 128-bit suffix. Replacement/renewal preserves the ID: the client issues only for the same subject, within the original service-stored scopes and relying parties, with a recalculated expiry. The service routes credential reads by the prefix and rejects submissions not starting with this account ID plus `.`.

## Directory

- `publish(account_id, PrincipalPublicationInput)` receives principal state. Optional-byte `metadata` supports homes upgraded one at a time; new homes may send complete `PrincipalState`. The caller must be a registered home. First publication requires account bytes 4..9 to match that home's Xid allocator fingerprint, the first 5 bytes of the `SHA-256` commitment to `("dmsg", environment, issuer_namespace, home)`; subsequent publication accepts only the recorded home. Older versions return the current publication; identical content at the same version is idempotent, differing content yields `IdempotencyConflict`, and a newer version requires a greater `updated_at`.
- Rendering: JCS JSON, lowercase `type`, configured `controller_source` for `controllers` and `retired_controllers`, configured `delegation_service`, a stable HTML alias and user-named `links` preserved verbatim. SDK `validate_principal_document` runs before publication.
- Configuration: `principal_origin`, `controller_source` and `delegation_service` are pathless HTTPS origins, at most 512 bytes; `profile_url_prefix` is at most 2 KiB. URLs reject raw control bytes, double quotes and backslashes; use URL percent-encoding where needed. These four values enter principal IDs/documents and are immutable after installation. `custom_domains` contains at most 8 lowercase domains, including the principal-origin hostname, and is replaced wholesale by `admin_set_custom_domains`.
- HTTP: `GET /<account_id>` returns a 200 document with `content-type: application/json`, `access-control-allow-origin: *` and `cache-control: public, max-age=30`. `/.well-known/ic-domains` serves custom domains; `/favicon.ico` serves the monochrome dMsg icon as `image/x-icon`, cached for 24 hours. Other paths return certified 404 JSON. All response headers are certified with response-only certification for gateway verification. No redirects are used, since the final URL must equal document `id`.
- HTTP routing and certification use the same path rules: percent-decode, ignore empty segments caused by consecutive slashes, and preserve trailing-slash distinctions. Equivalent paths return the same certified response.
- Only two ingress classes are accepted: `publish` from registered homes, and controller/governance calls. Other ingress is rejected before execution; use query calls for reads. `directory_stats` returns published-account count, stable pages and cycles balance.
- Directory stable schema 7 stores exact bodies and their SHA-256 hashes, with certification trees in stable memory. Routine upgrades recertify only 404, domains and favicon without visiting documents. Home migration (`handoff`) is not implemented.

## Security snapshots

`SecuritySnapshot` schema 4 includes `principal_updated_at`, the current principal state's `updated_at`, or `null` before activation. If a relying party/service finds a greater value in the certified snapshot than in the directory document, publication is lagging and it should not accept new events under the old document.

## Example document

```json
{"aliases":["https://dmsg.net/<account_id>"],"controllers":[{"delegation":{"audiences":["https://dmsg.net"],"scopes":["message.draft"]},"id":"did:agent:<key>","name":"dMsg signer #1","source":"https://dmsg.net","valid_from":1790000000001}],"delegation_service":"https://api.dmsg.net","id":"https://id.dmsg.net/<account_id>","links":[{"name":"Website","rel":"related","url":"https://example.com/"}],"name":"Panda","protocol":"agent-delegation/1.0","type":"person","updated_at":1790000000001}
```

## Verification

- `dmsg_protocol::agent` unit tests: SDK-validated rendering, state invariants, directory configuration, registration-approval and controller-PoP digest vectors (`controller_pop_v1` in `protocol_vectors.json`).
- `dmsg_user` unit tests: lifecycle monotonicity, limits, replay, unchanged `security_epoch`, and rejection of invalid PoP.
- PocketIC `agent::self_held_principal_publishes_certified_documents_and_its_grants_are_accepted`: activation, invalid-PoP rejection, registration, publication, `ic-response-verification` checks of certified 200/404/domain responses, snapshot schema 4, acceptance of a local Rust-SDK-signed grant by `validate_delegation_acceptance`, rejection of out-of-scope or others' keys, rejection of retired keys after rotation while a new key with the same ceiling can revoke old credentials, publication rules and unchanged documents across upgrades.
- `dmsg_client` tests: locally signed grants/revocations accepted by SDK `validate_delegation_acceptance` against directory-rendered documents; ceiling violations rejected; nonce `max(last + 1, created_at)` no earlier than `valid_from`; validity over 366 days rejected; controller keys stored at fixed vault IDs.
- Desktop E2E `desktop_agents` in `tests/dmsg_integration/tests/desktop.rs`: enable, register, publish through real `dmsg_directory`, identical resubmission after a lost grant reply, relying-party verification of `usable`, and unusability after revocation.

These are local tests, not proof of production deployment, measured capacity or a completed security audit.

## Unified public identity document (2026-10-22)

User public profiles directly use `agent_protocols::delegation::PrincipalDocument`. `id.dmsg.net/<account_id>` returns its JCS JSON; `dmsg.net/<account_id>` and certified-handle-resolved `dmsg.net/<handle>` render the same document. Canonical `id` always uses the principal origin; existing signature issuers and derivation domains are unchanged. `aliases` automatically includes the stable HTML address without duplicating transferable-handle ownership.

`PrincipalState.metadata` carries only the SDK display members' canonical JSON, at most 32 KiB, across Candid/stable-storage boundaries; it is not a second public Profile. Names are at most 80 characters, descriptions 2000, links 100. Links contain `name/url/rel`, with HTTPS URLs up to 2048 bytes. `extra["dmsg.link_images"]` maps link URLs to HTTPS image URLs. Metadata counts toward the 64 KiB total document budget, while reserving space for future controller retirement.

Publishing a profile is an account mutation plus a directory push; ordinary messages, content grants and file operations do not gain on-chain writes. `updated_at` and internal version increase monotonically without advancing device `security_epoch`. Failures/unknown replies preserve the committed version for `publish_principal` retry; the client must distinguish local commit from directory publication.

`principal_document(account_id)` returns `CertifiedBatch` containing exactly the HTTP JSON bytes at `[b"documents", account_id[12]]`. The root is `fork(labeled("documents", documents_root), labeled("http_expr", http_root))`; each read surface retains the other branch's pruned hash. SDK and native clients independently verify the IC root, canister, 60-second certificate freshness, account path and canonical document ID. HTTP headers and certification expressions are unchanged. User stable schema is 15 and directory schema is 7.

Existing production data uses in-place `upgrade`: directory supports 6 → 7, preserving each original JSON body while building the additional tree; user supports 14 → 15, preserving accounts, unlock master secret and all stable tables, defaulting only missing principal display metadata to `{}`. Upgrade directory before homes. Publication input `metadata` is optional bytes so old homes remain compatible. Same-version retries of old state digests require identical original fields and empty metadata; existing documents are not rewritten using the new renderer. Later versions use current rules. Other old schemas are outside this migration.

`scripts/test-dmsg-principal-upgrade.sh` builds unchanged old Wasm from pinned commit `70d67c86`, checks Candid compatibility, and uses PocketIC to verify retained accounts/devices/bindings, content roots, unlock keys, execution evidence, controllers, original document bodies and certified reads, followed by edits, retries and another upgrade.

The old `dmsg/profile/v1` user command, COSE Profile object and cloud CRUD are removed; agents' `agent-profile/1.0` remains separate. Avatars are explicitly public uploads whose URLs remain unchanged and readable after commit; removing document references does not delete images, and ordinary collection rejects committed avatars. Desktop upload returns only the URL; publishing the document changes the profile, preserving editing drafts.

The recorded local verification covered:

- PocketIC profile and controller lifecycle: identical HTTP/query bodies, controller preservation on profile edits, stale-edit rejection, and TypeScript-SDK verification of real-Wasm certification evidence.
- `desktop_principal_documents`, `desktop_content_and_sync` and `desktop_channels`: real daemons, canisters and local relay across profiles, avatars, locked permissions, content sync and channel-name reads.
- Electron smoke: imported legacy names, named links and images, avatar operations preserving drafts, pending publication and retry. The final smoke skipped native macOS fullscreen checks; other checks passed.
- Web: real-Wasm proofs render documents, while altered proofs prevent rendering. Live-share and independent certified-fixture checks were outside this profile verification and were not run.

## Client alignment with 0.12.6

- Normative vectors, schemas and shared boundary cases track the SDK; provenance and offline commands are in the [vectors README](../../tests/vectors/agent-protocols/README.md). This alignment changed no canister Candid interface or stable layout.
- Before Mail publication, the host atomically saves connector state and encrypted key snapshots. Restart restores them before publication, clearing the snapshot only after normal key storage confirms persistence. Keys never enter MCP output. Remote owner inventory is separate from local read-only inventory; the former is authorized by relay grants, paginated and persisted for recovery. Recovering metadata does not recover keys or historical plaintext.
- Desktop public-Agent-Profile reads persist the greatest verified nonce and reject older versions or same-nonce conflicts. Discovery cannot change configured service origins. SDK Profile pins are also persisted in connector snapshots.
- An explicit controller nonce conflict permits at most one re-sign using valid, bounded `Max-Seen-Nonce`. The approved ID, generation, scope, audiences and expiry stay fixed; current authority is rechecked before signing. The retry count is journaled encrypted before retry and does not reset after restart. Unknown outcomes resubmit original bytes; other refusals require new approval.
