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
dmsg_types = "0.3"
dmsg_protocol = "0.3"
```

The runtime dependency is **dmsg_protocol → dmsg_types**. The reverse reference in the repository is a development dependency used by dmsg_types contract tests and vector generation; applications using only the types do not pull in this protocol crate. Consumers import public DTOs and errors from dmsg_types directly. The examples below also use `ed25519-dalek = "3"` and, for the approval example, `candid = "0.10"`.

Publication settings do not prove a version is already on crates.io. In this checkout dmsg_types and dmsg_protocol are both version 0.3.0; see the release notes below for publication order.

## API guide

| Task | Entry points | What remains the caller's responsibility |
| --- | --- | --- |
| Encode protocol records | `canonical`, `decode_canonical`, `sha256`, `digest` | Use the exact public types, domain and value shape; apply business validation |
| Prepare a document | `validate_statement`, `statement_purpose`, `prepare_cose` | Obtain user consent, choose a trusted key and invoke the signer |
| Assemble/verify | `PreparedAttestation::finish`, `verify_artifact` | Establish issuer identity, authorization and current status |
| Describe a signing key | `cose_algorithm`, `public_cose_key`, `key_thumbprint` | Authenticate the key source; do not treat a thumbprint as proof of ownership |
| Identify a signer | `account_issuer`, `validate_uri`, `validate_namespace` | Configure a trusted namespace and bind it to authenticated evidence |
| Prepare ICP approval | `prepare_attestation`, `app_action_statement`, `approval_message`, `ATTEST_APPROVAL_DOMAIN`, `attest_approval_command`, `DERIVE_APPROVAL_DOMAIN`, `derive_approval_command`, `execution_request_id` | Obtain current account/device state, sign the Sig_structure and the digest with the device key, submit to the user home |
| Validate request inputs | `DeviceInputExt`, `CoseInitExt`, `validate_origin`, `validate_transport_key` | Perform server-side authorization, proof-of-possession checks and state transitions |
| Configure a COSE executor | `content_root_context`, `cose_pins::master_key_pin` (feature `cose-pins`) | Create the canister first: the pin depends on its ID; initialize and compare the fetched fingerprint |
| Check execution evidence | `signature_digest`, `execution_receipt_key`, `match_execution_receipt` | Verify IC certificate, expected canister, witness, path and leaf bytes first |
| Bind roots and keys | `root_recipients_digest`, `root_bundle_digest`, `recovery_device_message`, `controller_pop_message` | Build bundles and proofs in the client; the services recompute and compare |
| Handle names | `normalize_handle`, `price`, `charge_terms_digest` | Execute name/ledger operations in their services |
| Check basic constraints | `authenticated`, `nonzero`, `expiry`, `check_sequence`, `verify` | Supply trusted caller/time/state; these helpers do not read or mutate it |

Rustdoc describes parameters, failure behavior and trust boundaries for each entry point. `verify` is raw strict Ed25519 verification; `verify_artifact` additionally checks the COSE document profile. `authenticated` only excludes anonymous and management Principals; it does not establish account membership.

Import commercial helpers from `dmsg_protocol::billing::monthly_allowance` and `dmsg_protocol::{membership::mul_div, integration::required_panda_stake}`. Amounts use integer atomic units and business times use UTC Unix milliseconds. Quotes and stake thresholds round up; monthly allowances sum weighted durations before rounding down once. Digest construction performs no authorization; rustdoc describes each validation boundary.

## Sign and verify a document locally

This complete example uses a deterministic **test key**. Production signing must use securely generated or externally managed keys.

```rust
use dmsg_protocol::{account_issuer, key_thumbprint, prepare_attestation, sha256, verify_artifact};
use dmsg_types::{AccountId, Hash, Statement, StatementContent};
use ed25519_dalek::{Signer, SigningKey};

