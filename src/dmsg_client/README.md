Rust core of the dMsg client: encryption, storage, accounts, sync and protocol
state machines for the `dmsg` daemon and, later, the mobile apps. It has no
operating-system or daemon dependencies of its own: no process locks, no
HTTP server, and the credential store comes in through the
[`secure::SecureStorage`] trait.

A [`Client`] is one device: its workspace, a login and, while unlocked, its
keys. The daemon in `src/dmsg_cli` owns one instance.

| Module | Role |
| --- | --- |
| [`codec`] | Canonical CBOR (RFC 8949 core deterministic) through `dmsg_protocol::canonical`; a bounded decoder for incoming records (24 levels, 50,000 items, 25,000 entries per container, strictly ascending map keys, no floats or foreign tags); strict base64url |
| [`crypto`] | `D(K, C)` (HKDF-SHA-256), untagged COSE_Encrypt0 with A256GCM, HPKE base mode (X25519, HKDF-SHA-256, AES-256-GCM) with the context as both info and AAD, Ed25519 device keys |
| [`config`] | Deployment parameters and the background-online policy, fixed for a client's lifetime |
| [`store`] | SQLite workspace: plaintext metadata, sealed records, encrypted objects with their sync state, item heads, ciphertext chunks by SHA-256, and sync jobs |
| [`workspace`] | Device keys, the local data key and the unlock chain: a provisional key before the device belongs to an account, then `LUK = HKDF(unlock_secret, ["dmsg/local-unlock/2", name])`; fast unlock seals a second copy under a key the credential store keeps |
| [`secure`] | The credential-store trait: the desktop daemon uses the system store, mobile apps the Keychain or Keystore |
| [`ic`] | `ic-agent` calls with Candid-typed replies; a lost update reply is `ExecutionUnknown` |
| [`account`] | Certified security (public, no login needed) and account state (with the login), the encrypted operation journal, account creation, device-approved mutations and pairing |
| [`root`] | RootBundle v2: wrapping, parsing, opening with the previous-root chain, and the reserve → wrap → upload → commit sequence |
| [`cloud`] | dmsg-cloud/1: security evidence, device commands and HTTP proof of possession, presigned commands, chunks, stored-command verification |
| [`upload`] | Account uploads, resumable after the chunks already stored |
| [`content`] | Encrypted object versions, the cloud object container, upload manifests and local writes; a version on a stale base is a conflict copy |
| [`vault`] | Items and their versions: add, edit, trash, restore, local search, conflict copies and their resolution |
| [`files`] | Files up to 100 MiB in 1 MiB chunks with deterministic nonces; resumable import, verified export |
| [`sync`] | Content jobs, resumable uploads, presigned vault revisions, verified export snapshots, conflict copies and tombstones |
| [`profile`] | The public profile: publishing only chosen fields, reading one's own or another account's, checked against its devices |
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
| [`wallet`] | ICRC transfers and approvals by the login's Principal or a PEM wallet, journaled before they are sent; a lost reply resends the same bytes |
| [`commerce`] | The certified catalog and entitlement, cash checkouts and PANDA membership claims for the account's own subscription |
| [`handle`] | Names: certified resolution, legacy claims and registrations as account-authorized intents |
| [`agent`] | Agent Delegation with the account as principal: enabling it, controller keys kept as vault items, and grants and revocations signed here, journaled, then submitted to the delegation service |
| [`agents`] | The Agent IDs this device hosts: keys as vault items, their mail keys sealed on the device, residency in the credential store, the credential an agent's MCP session presents, and the capabilities granted over the account |

The byte rules follow the [encryption design](../../docs/dmsg_encryption_zh.md),
the [account and root contract](../../docs/protocol/account_root_zh.md), the
[cloud contract](../../docs/protocol/cloud_zh.md) and
[dmsg-login/1](../../docs/protocol/login-v1_zh.md),
[dmsg-connect/1](../../docs/protocol/connect-v1_zh.md) and the
[principal contract](../../docs/protocol/agent_zh.md). They are verified against
the language-independent vectors in
[tests/vectors/client](../../tests/vectors/client/README.md).

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

Pulling reads a complete export snapshot, checks every signed entry against
the certified devices and every chunk against its digest, caches the
ciphertext, opens every new version (a file's every chunk included), and
only then moves the heads, in one transaction. A snapshot older than one
already applied, or one that drops a version the relay accepted, is refused.

Conflicts: a local version the relay's head does not lead to becomes a
conflict copy and is still sent, so the relay keeps it for every device. The
relay refuses only a tombstone or a restore against a moved head; such a
version stays a conflict copy on its device. A deleted item takes nothing
but an explicit restore, so an older edit never revives it. Resolving a copy
writes a new head that names it, and the earlier copies of its branch, in
`resolvedConflicts`.

## Approvals

Every operation this device signs for a person waits in the approval center:
a formal statement, a capability change or revocation, a web request.
Anything may ask; only a person answers, by the digest of the request as it
was shown. Approving executes once. A formal attestation is signed and
journaled before it is sent, so a lost reply is settled by its execution ID
and never signed again; its artifact and certified receipt are kept and
export as a bundle that `attest::verify_bundle` checks without the device.

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
different operation.

## Payments

The dMsg account never holds funds: the payer is the login's Principal or a
PEM wallet the CLI names. Every ICRC transfer or approval is journaled with
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
of the new key over the request ID. A grant or revocation is approved in the
center, then signed on a device with `FormalApprove` with the nonce
`max(last + 1, created_at)`, journaled with that nonce, and submitted. A lost
reply or a 5xx/408/429 answer resubmits the same envelope; any other refusal
is final, and a `Max-Seen-Nonce` within the protocol's bound moves the next
nonce past it.

Agent IDs this device hosts are vault items too, named by a label: the same
label on another device of the account brings the same Agent ID. Unlocked,
the daemon signs with the vault copy; locked, only an Agent ID made resident,
whose seed the credential store also keeps. Each Agent ID may have one
credential for its MCP sessions, of which only a digest is kept. Capability
grants are sealed under the LDK; without one an agent reads only public
profiles. The Agent Mail decryption keys of an Agent ID's mailboxes come from
its connector, which keeps none: they are sealed under the LDK and never
synced, and a resident Agent ID keeps a copy beside its seed. Keys it makes
while the device is locked live in that copy until the next unlocked call
folds them in; turning residency off folds them in first, so it needs the
device unlocked.

## Locking

`lock` drops the login and every key. With `background_online` the device
signing key stays: presigned and already sealed versions are still sent and
ciphertext is still fetched, while nothing can be read, decided or resealed.
Fast unlock opens the stored root without a login for seven days after the
last login unlock; online, the device then checks that it is still approved.
A device its account revoked erases its workspace.

```sh
cargo test -p dmsg_client
```

The end-to-end checks against real canisters and the relay are in
`tests/dmsg_integration/tests/desktop.rs`.
