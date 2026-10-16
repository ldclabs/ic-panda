# dMsg team wallet vectors

Language-independent byte rules of `dmsg-wallet/1` (the public contract is
[wallet_zh.md](../../../docs/protocol/wallet_zh.md)). Both files are generated
by the Rust implementation, compared on every test run, and rewritten with
`DMSG_WRITE_WALLET_VECTORS=1`. All keys, Principals and accounts are synthetic
and test-only.

| File | Content | Generator | Readers |
| --- | --- | --- | --- |
| `protocol-v1.json` | The commitments members and devices approve (proposal, decision, join, activation, cosign message), the activation deposit subaccount, a six-event audit chain, address encodings (ICP accounts, Bitcoin Taproot key path, P2WPKH and the native Taproot tree with leaves, control blocks and output scripts, EVM EIP-55, Solana base58) and recipient scripts | `dmsg_protocol` `wallet::vector_tests` | the same tests rebuild every preimage from its typed `value` |
| `plans-v1.json` | One plan per external-chain intent: Bitcoin Taproot key path, P2WPKH batch, Taproot cosigned and delayed script paths; EVM native, ERC-20 and nonce cancellation; Solana SOL batch, SPL with token-account creation, nonce-account creation and a durable-nonce transfer; with deterministic signatures, the signed transaction and its ID, and six tampered plans | `dmsg_wallet` `chains::tests::vectors` (the canister's own builders, as `prepare_execution` calls them) | `dmsg_client` `wallet::tests::decodes_the_wallet_plan_vectors` (the client's independent decoder) |

```sh
cargo test -p dmsg_protocol wallet::vector_tests
cargo test -p dmsg_wallet chains::tests::vectors
cargo test -p dmsg_client wallet::tests
# after an intended change:
DMSG_WRITE_WALLET_VECTORS=1 cargo test -p dmsg_protocol wallet::vector_tests
DMSG_WRITE_WALLET_VECTORS=1 cargo test -p dmsg_wallet chains::tests::vectors
```

## Conventions

- `*_hex` fields are lowercase hex; amounts that may exceed 2^53 are decimal
  strings.
- A commitment is `SHA256(CBOR([1, domain, value]))` with core deterministic
  encoding. `value` uses the typed JSON form of
  [`tests/vectors/client`](../client/README.md) (`{"uint": "1"}`,
  `{"bytes": "00"}`, `{"map": [[k, v]]}`, ...); map entries appear in
  canonical order. Readers rebuild the array from `domain` and `value` and
  must match both `preimage_hex` and `digest_hex`.
- Audit events chain from 32 zero bytes; each event's `prev_hex` is the hash
  of the one before, and `head_hex` is the last hash.
- `view_cbor_hex`, `intent_cbor_hex` and `plan_cbor_hex` are the canonical
  CBOR of the Rust `WalletView`, `Intent` and `ExecutionPlan`. Readers in
  other languages can rely on `unsigned_hex`, `digests_hex`, `summary`,
  `signatures_hex` and `signed_hex` alone.
- A Bitcoin plan's unsigned bytes are the transaction without witnesses, so
  its ID is the reversed double SHA-256 of `unsigned_hex`. An EVM ID is the
  Keccak-256 of the signed bytes; a Solana ID is the base58 signature.
- Digests are what threshold keys sign, except on Solana, where Ed25519 signs
  the whole message and the digest is its SHA-256. A native Taproot cosigner
  signs the same BIP-341 sighash as the chain key.

## Not covered here

- **Policy evaluation and approval checks** (tiers, rolling windows, roles,
  `check_wallet_approval`) are logic, not bytes; `dmsg_protocol`
  `wallet::tests` and the PocketIC suites cover them.
- **Real threshold signatures** come only from the management canister; the
  PocketIC suite `control_plane::wallet` checks them with the `bitcoin`,
  `alloy` and Solana crates. The signatures here are from synthetic keys.
- **Cosigner envelopes** (FIDO2, WebAuthn) depend on authenticator data and
  counters; `dmsg_protocol` `wallet::tests` covers them.
