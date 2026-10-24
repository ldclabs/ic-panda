# dMsg Public Protocol: Statements and ICP Control Interfaces

English | [简体中文](README_zh.md)

The current contract has three document profiles v1, 12-byte Xid accounts and device-signed attestations under `dmsg/attest/v1`. [Application actions](app-action.md) have their own closed schema and admission rules; the generic document signer rejects them. Exact statement bytes are in [statements.cddl](statements.cddl), and ICP interfaces in each canister's `.did`. Media types are experimental project names, not registered standards. “Statement v3” names an [archived design iteration](../../archived/docs/statement-v3-design.md), not a shared wire version.

## Protocol map

| Area | Contract |
| --- | --- |
| Accounts, devices and recovery | [Account/root](account_root.md) · [Browser login](login-v1.md) |
| Web application connection | [dmsg-connect/1](connect-v1.md) |
| Authentication and typed actions | [Integration](integration.md) · [App actions](app-action.md) · [Generated CDDL](integration.cddl) |
| Subscription checkout and PANDA membership | [Commerce 2](commerce.md) · [Resource CDDL](commerce.cddl) |
| Encrypted content and files | [Cloud](cloud.md) · [Channels](channel_migration.md) |
| Agent identity and events | [Agent Delegation](agent.md) · [Mail events](mail_events.md) |
| Approved credential use | [Secrets](secrets.md) |
| Team wallets | [dmsg-wallet/1](wallet.md) |
| Legacy data | [Read-only archive](legacy_archive.md) · [Freeze evidence](legacy_freeze.md) |

`dmsg_types` defines public data contracts only; `dmsg_protocol` implements canonical encoding, profile verification, and identity adaptation; `dmsg_runtime` and each canister persist internal state. Independent third-party signature verification does not require implementing account databases, handle management, or paid delivery services.

## Exchange Objects

The exchange object is an RFC 9052 tagged COSE_Sign1 structure: `18([protected_bstr, unprotected_map, payload_bstr, signature_bstr])`. The Rust `Statement { issuer, subject, issued_at, content }` represents a preparation and parsing view, **not an extra wire payload wrapper**. `SignedArtifact { cose_sign1, cose_key }` is an optional packaging DTO.

| Content | Protected header `typ` (16) | payload | Other protected headers |
| --- | --- | --- | --- |
| Text | `application/vnd.dmsg.text-statement+cose;v=1` | Raw UTF-8, 1..4096 bytes | 3=`text/plain;charset=utf-8`, crit=[15,16] |
| Digest | `application/vnd.dmsg.digest-statement+cose;v=1` | Original SHA-256, 32 bytes | 258=-16, optional 259=original media type, 260=original URI, crit=[15,16,258] |
| File statement | `application/vnd.dmsg.file-statement+cose;v=1` | Deterministic CBOR: text and one file's SHA-256, optional file metadata | 3=`application/cbor`, crit=[15,16]; no 258/259/260 |

`StatementContent::FileStatement { text, sha256, content_type, location }` signs a statement about one exact file. Its payload is the closed integer-key map `{1: text, 2: sha256, ?3: content_type, ?4: location}`. Text is preserved verbatim as 1..4096 UTF-8 bytes; SHA-256 is a 32-byte bstr over the original file bytes. Optional media type and URI follow the digest profile's validation bounds, but live inside this payload. Missing optional keys are omitted, never null. The payload must use RFC 8949 core deterministic CBOR (definite lengths, shortest encodings, sorted keys), with no duplicate/unknown keys, tags or trailing bytes, and is capped at 16,384 bytes. Verification preserves original COSE protected bytes while requiring this payload's canonical encoding.

The text and file digest are jointly signed. The text can express agreement, objections or observations; the profile itself grants no publishing/acceptance authority and does not prove that the signer reviewed the file. `subject` may identify the wider project or matter; the SHA-256 field identifies the exact file version. Verification never automatically fetches `location`. Business actions requiring machine-readable roles, actions or project permissions need their own explicit profiles. The `signDocument` request of dmsg-connect/1 represents this content as `{kind: "file_statement", text, sha256, contentType?, location?}`, with lowercase hex SHA-256, and the approval shows the text alongside the digest and original-file verification status.

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

Identity adapters accept explicit namespaces: `account_issuer("https://dmsg.example/u/", account)`, or a Principal issuer formed by appending canonical Principal text to its namespace, such as `https://id.example/ic/mainnet/aaaaa-aa`. Namespaces are fixed URI prefixes ending with `/` or `:`, without query or fragment, at most 8128 bytes. Account migration does not alter identity URIs; no zero-padding, truncation, or hashing is used to fit length constraints.

