# dmsg_types

English | [简体中文](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/README_zh.md)

`dmsg_types` defines the public Rust data contracts for dMsg document signing, account and device control, name ownership, and optional paid delivery. dMsg separates independently verifiable COSE documents from account authorization, key execution, and funds management on ICP. Integrating document verification does not require implementing the account, inbox, or payment systems.

This crate provides types, Serde/Candid representations, and a few conversion and result-access methods. **It does not sign, verify signatures, make network calls, or authorize operations.** Constructing a value or successfully deserializing it does not establish valid input, a valid signature, or caller permission. The companion [dmsg_protocol](https://github.com/ldclabs/ic-panda/tree/main/src/dmsg_protocol) crate provides encoding, validation, and approval-digest construction; the runtime and canisters implement storage and platform calls.

## Getting started and references

The package version in this repository is `0.1.0`. Consult `Cargo.toml` for publication settings; enabling publication does not mean the version has been uploaded to crates.io. During development, use a path dependency:

```toml
[dependencies]
dmsg_types = { path = "../ic-panda/src/dmsg_types" }
```

The path is relative to the consuming project's `Cargo.toml`; adjust it for your checkout. Once the version is published, use `dmsg_types = "0.1"`. Add the companion protocol crate when you also need protocol encoding or signature verification.

- [Public protocol and byte rules](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README.md): the starting point for independent implementations in other languages.
- [Statement CDDL](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/statements.cddl): COSE document structure.
- [Interoperability vectors](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/tests/protocol_vectors.json): fixed encoding and signature examples.
- [Canister implementation and validation boundaries](https://github.com/ldclabs/ic-panda/blob/main/docs/dmsg_canisters_zh.md): service responsibilities, builds, and deployment status.

These links point to the public repository's main branch, which changes during development. Pin types, protocol definitions, Candid interfaces, and vectors to the same commit for an integration. Rust API comments are in English for IDE and rustdoc use. The English README also serves as the crate documentation homepage; maintain both language versions together.

## Terminology

| Term | Meaning and boundaries |
| --- | --- |
| `AccountId` | Stable dMsg account identity allocated by the user canister: a 12-byte Xid, not an ICP Principal. Replacing login bindings or devices should not replace the account identity. |
| Principal / auth binding | ICP caller identity and its login binding to an account. Successful login does not automatically grant device approval or decryption rights. |
| ICRC `Account` | Ledger payment address consisting of an owner Principal and an optional 32-byte subaccount, distinct from a dMsg account ID. |
| handle | A registrable, transferable name pointing to an `AccountId`. Name ownership and login authentication are managed separately. |
| issuer / subject | `Statement.issuer` is the signer's canonical URI. Optional `subject` identifies the object being described; it is not the former account-ID field. |
| device / capability | Registered device public keys and explicit permissions. The device role `Administrator` is distinct from an ICP canister controller. |
| approval | A device signature authorizing a specific operation and its account security state, sequence, deadline, and other context. It is not a general login credential. |
| home user / home COSE | Fixed canisters responsible for account authorization and key execution, respectively. Derived key identity depends on the COSE home and derivation parameters. |
| content root / generation | The generation of a content root and a commitment to its external encrypted bundle. These types contain no plaintext root key; vetKD returns an encrypted derivation result. |
| security epoch / account version | The former invalidates stale security approvals; the latter supports optimistic concurrency for account mutations. They are not interchangeable. |
| artifact / execution receipt | A portable COSE signed document versus ICP execution evidence that separately binds a request, the bytes to sign, and the key. |
| offer / quote / admission receipt | Recipient payment authorization, fixed delivery terms, and proof of service admission, respectively. None establishes that a recipient read or replied to the message. |
| escrow / transfer leg | The escrow tracks held funds and the settlement/refund decision; a transfer leg records one specific outgoing ledger transfer. |

## Module guide

| Module | Main types | Use cases |
| --- | --- | --- |
| `signing` | `Statement`, `StatementContent`, `SignedArtifact`, `VerificationReport` | Prepare, exchange, and parse signed documents independently of ICP |
| `account_id` | `AccountId` | Stable account identity; re-exports `ic_auth_types::Xid` |
| `protocol` | `Hash`, `OpId`, `Approval`, `Error`, `CertifiedBatch` | Shared bytes, time units, device approvals, errors, and certified queries |
| `cose` | `SignRequest`, `DeriveRootRequest`, `KeyDescriptor`, `ExecutionResult`, `ExecutionReceipt` | ICP formal signing, vetKD, key provenance, and execution reconciliation |
| `user` | `AccountInfo`, `AccountMutation`, `Device`, `SecuritySnapshot` | Accounts, devices, recovery, and root commitments |
| `handle` | `HandleIntent`, `HandleRecord`, `HandleOperation` | Name registration, transfer, and frozen-name import |
| `payment` | `PaymentOffer`, `EscrowInfo`, `TransferLeg` | Recipient authorization, escrow accounting, and ledger transfers |
| `profiles::delivery` | `Quote`, `OpenEscrow`, `SignedReceipt` | Optional paid-delivery application protocol |
| `account` | `account_cbor` | Explicit Serde/CBOR representation of ICRC Account |

Types in `protocol` and `signing`, plus `AccountId`, are re-exported at the crate root. Import other types through their modules. An “optional” profile is a business protocol an integrator may choose not to use, not a Cargo feature.

## Rust examples

### Prepare document content

```rust
use dmsg_types::{Hash, Statement, StatementContent};

let statement = Statement {
    issuer: "https://example.org/signers/alice".into(),
    subject: Some("Release approval".into()),
    issued_at: Some(1_800_000_000), // Claimed Unix seconds, not an execution deadline.
    content: StatementContent::Text("I approve release 1.0.".into()),
};
assert!(matches!(statement.content, StatementContent::Text(_)));

// Compute SHA-256 over the original file bytes in a real integration; this only demonstrates construction.
let digest_content = StatementContent::Digest {
    sha256: Hash::new([0x42; 32]),
    content_type: Some("application/pdf".into()),
    location: None,
};
assert!(matches!(digest_content, StatementContent::Digest { .. }));
```

`Statement` is a preparation/parsed view. Do not serialize its Rust enum and use that encoding directly as the signing payload. The wire object is tagged COSE_Sign1: a text payload contains raw UTF-8 (1..4096 bytes), and a digest payload contains the original content's 32-byte SHA-256. The issuer, subject, and issued_at become protected CWT claims. Text is neither trimmed nor Unicode-normalized. URIs are identifiers and do not trigger automatic network discovery.

### Account text and time conversion

```rust
use dmsg_types::{AccountId, millis_to_nanos, nanos_to_millis, MINUTE};

// These bytes only demonstrate encoding; obtain real account IDs from the user canister's creation result.
let account = AccountId([1; 12]);
let text = account.to_string();
assert_eq!(text.len(), 20);
assert_eq!(text.parse::<AccountId>().unwrap(), account);

let deadline_ms = 1_800_000_000_000u64 + MINUTE;
let ledger_time_ns = millis_to_nanos(deadline_ms).unwrap();
assert_eq!(nanos_to_millis(ledger_time_ns), deadline_ms);
assert!(millis_to_nanos(u64::MAX).is_err()); // Overflow does not silently wrap.
```

### Distinguish completed, pending, and unknown outcomes

```rust
use dmsg_types::{cose::{ExecutionOutcome, ExecutionResult}, Error, Hash};

let result = ExecutionResult {
    request_id: Hash::new([7; 32]),
    outcome: ExecutionOutcome::Unknown(Error::ExecutionUnknown),
    charged_cycles: 0,
};
assert!(!result.is_terminal());
assert_eq!(result.output(), Err(Error::ExecutionUnknown));
// Query and reconcile the original request_id; do not automatically create another signing request.
```

`output()` returns `Error::Pending` while an operation is pending and `Error::ResultExpired` when its retained output has been removed. `is_terminal()` returns true only for Completed, Failed, and ResultExpired. Unknown does not mean the operation did not execute.

## Integration workflows

### Independent document verification

Obtain a `SignedArtifact` and use the companion `dmsg_protocol::verify_artifact` to check the COSE profile and mathematical signature. To verify a file, provide the original bytes to `verification_report`; without original content, a digest document's content status is `NotProvided`. The attached `cose_key` is only a public key and cannot establish the issuer's identity by itself. Check `issuer_binding`, `authorization`, `timestamp`, and `current_status` against their respective evidence, leaving them `NotChecked` when not verified.

Supported signing algorithms are Ed25519 (COSE -19) and ES256K (-47). vetKD derives content roots and is not a document-signature algorithm. An ordinary signature does not include a trusted timestamp: `Statement.issued_at` is the signer's time claim. Expiration of an execution approval does not automatically invalidate a completed document signature.

### ICP formal signing

1. Identify the user/COSE homes from deployment configuration, query the account and authenticated key descriptor, and check device permissions, `security_epoch`, and the device sequence.
2. Freeze the complete `Statement`, `SigningKeyRef`, extension-verified origin, and `max_cycles`. Use protocol helpers such as `SignRequestExt` to convert the request, construct the request ID and approval digest, and then sign with the device's Ed25519 key.
3. Call the user canister's `sign`, query the original request according to `ExecutionResult`, and extract `ExecutionOutput::Signature` on completion.
4. If you need evidence of dMsg execution authorization, query `get_execution_receipt`, verify the IC certificate/witness, and then match the artifact and receipt with `match_execution_receipt`. This Rust helper checks bindings only; it does not verify IC certificates.

A portable statement contains no request_id, origin, or execution deadline; these belong to the execution context. The device signature binds the origin string but cannot independently establish that the string came from the browser. The low-level `ExecutionGrant` is a restricted cross-canister contract. Its public type does not imply that any caller may execute it.

### Accounts, devices, and content roots

`AccountMutation` binds `expected_version`, the complete `AccountCommand`, and an `Approval`. After a version conflict, read the new state and prepare a new approval instead of editing the version of an already signed request. Enrolling a device requires proof of possession of the corresponding private key. Login Principals, device signing keys, and HPKE encryption keys have separate responsibilities.

Root rotation follows `ReserveRoot` → derive the candidate root/prepare its external encrypted bundle → `CommitRoot`. The operation ID, expected generation, and security state must match. `ContentRootRef` holds only a bundle commitment and derivation information. `VaultWriteState::RekeyRequired` prevents further writes using the old root. The user contract describes recovery-material versions, waiting periods, and dispute confirmation; these are distinct from local UI locking.

### Optional paid delivery

The recipient device signs a `PaymentOffer`, and the service creates and signs a `Quote`. The payer fixes the terms with `OpenEscrow`, then funds the escrow subaccount. After admission, the service returns a `SignedReceipt`; the payment canister verifies it before committing settlement or refund. See the [payment Candid interface](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_payment/dmsg_payment.did) for integration details.

`FundsDecision::SettlementCommitted` and `RefundCommitted` are mutually exclusive, but neither guarantees successful payout. Continue checking each `TransferLeg.status`. Reconcile unknown ledger outcomes first, preserving the original memo and nanosecond `created_at_time` on retries. `Quote.amount = recipient_net + service_fee + fee_reserve`. `EscrowInfo` satisfies `confirmed_in = liabilities + transferred + network_fees`. All amounts use the ledger's integer base units.

## Encoding, units, and certification

| Data | Contract |
| --- | --- |
| `AccountId` | 12 bytes; CBOR bstr / Candid blob. Human-readable Serde and display use a canonical 20-character lowercase base32hex Xid ending in 0 or g |
| `Hash` / `OpId` | 32 bytes; CBOR bstr / Candid blob with length validation on decode. These are aliases: Rust does not prevent mixing semantically different 32-byte values |
| COSE `kid` | Opaque byte string, 1..256 bytes in the document profile; do not infer account identity from length |
| ICRC Account | `{owner: bstr, subaccount: bstr(32) / null}`; contract fields use `account::account_cbor` for this explicit representation |
| Business time | `u64` Unix milliseconds; durations also use milliseconds. Deadlines generally require `now < expires_at` |
| Statement `issued_at` | Optional `i64` Unix seconds, unlike the milliseconds in `PaymentOffer.issued_at` |
| IC certificate / ICRC `created_at_time` | Unix nanoseconds; `nanos_to_millis` discards fractional milliseconds |
| Amounts / cycles | Amounts are `u128` ledger base units; cycles measure ICP execution cost and are a separate unit |

Serde defines the data representation. Deterministic encoding still requires the protocol crate or an independent implementation of the public specification: RFC 8949 core deterministic CBOR, with u128 values larger than u64 encoded using tag 2 and a shortest-form big-endian byte string. Do not pass amounts through JavaScript Number. Preserve the original COSE protected-header bytes during verification; do not reorder them before checking the signature.

The browser bridge uses the separate `dmsg-extension/3` JSON contract: accountId is Xid text, digests/requestId/nonce are lowercase hex, and large integers are decimal strings. Ordinary Rust Serde JSON is not the bridge protocol. Rust enum memory layout is not a wire format either.

Certified queries return `CertifiedBatch`. Verify the trusted IC root, expected canister, certificate time, witness path, and exact leaf bytes. The account security leaf uses a single raw AccountId path segment, and an escrow leaf uses a single raw escrow_id. Execution receipt paths are `b"execution/" || account_id || request_id`. `SecuritySnapshot.devices_root` commits to the complete device map, including revocation and sequence fields, rather than a reduced device list. Current account security snapshots use a freshness boundary of certificate time +60 seconds; do not mechanically apply that window to historical execution receipts. Neither `AccountInfo` nor `DeviceEvidence` alone is a certified proof.

## Error handling and interface references

| Situation | Integration response |
| --- | --- |
| `AuthRequired`, `DeviceNotApproved`, `Forbidden` | Check login bindings, device status, role, and capabilities rather than retrying blindly |
| `VersionConflict`, `PolicyStale` | Refresh state and prepare new authorization |
| `IdempotencyConflict` | Parameters changed under the same ID; fix client retry logic |
| `Pending`, `ExecutionUnknown` | Keep the original operation identity and query/reconcile; do not assume failure |
| `ResultExpired` | Output is beyond its retention period; this does not authorize replay of the old approval |
| `FeeBlocked` | Resolve ledger fees and approved limits; do not increase fees without authorization |
| `InvalidInput`, `UnsupportedProtocol` | Correct the request fields or protocol version; diagnostic strings are not stable error codes |

Transport timeouts and Candid decoding errors are not part of this crate's `Error`. A network failure before receiving a `Result<T>` may still require reconciliation using the original ID. Consult each service's Candid and public implementation for exact method signatures, caller restrictions, and parameter order:

- [user](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_user/dmsg_user.did)
- [COSE](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_cose/dmsg_cose.did)
- [handle](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_handle/dmsg_handle.did)
- [payment](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_payment/dmsg_payment.did)

## Local validation and publication preparation

Run from the workspace root:

```sh
cargo test -p dmsg_types --locked
RUSTDOCFLAGS="-D warnings" cargo doc -p dmsg_types --no-deps --locked
cargo run -p dmsg_types --example protocol_vectors --locked > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

Rust code blocks in the English README run as doctests. Keep example code identical in both versions, translating only comments. The crate enables `missing_docs` warnings; document new public items as they are added. See the public implementation documentation for protocol and real-canister integration tests.

Before publishing, review publication settings in `Cargo.toml`, how the repository-only local `dmsg_protocol` dev-dependency is handled in the release package, and run package/publish dry-runs against the actual package. Complete documentation does not establish production deployment, capacity, external-service readiness, or audit acceptance. These are development interfaces with no compatibility promise for earlier experimental encodings or stable-storage layouts.
