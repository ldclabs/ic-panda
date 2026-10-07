# dmsg_protocol

> Third-party v2 contracts and current implementation boundaries are documented in [integration](../../docs/protocol/integration.md). Registration and dedicated authentication are implemented; generic v2 checkout/membership consumers remain subsequent work, without protocol fallback.

English | [简体中文](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_protocol/README_zh.md)

Deterministic encoding, document signature verification, and request helpers for dMsg. This crate implements the public protocol on top of [dmsg_types](https://github.com/ldclabs/ic-panda/tree/main/src/dmsg_types). It performs local computation only: no ICP calls, ledger queries, canister storage, network discovery, or account authorization.

Use it to prepare portable COSE documents, verify returned artifacts, construct device approval digests, and check bindings between documents and independently authenticated execution receipts. Common helpers are re-exported at the crate root. Commercial and shared-membership helpers are imported from the public `billing` and `membership` modules.

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
dmsg_types = "0.2"
dmsg_protocol = "0.2"
```

The runtime dependency is **dmsg_protocol → dmsg_types**. The reverse reference in the repository is a development dependency used by dmsg_types contract tests and vector generation; applications using only the types do not pull in this protocol crate. Consumers import public DTOs and errors from dmsg_types directly. The examples below also use `ed25519-dalek = "3"` and, for the approval example, `candid = "0.10"`.

Publication settings do not prove a version is already on crates.io. Both packages are currently version 0.2.0 in this checkout; see the release notes below for publication order.

## API guide

| Task | Entry points | What remains the caller's responsibility |
| --- | --- | --- |
| Encode protocol records | `canonical`, `decode_canonical`, `sha256`, `digest` | Use the exact public types, domain and value shape; apply business validation |
| Prepare a document | `validate_statement`, `statement_purpose`, `prepare_cose`, `parse_signing_input` | Obtain user consent, choose a trusted key and invoke the signer |
| Assemble/verify | `PreparedStatement::into_signature`, `PreparedSignature::finish`, `verify_artifact` | Establish issuer identity, authorization and current status |
| Describe a signing key | `cose_algorithm`, `public_cose_key`, `key_thumbprint` | Authenticate the key source; do not treat a thumbprint as proof of ownership |
| Identify a signer | `account_issuer`, `validate_uri`, `validate_namespace` | Configure a trusted namespace and bind it to authenticated evidence |
| Prepare ICP approval | `SignRequestExt`, `approval_message`, `EXECUTE_APPROVAL_DOMAIN`, `execute_approval_command`, `execution_request_id` | Obtain current account/device state, sign the digest and submit to the user home |
| Validate request inputs | `DeviceInputExt`, `CoseInitExt`, `KeyRequestExt`, `validate_origin`, `validate_transport_key` | Perform server-side authorization, proof-of-possession checks and state transitions |
| Configure a COSE executor | `content_root_context`, `cose_pins::master_key_pins` (feature `cose-pins`) | Create the canister first: pins depend on its ID; initialize and compare the fetched fingerprints |
| Check execution evidence | `match_signing_result`, `signature_digest`, `execution_receipt_key`, `match_execution_receipt` | Verify IC certificate, expected canister, witness, path and leaf bytes first |
| Handle recovery/names | `recovery_confirmation_message`, `normalize_handle`, `price`, `charge_terms_digest` | Execute recovery policy and name/ledger operations in their services |
| Check basic constraints | `authenticated`, `nonzero`, `expiry`, `check_sequence`, `verify` | Supply trusted caller/time/state; these helpers do not read or mutate it |

Rustdoc describes parameters, failure behavior and trust boundaries for each entry point. `verify` is raw strict Ed25519 verification; `verify_artifact` additionally checks the COSE document profile. `authenticated` only excludes anonymous and management Principals; it does not establish account membership.

Import commercial helpers from `dmsg_protocol::billing::monthly_allowance` and `dmsg_protocol::{membership::mul_div, integration::required_panda_stake}`. Amounts use integer atomic units and business times use UTC Unix milliseconds. Quotes and stake thresholds round up; monthly allowances sum weighted durations before rounding down once. Digest construction performs no authorization; rustdoc describes each validation boundary.

## Sign and verify a document locally

This complete example uses a deterministic **test key**. Production signing must use securely generated or externally managed keys.

```rust
use dmsg_protocol::{
    account_issuer, key_thumbprint, match_signing_result, parse_signing_input,
    prepare_cose, public_cose_key, sha256, verify_artifact,
};
use dmsg_types::{cose::Algorithm, AccountId, Statement, StatementContent};
use ed25519_dalek::{Signer, SigningKey};

let signer = SigningKey::from_bytes(&[7; 32]); // Test fixture only.
let public = signer.verifying_key().to_bytes();
let algorithm = Algorithm::Ed25519;
// An empty kid is allowed here to compute the thumbprint first.
let fingerprint = key_thumbprint(&public_cose_key(&algorithm, &[], &public).unwrap()).unwrap();
let original = b"Release 1.0 specification";
let statement = Statement {
    issuer: account_issuer("https://example.org/u/", &AccountId([1; 12])),
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
let artifact = parse_signing_input(&tbs)
    .unwrap()
    .into_signature(&public)
    .unwrap()
    .finish(signature)
    .unwrap();
match_signing_result(&artifact, &tbs, fingerprint).unwrap();
// The verified statement commits to the original bytes' SHA-256.
assert_eq!(verify_artifact(&artifact).unwrap(), statement);
```

`prepare_cose` returns an unsigned message and `Sig_structure = CBOR(["Signature1", protected_bstr, h'', payload_bstr])`. Text uses raw UTF-8 (1..4096 bytes); digest documents use the original bytes' 32-byte SHA-256. `FileStatement` uses a deterministic CBOR payload combining verbatim text, a file SHA-256 and optional media type/location, under `FILE_STATEMENT_PROFILE` with the `Statement` key purpose. Its closed schema and limits are specified in [the public protocol](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README.md). The Rust Statement enum is not an extra wire payload. Issuer, optional subject and claimed issued_at are protected CWT claims; request ID, browser origin and execution deadline are separate execution metadata.

| Algorithm | COSE label | Signer input | Signature/public key supplied to `finish`/`into_signature` |
| --- | --- | --- | --- |
| Ed25519 | -19 | Exact tbs bytes | 64-byte signature; raw 32-byte public key |
| ES256K | -47 | SHA-256(tbs) for a prehash API | 64-byte r\|\|s, not DER, normalized to low-S; SEC1 secp256k1 public key |

When using an API that hashes internally, pass tbs once rather than hashing it twice. vetKD is not a document-signature algorithm. `PreparedSignature::finish` assembles and validates structure but does **not** verify the signature; `match_signing_result` or `verify_artifact` does. `public_cose_key` returns encoded COSE_Key bytes; its input is raw key material. `key_thumbprint` hashes required public COSE members, excluding kid/alg/key_ops and expanding compressed EC y coordinates. It is not raw-key SHA-256 or a complete key validator.

`parse_signing_input` returns an immutable `PreparedStatement` with `statement()`, `algorithm()` and `kid()` accessors. Call `into_signature(public)` before dispatch and `PreparedSignature::finish(signature)` after the response to reuse validated framing and the encoded public key. Signature verification is still required before accepting the artifact.

## Construct an ICP device approval

The following constructs a local request and device signature; it does not contact a canister or grant permission. Real account IDs, device IDs, epochs, sequences and signing-key references must come from the configured, authenticated services. The fixture IDs and keys are for demonstration only.

```rust
use candid::Principal;
use dmsg_protocol::{
    approval_message, execute_approval_command, execution_request_id, SignRequestExt,
    EXECUTE_APPROVAL_DOMAIN,
};
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
        signature: Default::default(), // Excluded from the approval digest.
    },
};
let mut execution = request.into_execution().unwrap();
let home_user = Principal::from_slice(&[1, 1]); // Deployment fixture.
let digest = approval_message(
    home_user,
    &execution.account_id,
    EXECUTE_APPROVAL_DOMAIN,
    &execute_approval_command(&execution),
    &execution.approval,
);
let device_key = SigningKey::from_bytes(&[9; 32]); // Test fixture only.
execution.approval.signature = device_key.sign(digest.as_slice()).to_bytes().into();
assert_eq!(execution.approval.request_id, request_id);
```

`SignRequestExt::into_execution` validates the origin and statement and prepares bytes with signing generation 1. It preserves the supplied fingerprint and approval; it does not authenticate them. The execution approval passes `EXECUTE_APPROVAL_DOMAIN` (`dmsg/execute/v3`) and `execute_approval_command`, which is `(kind, max_cycles)`, to `approval_message`, wrapped in `dmsg/device-approval/v2`; the user canister verifies the same pair. Changing any approved field requires a new approval. The typed `sign` endpoint accepts SignRequest; the low-level ExecuteRequest also represents root derivation and is not an unrestricted raw-signing endpoint.

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

AccountId is 12 binary bytes and 20 characters of canonical Xid text. Hash/OpId are 32-byte strings. Identity adapters require an explicit canonical URI namespace ending in `/` or `:`, without query/fragment; they neither discover services nor establish account existence. `validate_origin` accepts an exact HTTPS origin or a 32-letter a..p Chrome extension ID, without a trailing path; Local deployments also accept exact loopback HTTP origins. The user and COSE homes check it against their own deployment environment. Syntax checks do not prove the browser's actual origin.

Business timestamps and durations use milliseconds, while Statement.issued_at uses seconds and ICRC created_at_time uses nanoseconds. `expiry` requires a strictly future deadline within the supplied maximum lifetime. `check_sequence` returns ResultExpired for old sequences and VersionConflict for future/exhausted sequences; it does not update the counter. `price` uses a fixed PANDA schedule with 8 decimal places and requires an already validated handle; it is not a generic token price oracle.

## Evidence and timestamp boundaries

`verify_artifact` checks the profile and mathematical signature only. Compare original content with the verified statement yourself: embedded text must match exactly, and a digest or file statement must match the original file's SHA-256; embedded opinion text does not verify the referenced file. Issuer binding, authorization, current status and timestamp trust are not checked.

Before calling `match_execution_receipt`, independently verify the IC certificate against a trusted root, the expected user canister, witness, requested path and leaf bytes. `execution_receipt_key` constructs the single raw path segment `b"execution/" || account_id[12] || request_id[32]`. The matcher requires schema 1 and Completed, then matches issuer, signing-bytes digest, public-key thumbprint and raw-signature digest. It does not independently check the receipt's account/request IDs, origin, deadline or external project permissions. Authenticate the expected path and separately apply any additional policy.

`signature_digest` is SHA-256 of the raw signature bytes used by execution receipts; it does not verify the signature or dMsg profile.

Verification accepts an opaque token of at most 131,072 bytes at unprotected header 270 without checking it. An encoded COSE_Key is limited to 2,048 bytes and a COSE_Sign1 artifact to 196,608. Trusted timestamp verification, TSA networking, SCITT transparency, archival and chain anchoring are outside this crate.

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

For COSE deployment pins, run `cargo run -p dmsg_protocol --features cose-pins --example cose_pins -- <canister-id> Production`; it prints the `masters` field of a `CoseInit` derived offline from the mainnet master keys (`pocketic` as the third argument selects the PocketIC and local dfx keys). For offline verification, run `cargo run -p dmsg_protocol --example verify -- artifact.cbor`. The input is a CBOR SignedArtifact record containing cose_sign1 and cose_key byte strings, not a bare COSE_Sign1 file. Output reports mathematical verification only. Local performance benchmarks use `cargo bench -p dmsg_protocol --bench validation --locked`; these measure host Rust execution, not canister instructions or end-to-end latency. Real-Wasm integration tests use `POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh`.

## Release notes for maintainers

Publish dmsg_types first, then dmsg_protocol. The latter's dependency specifies both a local path and version 0.2.0; Cargo uses the registry version in a published package. Keep that version requirement aligned with the public contracts.

0.2.0 removes 0.1.x public items that no production code used. Replace `finish_cose` with `parse_signing_input(tbs)?.into_signature(public)?.finish(signature)`, `ExecuteRequestExt::approval_message` with `approval_message` over `EXECUTE_APPROVAL_DOMAIN` and `execute_approval_command`, and `ExecutionResult::output()` with a match on `ExecutionOutcome::Completed`. `dmsg_types::handle::HandleInit` gains the required `governance` field and replaces `home_user` with `environment`, `issuer_namespace` and an append-only `user_homes`, routed by each account ID's allocator fingerprint. The new `handle_bucket` and `HANDLE_BUCKET_BITS` define where the registry certifies a handle. `CoseInit` replaces `initial_home_user` with `user_homes` and gains `governance`; `PaymentInit` replaces `home_user` with `environment`, `issuer_namespace` and `user_homes`, and `PaymentConfiguration` lists `user_homes`; `DirectoryInit` gains `governance`. The new `agent::check_user_home`, `validate_user_homes`, `account_home`, `is_account_home`, `validate_custom_domains` and `MAX_USER_HOMES` (64) give every service the same allocator-fingerprint routing. `MIN_HANDLE_PRICE`, and so `price` for 7–20-byte names, drops from 5,000 to 100 PANDA to match the live legacy registry. `ProductRegistration` replaces `beneficiary_authority` with an append-only `beneficiary_authorities` list, and `validate_subject` requires the subject's authority to be listed; `CommerceInit` replaces `max_subjects` and `daily_orders` with `limits: CommerceLimits`. `dmsg_types::integration::LEASE_RENEW_WINDOW_MS` (10 minutes) is the lease renewal window shared by commerce and membership. `CommerceLimits` gains `calls_per_caller`, and the new `CommerceStats` reports a commerce canister's live counts; `PandaServiceConfig` replaces `commerce_canister` with the append-only `commerce_homes: Vec<CommerceHome>`, the commerce canister of each user home, and gains `qualifications_per_minute`. `ExecutionResult` gains `cycles_charged`, the threshold fee a returned management call consumed, to which COSE and user budgets settle; the new `CoseStats` reports a COSE executor's live counters. `content_root_context` builds the vetKD content-root context, and the optional `cose-pins` feature adds `cose_pins::master_key_pins` and the `cose_pins` example for offline COSE master-key pins.

Cargo omits the path-only dmsg_protocol development dependency from the normalized dmsg_types package manifest. That avoids a publication dependency cycle, but its repository contract tests and vector example still require the checkout's development dependency. Run these from the workspace; a packaged dmsg_types test suite is not equivalent.

Validate the dmsg_types package first. Once its version is available in the registry, validate dmsg_protocol with `cargo package -p dmsg_protocol --locked` and `cargo publish -p dmsg_protocol --dry-run --locked` before publishing. A local checkout containing dmsg_types is not a substitute for its registry availability during normal package resolution. Development interfaces do not promise compatibility with earlier experimental encodings. Production deployment, external services, capacity and audit acceptance require separate validation.


Application-action v1 is a separate closed profile with an `AppAction` key purpose.
See [the profile and implementation boundary](../../docs/protocol/app-action.md).
Rust COSE preparation/verification supports it; `dmsg_user.sign` and the document
browser flow explicitly reject it pending the authorized action integration.
