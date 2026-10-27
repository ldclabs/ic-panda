# dmsg-cloud/1 client interoperability contract

English | [简体中文](cloud_zh.md)

This describes the client's wire contract and content mapping, not server implementation. The client lives in `src/dmsg_client`: `cloud`, `upload`, `sync`, `library`, `connect`, `evidence`, `channel_log`, `channel`, `inbox` and `payment`. Account/execution proofs continue using existing public Rust types.

## Identifiers and encoding

| Value | Wire representation |
| --- | --- |
| AccountId | 12 bytes in Candid/certified paths; canonical 20-character lowercase Xid in URL/JSON |
| Device, request, object and version IDs | 32 bytes each; nonzero lowercase hex in cloud JSON |
| Digest | Lowercase SHA-256 hex; not an automatic public plaintext-file digest |
| Signature, ciphertext, certificate and witness | Canonical unpadded base64url |
| Principal | Candid Principal; raw bytes in certified-leaf CBOR; canonical text in JSON |
| Time | Security evidence, HTTP and business deadlines use Unix milliseconds; document CWT iat stays seconds |
| Amount | JSON decimal string, converted by commerce/payment contracts; never JS Number |

Request bodies use deterministic CBOR. Floats, duplicate keys, trailing bytes, excessive depth and unsupported versions fail. HTTP PoP and content commands are distinct COSE profiles, not formal document signatures or authority to mutate account roots or pay.

Text preserves valid UTF-8 without Unicode normalization or stripping leading U+FEFF. Rust strings and TypeScript encoding retain signed text; TypeScript rejects lone UTF-16 surrogates without a valid UTF-8 representation. See SDK encoding/round-trip tests.

## Device commands and HTTP PoP

Commands carry only `{cose_sign1: base64url}`, without a conflicting extra account, payload or signature. They use tagged COSE_Sign1, Ed25519 -19, empty unprotected headers and empty external AAD.

Protected headers are `1=-19`, `2=[15,16]`, `3="application/cbor"`, `4=device ID bytes`, `15={1:account issuer URI}`, `16=profile`. Sign the complete original `Sig_structure` bytes, not a preliminary SHA-256 digest.

- Command profile: `application/vnd.dmsg.command+cose;v=1`.
- Command payload: `{protocol:"dmsg-cloud/1", action, security_epoch, request_id, deadline, payload}`.
- HTTP profile：`application/vnd.dmsg.http-pop+cose;v=1`。
- HTTP payload：`{protocol:"dmsg-cloud/1", security_epoch, request_id, deadline, audience, method, target, body_digest}`。
- `X-Dmsg-Pop` directly carries base64url HTTP COSE. Audience is the fixed API origin; body_digest is SHA-256 of actual transmitted bytes. GET body is empty bytes, not CBOR null.
- Target is canonical path/query; duplicate query keys, encoded slashes and cross-origin URLs fail. Signing and transmission use identical serialized bytes.
- Submission requires `now < deadline <= now+60000`, also bounded by the evidence's validity. Recheck expiry after the device-signature callback.
- Business and HTTP request_id must match. Re-signing HTTP PoP changes neither command nor operation ID. Query original operations for unknown results; never automatically use a new ID.

dmsg-connect/1 and hints add seven actions, with routes/rules in [connect-v1](connect-v1.md) sections 7.3/7.5: `dmsg/connect/invite/v1`, `dmsg/connect/pairing/grant/v1`, `dmsg/connect/pairing/revoke/v1`, `dmsg/connect/authorize/v1`, `dmsg/connect/respond/v1`, `dmsg/hint/token/create/v1`, `dmsg/hint/token/revoke/v1`. Pairing grants/revocations, authorization and replies require `FormalApprove` in the certified device table; capability checks are action-specific. The respond body is `{cose_sign1, sealed}`, with sealed response beside the command, at most 512 KiB; payload commits only its SHA-256. This is the sole command body allowing a field beside cose_sign1.

`dmsg_client::cloud::sign_command` builds command envelopes and `sign_http` HTTP PoP. User profiles do not use this command channel. Each resource contract defines action payloads, authority and transitions; generic encoding grants no additional capability.

