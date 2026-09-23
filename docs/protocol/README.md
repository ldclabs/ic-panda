# dMsg Public Protocol: Statements and ICP Control Interfaces

English | [简体中文](README_zh.md)

The current implementation adopts the [Statement v3 design](statement-v3-design.md): three document profiles v1, 12-byte Xid accounts, and execution approval domain v3. Specific byte formats are defined in [statements.cddl](statements.cddl); ICP interfaces are governed by each canister's `.did` declaration. Profile media types are experimental project names and have not yet been registered or adopted by standards bodies.

`dmsg_types` defines public data contracts only; `dmsg_protocol` implements canonical encoding, profile verification, and identity adaptation; `dmsg_runtime` and each canister persist internal state. Independent third-party signature verification does not require implementing account databases, handle management, or paid delivery services.

## Exchange Objects

The exchange object is an RFC 9052 tagged COSE_Sign1 structure: `18([protected_bstr, unprotected_map, payload_bstr, signature_bstr])`. The Rust `Statement { issuer, subject, issued_at, content }` represents a preparation and parsing view, **not an extra wire payload wrapper**. `SignedArtifact { cose_sign1, cose_key }` is an optional packaging DTO.

| Content | Protected header `typ` (16) | payload | Other protected headers |
| --- | --- | --- | --- |
| Text | `application/vnd.dmsg.text-statement+cose;v=1` | Raw UTF-8, 1..4096 bytes | 3=`text/plain;charset=utf-8`, crit=[15,16] |
| Digest | `application/vnd.dmsg.digest-statement+cose;v=1` | Original SHA-256, 32 bytes | 258=-16, optional 259=original media type, 260=original URI, crit=[15,16,258] |
| File statement | `application/vnd.dmsg.file-statement+cose;v=1` | Deterministic CBOR: text and one file's SHA-256, optional file metadata | 3=`application/cbor`, crit=[15,16]; no 258/259/260 |

`StatementContent::FileStatement { text, sha256, content_type, location }` signs a statement about one exact file. Its payload is the closed integer-key map `{1: text, 2: sha256, ?3: content_type, ?4: location}`. Text is preserved verbatim as 1..4096 UTF-8 bytes; SHA-256 is a 32-byte bstr over the original file bytes. Optional media type and URI follow the digest profile's validation bounds, but live inside this payload. Missing optional keys are omitted, never null. The payload must use RFC 8949 core deterministic CBOR (definite lengths, shortest encodings, sorted keys), with no duplicate/unknown keys, tags or trailing bytes, and is capped at 16,384 bytes. Verification preserves original COSE protected bytes while requiring this payload's canonical encoding.

The text and file digest are jointly signed. The text can express agreement, objections or observations; the profile itself grants no publishing/acceptance authority and does not prove that the signer reviewed the file. `subject` may identify the wider project or matter; the SHA-256 field identifies the exact file version. Verification never automatically fetches `location`. Business actions requiring machine-readable roles, actions or project permissions need their own explicit profiles. The extension bridge represents this content as `{kind: "file_statement", text, sha256, contentType?, location?}`, with lowercase hex SHA-256, and reviews the text alongside the digest and original-file verification status.

All three profiles require protected headers `alg` (1), `kid` (4), CWT claims (15), and `typ` (16). Claims include mandatory `iss` (1), optional `sub` (2), and optional `iat` (6). The digest profile forbids header 3 per RFC 9995; file size is not a mandatory field. None of these profiles includes `request_id`, `origin`, `audience`, or execution deadlines.

`iss` identifies the signer; `sub` identifies the declared subject and may be omitted. `iat` is an i64 Unix **second** timestamp asserting the signer's claimed time, not a trusted timestamp. Regular document signatures do not expire when execution approvals expire. Statements containing audiences, authorization periods, or new business semantics require distinct profile definitions; current endpoints reject unrecognized `typ`, claims, protected headers, or critical semantics.

## Identity and Byte Rules

`iss` uses a canonical absolute ASCII URI, 1..8192 bytes, free of credentials, control characters, or invalid percent-encodings. Creation endpoints must predetermine the canonical string; signing and verification do not implicitly rewrite URLs. `sub` is a StringOrURI of 1..8192 UTF-8 bytes without control characters; if containing a colon, it must satisfy the same URI rules. Plain text undergoes no Unicode normalization or whitespace trimming. URIs serve strictly as identifiers and do not trigger automated network discovery.

| Type | Binary | Text |
| --- | --- | --- |
| dMsg `AccountId` | `ic_auth_types::Xid` alias, Candid blob / CBOR bstr, 12 bytes | Canonical Xid: 20-character lowercase base32hex, ending in 0 or g |
| ICP Principal | Candid principal / CBOR raw bstr, 0..29 bytes | Standard Principal text; empty-byte management canister is also a valid representation |
| Device, Operation, SHA-256 | Distinct semantic types, 32 bytes | Bridge interfaces explicitly specify encoding; identity types are not inferred from length |
| COSE kid | Opaque bstr, bounded to 1..256 bytes in current profiles | No implicit account semantics |

