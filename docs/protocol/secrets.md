# Controlled credential use on this device

English | [简体中文](secrets_zh.md)

2026-10-24. This describes the public client implementation, not deployment or real-GitHub acceptance.

## Scope and permissions

The owner creates a device-local policy. An Agent requests one operation through `secret.use:<policy_id>`; the daemon executes only after a person approves the complete plan. The MCP adapter supports only GitHub.com `issue.create` in a specified repository; policy configuration does not approve execution. MCP has no general secret-resolution, HTTP-proxy, command-execution or automatic-retry interface. Owner process delivery, SSH/Git, mounts and snapshots are described below.

`secret.use:*` is invalid. Agent Delegation, `vault.read/write` and `files.read` do not imply it. Policies/grants are LDK-sealed on this device, not synced or copied into resident Agents' locked-state permissions. Limits are 64 policies and the existing 64 capability grants per device.

MCP `dmsg_vault_read` always masks concealed/otp fields. The `secret` parameter is removed and rejected even when false, as is `vault.secret`. Old grants no longer match or convert automatically. Agent writes to existing entries target only the current version: `base` checks concurrency and stale values return `VERSION_CONFLICT`, without historical access. Agents cannot change tags or turn concealed fields into plaintext types; creation still requires an authorized write tag. Reserved `dmsg-` entries are excluded from all these paths. Owner reveal/copy/OTP is unchanged.

## References and policies

Reference text is `dmsg-secret://<account_id>/<item_id>/<field_id>`, with object:

```json
{"schema_version":1,"account_id":"040g2081040g2081040g","item_id":"1111111111111111111111111111111111111111111111111111111111111111","field_id":"credential"}
```

Reject noncanonical account IDs, item IDs other than 64 lowercase hex characters, invalid field IDs, query/fragment/userinfo/escapes/extra paths. Execution reads only current undeleted, conflict-free safe entries; API credentials must be concealed, not OTP, wallet or reserved items. Recreating the same field ID preserves the reference; approval pins the item revision, invalidated by subsequent changes.

Vault encryption covers whole entries. Metadata validation opens them only inside the trusted core and clears field buffers; credentials do not reach renderers, MCP or network before approval. Execution obtains Zeroizing credential buffers only at the commit point; there is no resolve RPC returning a value.

`UsePolicy` returns `{id, account_id, device_id, version, policy, created_at, updated_at}`, with policy:

```json
{
  "schema_version":1,
  "name":"Development issues",
  "enabled":true,
  "adapter":"github.issues/1",
  "credential":{"schema_version":1,"account_id":"040g2081040g2081040g","item_id":"1111111111111111111111111111111111111111111111111111111111111111","field_id":"credential"},
  "target":{"api_origin":"https://api.github.com","owner":"example","repo":"test"},
  "operations":["issue.create"]
}
```

Creation generates a random 32-character lowercase hex ID. Update/delete require `base_version`; disabling is immediate and increments version, while enabling/expanding authority needs approval. Optional `grant_to` grants request authority until revoked, not approval of each use. It is displayed in the same configuration approval and committed atomically with policy/approval completion in SQLite. GitHub redirects must not change repositories automatically; a path is not permanent repository identity.

## Owner RPC and CLI

| RPC | Input and result |
| --- | --- |
| `secrets/ref` | `item_id, field_id` → `{uri, reference}`, without value |
| `secrets/policies/list` | `after?, limit?`（≤64）→ `{policies, next, device_local:true}` |
| `secrets/policies/save_request` | `source, id?, base_version?, policy, grant_to?` → existing approval view |
| `secrets/policies/delete_request` | `source, id, base_version` → deletion approval |
| `secrets/policies/disable` | `id` → disabled policy |
| `secrets/uses/get` | `request_id` → status/filtered result and owner approval; only ID/status when locked |
| `secrets/uses/cancel` | `request_id` → cancel awaiting_user only |
| `secrets/uses/list` | `before?` (last request ID on previous page), `actor?, kind?, limit?` (≤100); P0 kind is secret_use only → `{uses,next,device_local:true}` |
| `secrets/uses/mark_reviewed` | `request_id` → adds reviewed_at only to lost, without changing execution outcome |

