# Statement v3 Design

English | [简体中文](statement-v3-design_zh.md)

Status: As of 2026-09-23, public types, three document profiles, Xid accounts, certified execution receipts, and SDK bindings have been implemented according to this design; the actual contracts are governed by the [protocol specification](README.md) and [CDDL](statements.cddl). Initial evaluation was based on `4fc3ca2`. Full production UI, TSA trust verification, and multi-allocator registry are out of scope for this round. "v3" is the design iteration name; wire protocol versions are explicitly defined by specific profiles.

## 1. Design Decisions

Adopt standard COSE Signed Statements as the exchange object, narrowing dMsg custom additions to well-defined profiles, identity adapters, and execution authorizations. Eliminate the previously fixed `Statement { schema, subject, request_id, origin, audience, expires_at, body }` generic wrapper.

| Current Design | Proposal |
| --- | --- |
| Private BIP340 algorithm tags | Removed from dMsg signing profiles and corresponding endpoints; retains shared crypto library capabilities for other consumers |
| `request_id` embedded in statements | Retained only in execution requests, deduplication state, and certified execution receipts |
| `subject` represents signing account and is fixed 32 bytes | Signing entity replaced by `iss`; `sub` represents declared subject; both adopt CWT text semantics |
| Internal `SubjectId` directly reuses `Hash32` | Uses independent `AccountId`, stored as 12-byte Xid and persistently allocated by the user canister |
| `origin` mandatory | Relegated to device approval / execution context; defined by dedicated profiles only when business origin is strictly required |
| `audience` mandatory | Only audience-restricted protocols use standard `aud`; public document statements do not mandate it |
| `expires_at` represents request deadline | Retained in execution context; validity of the statement itself is defined separately per profile |
| Integer schema + Rust enum shapes | Identifies purpose and version of the complete signed object via protected `typ`; payload adheres to the corresponding content format |
| `FileAttestation { sha256, size }` | Simple digest attestation uses COSE Hash Envelope; business signings (publishing/acceptance) use complete business statements |
| `kid` and signature both fixed length | `kid` is opaque variable-length bytes; signature and public key lengths are validated by the chosen algorithm |

Ed25519 (COSE alg -19) is the primary required signature algorithm; ES256K (-47) is an optional algorithm for ICP adaptation. Requests cannot require disabled algorithms, nor automatically substitute algorithms on failure.

## 2. Standards Basis and Application Choices