Rust `Cloud` submits evidence, signed requests and ciphertext chunks with bounded response sizes, no redirects and no cookies. Safe reads retry within a budget; generic transport never automatically resends non-GET requests. Upload/sync recovery follows its own idempotency contract after reconciling the original operation. Network waits occur outside `Hold`'s synchronous Client steps; budgets are in the [client transport guide](../../src/dmsg_client/README.md#sync). Dedicated modules implement payments/subscriptions.

## Registration admission tickets

When a home configures `admission_key`, `create_account` requires a cloud-issued `AdmissionTicket`. Before creation the client requests:

```text
POST /v1/account-admission
content-type: application/cbor
body: { home: bstr(user home Principal), principal: bstr(creating login Principal) }

200 { ok: true, data: { expires_at: <Unix milliseconds>, signature: <base64url> } }
```

The admission key signs `digest("dmsg/account-admission/v1", (home, principal, expires_at))` with Ed25519; see vector `account_admission_v1`. No PoP is needed: only the bound principal can use the ticket by calling create_account, and each principal can create one account per home. The service issues only for configured homes and rate-limits request sources, returning `QUOTA_EXCEEDED` when exhausted. Expiry is at most 5 minutes after issuance; the home accepts at most 10 remaining minutes to allow clock margin. Tickets grant no account/device permissions and unused tickets expire.

## Account evidence

The account module queries `security_snapshot_batch` and `get_device_bundle`. Verify the pinned user canister, ICP BLS root, certificate/time, single AccountId path and witness before issuer/home/account and complete device-root checks.

Device root is `digest("dmsg/devices/v1", BTreeMap<Hash,Device>)`, retaining all public fields, revocations and next_sequence. Keys are raw bytes, not hex strings. If devices change between queries, mismatched roots fail rather than combining snapshots.

Evidence bridge shape:

```text
{schema:1, canister, certificate,
 entries:[{account_id, value, witness, devices}]}
```

value/witness/certificate contain raw certified bytes; devices is canonical CBOR of the entire device map. Cache expiry is certificate time +60 seconds, never refreshed by reads/submissions. Choose only nonrevoked, suitably capable devices from verified records; returned records are not all active authorizations.

