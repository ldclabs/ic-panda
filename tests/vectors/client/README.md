# dMsg client vectors

Language-independent byte rules of the dMsg client, exported in work package W0
from the browser extension `src/dmsg_app` before it is retired. The Rust client
(`src/dmsg_client`) must reproduce or open every item here; the files stay after
the extension is deleted (W9) and become the reference for later client work.
All keys, seeds and accounts are synthetic and test-only.

| File | Content | Rust test |
| --- | --- | --- |
| `primitives-v1.json` | Canonical CBOR encodings and rejections, base64url, Xid, `D(K, C)`, the local unlock key, COSE_Encrypt0 seals and rejections, HPKE with a fixed ephemeral key, Ed25519 | `codec::tests`, `crypto::tests` |
| `content-v1.json` | Encrypted objects (new, edited, tombstone, file entries), the cloud object container, file chunks and the chunk nonce rule, upload manifests | `tests::opens_content_objects`, `reproduces_file_chunks`, `opens_upload_manifests` |
| `root-v2.json` | RootBundle v2 for two generations: device envelopes, signature, the three on-chain digests, commitment and previous-root link | `tests::opens_root_bundles` |
| `channel-v1.json` | One channel epoch: rotation envelopes and manifest, key confirmation, a message, a history grant and an attachment | `tests::opens_channel_*` |
| `inbox-v1.json` | The inbox key object ID and a letter envelope | `tests::opens_inbox_letters` |
| `cloud-v1.json` | `dmsg-cloud/1` device command and HTTP PoP (COSE_Sign1) | `tests::reproduces_cloud_commands_and_http_pop` |

```sh
cargo test -p dmsg_client
pnpm --dir src/dmsg_app exec vitest run tests/client-vectors.test.ts tests/cloud.test.ts
```

## Generation

[`client-vectors.test.ts`](../../../src/dmsg_app/tests/client-vectors.test.ts)
runs the extension's real engine and primitives with `crypto.getRandomValues`
replaced by a SHA-256 counter stream keyed by the test name and `Date` fixed at
`1760000000000`. Every artifact, including HPKE ephemeral keys and the IBE seed,
is therefore reproducible: a normal run regenerates each file and fails unless
it matches byte for byte. To rewrite them:

```sh
DMSG_WRITE_CLIENT_VECTORS=1 pnpm --dir src/dmsg_app exec vitest run tests/client-vectors.test.ts
```

`cloud-v1.json` comes from `tests/cloud.test.ts` and is rewritten with
`DMSG_WRITE_CLOUD_VECTOR=1`. The private relay keeps its own copy
(`dmsg-core/interop/cloud-v1.json`); change both together.

## Conventions

- `*_hex` fields are lowercase hex; base64url strings appear exactly as on the wire.
- A typed value is the JSON form of one CBOR item, as in
  [`protocol_vectors.json`](../../../src/dmsg_types/tests/protocol_vectors.json):
  `{"uint": "1"}`, `{"int": "-1"}`, `{"text": ""}`, `{"bytes": "00"}`,
  `{"bool": true}`, `{"null": true}`, `{"array": []}`, `{"map": [[k, v]]}`,
  `{"tag": 2, "value": ...}`. Map entries are listed in source order; the
  canonical encoding sorts them.
- A context or AAD is `{"value": <typed>, "hex": <canonical bytes>}`. Readers
  must build it from record fields and match `hex`, not decode `value`.
- Objects other than `chunk_rules`, `seal` and `hpke` used engine-chosen keys
  and nonces; readers verify them by recomputing every context and digest and
  opening the ciphertext with the recorded key or seed.

## Not covered here

- **Device-local storage** (`dmsg/local-key/2`, `dmsg/device-bundle/1`, journals,
  job records, the WebAuthn PRF path): the desktop keeps its own SQLite storage
  and OS keychain, so these are not byte contracts.
- **Rules owned by the Rust crates**: approvals, COSE statements, account
  mutations, root digests, payment and agent formats have their own vectors in
  `dmsg_types` and `dmsg_protocol`.
- **The IBE recovery envelope** is a real vetKD round trip, covered by the
  PocketIC tests; `root-v2.json` only fixes its bytes inside the bundle.
- **Legacy migration** (`dmsg-legacy-pair/1`, archive decryption) is replaced in
  W10, which exports its own vectors before the extension is deleted.
- **The browser bridge** (`dmsg-extension/4`) is retired; see
  [dmsg-connect/1](../../../docs/protocol/connect-v1_zh.md).