New messages use RFC 8949 §4.2.1 core deterministic CBOR: shortest encoding, keys sorted by encoded byte order, no floating-point numbers, duplicate keys, or trailing bytes. Existing COSE verification preserves original protected bytes and must not re-sort prior to verification. Local signing endpoints accept only inputs prepared according to profile rules. To-be-signed structures are capped at 65536 bytes, COSE_Key at 2048 bytes, signature artifacts at 196608 bytes, and TSA tokens at 131072 bytes. The library decoder enforces recursion limits; the SDK additionally enforces limits of 24 levels and 50000 nodes.

ICP business time and approval durations are u64 Unix **milliseconds**, evaluated using `now < expires_at`. Certificates and ICRC `created_at_time` are in nanoseconds; ledger retries retain original values. CBOR u128 values exceeding u64 use tag 2 shortest big-endian bstr; they must not be converted via JS Number. ICRC Account is `{owner: bstr, subaccount: bstr .size 32 / null}`.

Web applications reach dMsg through [dmsg-connect/1](connect-v1.md): its page frames use Xid text for `accountId`, lowercase hex for IDs, nonces and digests, standard base64 for keys, payloads and signatures, and decimal strings for large integers; statement `issuedAt` is in seconds, while outer `expiresAt` is in milliseconds. Standard Rust serde JSON output is not equivalent to these frames. The page's origin is reported by the browser and relayed by the bridge page; the dMsg client checks it against the application's registration.

## Signatures and Keys

To-be-signed bytes are strictly `CBOR(["Signature1", protected_bstr, h'', payload_bstr])`, with empty `external_aad`.

| Algorithm | COSE alg | Signature | Public Key |
| --- | --- | --- | --- |
| Ed25519 | -19 | Direct signature over Sig_structure | OKP, crv=6, x=32 bytes |

dMsg supports Ed25519 only; as of 2026-10-07 there are no ES256K, BIP340 endpoints or private algorithm tags. Failures do not trigger automatic algorithm switching. Public-only COSE_Key forbids private key `d` (-4); `alg` must match, `key_ops` if present must permit `verify`, and `kid` if present must match the protected header.

Public key thumbprints follow RFC 9679 SHA-256 over the required public parameters `{1: 1, -1: 6, -2: x}`, excluding `kid`, `alg` and `key_ops`. dMsg formal documents are signed by the account's device Ed25519 keys, and the `kid` is that device key's thumbprint; whether the device belongs to the account and whether the account service authorized the signature is proven by the certified execution receipt, never by the key itself.

vetKD is used only for content-root recovery envelopes: the context is `["dmsg/content-root/v2", environment, 2]` and every account shares the derived public key; the IBE identity of a root generation is `[account_id, generation]`. It is not a document-signature algorithm.

## ICP Execution Approvals and Receipts

`digest(domain, value) = SHA256(CBOR([1, domain, value]))`.

```text
request_id = digest("dmsg/execution-request/v2",
  [account_id, security_epoch, device_id, sequence])

signature = Ed25519(device_key, Sig_structure)          // kid = RFC9679(device_key)

digest("dmsg/device-approval/v2", [
  target_user_principal_bytes, account_id, "dmsg/attest/v1",
  device_id, security_epoch, sequence, request_id, expires_at,
  digest("dmsg/attest/v1", [statement, checked_browser_origin, signature])
])
```

The device signs the Sig_structure and then this approval digest with the same Ed25519 key; `statement` is the CBOR of the Rust `Statement` (`Option` as value or null, unit enums as name strings, payload enums as single-entry maps). Application actions use `AppActionAttestRequest { account_id, issuer, action, signature, approval }`, whose statement is `{issuer, content: {AppAction: action}}` and whose origin is `action.origin`. A recovered device's root derivation uses `dmsg/derive-root/v1` with the command `[generation, transport_key (bstr .size 48), max_cycles]`. Account mutations use the independent `dmsg/account/v2` domain with command `[expected_version, AccountCommand]`.

Protected headers, payload and approval context are frozen prior to signing. The user home checks that `issuer` belongs to the account, that the device is an active device of the account with `FormalApprove`, that the purpose is allowed by the account policy and the monthly quota has room, verifies both signatures, and in the same message charges the quota, stores the artifact and writes the certified receipt. Browser origin is limited to 256 bytes and represents an exact HTTPS origin (Local deployments also accept exact loopback HTTP origins); it is checked by the dMsg client, and device signatures do not independently authenticate browser origin.

Idempotency is scoped to `account_id/request_id`; requests with the same ID but differing parameters are rejected. A lost reply is replayed with `get_attestation(account_id, request_id)`, which returns the same artifact. Strict device sequence numbers and execution watermarks continue to prevent replay attacks after results are cleaned up; an exact replay of a cleaned request returns `ResultExpired`.

`get_execution_receipt(account_id, request_id)` returns a certified ICP leaf; querying requires account authentication. The path is `b"execution/" || account_id[12] || request_id[32]`. `ExecutionReceipt` (schema 2) stores issuer, device/epoch, approval time/deadline, origin, SHA-256 of to-be-signed bytes, the device key thumbprint, and SHA-256 of raw signature bytes. It does not modify portable signature artifacts. Upgrades reconstruct leaves from stable execution records, and leaves are deleted when results are pruned; the query then returns a certified absence proof.

