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
| [`login`] | dmsg-login/1 on the client side: the session key, the login page URL and the callback check |

The byte rules follow the [encryption design](../../docs/dmsg_encryption_zh.md),
the [account and root contract](../../docs/protocol/account_root_zh.md), the
[cloud contract](../../docs/protocol/cloud_zh.md) and
[dmsg-login/1](../../docs/protocol/login-v1_zh.md). They are verified against
the language-independent vectors in
[tests/vectors/client](../../tests/vectors/client/README.md). Formats no
module implements yet (channels, inbox) are read by the crate's `tests`
module from the same vectors.

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