Identity adapters accept explicit namespaces: `account_issuer("https://dmsg.example/u/", account)` or `principal_issuer("https://id.example/ic/mainnet/", principal)`. Namespaces are fixed URI prefixes ending with `/` or `:`, without query or fragment, at most 8128 bytes. Account migration does not alter identity URIs; no zero-padding, truncation, or hashing is used to fit length constraints.

New messages use RFC 8949 §4.2.1 core deterministic CBOR: shortest encoding, keys sorted by encoded byte order, no floating-point numbers, duplicate keys, or trailing bytes. Existing COSE verification preserves original protected bytes and must not re-sort prior to verification. Local signing endpoints accept only inputs prepared according to profile rules. To-be-signed structures are capped at 65536 bytes, COSE_Key at 2048 bytes, signature artifacts at 196608 bytes, and TSA tokens at 131072 bytes. The library decoder enforces recursion limits; the SDK additionally enforces limits of 24 levels and 50000 nodes.

ICP business time and approval durations are u64 Unix **milliseconds**, evaluated using `now < expires_at`. Certificates and ICRC `created_at_time` are in nanoseconds; ledger retries retain original values. CBOR u128 values exceeding u64 use tag 2 shortest big-endian bstr; they must not be converted via JS Number. ICRC Account is `{owner: bstr, subaccount: bstr .size 32 / null}`.

The browser JSON protocol is `dmsg-extension/4`: `accountId` is Xid text, SHA-256/requestId/nonce are lowercase hex, and large integers are decimal strings; statement `issuedAt` is in seconds, while outer `expiresAt` is in milliseconds. Standard Rust serde JSON output is not equivalent to this bridge protocol.

## Signatures and Keys

To-be-signed bytes are strictly `CBOR(["Signature1", protected_bstr, h'', payload_bstr])`, with empty `external_aad`.

| Algorithm | COSE alg | Signature | Public Key |
| --- | --- | --- | --- |
| Ed25519 (base) | -19 | Direct signature over Sig_structure | OKP, crv=6, x=32 bytes |
| ES256K (optional) | -47 | SHA-256(Sig_structure), ECDSA r\|\|s | EC2, crv=8, x/y each 32 bytes; verification also accepts compressed y |

dMsg no longer provides BIP340 endpoints or private algorithm tags. Failures do not trigger automatic algorithm switching. Public-only COSE_Key forbids private key `d` (-4); `alg` must match, `key_ops` if present must permit `verify`, and `kid` if present must match the protected header.

Public key thumbprints follow RFC 9679 SHA-256: only required public parameters are encoded, excluding `kid`, `alg`, `key_ops`; EC2 `y` must be expanded into full coordinates first. Compressed and uncompressed forms of the same key therefore produce the identical thumbprint. The ICP signing adapter uses this thumbprint as `kid`; general profiles do not mandate that all implementations choose `kid` this way.

ICP fixed derivation domain is upgraded to `dmsg/formal/v2`, with `derivation_version=2`; vetKD input is `[account_id, generation]`, and context is `["dmsg/content-root/v2", environment, 2]`. Xid alters key derivation inputs; new development instances are used, and legacy IDs cannot be truncated to represent the same key.

## ICP Execution Approvals and Receipts

`digest(domain, value) = SHA256(CBOR([1, domain, value]))`.

```text
request_id = digest("dmsg/execution-request/v2",
  [account_id, security_epoch, device_id, sequence])

kind = {Sign: {
  key: {purpose, algorithm, generation: 1},
  to_be_signed: Sig_structure_bstr,
  public_key_fingerprint: RFC9679_SHA256,
  origin: checked_browser_origin
}}

digest("dmsg/device-approval/v2", [
  target_user_principal_bytes, account_id, "dmsg/execute/v3",
  device_id, security_epoch, sequence, request_id, expires_at,
  digest("dmsg/execute/v3", [kind, max_cycles])
])
```

Devices sign this digest strictly using Ed25519. `purpose` is `Statement` for text/file statements and `FileAttestation` for digest documents; `algorithm` is `Ed25519` or `EcdsaSecp256k1`. Root derivation kind is `{Derive: {generation, root_op_id: bstr/null, transport_key: bstr .size 48}}`. Account mutations use the independent `dmsg/account/v2` domain with command `[expected_version, AccountCommand]`. CBOR unit enums are encoded as string names, payload enums as single-entry maps, and `Option` as values or null.

Protected headers, payload, key thumbprints, and approval context are frozen prior to signing. User home verifies that `issuer` belongs to the current account; COSE home verifies the `kid` and thumbprint of the actual derived key. Browser origin is limited to 256 bytes and represents an exact HTTPS origin or Chrome extension origin; verified by the extension, device signatures do not independently authenticate browser origin.