let signer = SigningKey::from_bytes(&[7; 32]); // Test fixture only.
let public = Hash::new(signer.verifying_key().to_bytes());
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
// The kid is the key's RFC 9679 thumbprint.
let prepared = prepare_attestation(&statement, &public).unwrap();
let thumbprint = prepared.thumbprint;
let signature = signer.sign(&prepared.to_be_signed).to_bytes();
let artifact = prepared.finish(&signature).unwrap();
// The verified statement commits to the original bytes' SHA-256.
assert_eq!(verify_artifact(&artifact).unwrap(), statement);
assert_eq!(key_thumbprint(&artifact.cose_key).unwrap(), thumbprint);
```

`prepare_cose` returns an unsigned message and `Sig_structure = CBOR(["Signature1", protected_bstr, h'', payload_bstr])`. Text uses raw UTF-8 (1..4096 bytes); digest documents use the original bytes' 32-byte SHA-256. `FileStatement` uses a deterministic CBOR payload combining verbatim text, a file SHA-256 and optional media type/location, under `FILE_STATEMENT_PROFILE` with the `Statement` key purpose. Its closed schema and limits are specified in [the public protocol](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README.md). The Rust Statement enum is not an extra wire payload. Issuer, optional subject and claimed issued_at are protected CWT claims; request ID, browser origin and execution deadline are separate execution metadata.

| Algorithm | COSE label | Signer input | Public key (`prepare_attestation`) / signature (`finish`) |
| --- | --- | --- | --- |
| Ed25519 | -19 | Exact tbs bytes | Raw 32-byte public key; 64-byte signature |

When using an API that hashes internally, pass tbs once rather than hashing it twice. vetKD is not a document-signature algorithm. `PreparedAttestation::finish` assembles and validates structure but does **not** verify the signature; `verify_artifact` does. `public_cose_key` returns encoded COSE_Key bytes; its input is raw key material. `key_thumbprint` hashes required public COSE members, excluding kid/alg/key_ops and expanding compressed EC y coordinates. It is not raw-key SHA-256 or a complete key validator.

`prepare_attestation` names the signing key by its RFC 9679 thumbprint (the kid) and returns the Sig_structure, the encoded public key, the thumbprint and the key purpose. For another kid, sign the message `prepare_cose` returns and encode the key with `public_cose_key`. Signature verification is still required before accepting the artifact.

## Construct an ICP device approval

The following constructs a local attestation request and device signatures; it does not contact a canister or grant permission. Real account IDs, device IDs, epochs and sequences must come from the configured, authenticated services. The fixture IDs and keys are for demonstration only.

```rust
use candid::Principal;
use dmsg_protocol::{
    approval_message, attest_approval_command, execution_request_id, prepare_attestation,
    ATTEST_APPROVAL_DOMAIN,
};
use dmsg_types::{AccountId, Approval, AttestRequest, Hash, Statement, StatementContent};
use ed25519_dalek::{Signer, SigningKey};