| Standard | Utilized Parts |
| --- | --- |
| [RFC 9052](https://www.rfc-editor.org/rfc/rfc9052.html) | COSE_Sign1, COSE_Key, protected headers, Sig_structure |
| [RFC 9597](https://www.rfc-editor.org/rfc/rfc9597.html) | Carrying CWT claims in protected header 15, usable for non-CWT payloads |
| [RFC 8392](https://www.rfc-editor.org/rfc/rfc8392.html) | Existing types and semantics for iss/sub/aud and time claims |
| [RFC 9596](https://www.rfc-editor.org/rfc/rfc9596.html) | Protected header 16: `typ` for complete COSE objects |
| [RFC 9943](https://www.rfc-editor.org/rfc/rfc9943.html) | SCITT issuer, subject, statement, and evidence separation; profile definitions on demand |
| [RFC 9995](https://www.rfc-editor.org/rfc/rfc9995.html) | Hash Envelope for file digests |
| [RFC 9679](https://www.rfc-editor.org/rfc/rfc9679.html) | Portable COSE public key thumbprints |
| [RFC 9921](https://www.rfc-editor.org/rfc/rfc9921.html) | RFC 3161 timestamp evidence over COSE signatures |

RFC 9943 was published in June 2026, and RFC 9995 was published in July 2026. The current proposal reuses their relevant formats without asserting that existing canisters implement SCITT transparency services or regulated TSAs.

This draft selects mandatory protected fields, enabled algorithms, URI conventions, purpose profiles, and implementation resource bounds. These choices should be explicitly designated as application constraints rather than universal requirements for all COSE messages. Concrete media types and version parameters are frozen in the CDDL; using experimental names does not claim registered status.

## 3. Identity Identifiers

### 3.1 Distinguishing issuer and subject

- `iss`: The signing entity responsible for the statement, which may be a user, organization, device, or service. Binding it to the actual signing key requires verifiable evidence.
- `sub`: The object being described by the statement, such as a file, project, account, or device. It can also correlate multiple statements concerning the same object.
- `kid`: A hint for selecting the verification key; does not equal an account identity and does not guarantee global uniqueness.

The `sub` claim may be omitted for general document profiles; SCITT-oriented profiles must supply `iss` and `sub` according to their requirements. Signers must not be duplicated into the declared subject merely to fill fields.

### 3.2 Recommendation: Signed objects use text identity references

CWT `iss` and `sub` are text strings. This draft further recommends that issuers use canonical absolute URIs to convey both identity namespace and identifier. Subjects may be absolute URIs or text identifiers explicitly defined by profiles and interpreted within the issuer scope.

The following are formatting examples only and do not indicate that corresponding services are deployed:

| Input Identifier | Adapted Issuer Example |
| --- | --- |
| TokenList Xid, 12 raw bytes | `https://tokenlist.example/users/<canonical_20_char_xid>` |
| ICP Principal, 0–29 raw bytes | `https://identity.example/ic/mainnet/principals/<canonical_principal_text>` |
| dMsg AccountId, 12 raw bytes Xid | `https://dmsg.example/u/<canonical_20_char_xid>` |

Identifier types must not be inferred from length alone: 12 bytes may also represent a Principal. Adapters must know the applicable namespace, encoding scheme, and network context. The empty-byte representation of a Principal is a valid identifier for the management canister; whether a given signing endpoint permits it as an account is determined by that endpoint's permission rules.

SDK/ICP adaptation interfaces may continue accepting native bytes, converting them losslessly into canonical identity references via dedicated adapters. No zero-padding, truncation, or preliminary SHA-256 is required. URIs serve strictly as identifiers; validators do not perform automated network fetches based on untrusted issuer/sub values.

Identity URI generation rules must remain stable. Existing signatures are verified against their original string verbatim; URLs must not be reconstructed, case-modified, or reassigned to different namespaces prior to signature verification. Identity namespaces must not track migratable storage shards or ephemeral signing keys. Signers must not treat arbitrary caller-supplied issuers as authenticated identities.

### 3.3 Trade-offs of native binary schemes

If cross-language consumers explicitly require preserving raw bytes within signed objects, a distinct `{ namespace: uri, id: bstr }` schema could be designed. While supporting Xids, Principals, and other identifiers, this represents a new data format requiring defined namespaces, canonicalization, and validation rules.

This draft prioritizes established CWT text rules. One cannot simply replace standard `iss`/`sub` values with `bstr` while continuing to claim compliance with the standard CWT claim definition. Furthermore, internal dMsg account ID lengths should not impose type constraints on general statements.

### 3.4 dMsg internal accounts adopt Xid

Accounts use the semantic name `AccountId`, directly re-exporting `ic_auth_types::Xid` in Rust without maintaining local wrapper types. `AccountId([u8; 12])` constructs raw byte values, while canonical text, JSON, CBOR, and Candid encodings are provided by the shared type.

| Usage Location | Representation |
| --- | --- |
| Candid / CBOR account fields | 12-byte blob / bstr |
| Stable storage primary keys | 12 bytes; Storable/indexing implementations remain inside canisters |
| JSON / URLs / logs | Canonical 20-character Xid text |
| COSE `iss` | URI under a stable identity namespace containing the Xid text |

Digests, key identifiers, device identifiers, and operation identifiers each follow their own contracts. Adopting this account type is not a bulk replacement for all 32-byte values.

Issuance directly reuses `ic_auth_types` 0.10.4 `XidGenerator`, explicitly passing the namespace fingerprint and timestamp, and persisting the returned new state. dMsg retains only local namespace validation and business error mappings.

```text
AccountId = timestamp_seconds[4] || allocator_fingerprint[5] || counter[3]
```

The three segments are big-endian encoded. `XidGenerator` retains `profile_version`, `fingerprint`, `last_second`, and `next_counter`; user configuration separately stores the full `allocator_namespace_digest`. The fingerprint origin incorporates the fixed application namespace, environment, and creating canister ID; exact domains and encodings are frozen in the public protocol and interoperability vectors.

Issuance rules:

1. Convert business millisecond timestamps to whole seconds only at the allocator boundary, verifying they fit within a `u32`.
2. Reset the counter to 0 when time advances to a new second; reuse the stored second and increment the counter during the same second or clock skew.
3. Reject issuance when the 24-bit counter is exhausted until time advances past the stored second; do not wrap modulo or overwrite existing IDs. Reject explicitly if time exceeds `u32` without truncation.
4. `allocate` returns candidate IDs and new candidate states; errors do not alter the existing state. Check that the local account key does not already exist before committing.
5. Account creation endpoints perform authentication, device PoP, unique binding, and quota checks before writing allocator, account, authentication index, and billing/quota state in a single synchronous execution segment without awaits. All checks capable of returning `Err` execute before state writes; traps roll back via canister message transactions.
6. Retries of existing creation requests return the already created account without reallocating. Initial creation no longer depends on `raw_rand`; existing staging records for that asynchronous step are removed with the workflow refactor.

Allocators are internal state and do not enter public type libraries. Upgrades restore original seconds and counters, verifying namespace, environment, creating canister, and allocator version; initialization must not silently reset state due to configuration changes or decoding errors. Restoring from backups must not treat historical issuance watermarks as current when resuming allocation.

The persistent second/counter of a single allocator provides non-reuse guarantees; the 5-byte truncated hash does not itself guarantee absolute uniqueness across allocators. A fixed single user home can safely use local issuance. Before scaling to multiple user homes, fingerprints must be registered and verified against an account allocation authority to ensure allocator fingerprints do not collide within the same namespace or get reassigned. Fingerprint collisions must be handled prior to enabling new allocators, rather than reconciling after two shards independently create accounts with identical IDs.

Accounts retain their `AccountId` when moved to other storage locations; the target location creates new accounts using its own allocator without copying or resetting the original allocator. The issuer URI namespace remains stable and does not change to the current shard address.

Xids are public structured identifiers reflecting allocation time and order within a given allocator. Authorization continues to be enforced through principal bindings, devices, and policies; Xids do not serve as secret credentials or trusted timestamps. Descriptions of random account IDs in companion designs must be synchronized with this generation scheme.

Implementation synchronously audits account references, certified leaves, indexing and pagination prefixes, device approval digests, name ownership, payment offers, signing key paths, and vetKD inputs across all four canisters. Because account bytes feed key derivation, transitioning from legacy experimental IDs to Xids alters derivation inputs; truncating legacy IDs cannot be treated as identity-preserving key transformations. Fresh instances and new test vectors must be used per development guidelines.

## 4. Separation of Statements and Execution

The execution layer continues to persist and validate: `account_id`, `request_id`, target canister, actual request origin, device, security version, approval sequence, execution deadline, and maximum cycle budget.

Approvals must bind this entire context, along with the digest of final COSE to-be-signed bytes and immutable target key descriptions. Protected headers and payloads are frozen prior to user confirmation; retries must not refresh content or timestamps. Following execution, queries or certified receipts bind the `request_id` to to-be-signed digests and signature outputs.

Removing `request_id` from statements does not remove deduplication: the same execution still uses the same operation ID, unknown outcomes query the original request, and non-replayable sequence state is preserved after result cleanup. `request_id` must not be renamed to `nonce` or `cti` and forcibly reintroduced into general document statements.

Identical issuers, keys, protected headers, and payloads may produce identical signature artifacts. When distinguishing between "two signing events of identical content", event IDs or timestamps belong in explicit business statements or execution evidence. File digests and signature digests cannot substitute for idempotent operation IDs across all business logic.

## 5. Document Profiles

### 5.1 Inline Content

Protected headers include signature algorithm, `typ`, identity claims, and key references. Content type (3) identifies payload formatting; payloads can be explicitly encoded text, CBOR, or other supported formats. Unrecognized formats do not enter formal dMsg signing endpoints.

Plain text signs raw UTF-8 bytes directly, without `{ Statement: { text } }` wrappers. Structured statements carrying business semantics must be encoded according to their own public schemas. Signers must not reorder external JSON or adjust whitespace by default; profiles requiring JCS or deterministic CBOR must declare these rules explicitly.

### 5.2 File Digest

Adopts RFC 9995: payload is a digest; protected header 258 identifies the hash algorithm, 259 may identify original content type, and 260 may identify original content location. This profile forbids content type (3) and cannot reuse header requirements intended for inline messages.

Version 1 enables SHA-256 (header 258 value -16), with a 32-byte payload. This length derives from the digest algorithm and is independent of issuer/sub/kid lengths. No generic `size` field is needed; post-retrieval digest verification binds the exact byte sequence. Signing file length, release version, licensing, or acceptance semantics requires signing full manifests or business statements.

"Certifying a digest of these bytes", "publishing a file as project maintainer", and "accepting delivery of an asset" represent distinct business semantics. TokenList file release statements may include project, file version, and authorization provenance, with TokenList validating roles directly, rather than imposing these fields onto generic file attestations.

### 5.3 Statements About Files

`StatementContent::FileStatement` combines nonempty verbatim text and the SHA-256 of one exact file, with optional file media type and location. This is an inline structured statement, using the independent `application/vnd.dmsg.file-statement+cose;v=1` profile and content type `application/cbor`. Its closed payload schema uses integer keys 1=text, 2=SHA-256, optional 3=media type and 4=location, encoded as deterministic CBOR; the exact limits and byte rules are frozen in [the protocol](README.md) and [CDDL](statements.cddl). Hash Envelope headers 258/259/260 do not apply because this payload is a statement containing a digest, not a digest of the entire preimage.

Text and digest are inseparable under the signature. The key purpose is `Statement`; pure digests retain `FileAttestation`. Existing execution approvals bind the entire prepared object without another signing operation. With no original file, verification reports a valid signature but `content=NotProvided`; matching file bytes yield `Verified`, and a mismatch is an error. Confirmation displays both text and digest without claiming file review when no original bytes were checked. The profile expresses the signer's statement, not automatic publishing/acceptance authority; executable business semantics still require their own explicit schema and policy.

## 6. Candidate CDDL Structure

The following illustrates design structure; implementations flesh out concrete profile media types, semantic constraints, and interoperability vectors. This is not a newly published standalone contract.

```cddl
signed-statement = inline-statement / digest-statement

inline-statement = #6.18([
  protected: bstr .cbor inline-headers,
  unprotected: evidence-headers,
  payload: bstr,
  signature: bstr
])

digest-statement = #6.18([
  protected: bstr .cbor digest-headers,
  unprotected: evidence-headers,
  payload: bstr .size 32,
  signature: bstr
])

common-headers = (
  1: -19 / -47,          ; enabled signature algorithms
  4: bstr,              ; variable-length kid in this key profile
  15: identity-claims,   ; RFC 9597
  16: tstr              ; recognized, versioned profile / typ
)

inline-headers = {
  common-headers,
  2: [15, 16],
  3: tstr               ; inline payload content type
}

digest-headers = {
  common-headers,
  2: [15, 16, 258],
  258: -16,             ; SHA-256
  ? 259: tstr,
  ? 260: tstr
}

identity-claims = {
  1: tstr,              ; issuer: canonical absolute URI in this profile
  ? 2: tstr,            ; subject: the object being described
  ? 6: int              ; optional claimed iat, integer Unix seconds
}

evidence-headers = {
  ? 270: bstr           ; optional RFC 9921 CTT token
}
```

Here `crit` (2) lists protected headers that this profile requires consumers to understand. Decoders must reject duplicate fields and protected/unprotected header conflicts, recognize the rules corresponding to `typ`, and validate mandatory fields. Unknown parameters in `crit` must be rejected; recognizing a CWT container does not allow ignoring unknown authorization semantics within it. Profiles involving `aud`, `exp`, `nbf`, or other claims declare rules independently, rather than enabling arbitrary authorization through an open claims map.

Protected header bytes in the CDDL use `.cbor` to bind concrete structures. If outer evidence envelopes are defined, embedded COSE/key bytes must similarly reference their actual structures rather than defining unreferenced non-terminals and treating content as arbitrary `bstr`.

## 7. Time, Purpose, and Extensibility

- Request deadlines continue using integer milliseconds from existing ICP interfaces; CWT time claims follow their own `NumericDate` rules. When this profile uses `iat`, integer seconds are used; raw millisecond values must not be inserted.
- General document signatures do not expire by default. Business authorization validity and execution deadlines for signing requests are expressed separately and do not substitute for one another.
- `iat` is a signer-asserted time and does not constitute trusted time attestation. TSA validation is processed separately and cannot be accomplished via a local timestamp field.
- `typ` identifies the purpose and version of the complete signed object, while content type identifies the format of inline payloads; their responsibilities are distinct.
- Protocol extensibility does not imply dMsg allows arbitrary signatures. dMsg enables only parseable, displayable, policy-permitted profiles; it does not download executable interpreters based on URIs.
- Introducing new critical semantics requires explicit new profiles/versions. Unknown critical semantics are rejected and must not be downgraded or interpreted as generic text signatures.
- Resource bounds on URIs, headers, payloads, evidence, nesting depth, and entry counts are published by concrete implementations/profiles and maintained separately from fixed-length account IDs.

## 8. Public Keys, TSA, and Evidence

Public keys use `COSE_Key`. When public key thumbprints must be recomputed across implementations, RFC 9679 is used with a fixed digest algorithm; `kid` remains a variable-length selection hint and is not required to equal platform-derived IDs. ICP key origins, mainnet configuration, and account bindings reside in independent identity/key evidence.

TSA adopts RFC 9921 CTT over COSE signatures. File digests and TSA message imprints are distinct inputs and do not substitute for each other. Appending timestamps or other evidence alters outer evidence byte sequences, requiring distinct identifiers for signed content, execution records, and evidence package versions.

COSE protected headers are not encrypted; they do not contain private file names, download URLs, or unnecessary account details by default. Sensitive evidence can be encrypted in its entirety. When salted commitments to original content are needed, an appropriate preimage/profile must be defined rather than masquerading salted commitments as raw file SHA-256 hashes.

ICP certificates/witnesses, TSA tokens, and SCITT receipts follow distinct verification rules. Generic ICP state anchoring cannot be called a SCITT receipt merely because it is placed in a field. Verification results report content, signature, identity binding, historical authorization, time evidence, and current status individually, rather than conflating them into an ambiguous success boolean.

## 9. Implementation Scope and Acceptance

1. Freeze all three document profiles, URI generation rules, and supported algorithms; provide explicit adapters for Xids, Principals, and dMsg IDs.
2. Separate generic signature data from ICP execution requests, standardize internal accounts on `AccountId`/`Xid`, and implement persistent allocation and atomic commit rules from Section 3.4. Generic statements use identity URIs and do not import account storage types.
3. Remove dMsg BIP340 endpoints and private algorithm tags while maintaining independent verification for Ed25519 and ES256K; retain shared library support for other consumers.
4. Reuse `cose2` message and asynchronous signing interfaces, handling new protected headers, CWT semantics, and `crit` at the protocol layer. The generic Header map accommodates these tags without reinventing COSE decoders.
5. Verify approval bindings, idempotency, unknown outcome handling, upgrade recovery, and cross-language byte consistency after moving request metadata out of statements.
6. Positive test vectors cover lossless mapping of Xids/Principals, inline text, file digests, structured file statements, and TSA token attachment; negative vectors cover invalid identity namespaces, algorithms, `typ`, critical fields, time units, and identity/key bindings.
7. Xid issuance tests cover same-second concurrency, clock skew, upgrade recovery, counter and timestamp range exhaustion, configuration mismatches, duplicate registration retries, state immutability on rejection, and commit consistency across accounts, indices, and allocators; multi-allocator fingerprint collision handling is verified prior to enabling sharding.

Acceptance centers on third parties being able to understand messages using existing COSE/CWT tools while writing minimal custom code for business profiles and trust policies. Adopting standard formats does not automatically accomplish identity verification, project authorization, transparency logging, or timestamp service operations.