User profiles are official `PrincipalDocument` values managed by `dmsg_user` and published by `dmsg_directory`; reads/certification are in [the principal contract](agent.md#unified-public-identity-document-2026-10-22). Cloud user Profile storage and `dmsg/profile/v1` are removed; avatar uploads retain device PoP and object-commit verification.

## Local content and cloud mapping

Accounts, devices and roots follow the [account/root contract](account_root.md). `content` creates persistent jobs per local version; `sync` uploads ciphertext and submits signed revisions. Workspaces without account/device authority cannot write content. Current mappings are:

| Local source | Cloud target | Mapping and checks |
| --- | --- | --- |
| `workspace::Meta.account` / `EncryptedObject.subjectId` | Actual `account_id` | Canonical bound-account Xid, never a local random identifier |
| `EncryptedObject.id/revision/parent` | `item_id/revision_id/base_revision` | Preserve provenance; parent revision belongs to the same item; compound local `key` is not a wire ID |
| `generation/deviceId` | Root generation / signing device | Use the certified committed root/devices, not self-reported online state |
| Content AAD | `['dmsg/content/1', environment, accountIdText, generation, id, revision, deviceId, kind, parent, tombstone]` | Subject is the actual Xid; identity/version/context are authenticated, so changed metadata cannot reuse ciphertext |
| Item-key wrap | `['dmsg/wrap/1', accountIdText, id, generation, revision, 'item-version']` | Wrap per-version keys under the matching root generation; verify old versions with original AAD/historical roots |
| Encrypted object | `kind=vault` upload followed by signed vault revision | Canonical CBOR ciphertext container; hash actual upload bytes, not local `record.digest` |
| Vault revision | `{item_id,revision_id,base_revision,upload_id,tombstone,restore}` | Finish upload before referencing upload_id; deletion references null; restore names the current tombstone; server determines conflicts |
| File-manifest `chunks[].size` | Upload-plan `chunks[].size` | Local plaintext length versus actual ciphertext length after base64url decoding; retain both measures |
| Local chunk ID `file:version:index` | `upload_id + index` | object_id/version_id identify file provenance; independent idempotent upload ID; preserve/check original chunk AAD |
| File key/name/MIME/plaintext digest | Encrypted manifest / vault content | Never in public upload plan; manifest_digest/size describe actual encrypted bytes, with no plaintext FileKey upload |
| Empty file | Encrypted manifest plus nonempty format placeholder | Cloud requires a nonempty chunk; use the placeholder below, never fake plaintext data |
| Public user profile | Official PrincipalDocument via user/directory on chain | Cloud stores public avatar bytes only, not another user-profile projection; inbox policy is separate |
| Local channel/message drafts | Signed genesis/control/epoch/message | Check current membership/head/epoch; local state/sequence is not online authority |
| Local jobs/journal | Resource operation queries and actual receipts | Reconcile original request ID; locally queued/sent is not service acceptance |

The local library (`dmsg_client::library`) defaults to 4 GiB, stores encrypted chunks in `blobs/` and does not auto-sync. Cloud backup/sharing currently allow 100 MiB. `kind=backup` is explicit; channel attachments use independently encrypted share editions. Vault entries contain no file chunks; selected import/snapshot attachments enter Files under the [Secrets contract](secrets.md). Account `kind=file` uploads serve legacy `migration_part` archive segments.

Plaintext chunks are 1 MiB, ciphertext budget is `1 MiB + 64 bytes`, and manifests are at most 64 KiB. Actual ciphertext bytes count against quotas. Excess fails explicitly without truncation.

After plan expiry/root rotation, query the original revision operation and each upload. Reuse committed objects. Explicitly cancel staging uploads that cannot continue before rewrapping manifests under the current root with a new upload_id. Preserve object/revision/request IDs and encrypted file chunks. Keep jobs available for retry while quota release remains pending.

Pulls verify versions/gaps using complete export snapshots, checking/caching ciphertext chunk by chunk and reusing verified chunks. Heads/cursors advance atomically only after all manifests/files verify. Account pulls are no longer limited by a 256 MiB backup package, though individual file/chunk/manifest/evidence limits remain. Cached chunks from interruptions do not imply content commit. After unlocked sync, collection retains all chunks/manifests in the applied snapshot (otherwise next pull redownloads them), unsent-version jobs, unfinished imports and unexpired sealed upload plans for channel envelopes/attachments and rotation. Replaced versions, expired/abandoned uploads and cached channel downloads can be collected. Resolved conflict copies and history still in relay snapshots are retained.

Export signatures: every object/revision carries the signed command's `security_epoch`; pages provide `security: {"<epoch>": {evidence, stored_at}}`, the evidence checked when the relay first accepted a reservation/revision in that epoch. Each pull verifies certificate, account leaf and device table once per epoch at stored_at, requiring matching epochs, then verifies each command under its then-nonrevoked content_sign device. Do not use only current devices: full recovery replaces them, and adding at the 16-record bound removes the oldest revoked entry, yet historical versions must remain verifiable. Entries without their epoch evidence fail.

Object payloads are bounded by 200,000 bytes (`content::MAX_OBJECT`) for reads and writes. Channel indexes derive from verified decrypted content and remain local head metadata, changing neither AAD nor upload fields. Conflict relationships are encrypted Item `resolvedConflicts`, retained by edits and carried in backup/sync.

Legacy dMsg encrypts the whole file once with COSE_Encrypt0 in `ChannelMessages.svelte`, then IC OSS `toFixedChunkSizeReadable` / `upload_chunks` sends **256 KiB ciphertext transport fragments**. `ChannelFileCard.svelte` decrypts the reassembled bytes. OSS Rust `file.rs` and TS `stream.ts` define `CHUNK_SIZE=256*1024`. These are not independently AEAD-authenticated fragments. Migration preserves indices/lengths/original bytes and verifies reassembled COSE, never applying the new 1 MiB plaintext rule or relabeling fragments as authenticated chunks. This follows local legacy-client/OSS source; live versions require [read-only inventory](../legacy_inventory.md).

The table defines fields, AAD, authority and size semantics. Object chunks are canonical CBOR `{format:"dmsg-cloud-object/1", record:EncryptedObject}`. The record retains original encrypted fields under the existing content rules, without plaintext Item/FileKey.

Upload-manifest plaintext is canonical CBOR `{format:"dmsg-cloud-manifest/1", account_id, upload_id, object_id, version_id, kind, root_generation, chunks:[{digest,size}], content}`. IDs use the JSON text forms above; chunks match the plan. Vault content is null; file content is FileManifest with plaintext size/digest/key, all inside manifest encryption. The target root wraps it with random-nonce COSE_Encrypt0 AES-256-GCM and external AAD `['dmsg/cloud-manifest/1', account_id, upload_id, object_id, version_id, kind, root_generation]`. manifest_size/digest cover final encrypted bytes. A manifest omits its own digest to avoid recursion.

Roots (`kind=root`) are an exception: manifest is raw RootBundle bytes ([account/root](account_root.md)); object_id is hex of the on-chain bundle_digest, which binds recipients/body rather than raw bytes. `GET /v1/accounts/:account/root` matches root_generation/object_id against certified content_root_generation/content_root_digest. The client still checks bundle_digest after downloading.

Public avatars (`kind=avatar`) are also unencrypted: a single PNG/JPEG/WebP original of 12 bytes to 512 KiB, checked by file header, with root_generation/epoch 0, control_head null and random object/version IDs. Manifest is plaintext canonical CBOR `{format:"dmsg-cloud-avatar/1", account_id, upload_id, mime}`, stored but not read by the relay. Commit publishes the object; finalize/upload status returns final `public_url`, in production `https://assets.dmsg.net/avatars/:account/:upload`, referenced verbatim by PrincipalDocument avatar_url. Never construct image URLs from API origin. `status=committed` alone is upload commit; absent public_url means publication is unfinished and the same ID must retry finalize, not reserve/upload anew. Publication failure may return retryable 503 and also recover in background; repeated finalize/operation reads return the same URL. Only committed avatars are public. Removing document references does not delete/disable URLs; ordinary collection refuses avatars. Immutable one-year caching requires a new URL for a new avatar; public reads have credential-free CORS. The old `/public/accounts/:account/avatar/:upload` route is removed. Local development reads `/avatars/:account/:upload` from its public bucket, checking the directory's HTTPS delegation_service alias before mapping to loopback API; production uses returned URLs directly. The client saves its latest upload plan, reusing the ID for the same image and reserving anew only after a definite uncommitted terminal plan.

Empty manifests retain size=0, the empty-file digest and no file chunks. Upload uses canonical CBOR `{format:"dmsg-cloud-empty-file/1"}` as the required nonempty placeholder, not a plaintext chunk. Recovery checks the marker, zero length and digest and never returns placeholder bytes as content.

Backups (`kind=backup`) explicitly upload one local version using existing encrypted blobs without re-encryption; empty files use the placeholder. object_id/version_id are random, with source identity only inside the manifest. root_generation is current, epoch=0 and control_head=null. The dmsg-cloud-manifest/1 content is canonical CBOR `{format:"dmsg-backup/1", file:FileManifest (including key), path:[folder names...], tags, note, created_at, modified_at, device}`, root-wrapped and unreadable to the relay. `GET /v1/accounts/:account/backups?after=&limit=` pages committed backups and collection tombstones, carrying signed plans, reservation evidence/stored_at, uploader and commit time. As for channel descriptors, the client verifies dmsg/upload/reserve/v1 under the then-valid content_sign device from that evidence, downloads and opens the manifest under its generation root to build backup inventory. Current-device membership is not required for historical verification after recovery or pruning revoked devices. Backups are outside vault export snapshots. Explicit `dmsg/object/collect/v1` removes cloud copies; deletion is not mirrored between local and cloud storage.

Public clients implement writers/readers, recovery jobs and encrypted-container round trips. See test entry points below. Legacy archives have a separate format and cannot masquerade as new content.

## Verification and release boundaries

- Fixed command/HTTP vectors: `tests/vectors/client/cloud-v1.json`, generated publicly and byte-checked independently by the companion service.
- Unit tests: `cargo test -p dmsg_client`, including those vectors.
- E2E: desktop clients on PocketIC/local workerd relay, in [`tests/dmsg_integration`](../../tests/dmsg_integration/README.md)'s desktop.rs. Fresh accounts and local roots are used without production canisters.
- Negative cases include wrong canister/account, expired evidence, wrong device maps, forged certificates, changed signatures/HTTP bodies, wrong resources and old protocol versions.
- Handle device/signature/resource/runtime refusals explicitly; readiness is not business authorization. UI is wired, but production configuration/versions, real accounts and E2E acceptance require separate evidence. This documentation update did not rerun these tests.

## Background ciphertext and certificate freshness

Locked daemons complete only already-signed submissions: fixed vault revisions and GETs for the same operation. PoP binds origin/path/method/body digest/account/device and at most 45 seconds, without renewable sessions. Unknown replies query original IDs; expiry pauses work. Explicit background-online mode retains the device signing key to submit versions needing no rewrapping and cache ciphertext, not read content. Rewrapping, reauthorization and local encrypted-state confirmation wait for unlock.

Public `Certification::batch` reads IC time once so queries depend on current batch time; a transport nonce alone cannot bypass replica caching by caller/method/arguments. This matches the [IC query cache](https://github.com/dfinity/ic/blob/master/rs/execution_environment/src/query_handler/query_cache.rs) and has PocketIC coverage for certificate refresh without account writes. Client/Worker 60-second freshness and rollback protection remain.

Payment `get_configuration_certified` provides `configuration`, `signer/<u64be>` and `fee/<u64be>` leaves for receiving routes, quote/receipt keys and fee policy. It changes no existing escrow terms or fund state machine.


## Inbox terminal-state management

Inbox `entries[].value` varies by state. `visible` retains signature/evidence/ciphertext; `aborted` returns only `order_id/sender/recipient/state/created_at/read/archived/resolved/abort_reason`, without ciphertext, signed envelope or evidence. Both share next_cursor and project latest values by order_id. Unaccepted/in-flight items are not inbox content.

Recipient status/reconcile/mark return the same redacted projection. Senders may query original orders for reconciliation. Post-admission refund does not reopen contact automatically; recipients explicitly set `reopen_contact=true` in `dmsg/inbox/mark/v1`. UI shows aborted as undelivered management state, without decrypting or labeling it an authenticated message. Repeated reopening does not decrement slots again.

## MCP Events relay endpoints

Local agents lack public webhook addresses. The relay issues device callback endpoints receiving upstream MCP Events draft webhook delivery with Standard Webhooks. Client code is `src/dmsg_client/src/mcp_events.rs`.

Commands/routes for `content_sign` devices:

| Action | Route | Payload |
| --- | --- | --- |
| `dmsg/mcp-events/endpoint/create/v1` | `POST /v1/accounts/:account/mcp-events/endpoints` | `{callback_id: bstr(16), secret}` |
| `dmsg/mcp-events/endpoint/rotate/v1` | `POST /v1/accounts/:account/mcp-events/endpoints/:callback/rotate` | `{callback_id, secret}` |
| `dmsg/mcp-events/endpoint/delete/v1` | `POST /v1/accounts/:account/mcp-events/endpoints/:callback/delete` | `{callback_id}` |
| `dmsg/mcp-events/ack/v1` | `POST /v1/accounts/:account/mcp-events/ack` | `{callback_id, through_seq}` |

- Devices generate callback_id, 32 lowercase hex characters in URL/JSON. Callback URL `<API origin>/mcp-events/<callback_id>` shares device API origin and comes from creation's url. Devices generate `secret` as whsec_ plus standard base64 of 24–64 random bytes; old keys verify for 24 hours after rotation. At most 16 active endpoints per account.
- Endpoints bind creator device and its HPKE key. Only it may rotate/read/ack; it or a RootManage device may delete. Revocation or loss of ContentSign suspends subsequent delivery.
- `GET /v1/accounts/:account/mcp-events/endpoints` lists endpoints without keys, including state (active/suspended/deleted), subscription_id for X-MCP-Subscription-Id verification, seq and pending count.
- `GET /v1/accounts/:account/mcp-events?endpoint=&after=&limit=` returns records `{seq, kind, webhook_id, received_at, sealed?, dropped?}` with seq > after and cursor, at most 100/about 1 MiB per page. Kinds are event/verification/gap/terminated/relay_gap. relay_gap has no sealed value; dropped counts unfetched records expired after 7 days. Acknowledgement deletes records; sequences are never reused.

`sealed` is base64url `CBOR({enc, ciphertext})`, HPKE Base X25519/HKDF-SHA256/AES-256-GCM with `info = aad = CBOR(["dmsg/mcp-event/1", account, device_id_hex, callback_id, seq])`. Plaintext is `CBOR({headers, body, received_at})`: lowercase webhook-id/webhook-timestamp/webhook-signature and optional x-mcp-subscription-id headers, original body bytes and relay receipt Unix milliseconds. The relay sees ingress plaintext for verification/type/challenge handling but stores ciphertext only.

Webhook ingress is only `POST /mcp-events/<callback_id>` with application/json and at most 262,144 bytes, plus `GET /.well-known/mcp-webhook-receiver.json`, outside device PoP/Origin checks. Timestamp outside ±300 seconds or invalid v1 returns 401; oversize 413, non-JSON 415, unknown/deleted/suspended endpoint 410; over 120/minute, 1,000 unacknowledged records or 16 MiB returns 429. Duplicate webhook-id returns the first result. Verified verification envelopes echo `{"challenge": …}`. Each new record emits an mcp-event hint (connect-v1 7.5).

Devices reverify v1 with locally retained keys and a 5-minute tolerance against received_at. If upstream publishes Ed25519 JWKs at `/.well-known/mcp-webhook-jwks.json`, also verify v1a over id.timestamp.body. Drop invalid v1a deliveries; the relay holds v1 keys and could otherwise forge such deliveries.

## Public profile pages

Handle queries/proofs use canonical lowercase names. Display certified display_handle when present, otherwise preserve URL casing for old records. Descriptions link only explicit HTTP/HTTPS addresses; other text is escaped.

Public routes are `/<account_id>` and `/<handle>`; permanent links use account IDs. Verify handles with `dmsg_handle.resolve_handle_certified`, select home by the registry allocator fingerprint, then read certified PrincipalDocument from its configured directory. Pages no longer read Cloudflare user profiles or show a device-signature-verified label.

A dedicated Web Worker reads configuration/handle/document in parallel and checks certificates, signatures and bindings. The main thread renders shell/loading first and only verified profile content. Navigation cancellation aborts related requests; worker restarts preserve session rollback records. Deployment home/directory IDs are speculative request hints, still checked against registry/home configuration, with fallback when stale. Verified configuration/profile proofs are cached in memory for 30 seconds, always revalidated against their original 60-second freshness; expired proofs refetch, and unpublished/not-found/failure results are not cached. Explicit retry bypasses cache; observed-document rollback checks remain.

JSON/HTML share source, fields and update time, with independent browser verification of directory certificates. Named links and `extra["dmsg.link_images"]` display together. Avatar/link images use document HTTPS URLs with `referrerpolicy="no-referrer"`; deployment CSP img-src must permit those origins. Legacy import creates a draft of name, description, named links, image extensions and avatar URL; only user publication writes it on chain.

Old public profiles remain `/legacy/users/<username>`, the owner's profile `/legacy/profile`. Missing new mappings may query frozen reservations to offer the old-profile link, never treating old owners as new accounts. Only public-avatar GET/HEAD/OPTIONS retains credential-free CORS; device APIs are not browser-accessible. See [PrincipalDocument](agent.md#unified-public-identity-document-2026-10-22).

## Cloud-copy management (2026-10-22)

UploadPlan optionally carries `management:{generation,ciphertext}`, at most 16 KiB of ciphertext. Uploaders encrypt JSON provenance/recovery metadata under the account root using COSE_Encrypt0 AES-256-GCM with canonical external AAD `['dmsg/storage-management/1',account_id,upload_id,object_id,version_id,generation]`. It records source file/version, name/size and link/attachment publication recovery information. The service lacks keys/plaintext. Without historical roots, show IDs, not invented names/links. Absent optional fields are omitted, leaving existing signature vectors unchanged.

| Interface | Contract |
| --- | --- |
| `GET /v1/{accounts,channels}/:id/storage?after=&limit=` | At most 100 uploads per page with entries/next_cursor/end, filtered by account/channel epoch permissions. Includes kind, bytes, payer/uploader, generation, status, charged_bytes/reserved_bytes (null if unknown), collection capabilities/errors and owner management ciphertext; even empty pages end only by end |
| `GET /v1/accounts/:id/storage/ledger` | Paginated owner's charges/current totals, consumer/charge/upload IDs and balances; grants no content/deletion authority |
| `GET /v1/{accounts,channels}/:id/storage/deletions` | Visible tombstones by record sequence; retain original messages and show attachments deleted |
| `POST /v1/{accounts,channels}/:id/storage/retry` | `dmsg/storage/retry/v1 {upload_id,generation}` retries existing cleanup only; no new deletion or early quota release |

`dmsg/channel/message/v1` adds optional attachment_upload, supplied by current attachment messages to bind the sender's committed file in the current epoch. Original-message replay returns its receipt; republishing the attachment, including after deletion, fails. A sender or owner/admin still entitled to read that epoch may collect it through dmsg/object/collect/v1. Billing-transfer periods and envelopes reject deletion; payer identity grants no authority. Afterwards object reads return 410 while the original message/deletion evidence remain.

Accepted deletion is separate from physical deleted and quota released. Show freed space only when deleted and both charged_bytes/reserved_bytes are 0; unknown balances/failures stay pending. Recovery uses original upload/share/message IDs. Other devices may decrypt uploaded management records to resume unpublished links/messages; completed publication never resends. Keep locally unconfirmed reservations too: one remote NotFound before expiry cannot prove cancellation.

The desktop Cloud storage interface was removed on 2026-10-28. The generic daemon APIs `files/storage`, `files/storage_refresh` and `files/storage_action` are **deprecated** and remain callable for explicit clients. Membership shows usage through `files/cloud_usage`; Files keeps dedicated Links and Cloud backups views, using their content-specific APIs. The underlying HTTP contracts above remain supported for encrypted provenance, recovery and deletion synchronization. Local deletion does not affect cloud copies; cloud deletion cannot recall downloads. Inventory scans are not atomic across resources; discrepancies require refreshing, never editing quotas. `print_storage_management_vector` emits Rust interoperability vectors.

## Direct private-file reads (2026-10-23)

Ordinary backup/file/share authorization is `GET /v1/{accounts|channels}/:resource/objects/:upload/download` with HTTP PoP. Success data is `{upload_id,expires_at,manifest:{url,size,digest},chunks:[{url,size,digest},...]}`. URLs authorize GET of exact objects for at most 60 seconds, bounded by request/evidence expiry. Clients compare correspondence, actual size, SHA-256 and decrypted integrity to the uploader-signed plan/authenticated manifest; download responses do not replace uploader signatures.

Bytes come directly from private S3 without device proofs, Cookie, Referer or redirects. Accept HTTPS R2 account domains including jurisdiction; local development may use the current loopback relay's /__r2/ adapter. URLs remain in operation memory, never local catalogs or ordinary share links. 403/404/transient failures cause one bounded reauthorization and retry of failed chunks only. Revocation/deletion ends at authorization; authorized-but-missing chunks fail integrity. Root/vault/epoch envelopes retain per-read device authentication.

Share descriptors remain `GET /public/shares/:share`; public /manifest and /chunks/:index are removed. The page generates a nonzero random 32-byte session ID and uses `POST /public/shares/:share/downloads/:session` to start/retry that session, GET on the same path to renew. Data adds session_expires_at; the session lasts at most one hour and no later than share expiry. Renewal neither extends it nor consumes another count; absent/expired sessions fail. Lost start replies reuse the ID; known sessions renew only by GET; explicit new downloads create new IDs. Session IDs are bearer secrets held only in page memory.

Count limits count new download sessions; started sessions can finish after the quota is exhausted. Revocation/deletion stops issuance, but old URLs may repeatedly read ciphertext until expiry/physical deletion. URLs contain no decryption key; in-progress transfers/saved copies cannot be recalled. Client completion follows whole-file verification. `downloads::tests::print_download_vector` emits cross-language fixtures whose fixed keys prove no real deployment.
