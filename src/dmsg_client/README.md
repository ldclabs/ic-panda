Rust core of the dMsg client: encryption, storage, accounts, sync and protocol
state machines for the `dmsg` daemon and, later, the mobile apps. It has no
operating-system or daemon dependencies of its own: no process locks, no
keychain, no HTTP server.

A [`Client`] is one device: its workspace, a login and, while unlocked, its
keys. The daemon in `src/dmsg_cli` owns one instance.

| Module | Role |
| --- | --- |
| [`codec`] | Canonical CBOR (RFC 8949 core deterministic) through `dmsg_protocol::canonical`; a bounded decoder for incoming records (24 levels, 50,000 items, 25,000 entries per container, strictly ascending map keys, no floats or foreign tags); strict base64url |
| [`crypto`] | `D(K, C)` (HKDF-SHA-256), untagged COSE_Encrypt0 with A256GCM, HPKE base mode (X25519, HKDF-SHA-256, AES-256-GCM) with the context as both info and AAD, Ed25519 device keys |
| [`config`] | Deployment parameters, fixed for a client's lifetime |
| [`store`] | SQLite workspace: plaintext metadata, sealed records, encrypted objects and item heads |
| [`workspace`] | Device keys, the local data key and the unlock chain: a provisional key before the device belongs to an account, then `LUK = HKDF(unlock_secret, ["dmsg/local-unlock/2", name])` |
| [`ic`] | `ic-agent` calls with Candid-typed replies; a lost update reply is `ExecutionUnknown` |
| [`account`] | Certified account state, the encrypted operation journal, account creation, device-approved mutations and pairing |
| [`root`] | RootBundle v2: wrapping, parsing, opening with the previous-root chain, and the reserve → wrap → upload → commit sequence |
| [`cloud`] | dmsg-cloud/1: security evidence, device commands and HTTP proof of possession, chunks, stored-command verification |
| [`upload`] | Account uploads shared by root bundles and vault objects |
| [`vault`] | Encrypted items, the cloud object container and manifest, and sync: upload and vault revision out, a verified export snapshot in |
| [`login`] | dmsg-login/1 on the client side: the session key, the login page URL and the callback check |

The byte rules follow the [encryption design](../../docs/dmsg_encryption_zh.md),
the [account and root contract](../../docs/protocol/account_root_zh.md), the
[cloud contract](../../docs/protocol/cloud_zh.md) and
[dmsg-login/1](../../docs/protocol/login-v1_zh.md). They are verified against
the language-independent vectors in
[tests/vectors/client](../../tests/vectors/client/README.md). Formats no
module implements yet (file chunks, channels, inbox) are read by the crate's
`tests` module from the same vectors.

```sh
cargo test -p dmsg_client
```

The end-to-end check against real canisters and the relay is
`tests/dmsg_integration/tests/desktop.rs`.