Receipt verification requires first validating the specified user canister's IC certificate, witness, path, and leaf value, followed by matching the issuer, to-be-signed digest, public key thumbprint, and signature digest. The SDK `verifyExecutionReceipt` performs both steps; Rust `match_execution_receipt` performs only binding checks, leaving certificate validation to the caller. A COSE_Sign1 without a receipt only proves that some device key signed those bytes, not dMsg authorization. A receipt proves execution authorization recorded by this service; it does not automatically prove external project permissions or current device state.

## Xid Issuance

The user canister uses `ic_auth_types::XidGenerator` for persistent ID allocation, formatted as `timestamp_seconds[4] || allocator_fingerprint[5] || counter[3]`. The fingerprint is derived from the first 5 bytes of `digest("dmsg/account-id-generator/v1", ["dmsg", environment, issuer_namespace, creating_canister])`, with the full namespace digest stored separately in configuration for upgrade validation. Counters advance during the same second or clock rollback; new seconds reset the counter to 0. Time overflow or counter exhaustion fails explicitly.

Once authentication, device PoP, quota, and unique binding checks succeed, a single execution segment without awaits commits the account, authentication index, quota, and allocator state. Retried requests for existing authentications return the existing account. A deployment may run several user homes; every service routes an account to the home named by the fingerprint embedded in its ID. The `dmsg_handle` registry's `user_homes` is the authoritative list and `registration_homes` the subset currently accepting new accounts; clients query `my_account` on every home in parallel to find a login's account. Cross-home Principal uniqueness is deliberately not enforced.

## Timestamps and Verification Reports

The RFC 9921 CTT SHA-256 MessageImprint is `SHA256(CBOR(signature_bstr))`, including the CBOR bstr header, distinguishing it from the raw signature digest in execution receipts. Header 270 carries an opaque token; verification neither requests nor validates TSA trust. CMS signatures, imprints, certificate chains, key purposes, policies, and revocation status require independent verification.

`verify_artifact` and SDK `verifyDocumentArtifact` check profile compliance and cryptographic signatures; the SDK result's `checks` field individually reports `signature`, `content`, `issuerBinding`, `authorization`, `timestamp`, and `currentStatus`; its top-level `signature` holds the raw signature bytes. Standalone embedded text is `verified`; when the original file of a digest or file statement is omitted, `content` is `not_provided`; when identities or TSAs are unverified, they are reported as `not_checked`. An embedded public key or a single boolean success result cannot serve as complete proof.

Current implementations do not provide TSA network/CMS validation, SCITT transparency services, long-term archiving, or on-chain state anchoring endpoints. Optional paid delivery, handle management, and ICP control contracts are maintained separately.

## Verification and Standards References

```sh
cargo test -p dmsg_types -p dmsg_protocol
cargo run -p dmsg_types --example protocol_vectors > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

Standards references for signatures, identity headers, and profiles: [RFC 9052](https://www.rfc-editor.org/rfc/rfc9052.html), [RFC 9597](https://www.rfc-editor.org/rfc/rfc9597.html), [RFC 8392](https://www.rfc-editor.org/rfc/rfc8392.html), and [RFC 9596](https://www.rfc-editor.org/rfc/rfc9596.html). Digests, public key thumbprints, and timestamps are based on [RFC 9995](https://www.rfc-editor.org/rfc/rfc9995.html), [RFC 9679 §4.2](https://www.rfc-editor.org/rfc/rfc9679.html#section-4.2), and [RFC 9921](https://www.rfc-editor.org/rfc/rfc9921.html). SCITT statement and evidence separation references [RFC 9943](https://www.rfc-editor.org/rfc/rfc9943.html); current document profiles do not claim implementation of its transparency service.

## Agent Delegation

A dMsg account can act as an Agent Delegation 1.0 principal: controllers are self-held Ed25519 keys kept in the client vault, registered with a proof of possession at the user home and used to sign events locally, and `dmsg_directory` publishes the principal document as ICP-certified HTTP. Interfaces and verification scope: [agent.md](agent.md) (Chinese).

## Commercial Services

The product-neutral `membership/2` and `dmsg-commerce/2` contracts are documented in [commerce.md](commerce.md), with generated [integration.cddl](integration.cddl) records and independent vectors. [commerce.cddl](commerce.cddl) retains only the resource/entitlement framing that still uses v1 domains. Paid delivery keeps its separate profile 2. These versions do not change formal statement or device attestation bytes.

Team wallets `dmsg-wallet/1`: the public contract for members, policy, proposals, address derivation, external-chain plans and cosigners is [wallet.md](wallet.md) (Chinese). Device approvals reuse `ApplicationApproval` with the new purposes `WalletCreate`, `WalletProposal` and `WalletJoin` and the device capability `WalletApprove`.