let account_id = AccountId([1; 12]);
let device_id = Hash::new([2; 32]);
let device_key = SigningKey::from_bytes(&[9; 32]); // Test fixture only.
let device_public: Hash = device_key.verifying_key().to_bytes().into();
let security_epoch = 1;
let sequence = 0;
let statement = Statement {
    issuer: "https://example.org/signers/alice".into(),
    subject: None,
    issued_at: None,
    content: StatementContent::Text("Approve release 1.0".into()),
};
// The device signs the exact Sig_structure; the kid is its RFC 9679 thumbprint.
let prepared = prepare_attestation(&statement, &device_public).unwrap();
let mut request = AttestRequest {
    account_id,
    statement,
    origin: "https://example.org".into(),
    signature: device_key.sign(&prepared.to_be_signed).to_bytes().into(),
    approval: Approval {
        device_id,
        security_epoch,
        sequence,
        request_id: execution_request_id(&account_id, security_epoch, device_id, sequence),
        expires_at: 1_800_000_060_000, // Unix milliseconds; use a valid live deadline.
        signature: Default::default(), // Excluded from the approval digest.
    },
};
let home_user = Principal::from_slice(&[1, 1]); // Deployment fixture.
let digest = approval_message(
    home_user,
    &request.account_id,
    ATTEST_APPROVAL_DOMAIN,
    &attest_approval_command(&request.statement, &request.origin, &request.signature),
    &request.approval,
);
request.approval.signature = device_key.sign(digest.as_slice()).to_bytes().into();
assert_eq!(prepared.thumbprint, dmsg_protocol::key_thumbprint(&prepared.cose_key).unwrap());
```

The approval passes `ATTEST_APPROVAL_DOMAIN` (`dmsg/attest/v1`) and `attest_approval_command`, the triple `(statement, origin, signature)`, to `approval_message`, wrapped in `dmsg/device-approval/v2`; the user canister verifies the same triple, the device signature over the Sig_structure, and records a certified receipt. Changing any approved field requires a new approval. Application actions use `app_action_statement` to build the statement of an `AppActionAttestRequest` and bind `action.origin`.

For account mutations, use `approval_message` with `dmsg/account/v2` and `(expected_version, command)`. A recovered device's root derivation uses `DERIVE_APPROVAL_DOMAIN` with `derive_approval_command`, which is `(generation, transport_public_key, max_cycles)`. Sequence consumption, deadline checks and permission decisions happen in the service. After an unknown outcome, reconcile the original request instead of generating a new signing operation.

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

Before calling `match_execution_receipt`, independently verify the IC certificate against a trusted root, the expected user canister, witness, requested path and leaf bytes. `execution_receipt_key` constructs the single raw path segment `b"execution/" || account_id[12] || request_id[32]`. The matcher requires schema 2, then matches issuer, signing-bytes digest, public-key thumbprint and raw-signature digest. It does not independently check the receipt's account/request IDs, origin, deadline or external project permissions. Authenticate the expected path and separately apply any additional policy.

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

For the COSE deployment pin, run `cargo run -p dmsg_protocol --features cose-pins --example cose_pins -- <canister-id> Production`; it prints the `master` field of a `CoseInit` and the content-root public key derived offline from the mainnet master key (`pocketic` as the third argument selects the PocketIC and local dfx keys). For offline verification, run `cargo run -p dmsg_protocol --example verify -- artifact.cbor`. The input is a CBOR SignedArtifact record containing cose_sign1 and cose_key byte strings, not a bare COSE_Sign1 file. Output reports mathematical verification only. Local performance benchmarks use `cargo bench -p dmsg_protocol --bench validation --locked`; these measure host Rust execution, not canister instructions or end-to-end latency. Real-Wasm integration tests use `POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh`.

## Release notes for maintainers

Publish dmsg_types first, then dmsg_protocol. The latter's dependency specifies both a local path and version 0.3.0; Cargo uses the registry version in a published package. Keep that version requirement aligned with the public contracts.

0.2.0 removes 0.1.x public items that no production code used. Replace `finish_cose` with `parse_signing_input(tbs)?.into_signature(public)?.finish(signature)`, `ExecuteRequestExt::approval_message` with `approval_message` over `ATTEST_APPROVAL_DOMAIN` and `attest_approval_command` for document attestations or `DERIVE_APPROVAL_DOMAIN` and `derive_approval_command` for root derivations, and `ExecutionResult::output()` with a match on `ExecutionOutcome::Completed`. `dmsg_types::handle::HandleInit` gains the required `governance` field and replaces `home_user` with `environment`, `issuer_namespace` and an append-only `user_homes`, routed by each account ID's allocator fingerprint. The new `handle_bucket` and `HANDLE_BUCKET_BITS` define where the registry certifies a handle. `CoseInit` replaces `initial_home_user` with `user_homes` and gains `governance`; `PaymentInit` replaces `home_user` with `environment`, `issuer_namespace` and `user_homes`, and `PaymentConfiguration` lists `user_homes`; `DirectoryInit` gains `governance`. The new `agent::check_user_home`, `validate_user_homes`, `account_home`, `is_account_home`, `validate_custom_domains` and `MAX_USER_HOMES` (64) give every service the same allocator-fingerprint routing. `MIN_HANDLE_PRICE`, and so `price` for 7–20-byte names, drops from 5,000 to 100 PANDA to match the live legacy registry. `ProductRegistration` replaces `beneficiary_authority` with an append-only `beneficiary_authorities` list, and `validate_subject` requires the subject's authority to be listed; `CommerceInit` replaces `max_subjects` and `daily_orders` with `limits: CommerceLimits`. `dmsg_types::integration::LEASE_RENEW_WINDOW_MS` (10 minutes) is the lease renewal window shared by commerce and membership. `CommerceLimits` gains `calls_per_caller`, and the new `CommerceStats` reports a commerce canister's live counts; `PandaServiceConfig` replaces `commerce_canister` with the append-only `commerce_homes: Vec<CommerceHome>`, the commerce canister of each user home, and gains `qualifications_per_minute`. `ExecutionResult` gains `cycles_charged`, the threshold fee a returned management call consumed, to which COSE and user budgets settle; the new `CoseStats` reports a COSE executor's live counters. `content_root_context` builds the vetKD content-root context, and the optional `cose-pins` feature adds `cose_pins::master_key_pin` and the `cose_pins` example for offline COSE master-key pins.

0.2.0 also removes threshold document signing and hosted controller signing. `SignRequest`, `AppActionSignRequest`, `AgentEventSignRequest`, `SigningKeyRef`, `KeySelector`, `ExecutionKind`, `ExecutionOutput`, `RootTarget`, `RecoveryPolicy`, `RecoveryConfirmation`, `SignRequestExt`, `KeyRequestExt`, `recovery_confirmation_message`, ES256K support (`k256`) and the `AgentController` purpose are gone. Documents are signed by device keys: `prepare_attestation` returns the Sig_structure and thumbprint and `PreparedAttestation::finish` attaches the device signature, `AttestRequest` / `AppActionAttestRequest` carry the device signature, and the approval binds `(statement, origin, signature)` under `ATTEST_APPROVAL_DOMAIN`; `ExecutionReceipt` is schema 2 without `max_cycles`. `ContentRootRef` becomes `{ generation, suite: "dmsg-root-v2", bundle_digest, recipients_digest, body_digest }` with `root_recipients_digest` and `root_bundle_digest`; `DeriveRootRequest` names a committed generation and is only accepted from the device a completed login recovery enrolled (`DERIVE_APPROVAL_DOMAIN`, `derive_approval_command`). Recovery is login-based: `RecoveryRequest` drops `generation`, `request_recovery` takes `(account_id, request, device_proof)` with `recovery_device_message`, `DisputeRecovery { op_id }` cancels, `SetRecoveryDelay` replaces `SetRecovery`/`ConfirmRecovery`, and `AccountInfo` exposes `recovery_delay_ms`, `pending_recovery` and `recovered_device`. `RegisterController` carries a `proof` built with `controller_pop_message`. `CoseInit` has a single `master: MasterKey`, `KeyDescriptor` describes the shared content-root key, `ExecutionGrant` carries `generation` and `transport_key`, and `SecuritySnapshot` is schema 4 without account status or recovery keys. `HandleInit` gains `registration_homes`; `AppRegistration` drops `user_homes` and `cose_homes`; `ExecutionWeights` drops `ecdsa_secp256k1`.

0.2.0 also makes application actions application-defined. `AppActionCommand`, `ActionReviewOutcome`, `ActionRequestedChange` and `action_input_hash` are removed: dMsg no longer contains TokenList commands or encodes TokenList's input. An application registers `AppRegistration.action_schema: Option<ActionSchema>`, required exactly with `SignAction` and checked by `validate_action_schema`. `AppAction.command` is an `ActionCommand { name, args: ActionArgs }` of the closed `ActionValue` model, and `AppAction` replaces `actor_id` with the opaque `actor` bytes and `input_hash`, `subject_hash`, `precondition_hash`, `role_snapshot_hash`, `signing_policy_hash` and `rule_set_hash` with `schema_hash` (`action_schema_hash`); the product commits its own context in `intent_hash`. `validate_action_command(action, schema)` checks a command against its schema, and `validate_action_admission` uses the registration's schema. The receiver, not dMsg, checks that its executed input projects to the signed command.

0.3.0 removes `parse_signing_input`, `PreparedStatement`, `PreparedSignature`, `match_signing_result` and `agent::delegation_id_prefix`, which no production code calls now that devices sign documents. Assemble an artifact with `prepare_attestation(statement, signing_pub)?.finish(signature)`; for another kid, sign the `prepare_cose` message and encode the key with `public_cose_key`. Bind an execution receipt with `match_execution_receipt`. 0.3.0 also removes `billing::usage_key`, which no service uses now that the user home keys monthly ledgers by account and month, adds `account_admission_message` for the registration admission ticket a user home verifies, and makes `validate_ed25519_key` public. The matching dmsg_types changes add `user::AdmissionTicket`, `UserInit.admission_key`, `CreateAccount.admission` and the `MAX_HOME_ACCOUNTS` (21,000,000), `MAX_OPERATION_RECEIPTS`, `MAX_PENDING_BINDINGS`, `BINDING_ACCEPT_MS`, `MAX_ADMISSION_TTL_MS` and `MAX_HOURLY_ACCOUNT_CALLS` constants, and remove `ExecutionUsage.held_units`. Following agent-protocols 0.11.3, which drops controller succession and lets a restricted controller manage the credentials within its ceiling, `controller_pop_message` no longer takes superseded generations, `HostedController` and `AccountCommand::RegisterController` lose `supersedes`, and `DirectoryInit.delegation_query_url` becomes `delegation_service`, the HTTPS origin of the delegation service.

Cargo omits the path-only dmsg_protocol development dependency from the normalized dmsg_types package manifest. That avoids a publication dependency cycle, but its repository contract tests and vector example still require the checkout's development dependency. Run these from the workspace; a packaged dmsg_types test suite is not equivalent.

Validate the dmsg_types package first. Once its version is available in the registry, validate dmsg_protocol with `cargo package -p dmsg_protocol --locked` and `cargo publish -p dmsg_protocol --dry-run --locked` before publishing. A local checkout containing dmsg_types is not a substitute for its registry availability during normal package resolution. Development interfaces do not promise compatibility with earlier experimental encodings. Production deployment, external services, capacity and audit acceptance require separate validation.


Application-action v1 is a separate profile with an `AppAction` key purpose. Each
application registers the schema of its own commands; see [the profile and
implementation boundary](../../docs/protocol/app-action.md). Rust COSE
preparation/verification supports it; `dmsg_user.attest` and the document browser
flow explicitly reject it, and `attest_app_action` is its only entry.
