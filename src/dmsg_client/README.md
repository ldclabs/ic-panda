Rust core of the dMsg client: encryption, storage, accounts, sync and protocol
state machines for the `dmsg` daemon and, later, the mobile apps. It has no
operating-system or daemon dependencies of its own: no process locks, no
HTTP server, and the credential store comes in through the
[`secure::SecureStorage`] trait.

A [`Client`] is one device: its workspaces, a login and, while unlocked, the
keys of the selected one. Each account has its own workspace, with its own
device keys, under `accounts/<account>/`; a login without an account uses
`logins/<principal>/` until it has one. A login selects its account's
workspace, so accounts sharing a computer never see each other's data. The
daemon in `src/dmsg_cli` owns one instance.

| Module | Role |
| --- | --- |
| [`codec`] | Canonical CBOR (RFC 8949 core deterministic) through `dmsg_protocol::canonical`; a bounded decoder for incoming records (24 levels, 50,000 items, 25,000 entries per container, strictly ascending map keys, no floats or foreign tags) whose type keeps every entry, refusing unknown fields while filling by default the fields an older record lacks; strict base64url |
| [`crypto`] | `D(K, C)` (HKDF-SHA-256), untagged COSE_Encrypt0 with A256GCM, HPKE base mode (X25519, HKDF-SHA-256, AES-256-GCM) with the context as both info and AAD, Ed25519 device keys |
| [`config`] | Deployment parameters and the background-online policy, fixed for a client's lifetime |
| [`store`] | SQLite workspace: plaintext metadata, sealed records, encrypted objects with their sync state, item heads, ciphertext chunks by SHA-256, sync jobs, and the file library's backup jobs in a table of their own |
| [`workspace`] | Device keys, the local data key and the unlock chain: a provisional key before the device belongs to an account, then `LUK = HKDF(unlock_secret, ["dmsg/local-unlock/2", name])`; fast unlock seals a second copy under a key the credential store keeps |
| [`secure`] | The credential-store trait: the desktop daemon uses the system store, mobile apps the Keychain or Keystore |
| [`ic`] | `ic-agent` calls with Candid-typed replies; a lost update reply is `ExecutionUnknown` |
| [`hold`] | Where an operation finds the client: held by its one caller throughout (`Client::hold`), or taken for each synchronous step by a caller that shares it, as the daemon does. Relay requests are signed in a step and sent after it, and the agents for the IC and the user homes leave the client the same way, so a shared client never waits on the network |
| [`account`] | Certified security (public, no login needed) and account state (with the login), the encrypted operation journal, account creation, device-approved mutations and pairing |
| [`security`] | Bound logins (`BindAuth` and `accept_auth_binding`, across devices by a token or on one device with a second login), removing a login, the sensitive-execution policy, the recovery delay and disputes, each through the approval center |
| [`succession`] | A planned device replacement: the new device's `dmsg-succession/1` request, the old device's review and approval, and each device's task of the handover steps it saw done |
| [`transfer`] | dmsg-device-transfer/1: local content from one active device of the account to another over a local connection. The signed hello with the session key sealed by HPKE, AES-256-GCM frames, the manifest of transferable record names, resumable sessions and the receiver's additive merge; the host supplies the connection |
| [`recovery`] | Recovery after every device is lost: the delayed takeover request, its completion, the one vetKD derivation of the committed root and opening the bundle's recovery envelope, then a new root |
| [`root`] | RootBundle v2: wrapping, parsing, opening with the previous-root chain, and the reserve → wrap → upload → commit sequence |
| [`cloud`] | dmsg-cloud/1: security evidence, device commands and HTTP proof of possession, presigned commands, chunks, stored-command verification |
| [`upload`] | Account uploads, resumable after the chunks already stored |
| [`content`] | Encrypted object versions, the cloud object container, upload manifests and local writes; a version on a stale base is a conflict copy |
| [`vault`] | Items and their versions: add, edit, trash, restore, local search, conflict copies and their resolution |
| [`files`] | The file format: 1 MiB chunks with deterministic nonces under a per-version key, verified reads and exports, and the `BlobStore` trait over where chunks live (the store's `chunks` table or the library's directory) |
| [`library`] | dMsg Files: the local encrypted file library. Folders, files and versions as sealed records indexed in memory while unlocked; chunks as files in `blobs/`; resumable imports with type sniffing and thumbnails; opened copies with change detection; the trash and chunk collection; shared editions under their own keys; what the daemon's loopback WebDAV mount and agents need (paths, single chunks and byte ranges read chunk by chunk, copies, an editor's safe save as a version); share links for people outside dMsg (`share` editions, manifests sealed under the share key, Argon2id password wraps, the links' records); explicit backups as `kind=backup` uploads, in passes the caller may bound in time, the account's backup inventory, fetching, deleting and verifying cloud copies, and freeing verified versions. Nothing syncs by itself and no deletion crosses to the other side |
| [`sync`] | Content jobs, resumable uploads, presigned vault revisions, verified export snapshots, conflict copies and tombstones |
| [`principal`] | Official PrincipalDocument, directory-certified JSON shared with the web page, journaled on-chain display updates, and immutable public avatar uploads; network reads use Hold |
| [`approval`] | The approval center: requests from the desktop, the CLI, agents and web pages, decided only by a person; their states, deadlines and limits |
| [`attest`] | Formal statements (text, digest, file) and application actions signed by this device, journaled before they are sent, reconciled by execution ID; portable bundles and their independent verification |
| [`connect`] | dmsg-connect/1 on the device: browser pairing, the relay mailbox, the checks of §6.3, authorization by compare-and-swap, replies sealed with HPKE in auth mode, extra delivery channels, `read`/`open` answers, the hint token |
| [`login`] | dmsg-login/1 on the client side: the session key, the login page URL and the callback check |
| [`evidence`] | Commands of other accounts as the relay stored them: the certified security evidence checked at the relay's acceptance time, the signing device and its signature |
| [`certified`] | Certified query answers: the certificate under the trusted root, its time, the canister's certified data, and one path proven present or absent |
| [`channel_log`] | The formal channel's control contract: genesis, members, roles, invitations, history grants and epochs, replayed independently of the relay |
| [`channel`] | Formal channels: the synced keyring, the verified control log, epoch rotation by a leased coordinator, messages and attachments, history grants, owner and sponsor transfers, reactivation, and journals reconciled by request ID |
| [`inbox`] | The contact policy and its synced inbox key, sealed letters, the paid-letter quote, escrow, admission and refund, and the recipient's letters checked against the sender's evidence |
| [`payment`] | The `dmsg_payment` escrow: certified terms and escrows, funding checks, decisions, payouts and refunds |
| [`payer`] | ICRC transfers and approvals by the login's Principal or a PEM identity, journaled before they are sent; a lost reply resends the same bytes |
| [`wallet`] | Team wallets (`dmsg-wallet/1`): this device's wallet key that joins bind, the hub's quotes, the activation deposits that created no wallet and their refund to the payer, proposals, and approval-center jobs to create, join, decide and cosign; chain parameters read from Esplora, EVM and Solana JSON-RPC; an independent decoder that re-derives every plan's transaction and messages to sign before cosigning or broadcasting; addresses checked against the wallet's key roots; the audit chain; cosigner keys in a platform keystore (the daemon's Secure Enclave on macOS) or sealed in software |
| [`commerce`] | The certified catalog and entitlement, cash checkouts and PANDA membership claims for the account's own subscription and for web applications' checkouts, and the records the payer may read and repair as payer, merchant or operator |
| [`handle`] | Names: certified resolution, legacy claims, registrations, transfers between accounts and charge reconciliation, as account-authorized intents |
| [`agent`] | Agent Delegation with the account as principal: enabling it, controller keys kept as vault items, and grants and revocations signed here, journaled, then submitted to the delegation service |
| [`agents`] | The Agent IDs this device hosts: keys as vault items, their mail keys sealed on the device, residency in the credential store, the credential an agent's MCP session presents, and the capabilities granted over the account, never over dMsg's own keys |
| [`audit`] | The audit record: who did what on this device, when and through which way in, with IDs and never content; readable while locked, never synced |
| [`mcp_events`] | MCP Events relay endpoints: callback URLs and `whsec_` secrets on the relay, a creation kept before it is sent and settled when its reply is lost, deliveries fetched, opened with the device's HPKE key, checked again (`v1`, and `v1a` against the upstream's published keys), kept sealed under the LDK and read by cursor; `refresh_due` reminders |
| [`legacy`] | Legacy migration formats: the `dmsg-legacy-pair/2` offer and its fragment link, opening a `dmsg-legacy-transfer/1` file (offer, frame order and lengths checked), the `dmsg-legacy-archive/1` schema, offline verification of old COSE messages, attachments and avatars, and a shared channel's scoped archive |
| [`legacy_proof`] | Frozen-source evidence: certified `legacy_snapshot` and `legacy_folder_manifest` ingress proofs and their page series, a shared channel's frozen source, old-identity approvals, the inheritance digests, and the frozen comparison of a personal archive |
| [`migration`] | Personal archives: offers sealed on the device, import (the file's digest recorded first, duplicates by content), archives kept as `migration` and `migration_part` records that sync, offline reports, old attachments, frozen comparison |

The membership display adds `limits.active_devices` from the existing plan ID
(Free 2, Plus 3, Pro 5, Max 10). `catalog_display` also reads the wallet hub's
current `HubPricing.quotas` for `limits.team_wallets`; an omitted tier is zero,
while a failed read is null with `wallet_quotas_error`. Existing display fields,
signed plan snapshots and the `catalog_view` API remain compatible.

The byte rules follow the [encryption design](../../docs/dmsg_encryption_zh.md),
the [account and root contract](../../docs/protocol/account_root_zh.md), the
[cloud contract](../../docs/protocol/cloud_zh.md) and
[dmsg-login/1](../../docs/protocol/login-v1_zh.md),
[dmsg-connect/1](../../docs/protocol/connect-v1_zh.md), the
[principal contract](../../docs/protocol/agent_zh.md) and the legacy
[archive](../../docs/protocol/legacy_archive_zh.md) and
[channel inheritance](../../docs/protocol/channel_migration_zh.md) contracts. They are verified against
the language-independent vectors in
[tests/vectors/client](../../tests/vectors/client/README.md).

## Vault imports

Vault imports parse 1Password (`.1pux`, CSV), Bitwarden (unencrypted JSON,
CSV), and browser password CSV exports locally. Source files and the
decompressed `export.data` are limited to 64 MiB. Previews never include
hidden field values and confirm the digest of the entire source file;
only identical item content is unticked as a duplicate. Selected items,
encrypted chunks and sync jobs are saved in one transaction, so a failed
batch leaves no partial import. Malformed CSV quoting is rejected and
rows with the wrong column count are skipped. Bitwarden CSV custom fields
are preserved together as one hidden field because that format lacks
their types and unambiguous multiline boundaries. Attachments, passkeys,
password history and unsupported fields are not imported; the preview
counts affected items so people can check the originals before removing
their plaintext exports.

## Sync

Every local version gets a job when it is written: its uploads, with the
manifests already sealed under the root, and a request ID. Jobs hold only
ciphertext, signatures and progress. Pushing uploads the chunks, resuming
after the ones the job recorded, then signs the vault revision together with
the proofs to submit it and to read its operation, valid for up to 45
seconds. Until then the submission and its reconciliation need no key, so
they finish even after the device locks. A version sealed under an older
root is resealed under the current one, and a lapsed upload is replanned
under a new ID, only once the relay confirms the old submission can no
longer commit.

Uploads and revision submissions retry a transient transport failure once
before ending the pass. Uploads recheck the same immutable plan and resume
from saved chunk progress; revisions reconcile the original operation before
resubmitting the same signed command within its deadline. A persistent
failure leaves the remaining jobs queued for the next sync.
`last_sync` stores the error code and a fixed stage name (security,
reconcile, export_start, export_page, download, apply, upload or finish),
never the raw error message or resource identifiers. A successful pass
clears both. The daemon schedules transient failures for another pass while
unlocked, even when periodic sync is off.

`diagnostic` emits allowlisted subsystem/cause/HTTP status records through
the `dmsg::diagnostic` log target at network and storage failure boundaries.
It does not log raw errors, request paths, headers or response bodies, and
does not change error codes or the handling of unknown mutation outcomes.
The daemon alone enables this target and validates its schema again before
writing it; the core requires neither an OS logger nor a daemon to run.

IC and relay reads share a bounded transport policy: at most three attempts,
five seconds per attempt and twelve seconds total, including response bodies,
with jittered backoff. Chunk reads of up to 1 MiB allow thirty seconds per
attempt and sixty in total, so that a slow link can still carry them. Only
transport interruptions and HTTP 408/429/500/502/503/504 retry; certificate
failures, authority refusals and integrity failures do not. IC queries have a
fifteen-second overall budget; update submission and status waiting have a
ninety-second overall budget. Individual mutation HTTP requests have a
thirty-second budget and are sent once. The custom IC transport replaces SDK
retry middleware, and reqwest protocol retries are disabled.
`NETWORK_UNAVAILABLE` identifies exhausted transient reads. An uncertain
update remains `EXECUTION_UNKNOWN`; its original journal/operation ID must be
reconciled. Existing upload/revision recovery still reconciles before
resubmitting the same idempotent operation. Query signature verification,
certificate roots, response limits and session-generation checks remain
enabled.

Pulling reads a complete export snapshot, checks every signed entry against
the devices of the security epoch it was signed under (the certified
evidence the relay kept for that epoch, checked once per epoch, so devices
replaced since by recovery or at the device bound still count) and every
chunk against its digest, caches the
ciphertext, opens every new version (a file's every chunk included), and
only then moves the heads, in one transaction. A snapshot older than one
already applied, or one that drops a version the relay accepted, is refused.

The chunk store keeps what may still be read. At the end of a sync while
unlocked, a chunk is deleted unless the snapshot just applied names it (the
next pull checks every one, so a deleted one would be downloaded again), an
unsent version's job does (it replans from them), or a channel or root
upload in flight does whose plan has not lapsed: an upload reads chunks only
before its plan lapses, and channel uploads are listed in one sealed record
for that until then. Replaced versions', lapsed or abandoned
uploads' and channel downloads' chunks go; resolved conflict copies and
older versions stay, as the relay keeps them. The freed pages return to the
file system in a workspace created since (`auto_vacuum = INCREMENTAL`). The
file library's chunks are not in this table: they are files in the
workspace's `blobs/` directory, which the library collects on its own
against every version, thumbnail, unfinished import and shared edition in
flight.

Conflicts: a local version the relay's head does not lead to becomes a
conflict copy and is still sent, so the relay keeps it for every device. The
relay refuses only a tombstone or a restore against a moved head; such a
version stays a conflict copy on its device. A deleted item takes nothing
but an explicit restore, so an older edit never revives it. Resolving a copy
writes a new head that names it, and the earlier copies of its branch, in
`resolvedConflicts`.

## File operation responsiveness

The daemon uses `library::import` and `library::reading` through `Hold` for
imports, exports, previews and verification. Each encrypted chunk is handled
in a bounded client step; session checks stop work after locking or account
changes. Imports retain resumable jobs, and exports become visible only
after complete integrity verification. Thumbnail decoding, disk-usage scans
and checking copies opened in another program run outside the client lock.
Shared editions are also re-encrypted in chunk-sized steps, and password
derivation runs outside the lock. Temporary chunk references protect an
edition from collection until its upload plan has been journaled.

Backup planning persists jobs and returns without waiting for uploads.
Ordinary uploads send four chunks at a time, with progress recorded only
after a complete acknowledged batch. Failed passes preserve that cursor.
Existing upload reservations resume until their actual plan/lease deadline;
a lease with less than five minutes remaining is not prematurely discarded.
Cloud storage inventories read up to four independent channels together;
source failures remain explicit and deletion still requires confirmed
physical cleanup and quota release.

## Approvals

Every operation this device signs for a person waits in the approval center:
a formal statement, a device's enrollment, capability change or revocation,
a web request.
Anything may ask; only a person answers, by the digest of the request as it
was shown. A high-risk request also waits for the device owner's presence
when the device has a presence key: a payment, a device's enrollment,
capability change or revocation, a change to the account's logins, signing
policy or recovery delay or to its Agent principal, and a team wallet's
creation, cosignature or transfer. Approving executes once, and a finished request is
forgotten 30 days after it was made. A source other than the desktop and the
CLI keeps at most five requests waiting. A request waits ten minutes, a
high-risk one five, and an agent's request for capabilities an hour: its
owner may be away, but its waiting requests hold part of the queue. An
application action is shown by
its registered schema's labels, and only when the schema's hash matches; a
file statement records the file's name, size and time, and a person can hash
the file again before answering. A formal attestation is signed and journaled
before it is sent, so a lost reply is settled by its execution ID and never
signed again; past the result's lifetime it is `lost`. Its artifact and
certified receipt are kept as an `attestation` object that syncs to the
account's other devices, and export as a bundle that `attest::verify_bundle`
checks without the device, check by check (content, signature, evidence,
binding), against the deployment's user homes: any canister can certify a
receipt.

### Approval keys

An account may require an approval key's signature for some approvals
(`approval_key`, `docs/protocol/account_root.md`): changes to approval keys,
the policies, the recovery delay and logins; adding, revoking and changing
devices; enabling the Agent principal and registering or retiring its
controllers; optionally payments and formal attestations. A device makes its
key with `ApprovalKeys` (`secure::ApprovalKeys`, the Secure Enclave in the
daemon), keeps its handle sealed under the LDK and registers it through the
approval center (`register_request`): the request is approved like any
high-risk one and the key signs its proof of possession with that same
answer. Once keys are required, a device without a working one makes a key,
proves it right away (`key_request`) and hands the token to an administrator
device with a key (`approve_request`).

Any active device can register, replace or retire its own approval key;
managing another device's key needs an administrator with `RootManage`.
While the hardware policy is off, key maintenance does not sign with the
old registered key, so a lost key can be replaced or retired. A new key
still proves possession, and retirement or registering another device's
key uses ordinary presence confirmation. Once the policy is on, these
changes require the registered key's proof as usual.

On a device whose key the account registered, the person answers a
protected or high-risk request through the key instead of the presence
check (`approval::Ask::Hardware`): the daemon asks first, without holding the
client, and the answer then signs the hardware commitment of the approval
once it is assembled (`Client::hardware_proof`), so the person is asked once
and the wait holds neither the client nor the device sequence. An
attestation the account protects asks in its run, outside the client, before
it reads and signs. A required approval on a device without its key fails
with `APPROVAL_KEY_REQUIRED` before anything is sent; nothing falls back to a
software key. A fingerprint-only key that fails after the fingerprints
changed is reported lost (`INTEGRITY_FAILED`), and one that cannot ask now
(no Touch ID keyboard) unavailable. An unreadable fingerprint state is
unknown, not evidence that the fingerprints changed; device-bound unlock
follows the same rule and keeps its hardware slot.

### Device succession

A new device replaces an administrator device of an account at its plan's
device limit (`succession`, `AddSuccessor` in the
[account contract](../../docs/protocol/account_root.md)). Before it joins, the
new device signs its enrollment with the old device's role and capabilities
(`request`) and, where its hardware keeps them, makes a new approval key and
cosigner key and proves both; the person is asked once for each. The token
(`dmsg-succession/1`, base64url, at most 8 KiB) lasts as long as a pairing
request. The old device checks it (`inspect_succession`: its account, its own
device ID as the one replaced, unexpired, every proof valid) and approves it
as a `DevicePair` with `succession: true`: the user home enrolls the new
device beyond the limit, and the old device rotates the content root to it.

Each device keeps a task of the succession, sealed under the LDK as
`succession:<request ID>`, with the steps it saw done: the enrollment, the
new approval key (the old device registers the token's key), the cosigner
keys (the old device rotates its keys in every wallet to the new one, before
the bindings move, since rotating calls the wallets as the old device), local
content, the wallet bindings (the new device joins each wallet replacing the
old one), native addresses, and the old device's revocation by the new one,
offered once content and native addresses are settled. `status` reads the
account and records the enrollment, the key and the revocation on either
device by itself. Until device-to-device transfer and native-address
migration exist, those two steps can only be left, listing what stays.
Cancelling keeps what was committed; revoking the new device undoes its
enrollment.

### Device transfer

`transfer` moves what the account's cloud does not hold, today the file
library, from one active device to another over a connection the host opens
([dmsg-device-transfer/1](../../docs/protocol/device_transfer.md)). The
receiver opens a session with a random ID, and the sender's hello carries a
new session key sealed to the receiver's HPKE key and signed by the sender's
device key; both check each other against the certified account state at
the start and again before the end. Every later frame is AES-256-GCM under
that key, bound to the session, the direction and the sequence number.

Only record names on the whitelist (`transfer::PREFIXES`, which classifies
every prefix the client writes) move. The receiver keeps the manifest sealed
as `transfer:<session>`, so a broken connection resumes with a new key and
asks only for what it lacks. It asks for a record's chunks before the record
and commits folders, versions and files in that order; a version is local
only when all of its chunks arrived. The merge only adds: records already
there stay as they are, files gain the versions that arrived, and nothing is
restored from the trash or deleted. The sender changes nothing on its side.

A web request (dmsg-connect/1) reaches the center only after the device
opened it, checked the pairing grant against the certified devices, the
request and page signatures, the certified application registration and the
payload; anything else is refused at once with a code. Approving journals the
decision, then authorizes the request on the relay: a compare-and-swap from
`pending` that pins it to this device. If another device or a cancellation
wins, nothing executes. Each state change is sealed to every page that asked
for the operation, with HPKE in auth mode under the device key. Locked, the
device only stores sealed envelopes, and the relay's hint channel, which needs
no signing key, tells it that something waits.

## Channels

The account keeps one synced object per channel: its name, the genesis it
trusts and every epoch key it holds, so a device added later reads the old
epochs. The verified control log, received messages and the journals of
operations in flight stay sealed on the device. Every control and every
other account's command is checked against the security evidence the relay
stored with it, at the time the relay accepted it, and the channel is
replayed from the genesis with `channel_log`; a relay cannot invent a member
or reorder the log. A membership change requires a new epoch: one member
takes a lease, seals the new key to every current member device, and
activates it with the recipients' digest and a key confirmation. An
operation keeps its request ID across retries and is re-signed under the
same ID only after its deadline, so a lost reply never becomes a second,
different operation. The journals of a sent message, a finished rotation
and a history grant go once their outcome is stored; the relay then refuses
the message's ID with other bytes.

An established channel reads the current head first and fetches control
pages only when it changed, still verifying every new control. With fresh
security and an unchanged head, sending text takes one head read and one
publish request. The checked durable ACK stores the outgoing message
locally without advancing the contiguous receive cursor; history sync fills
any earlier gap independently. Invitations and rotations use `Hold` during
network waits, and uploads batch up to eight device envelopes plus their
manifest before finalizing. These are client optimizations; they neither
change the wire contract nor relax security-evidence freshness.

## Team wallets

A team wallet signs every transaction itself; this client never holds a
chain key. It calls wallets with a wallet key of this device (an Ed25519 key
sealed under the LDK), which joining binds with a `WalletApprove` device
approval, because login delegations reach only fixed canisters. Creating,
joining, deciding and cosigning are approval-center jobs: the device signs an
`ApplicationApproval` for the exact commitment, the user home records it,
and the wallet or hub verifies it there. Before cosigning or broadcasting,
`wallet::decode::verify_plan` parses the plan's unsigned bytes on its own,
recomputes every message to sign and compares them with the intent and the
wallet's summary; it accepts every plan of
[tests/vectors/wallet/plans-v1.json](../../tests/vectors/wallet/README.md),
which the wallet's own builders produce, and refuses the tampered ones.
Addresses are re-derived from the wallet's key roots with
`ic-secp256k1` and `ic-ed25519`. The wallet record is read certified, and
each proposal's digest is recomputed from what it shows, so no single
replica can change what a person approves or cosigns.

## Payments

The dMsg account never holds funds: the payer is the login's Principal or a
PEM identity the CLI names (team wallets hold their own funds; see [`wallet`]). Every ICRC transfer or approval is journaled with
its exact arguments before it is sent; a lost reply sends the same bytes
again and the ledger's deduplication returns the original block. Only a
typed ledger rejection on a first attempt proves nothing was paid.
Checkouts, paid letters and name operations are journals fixed at their
quote: a person approves their digest in the approval center, then each step
(open, fund, admit, settle) is reconciled by its order or operation ID.

## Agents

The account may be an Agent Delegation principal at
`principal_origin/<account>`, published by the directory. Its controller keys
are Ed25519 seeds kept as vault items whose IDs their generation determines,
so every device holding the root signs with them. Enabling the principal and
registering, retiring, distrusting or renaming a controller are account
mutations an administrator device approves; registration proves possession
of the new key over the request ID. A grant or revocation goes to the newest
current controller whose ceiling covers its scopes and relying parties (for
a revocation or a replacement, those of the credential as the delegation
service holds it), so a restricted controller is never asked for what the
service would refuse; a grant may replace a credential under its ID, for
the same subject and no more than its scopes and relying parties. It is
approved in the center, then signed on a device with `FormalApprove` with the nonce
`max(last + 1, created_at)`, journaled with that nonce, and submitted. A lost
reply or a 5xx/408/429 answer resubmits the same envelope. A definitive
`409 nonce_not_greater` permits one journaled re-sign using a `Max-Seen-Nonce`
within the protocol's bound, under the unchanged approval; other refusals are final.

Agent IDs this device hosts are vault items too, named by a label: the same
label on another device of the account brings the same Agent ID. Unlocked,
the daemon signs with the vault copy; locked, only an Agent ID made resident,
whose seed the credential store also keeps. Each Agent ID may have one
credential for its MCP sessions, of which only a digest is kept; the owner
may withdraw it, or pause the Agent ID so it reaches nothing until resumed.
Removing an Agent ID revokes its grants and deletes its credential, resident
copies and mail keys; its key stays in the vault for the same label to bring
it back. Capability grants are sealed under the LDK, and one for an Agent
Protocols service names the origins it covers; without one an agent reads
only public profiles and reaches no service's content. An agent may ask for
capabilities: an approval request that grants them, once, under the
request's own ID when a person approves it. A resident Agent ID's live
grants are copied beside its seed, and it is checked against the copy while
locked. The Agent Mail decryption keys of an Agent ID's mailboxes come from
its connector, which keeps none: they are sealed under the LDK and never
synced, and a resident Agent ID keeps a copy beside its seed. Keys it makes
while the device is locked live in that copy until the next unlocked call
folds them in; turning residency off folds them in first, so it needs the
device unlocked.

## MCP Events

An endpoint is a callback URL on the relay's origin and a `whsec_`
secret, created for one upstream MCP Events subscription by the owner or an
Agent ID with `events.endpoint`. The relay checks the Standard Webhooks `v1`
signature of each delivery and seals the raw headers and body to this
device's HPKE key, with `info = aad = CBOR(["dmsg/mcp-event/1", account,
device, callback, seq])`. The device opens each record at its place in the
endpoint's stream, checks `v1` again with its own copies of the secrets
(within five minutes of the time the relay received it, so a delivery
fetched days later still verifies) and, when the upstream server bound to
the endpoint publishes Ed25519 keys at `/.well-known/mcp-webhook-jwks.json`,
the `v1a` signature: a delivery whose `v1a` fails is dropped, since only the
relay holds `v1`. Kept deliveries are sealed under the LDK in
`workspace.sqlite`, once per endpoint and `webhook-id`, for 30 days or the
newest 10,000; endpoints, secrets and the agents' context stay on this
device and never sync. Fetching needs the device unlocked.

Retention records each endpoint's highest deleted local sequence in the same
SQLite transaction as the deletion. Reads whose cursor precedes that position
return `truncated: true`, including when every record was pruned. Empty pages
advance past the lost position; paginated reads still deliver surviving older
records before advancing. Explicit acknowledgements create no loss marker,
and gaps in the global sequence caused by another endpoint are not losses.
The markers survive reopening the workspace and are removed when an ended
endpoint expires locally. The stdio server propagates truncation to push
subscribers before delivering the corresponding page.

A creation is sealed before it is sent and sent up to three times on a lost
reply; the relay replays a command with the same request ID and payload, so
at most one endpoint results. When every reply is lost the caller gets
`EXECUTION_UNKNOWN` and never the secret, and each fetch pass resubmits the
creation and deletes the endpoint it made; a creation not settled within
seven days is forgotten. The owner's endpoint list shows the creations still
settling and the endpoints of this device on the relay that it never handed
out.

## Account setup

`initialize_account` creates and binds a new account or finishes its initial
content root after an interruption. It reconciles the journal before checking
certified state, reuses a valid initial root job, and opens an existing committed
root without rotating it. `Status.setup_pending` is a UI hint for a bound
device whose last certified state has no content root; the operation always
checks fresh state. Account creation alone (`create_account`) still only
registers and binds the device.

Root, vault, account-file, backup, share and avatar upload plans leave five
minutes below the relay's 24-hour limit; channel plans retain their shorter
one-hour lifetime. Authentication and certificate freshness remain unchanged.
On `RESULT_EXPIRED`, root setup reconciles the old upload, cancels unfinished
staging, and reuses a committed upload. Otherwise it persists a new upload ID
and deadline while retaining the same root key, bundle bytes and IC reservation,
then retries once. This also resumes saved plans from older releases. Unknown
reads, cancellations and commits retain the original job for reconciliation.

## Locking

`lock` drops the login and every key. With `background_online` the device
signing key stays: presigned and already sealed versions are still sent and
ciphertext is still fetched, while nothing can be read, decided or resealed.
Fast unlock opens the stored root without a login for thirty days after the
last login unlock, which turns it on unless the person turned it off; online,
the device then checks that it is still approved. With a `PresenceKey`
(`secure::PresenceKey`, the Secure Enclave in the daemon) the credential
store keeps only the hardware's handle and peer key, every fast unlock asks
for the person, and the login of the last login unlock, sealed under the LDK,
comes back until it expires. A device its account revoked erases its
workspace.

`bind_device` binds the workspace to the device's hardware: a key-agreement
key in a credential-store slot of its own joins the LUK for the login unlock
and alone opens fast unlock, so every unlock needs the hardware; the LDK and
every record stay. Neither turning fast unlock off, signing out nor any
failure deletes the slot or falls back to the LUK alone; only a reset or
`unbind_device` does. It protects only copies of the workspace made after
binding; the device key kept with `background_online` and resident Agent
IDs' copies are outside it, and binding returns them.

Unlock returns before content sync. Hosts must schedule `sync::sync(&hold)`
after an unlocked result, as the daemon does. A matching stored root needs no
relay request; the online device revocation check still precedes access.
Content sync waits on the network outside `Hold::with`; callers serialize it
with flush passes. Locking invalidates an in-flight pass, including when the
same account is reopened before its response arrives.

```sh
cargo test -p dmsg_client
```

The end-to-end checks against real canisters and the relay are in
`tests/dmsg_integration/tests/desktop.rs`.

### Wallet execution and desktop forms

`wallet::ops` and wallet approval jobs run through `Hold`; their IC and chain waits do not hold the daemon client. `wallet::desktop` implements typed management/transfer forms, precise amounts, metadata, previews, receive QR codes, account-local RPC preferences and encrypted channel hints. `wallet::packets` checks and collects signatures for one immutable plan; import never executes. `wallet::audit` verifies incremental checkpoints and bounded historical pages. The [team-wallet guide](../../docs/dMsg_wallet.md) distinguishes these paths from future hardware and chain extensions.

Wallet display reads certified state and statistics together. Transfer previews
run independent fee, balance and token-metadata reads concurrently; explicit
ICRC ledgers and external chains do not depend on a hub-config lookup. Activation
deposits and distinct balance metadata use bounded concurrency, with time checks
after responses. Audit pages reuse the final page verified in the same read;
certified heads and execution-time validation remain authoritative.

## Cloud storage management

**Deprecated (2026-10-28):** `Client::files_storage`, `library::storage::refresh`
and `library::storage::act`. The generic desktop cloud-space page has been removed;
these APIs remain callable for explicit clients. Use `library::cloud::cloud_usage`
for account usage, `library::link` for share-link management, and `library::cloud`
for backups. Encryption, provenance synchronization and sharing estimates in
`library::storage` remain supported.

`library::storage` provides an encrypted local inventory snapshot, explicit paginated
refresh and quota reconciliation, sharing estimates, and bounded per-object management
outcomes. Upload plans optionally carry account-root-encrypted source and recovery
metadata, bound to the exact account/upload/object/version/generation context. The
relay receives neither filenames nor link keys in plaintext. Link publication and
channel attachment publication recover by their original IDs; local pending share
jobs keep their chunks until completion or confirmed cancellation.

Channel senders and owner/admin members with historical read access may retire file
attachments. Signed messages remain; incremental deletion markers let readers show
an unavailable attachment without treating it as an empty file. Epoch envelopes and
other recovery material retain their protection. Payer accounting does not imply
content access. UI release state requires both physical deletion and a settled zero
charge; failed cleanup can retry without issuing a new deletion generation.

The optional wire fields retain existing canonical vectors. To produce the public
storage-management vector using fixed test-only keys, set `DMSG_STORAGE_VECTOR_OUT`
and run `cargo test -p dmsg_client --lib print_storage_management_vector -- --ignored`.

### Private file downloads (2026-10-23)

Backups, channel files and their previews, and file objects in account synchronization use short-lived GET URLs from the authenticated `/objects/:upload/download` endpoint. The client authorizes an object once per fetch phase, batches up to eight direct reads, renews expiring grants, and retries failed blocks once after a fresh authority check. It bounds response sizes and verifies expected ciphertext digests before storing anything. URLs remain in operation memory; no device credentials or URL-bearing errors go to the data host. Root/vault/epoch-envelope reads retain device authentication. See the public [wire contract](../../docs/protocol/cloud_zh.md#私有文件直读2026-10-23) for expiry, deletion and bearer semantics. The independent download PoP fixture producer is an explicitly ignored test.

## Approved credential use

`secrets` holds device-local LDK-sealed policies, strict current-field references
and immutable use plans bound to a specific grant and the Agent's pause
generation, so resuming or regranting cannot revive an old request. `SecretPolicy` and `SecretUse` reuse the approval
center. The first execution step rechecks the actor, credential generation,
capability, policy, revision and unlocked session, records `started`, then
passes a single fixed GitHub request outside `Hold::with`. Network uncertainty
is terminal `lost`, never reconciled by resending. No extra execution ledger
or synchronized content kind is introduced. The whole Vault item must be
decrypted internally to read metadata; no value leaves that boundary before
the approved execution. See [contract](../../docs/protocol/secrets_zh.md) and
[validation](../../docs/protocol/secrets_zh.md#验证入口与边界).


## Secrets P1 开发工作流

实现进程单次交付、SSH/Git 逐次审批、机密文件挂载、秘密快照分享/导入、受限扫描与轮换记录。公开数据流、owner 接口与限制见 [Secrets 合同](../../docs/protocol/secrets_zh.md)，平台和实际测试边界见 [验证范围](../../docs/protocol/secrets_zh.md#验证入口与边界)。这些 owner 功能不为 MCP 增加明文取密或任意目录访问接口；团队共享保险库仍暂缓。

## Agent Protocols 0.12.6

The shared SDK and protocol fixtures are aligned with 0.12.6. Mail publication
uses a detached, domain-separated per-Agent checkpoint codec: the daemon can
commit encrypted Mail keys before networking without holding the client lock or
retaining the account content root. Existing sealed Mail keys and resident copies
remain the ordinary key store; checkpoint restoration precedes subsequent calls.

A definitive `409 nonce_not_greater` may cause one bounded re-sign under the
original approval. The journal persists the retry decision and next nonce first;
the credential ID, controller generation, scopes, audiences and expiry remain
unchanged, and signing rechecks current device/controller authority. A restart
cannot reset the retry budget. Network failures and 5xx/408/429 responses still
replay the exact signed envelope; other refusals require a new approval.

## Local display cache

`display` owns the three fixed sealed records `profile-display`,
`profile-avatar`, and `public-names`. Published directory timestamps order
profile/name observations; failed authentication does not create an absence.
Publication confirmation writes the display projection in the same client step.
The avatar is returned only for the current published URL. The name map is
bounded to 2048 observations and evicts the oldest successful observation.

`cache` owns the reconstructible domain snapshots. Singleton names are fixed;
resource IDs use HMAC with an LDK-derived purpose key. Encrypted headers bind
snapshots to their actual home, trust root and configured service, without
invalidating all snapshots when the home shard list grows. Personalized and
wallet-endpoint-dependent views include their relevant source context.
JSON projections are stored as sealed 512 KiB byte pages with a whole-snapshot
digest, atomically with their header. Each snapshot is at most 16 MiB; each domain
is bounded to 32 MiB of sealed records and 4096 records. Old successful
observations are evicted first; reads do not write access timestamps.
Wallet history, asset metadata, Inbox policy and cloud inventory use this same
store. Unsupported old display formats are discarded, never migrated by
resetting a workspace. Missing/corrupt pages are a miss rather than a partial
success. A failed replacement keeps the prior complete snapshot.

These types are display data, never authorization evidence. `Scoped` checks the
captured account, unlock generation and cache-clear generation at each local
step. A cache clear cannot erase journals, outbox, roots, device credentials,
wallet audit verification checkpoints, protocol pins or user content. Offline
access still depends on existing unlock eligibility; no offline write queue or
new recovery capability is introduced.

Native snapshot producers use `cache_scope`/`cache_save_checked` across network
waits as well as the daemon's `Scoped` wrapper. Invalidation fences pending
writes of the same kind even when disk cleanup fails; other kinds go on. Capacity uses exact SQLite counts and byte
sums rather than the first inventory page. Avatar downloads bind both URL and
publication version, so changing away from a URL and back cannot accept an older
in-flight image as the new publication.