Idempotency is scoped to `account_id/request_id`; requests with the same ID but differing parameters are rejected. Unknown outcomes reconcile against the original request. Strict device sequence numbers and execution watermarks continue to prevent replay attacks after completed results are cleaned up. Identical content can produce identical signatures; signature digests cannot serve as unique identifiers for all business operations.

`get_execution_receipt(account_id, request_id)` returns a certified ICP leaf; querying requires account authentication. The path is `b"execution/" || account_id[12] || request_id[32]`. `ExecutionReceipt` stores issuer, device/epoch, approval time/deadline, origin, cycle fee limit, status, SHA-256 of to-be-signed bytes, public key thumbprint, and SHA-256 of raw signature bytes. It does not modify portable signature artifacts. Upgrades reconstruct leaves from stable execution records, and leaves are deleted when results are pruned.

Receipt verification requires first validating the specified user canister's IC certificate, witness, path, and leaf value, followed by matching the `Completed` status, issuer, to-be-signed digest, public key thumbprint, and signature digest. The SDK `verifyExecutionReceipt` performs both steps; Rust `match_execution_receipt` performs only binding checks, leaving certificate validation to the caller. Historical receipts do not enforce the 60-second freshness window applied to account security snapshots; current authorization status is queried separately. A receipt proves execution authorization recorded by this service; it does not automatically prove external project permissions or current device state.

## Xid Issuance

The user canister uses `ic_auth_types::XidGenerator` for persistent ID allocation, formatted as `timestamp_seconds[4] || allocator_fingerprint[5] || counter[3]`. The fingerprint is derived from the first 5 bytes of `digest("dmsg/account-id-generator/v1", ["dmsg", environment, issuer_namespace, creating_canister])`, with the full namespace digest stored separately in configuration for upgrade validation. Counters advance during the same second or clock rollback; new seconds reset the counter to 0. Time overflow or counter exhaustion fails explicitly.

Once authentication, device PoP, quota, and unique binding checks succeed, a single execution segment without awaits commits the account, authentication index, quota, and allocator state. Retried requests for existing authentications return the existing account. No `raw_rand` or asynchronous creation staging tables are used. Currently, a single user home is fixed; future multi-allocator deployments must register and eliminate fingerprint collisions in advance, and truncated hashes cannot be treated as absolute global uniqueness guarantees.

## Timestamps and Verification Reports

The RFC 9921 CTT SHA-256 MessageImprint is `SHA256(CBOR(signature_bstr))`, including the CBOR bstr header, distinguishing it from the raw signature digest in execution receipts. `timestamp_imprint` requires canonical outer framing; `attach_unverified_timestamp_token` only populates header 270 and does not request or validate TSA trust. CMS signatures, imprints, certificate chains, key purposes, policies, and revocation status require independent verification.

`verify_artifact` and SDK `verifyDocumentArtifact` check profile compliance and cryptographic signatures; `verification_report` individually reports `signature`, `content`, `issuer_binding`, `authorization`, `timestamp`, and `current_status`. Standalone embedded text is `Verified`; when the original file of a digest or file statement is omitted, `content` is `NotProvided`; when identities or TSAs are unverified, they are reported as `NotChecked`. An embedded public key or a single boolean success result cannot serve as complete proof.

Current implementations do not provide TSA network/CMS validation, SCITT transparency services, long-term archiving, or on-chain state anchoring endpoints. Optional paid delivery, handle management, and ICP control contracts are maintained separately.

## Verification and Standards References

```sh
cargo test -p dmsg_types -p dmsg_protocol
cargo run -p dmsg_types --example protocol_vectors > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

Standards references for signatures, identity headers, and profiles: [RFC 9052](https://www.rfc-editor.org/rfc/rfc9052.html), [RFC 9597](https://www.rfc-editor.org/rfc/rfc9597.html), [RFC 8392](https://www.rfc-editor.org/rfc/rfc8392.html), and [RFC 9596](https://www.rfc-editor.org/rfc/rfc9596.html). Digests, public key thumbprints, and timestamps are based on [RFC 9995](https://www.rfc-editor.org/rfc/rfc9995.html), [RFC 9679 §4.2](https://www.rfc-editor.org/rfc/rfc9679.html#section-4.2), and [RFC 9921](https://www.rfc-editor.org/rfc/rfc9921.html). SCITT statement and evidence separation references [RFC 9943](https://www.rfc-editor.org/rfc/rfc9943.html); current document profiles do not claim implementation of its transparency service.

## Commercial Services

The product-neutral `membership/1` and `dmsg-commerce/1` contracts, delivery profile 2 and execution grant v3 are documented in [commerce.md](commerce.md), with [commerce.cddl](commerce.cddl) framing and independent commerce vectors. They do not change formal Statement or device execution approval bytes.