```text
dmsg secrets ref <item-id> <field-id>
dmsg secrets policies list
dmsg secrets policies save policy-request.json
dmsg secrets policies disable <policy-id>
dmsg secrets policies delete <policy-id> <base-version>
dmsg secrets uses list [--before <request-id>] [--actor <agent-id>]
dmsg secrets uses show <request-id>
dmsg secrets uses cancel <request-id>
dmsg secrets uses reviewed <request-id>
```

Configuration JSON is at most 64 KiB, containing only policy and optional id/base_version/grant_to, never tokens. Save/delete create approvals only. Use approval comes from desktop, or an interactive terminal with presence verification on a capable device; MCP, scripts and ordinary renderers cannot approve.

## MCP contract

| Tool | Behavior |
| --- | --- |
| `dmsg_secret_policies` | No arguments; lists authorized enabled policy IDs/names/versions/targets/operations, never credential references |
| `dmsg_secret_use_request` | `policy_id, operation, arguments, idempotency_key, reason?` → pending approval ID/status/deadline |
| `dmsg_secret_use_status` | `request_id` → own request's status/filtered result; current capability still required |
| `dmsg_secret_use_cancel` | `request_id` → cancel own undecided request |

`operation` is only `issue.create`; arguments contain title (1–256 Unicode characters), optional body (≤32 KiB UTF-8), with reason ≤4096 UTF-8 bytes. Idempotency keys are 1–128 ASCII letters/digits/`-_.`; extra arguments fail. Agent identity comes from verified credentials; inputs cannot set actor, device, confirmed or execution credentials.

Requests bind account/device, Agent credential digest, exact grant ID and pause generation at admission, daemon boot randomness, session_generation, policy version, field revision, fixed target and full arguments. Existing `dmsg/approval/v1` commits the complete plan, safely shown in full by UI. Idempotency is scoped to account/device/Agent: identical key/input returns the original request, changed input gives `IDEMPOTENCY_CONFLICT`. Terminal records remain 30 days, after which deduplication is not guaranteed. Resume after pause or regrant after revocation/expiry never revives old requests; use a new key/plan/approval.

Locked devices cannot read sealed ownership, so Agent status returns `LOCKED` without a plaintext ownership index. Owners may inspect minimal known-request state. The same Agent with a new credential may read old results but cannot execute requests invalidated by credential change.

## Approval, execution and failure

Use and policy changes require high-risk approval. Waiting expires after 5 minutes; approval immediately schedules execution, with no transferable token or lease returned to the Agent. Pending limits are 20 globally and 5 per Agent origin; external execution allows 1 per Agent and 4 per device, explicitly rejecting excess.

At synchronous commit, recheck account/session, credential, pause, grant, policy and revision, persist original approval with started=true, then acquire the credential. Network runs outside `Hold::with`. Lock/disable/revoke attempt to interrupt in-flight requests but cannot recall a sent POST. Lock retains no root for settlement; outstanding requests finish when the original account unlocks, never writing into another account.

| State | Meaning |
| --- | --- |
| awaiting_user | No external execution yet |
| rejected/cancelled/expired | No execution |
| executing | Approved; started distinguishes crossing the commit point |
| done | Creation confirmed; only number/html_url returned |
| failed | Local or explicit upstream rejection |
| lost | Creation may have happened; result unknown; never replay |

After restart/task loss, started requests become lost/EXECUTION_UNKNOWN and unstarted ones failed/SECRET_SESSION_ENDED. Generic reconcilers must not resend them. Manual review adds a marker only; another attempt requires a new idempotency key and approval.

HTTP uses fixed origin/path, normal TLS, no environment proxy/redirects/reqwest automatic retries; connect timeout 5 seconds, total 30 seconds, response ≤1 MiB. On 201 validate number and target-matching html_url and return only them. Explicit auth/argument rejection is failed; 503, interruptions, malformed success or redirects are lost. After sending, unrecognized errors also settle as lost/EXECUTION_UNKNOWN unless the adapter identifies an explicit HTTP rejection; unsent local failures have a separate path. Raw errors/bodies are not returned; known credential echoes are blocked, with SECRET_OUTPUT_BLOCKED still lost and never retried.

