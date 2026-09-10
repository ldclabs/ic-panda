# dmsg_protocol

English | [简体中文](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_protocol/README_zh.md)

Deterministic encoding, document signature verification, and request helpers for dMsg. This crate implements the public protocol on top of [dmsg_types](https://github.com/ldclabs/ic-panda/tree/main/src/dmsg_types). It performs local computation only: no ICP calls, ledger queries, canister storage, network discovery, or account authorization.

Use it to prepare portable COSE documents, verify returned artifacts, construct device approval digests, and check bindings between documents and independently authenticated execution receipts. All public APIs are re-exported at the crate root; the source modules are implementation organization, not public import paths.

## Installation and dependency direction

During development, use paths relative to your application's Cargo.toml:

```toml
[dependencies]
dmsg_types = { path = "../ic-panda/src/dmsg_types" }
dmsg_protocol = { path = "../ic-panda/src/dmsg_protocol" }
```

Once both versions are published, the corresponding registry dependencies are:

```toml
[dependencies]
dmsg_types = "0.1"
dmsg_protocol = "0.1"
```

The runtime dependency is **dmsg_protocol → dmsg_types**. The reverse reference in the repository is a development dependency used by dmsg_types contract tests and vector generation; applications using only the types do not pull in this protocol crate. Consumers import public DTOs and errors from dmsg_types directly. The examples below also use `ed25519-dalek = "3"` and, for the approval example, `candid = "0.10"`.

Publication settings do not prove a version is already on crates.io. Both packages are currently version 0.1.0 in this checkout; see the release notes below for first-publication order.

## API guide

| Task | Entry points | What remains the caller's responsibility |
| --- | --- | --- |
| Encode protocol records | `canonical`, `decode_canonical`, `sha256`, `digest` | Use the exact public types, domain and value shape; apply business validation |
| Prepare a document | `validate_statement`, `statement_purpose`, `prepare_cose`, `parse_signing_input` | Obtain user consent, choose a trusted key and invoke the signer |
| Assemble/verify | `finish_cose`, `verify_artifact`, `verification_report` | Establish issuer identity, authorization and current status |
| Describe a signing key | `cose_algorithm`, `public_cose_key`, `key_thumbprint` | Authenticate the key source; do not treat a thumbprint as proof of ownership |
| Identify a signer | `account_issuer`, `principal_issuer`, `parse_account_issuer`, `validate_uri`, `validate_namespace` | Configure a trusted namespace and bind it to authenticated evidence |
| Prepare ICP approval | `SignRequestExt`, `ExecuteRequestExt`, `approval_message`, `execution_request_id` | Obtain current account/device state, sign the digest and submit to the user home |
| Validate request inputs | `DeviceInputExt`, `CoseInitExt`, `KeyRequestExt`, `validate_origin`, `validate_transport_key` | Perform server-side authorization, proof-of-possession checks and state transitions |
| Check execution evidence | `artifact_signing_bytes`, `match_signing_result`, `execution_receipt_key`, `match_execution_receipt` | Verify IC certificate, expected canister, witness, path and leaf bytes first |
| Handle recovery/names | `recovery_confirmation_message`, `normalize_handle`, `price`, `charge_terms_digest` | Execute recovery policy and name/ledger operations in their services |
| Attach timestamp evidence | `timestamp_imprint`, `attach_unverified_timestamp_token`, `signature_digest` | Acquire and independently validate the TSA token and its trust chain |
| Check basic constraints | `authenticated`, `nonzero`, `expiry`, `check_sequence`, `verify` | Supply trusted caller/time/state; these helpers do not read or mutate it |

Rustdoc describes parameters, failure behavior and trust boundaries for each entry point. `verify` is raw strict Ed25519 verification; `verify_artifact` additionally checks the COSE document profile. `authenticated` only excludes anonymous and management Principals; it does not establish account membership.

## Sign and verify a document locally

This complete example uses a deterministic **test key**. Production signing must use securely generated or externally managed keys.

```rust
use dmsg_protocol::{
    account_issuer, finish_cose, key_thumbprint, match_signing_result,
    prepare_cose, public_cose_key, sha256, verification_report,
};
use dmsg_types::{cose::Algorithm, AccountId, Statement, StatementContent, VerificationStatus};
use ed25519_dalek::{Signer, SigningKey};

let signer = SigningKey::from_bytes(&[7; 32]); // Test fixture only.
let public = signer.verifying_key().to_bytes();
let algorithm = Algorithm::Ed25519;
// An empty kid is allowed here to compute the thumbprint first.
let fingerprint = key_thumbprint(&public_cose_key(&algorithm, &[], &public).unwrap()).unwrap();
let original = b"Release 1.0 specification";
let statement = Statement {
    issuer: account_issuer("https://example.org/u/", &AccountId([1; 12])).unwrap(),
    subject: Some("release/specification".into()),
    issued_at: Some(1_800_000_000), // Claimed Unix seconds, not trusted time.
    content: StatementContent::Digest {
        sha256: sha256(original),
        content_type: Some("text/plain".into()),
        location: None,
    },
};
let (_, tbs) = prepare_cose(&statement, &algorithm, fingerprint.as_slice()).unwrap();
let signature = signer.sign(&tbs).to_bytes().to_vec();
let artifact = finish_cose(&tbs, &public, signature).unwrap();
match_signing_result(&artifact, &tbs, fingerprint).unwrap();
let report = verification_report(&artifact, Some(original)).unwrap();
assert_eq!(report.signature, VerificationStatus::Verified);
assert_eq!(report.content, VerificationStatus::Verified);
assert_eq!(report.issuer_binding, VerificationStatus::NotChecked);
assert_eq!(report.timestamp, VerificationStatus::NotProvided);
```

`prepare_cose` returns an unsigned message and `Sig_structure = CBOR(["Signature1", protected_bstr, h'', payload_bstr])`. Text uses raw UTF-8 (1..4096 bytes); digest documents use the original bytes' 32-byte SHA-256. The Rust Statement enum is not an extra wire payload. Issuer, optional subject and claimed issued_at are protected CWT claims; request ID, browser origin and execution deadline are separate execution metadata.

| Algorithm | COSE label | Signer input | Signature/public key supplied to `finish_cose` |
| --- | --- | --- | --- |
| Ed25519 | -19 | Exact tbs bytes | 64-byte signature; raw 32-byte public key |
| ES256K | -47 | SHA-256(tbs) for a prehash API | 64-byte r\|\|s, not DER; SEC1 secp256k1 public key |

When using an API that hashes internally, pass tbs once rather than hashing it twice. vetKD is not a document-signature algorithm. `finish_cose` assembles and validates structure but does **not** verify the signature; `match_signing_result` or `verify_artifact` does. `public_cose_key` returns encoded COSE_Key bytes; its input is raw key material. `key_thumbprint` hashes required public COSE members, excluding kid/alg/key_ops and expanding compressed EC y coordinates. It is not raw-key SHA-256 or a complete key validator.

## Construct an ICP device approval

The following constructs a local request and device signature; it does not contact a canister or grant permission. Real account IDs, device IDs, epochs, sequences and signing-key references must come from the configured, authenticated services. The fixture IDs and keys are for demonstration only.

```rust
use candid::Principal;
use dmsg_protocol::{execution_request_id, ExecuteRequestExt, SignRequestExt};
use dmsg_types::{
    cose::{SignRequest, SigningAlgorithm, SigningKeyRef},
    AccountId, Approval, Hash, Statement, StatementContent,
};
use ed25519_dalek::{Signer, SigningKey};

let account_id = AccountId([1; 12]);
let device_id = Hash::new([2; 32]);
let security_epoch = 1;
let sequence = 0;
let request_id = execution_request_id(&account_id, security_epoch, device_id, sequence);
let request = SignRequest {
    account_id,
    key: SigningKeyRef {
        algorithm: SigningAlgorithm::Ed25519,
        kid: vec![3; 32].into(),
        public_key_fingerprint: Hash::new([3; 32]),
    },
    statement: Statement {
        issuer: "https://example.org/signers/alice".into(),
        subject: None,
        issued_at: None,
        content: StatementContent::Text("Approve release 1.0".into()),
    },
    origin: "https://example.org".into(),
    max_cycles: 1_000_000_000,
    approval: Approval {
        device_id, security_epoch, sequence, request_id,
        expires_at: 1_800_000_060_000, // Unix milliseconds; use a valid live deadline.
        signature: Vec::new().into(), // Excluded from the approval digest.
    },
};
let mut execution = request.into_execution().unwrap();
let home_user = Principal::from_slice(&[1, 1]); // Deployment fixture.
let digest = execution.approval_message(home_user);
let device_key = SigningKey::from_bytes(&[9; 32]); // Test fixture only.
execution.approval.signature = device_key.sign(digest.as_slice()).to_bytes().to_vec().into();
assert_eq!(execution.approval.request_id, request_id);
```

`SignRequestExt::into_execution` validates the origin and statement and prepares bytes with signing generation 1. It preserves the supplied fingerprint and approval; it does not authenticate them. `ExecuteRequestExt::approval_message` binds kind and max_cycles under `dmsg/execute/v3`, wrapped in `dmsg/device-approval/v2`. Changing any approved field requires a new approval. The typed `sign` endpoint accepts SignRequest; the low-level ExecuteRequest also represents root derivation and is not an unrestricted raw-signing endpoint.

For account mutations, use `approval_message` with `dmsg/account/v2` and `(expected_version, command)`. Recovery reconfirmation uses `recovery_confirmation_message` and the recovery key, not a device key. Sequence consumption, deadline checks and permission decisions happen in the service. After an unknown outcome, reconcile the original request instead of generating a new signing operation.

## Encoding and identity helpers

```rust
use dmsg_protocol::{canonical, decode_canonical, digest, normalize_handle};
use dmsg_types::Hash;

let value = Hash::new([0x42; 32]);
let encoded = canonical(&value);
assert_eq!(decode_canonical::<Hash>(&encoded).unwrap(), value);
assert_ne!(digest("example/a/v1", &value), digest("example/b/v1", &value));
assert_eq!(normalize_handle("Alice_01").unwrap(), "alice_01");
```

`canonical` uses RFC 8949 core deterministic CBOR. `digest(domain, value)` is `SHA256(CBOR([1, domain, value]))`. Both operate on trusted serializable values and panic if their Serialize implementation fails; neither enforces business semantics. `decode_canonical` caps input at 65,536 bytes and requires exact re-encoding, returning errors for malformed or noncanonical records. It is not the artifact decoder: external COSE verification must preserve the original protected bytes.

AccountId is 12 binary bytes and 20 characters of canonical Xid text. Hash/OpId are 32-byte strings. Identity adapters require an explicit canonical URI namespace ending in `/` or `:`, without query/fragment; they neither discover services nor establish account existence. `validate_origin` accepts an exact HTTPS origin or a 32-letter a..p Chrome extension ID, without a trailing path. Syntax checks do not prove the browser's actual origin.

Business timestamps and durations use milliseconds, while Statement.issued_at uses seconds and ICRC created_at_time uses nanoseconds. `expiry` requires a strictly future deadline within the supplied maximum lifetime. `check_sequence` returns ResultExpired for old sequences and VersionConflict for future/exhausted sequences; it does not update the counter. `price` uses a fixed PANDA schedule with 8 decimal places and requires an already validated handle; it is not a generic token price oracle.

## Evidence and timestamp boundaries

`verification_report` returns Verified for a valid signature. Embedded text is Verified; provided original content must match exactly or by SHA-256. A digest without original content is NotProvided. Issuer binding, authorization and current status remain NotChecked. Timestamp is NotProvided if absent, or NotChecked if an opaque token is attached. Verification failure returns an error rather than a report with a successful signature flag.

Before calling `match_execution_receipt`, independently verify the IC certificate against a trusted root, the expected user canister, witness, requested path and leaf bytes. `execution_receipt_key` constructs the single raw path segment `b"execution/" || account_id[12] || request_id[32]`. The matcher requires schema 1 and Completed, then matches issuer, signing-bytes digest, public-key thumbprint and raw-signature digest. It does not independently check the receipt's account/request IDs, origin, deadline or external project permissions. Authenticate the expected path and separately apply any additional policy.

| Helper | Exact operation | Does not do |
| --- | --- | --- |
| `signature_digest` | SHA-256(raw signature bytes) for execution receipts | Verify the signature or dMsg profile |
| `timestamp_imprint` | SHA-256(CBOR(signature bstr)), including its byte-string header; requires canonical outer framing | Verify a signature, acquire a TSA token or check TSA trust |
| `attach_unverified_timestamp_token` | Verify the artifact, then attach opaque token bytes at unprotected header 270 | Verify CMS, imprint, certificate chain, TSA policy or revocation |

A token is limited to 131,072 bytes, an encoded COSE_Key to 2,048, and a COSE_Sign1 artifact to 196,608. The timestamp attachment helper refuses an existing token with VersionConflict. Trusted timestamp verification, TSA networking, SCITT transparency, archival and chain anchoring are outside this crate.

## Errors, protocol references, and validation

Functions return `dmsg_types::Result<T>`. Common errors are InvalidInput for malformed semantic inputs, IntegrityFailed for invalid bytes/signatures/bindings, UnsupportedProtocol for unsupported algorithms or profile semantics, and QuotaExceeded for size limits. Consult individual rustdoc entries for exact mappings. Diagnostic strings are not stable machine-readable error codes. Network failures are handled by your transport, not this local crate.

Use the [public protocol](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README.md), [CDDL](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/statements.cddl), [types guide](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/README.md), and [vectors](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/tests/protocol_vectors.json) from the same commit. These links follow main and can change. The document profile media types are experimental project names, not registered standards.

Run from the workspace root:

```sh
cargo test -p dmsg_protocol -p dmsg_types --locked
RUSTDOCFLAGS="-D warnings" cargo doc -p dmsg_protocol --no-deps --locked
cargo clippy -p dmsg_protocol --all-targets --locked -- -D warnings
cargo run -p dmsg_types --example protocol_vectors --locked > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

The English README is included as crate documentation and its Rust examples run as doctests. Keep both language versions' example code identical, translating comments only. `missing_docs` warnings help maintain API coverage.

For offline verification, run `cargo run -p dmsg_protocol --example verify -- artifact.cbor`. The input is a CBOR SignedArtifact record containing cose_sign1 and cose_key byte strings, not a bare COSE_Sign1 file. Output reports mathematical verification only. Local performance benchmarks use `cargo bench -p dmsg_protocol --bench validation --locked`; these measure host Rust execution, not canister instructions or end-to-end latency. Real-Wasm integration tests use `POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh`.

## Release notes for maintainers

Publish dmsg_types first, then dmsg_protocol. The latter's dependency specifies both a local path and version 0.1.0; Cargo uses the registry version in a published package. Keep that version requirement aligned with the public contracts.

Cargo omits the path-only dmsg_protocol development dependency from the normalized dmsg_types package manifest. That avoids a publication dependency cycle, but its repository contract tests and vector example still require the checkout's development dependency. Run these from the workspace; a packaged dmsg_types test suite is not equivalent.

Validate the dmsg_types package first. Once its version is available in the registry, validate dmsg_protocol with `cargo package -p dmsg_protocol --locked` and `cargo publish -p dmsg_protocol --dry-run --locked` before publishing. A local checkout containing dmsg_types is not a substitute for its registry availability during normal package resolution. Development interfaces do not promise compatibility with earlier experimental encodings. Production deployment, external services, capacity and audit acceptance require separate validation.
