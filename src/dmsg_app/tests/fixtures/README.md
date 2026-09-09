# Public local test evidence

`execution-receipt.cbor` comes from the real dMsg Wasm integration test in PocketIC 16.0.0, with an NNS subnet for the certificate trust root. It contains only local test identities and public keys; no production data or private keys.

The CBOR tuple is `[root_key_der, observed_at_ms, certified_batch, signed_artifact, account_id, request_id, issuer]`. The SDK tests authenticate the actual BLS certificate and delegation chain before matching the receipt. The test clock is fixed to the recorded time; stale, altered, substituted and untrusted evidence must fail.

Regenerate after building the canisters, from the repository root:

```sh
DMSG_EXPORT_RECEIPT=/tmp/dmsg-execution-receipt.cbor \
POCKET_IC_BIN=/path/to/pocket-ic \
cargo test -p dmsg_integration --features pocketic-tests --test control_plane \
  identity_roots_certification_formal_signing_and_upgrade
cp /tmp/dmsg-execution-receipt.cbor src/dmsg_app/tests/fixtures/execution-receipt.cbor
```