Errors include `SECRET_REFERENCE_INVALID`, `SECRET_VERSION_STALE`, `SECRET_POLICY_STALE`, `SECRET_SESSION_ENDED`, `SECRET_OUTPUT_BLOCKED` and existing FORBIDDEN/LOCKED/EXPIRED/QUOTA_EXCEEDED. Test transport is local simulation only; production RPC has no endpoint override.

## Boundaries and verification

No canister/stable-layout change or new synchronization kind is involved. Policy restricts this dMsg entry point, not the token's actual GitHub authority or malicious same-user code. Restrict upstream repository/Issues write permissions separately.

The owner workflows below are implemented; shared team vaults are not. Unit/local HTTP mocks and Electron fixtures do not replace real GitHub, presence checks or filesystem tests; see [verification entry points](#verification-entry-points-and-boundaries).

## Local development workflows

These are owner control-plane interfaces, absent from the MCP catalog. Actions `secret_process`, `secret_ssh_binding`, `secret_ssh`, `secret_mount_config`, `secret_mount_read`, `secret_share`, `secret_package_import` and `secret_discovery_import` all require high-risk presence approval.

### Process delivery

`secrets/run/request` receives `executable {path, identity, sha256}`, the invoked program name `arg0`, argv, cwd, refs_file, complete refs and `files [{name, source}]`, returning request_id, a random claim only to the initiating CLI and a value-free approval. `secrets/run/claim {request_id, claim}` is single-use. `secrets/run/report` records CLI-reported start/exit without changing delivery facts. The CLI starts the pinned canonical path with argv[0] set to the approved `arg0`, so multi-call programs (such as busybox or a compiler invoked through a symlink) behave as when run directly.

`.env.refs` accepts only `NAME=dmsg-secret://account/item/field`, blank lines and full-line comments, with no shell interpretation. Source: ≤64 KiB/64 variables; argv: ≤256 items/64 KiB; environment values: ≤64 KiB total. `--file` allows 8 files of 8 MiB each from field URIs or `dmsg-file://file_id/version_id`, delivered as inherited sequential `/dev/fd/N` pipes. Output never enters MCP; EOF ends reads without another decryption/claim.

### SSH / Git

`secrets/ssh/keys` returns public keys/fingerprints/support reasons/local bindings only. `binding_request {reference, purposes}` allows only ssh_auth/git_sign; `disable {id}` stops new signing immediately. status returns the real socket path. Socket ownership is the current OS user, directory mode 0700 and socket 0600; at most 32 connections with 256 KiB frames.

SSH authentication requires a valid signed, non-forwarding `session-bind@openssh.com`, matching session, publickey, ssh-connection and original connection. Git allows only SSHSIG's git namespace, empty reserved and SHA-256/SHA-512 digests. Approval distinguishes protocol-digest review from original commit text and does not interpret a host-key fingerprint as verified DNS identity. Password-encrypted private keys and non-Ed25519 identities fail, without fallback to returning private keys. An invalid, forwarded or repeated session-bind closes the connection; other extension requests only receive a failure and keep it. If the peer disconnects or sends more data while a signature awaits approval, the connection closes and a late approval signs nothing.

### Virtual files

`secrets/mounts/prepare` previews paths, actual sizes and source versions without creating a volume. save_request creates approval; start/status/revoke_reads/stop control lifecycle. Input includes:

```json
{
  "project_label": "Local development",
  "project_directory": "/absolute/project",
  "mount_point": "/absolute/project/.dmsg-secrets",
  "entries": [{
    "relative_path": "service.json",
    "source": {"type": "library_file", "file_id": "<64 hex>", "version_id": "<64 hex>"}
  }],
  "aliases": [{"name": "service.json", "entry": "service.json"}]
}
```

Other sources are `field_utf8 {reference: URI}` and `refs_dotenv {variables: [{name, reference: SecretRef}]}`. Paths allow 8 levels and 64 files. Original JSON/PEM/binary bytes are unchanged. dotenv rendering rejects quotes, backslashes, dollar signs, newlines and NUL rather than guessing escaping.

The daemon chooses platform mode, not reader-supplied identity. Linux uses OS uid and the actual open handle; PID is a provenance hint, not process isolation. macOS uses uid with window_ms of 1–60,000 milliseconds and warns that other programs of the same user can read during that window. Pending IO expires in 30 seconds; cancelled callbacks reject late approval. Range reads are ≤1 MiB. released_bytes measures bytes handed to the backend, not complete application consumption. mmap/writeback are outside the supported contract.

The extension connects through a Unix socket in a dedicated 0700 directory without keys/plaintext backing files. The daemon verifies matching UID, fixed extension ID, FSKit entitlement and signing team. Ordinary owner RPC cannot read mount plaintext. Disconnection, source changes, lock/restart invalidate grants without reopening windows; native cache-revocation/unmount failures remain visible. Lock, logout and account switching stop and unmount every secret mount, leaving project aliases dangling meanwhile; unlock does not remount, so the owner starts it again and every read still needs a new approval. After a source change each read is refused at once and the daemon's periodic check then stops the volume.

### Sending and receiving snapshots

`secrets/shares/prepare {input, password?}` fixes item_id, revision, field_ids, include_notes, attachment file_id/version_id, expires_in_ms and max_download_sessions. Defaults are one day/one session; accepted ranges are 6 minutes–7 days and 1–100 sessions. Preparation creates independent keys/fixed identity but does not publish. Password derivation runs outside the core lock and passwords are not persisted.

`request {id}` creates approval. list/get default to metadata only; explicit `get {id, include_link:true}` returns a link only after confirmed publication without cancellation. reconcile queries original share ID; revoke revokes it. After publication commit, no second creation or generic recovery replay is allowed. Interrupted unpublished uploads require new approval while retaining upload/share identity and keys.

Outer packaging remains the encrypted share-file format. Inner MIME is `application/vnd.dmsg.secret-package`: four-byte big-endian JSON length, strict JSON manifest and ordered attachments. Manifest ≤256 KiB; package ≤100 MiB, 64 fields and 20 attachments. Unknown/duplicate fields, bad lengths/digests/trailing bytes fail. Include only selected fields/notes/attachments, excluding source account/item/field IDs, tags, authority/history. snapshot_id is independently random; fields use package-local numbering, with verified role suffixes preserving standard-template semantics.

`secrets/packages/import_preview {link, password?}` accepts only the configured login-site links, persisting a random download session before starting and reusing it on retry. Full verification returns metadata package_id/digest/preview. `import_request {selection}` binds fields, notes, attachment indexes and destination folder; approval atomically commits new Vault IDs, Files, outbox and receipt. Attachments default local-only. Download cache is ciphertext with LDK-sealed key.

Browsers display safely as plain text; users explicitly reveal/copy fields or save individual attachments, with no full plaintext-package download. Copies are independent and survive source edits/revocation. Browser receipt followed by client import uses another session; with the default one-session limit, choose the receiving route before claiming.

### Health, discovery and import

`secrets/health` returns rules, subject/revision, observation time/evidence. Cloud state retains last_checked_at; unrefreshed is not treated as safe. dismiss affects only the current finding ID. `rotation/create/update/list` proceeds through created, tested, references_checked and upstream_revoked; the new version must actually change the selected credential. Tests/upstream revocation are owner reports, not independently verified facts.

`secrets/discovery/start {path}` scans only a selected directory; status/cancel/import_request track that scan and explicit candidates. No symlink following, cross-device traversal or dependency/build directories; limits are 10,000 entries, 100 MiB total, 1 MiB per file and 256 candidates. Limits/unreadable entries report incomplete. Reports contain paths, file identity/full digest, variable names and public-key algorithms, never values. Rereads check original identity/digest; lock cancels the worker. Import generates references without modifying original files.

External vault/import_preview and vault/import add explicit attachment-ID selection and per-source omission reports. 1pux member paths, duplicate names, symlinks and sizes are bounded; only selected attachments enter encrypted staging. Visible writes and retry receipts commit atomically. Repeating the same complete source digest, field/attachment selection and destination folder returns the original receipt.

See [verification entry points](#verification-entry-points-and-boundaries) for platform limits. Mocks/development signatures do not replace full production/private-cloud integration, production macOS kernel integration or distribution notarization.

Files attachments inherit existing `files.read:<folder>` grants, including ancestors. Local/cloud storage is not Agent access control; import UI selects folders and discloses inherited authority. Unbacked-up does not mean inaccessible to Agents. Hidden Vault fields retain separate Vault/secret.use boundaries.


## Credential references and rotation dependencies

`secrets/references {reference}` returns reference/revision/dependencies/digest/device_local. Dependencies contain only type, object ID, display name, configuration/source version, state and suggested action, never values, command arguments, claim tokens, file keys or share URLs. Scope covers local saved policies/Agent grants, mounts, SSH bindings, link/channel snapshots selecting that field and retained process approvals. Unobserved .env.refs, cleared history and other devices are excluded; projects are not automatically scanned.

`secrets/rotation/dependencies {id}` refreshes the same list. `rotation/update {id,step,dependencies_digest?}` requires the just-reviewed full digest at references_checked, binding field identity, current Vault head and sorted dependency metadata. upstream_revoked recomputes it; changed configuration/grants/snapshot state/item versions reject old confirmation. New credential changes require created/tested/references_checked again; final upstream revocation remains an owner statement, not independent verification.

CLI: `dmsg secrets references <dmsg-secret://…>`, `dmsg secrets rotation dependencies <id>`, then `rotation update <id> references_checked --dependencies-digest <digest>` after review.

## Channel secret snapshots

- `secrets/shares/channel_prepare {channel_id,input}` uses explicit fields/notes/fixed attachments, without link expiry/session count. Read/verify channel control first; pin ID/name/epoch/control head/ACL version and original snapshot/upload/file-version IDs. Preparation uploads nothing.
- `secrets/shares/request/get/list/reconcile` reuse high-risk owner approval and job states. Recheck account session, sources and channel scope before approval, each encryption/upload segment and commit. Changed control requires new preparation/approval, never automatic delivery to a new membership scope.
- Snapshots are ordinary channel files under independent keys. After encrypted chunks/manifest upload, only that epoch's encrypted message carries the key. A dedicated journal bypasses ordinary channel outbox; storage recovery refuses secret_channel. After publication commit, query only the original operation ID, never re-sign/resend/create a second message. Receipt digest must match the original signature before published status.
- Channel delivery has no share link, session count or content expiry; publish_before only limits starting delivery. Epoch-readable members, including later joiners/history grantees, and their explicitly authorized readers may retain copies. Cancelling a job works only before commit and cannot recall published content. Existing retraction/attachment collection retain their contracts and cannot erase copies.
- `secrets/packages/channel_preview {channel_id,seq}` reads an attachment only from locally verified channel messages, never caller-injected keys/URLs/manifests. Verify message, upload statement, ciphertext digest, AEAD, full-file digest and inner package before value-free preview; no full plaintext package is exported to disk or Files.
- `secrets/packages/import_request` uses the original package digest, explicit selection and trusted approval to create new Vault/Files IDs. Repeat receipt of one message converges on one package; repeated imports return the original atomic receipt, but changed selections cannot reuse it. Attachments inherit destination-folder grants.

CLI: `dmsg secrets shares channel-prepare <channel_id> <selection.json>` and `dmsg secrets packages channel-preview <channel_id> <seq>`. Desktop use center selects the destination channel; channel snapshots offer import with every field/note/attachment initially unselected. These are owner controls, with no new MCP secret-resolution or delivery-initiation tool.

## Verification entry points and boundaries

| Scope | Executable evidence |
| --- | --- |
| References, policies, approvals, GitHub adapter, revocation/outcomes | [Core tests](../../src/dmsg_client/src/secrets/tests.rs) |
| Packages, link/channel delivery, import/integrity | [Share tests](../../src/dmsg_client/src/secrets/share_tests.rs) · [Package vectors](../../tests/vectors/client/secret-package-v1.json) |
| CLI, SSH/Git and Linux FUSE | [Local OS integration](../../src/dmsg_cli/src/secret_integration_tests.rs); ignore annotations name required tools, isolated containers or /dev/fuse permissions |
| Desktop approval/UI | [Secrets smoke](../../src/dmsg_desktop/scripts/secrets-smoke.mjs) |
| macOS FSKit | [Native module](../../src/dmsg_desktop/native/fskit/README.md): build, signing, enablement, kernel behavior and distribution are separate checks |

This documentation review did not run those tests. Synthetic fixtures/build success do not replace real GitHub/presence/production relay, cross-UID/open-descriptor behavior, production macOS mounting/revocation or distribution notarization. Missing/unavailable native modules return unavailable, never plaintext backing-file fallback. Historical verification is not proof that a current release passed.
