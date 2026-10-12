Rust core of the dMsg client: encryption, storage, accounts, sync and protocol
state machines for the `dmsg` daemon and, later, the mobile apps. It has no
operating-system or daemon dependencies of its own.

Work package W0 provides the foundation every other module builds on:

- [`codec`]: canonical CBOR (RFC 8949 core deterministic) through
  `dmsg_protocol::canonical`, a bounded decoder for incoming records (24
  levels, 50,000 items, 25,000 entries per container, strictly ascending map
  keys, no floats or foreign tags) and strict base64url.
- [`crypto`]: `D(K, C)` (HKDF-SHA-256), untagged COSE_Encrypt0 with A256GCM,
  HPKE base mode (X25519, HKDF-SHA-256, AES-256-GCM) with the context as both
  info and AAD, and Ed25519 device keys.

The byte rules follow the [encryption design](../../docs/dmsg_encryption_zh.md)
§4 and are verified against the language-independent vectors in
[tests/vectors/client](../../tests/vectors/client/README.md), which the retired
browser extension exported. The crate's tests also rebuild every content,
file, upload-manifest, root-bundle, channel, inbox and cloud-command context
from record fields and open the exported ciphertexts; those tuples are the
rules the vault, files, root, channel and inbox modules take over.

```sh
cargo test -p dmsg_client
```
