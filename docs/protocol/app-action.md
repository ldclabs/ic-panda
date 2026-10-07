# Application action profile v1

Status (2026-10-07): implemented. Browser `signAction`, product-authority admission,
account linkage, device signature and approval, the existing execution quota/journal,
confirmation UI and independent TokenList receipt consumption are connected. Actual
local Wasm covers protected TokenList session → prepare → device signature →
`dmsg_user.attest_app_action` → certified receipt → project commit and idempotent
replay; production release configuration is separate.

## Signed object

`typ` is `application/vnd.dmsg.app-action+cose;v=1`. Protected headers are exactly
`alg` (1), `crit` (2) = `[15,16]`, content type (3) = `application/cbor`, `kid` (4),
CWT claims (15) = `{1: issuer}`, and `typ` (16). There is no separate `sub` or
claimed `iat` that could contradict the typed action. External AAD is empty.
The signature is Ed25519 (-19) by the approving device key, exactly as for the
three document profiles; `AppAction` is a separate policy purpose in the account's
`SensitivePolicy`.

The payload is deterministic CBOR of `AppAction` from
[integration.cddl](integration.cddl). Unlike the existing document profiles,
application origin, receiver and intent validity window are signed content.
The full digest is `SHA256(CBOR([1, "dmsg/app-action/v1", action]))`.

The closed command variants are `TokenListCertifyDisclosure`,
`TokenListDecideReview`, `TokenListCertifyTransition` and
`TokenListApproveTransition`. They retain project/object/version, the actual
approve/refuse outcome, full rationale, requested changes and optional analysis
reference. A display view must be derived from these fields. There is no
independent `display_summary`, arbitrary method name, opaque executable input or
asset-transfer command. Another product requires a reviewed, versioned variant.

The product `actor_id` and dMsg `signing_account` are distinct signed identities.
The registered product authority confirms their current linkage before execution;
issuer/key provenance and the authenticated execution receipt are checked separately. A valid
signature does not itself establish that those two accounts are linked.

## Bounds and verification

- Action body: at most 48 KiB; total Sig_structure remains at most 64 KiB.
- Intent: positive validity interval of at most 300,000 milliseconds; historical
  signature parsing validates the interval shape without expiring the signature.
- Files: at most 64, strictly ascending unique ASCII file locators, positive
  revision, SHA-256, byte length up to 256 MiB, media type and explicit original
  or encrypted representation. Display names are also signed.
- Review changes: at most 64; rationale/details at most 4096 UTF-8 bytes; field
  locator at most 128 bytes. Unknown fields, commands, profiles or critical
  semantics are rejected. Missing options are encoded as `null` in this schema.
- Analysis URIs are canonical, printable ASCII HTTPS/IPFS URLs without credentials,
  at most 4096 bytes. They are never fetched automatically.

`validate_action_admission` additionally checks the trusted app registration,
exact environment/origin/config version, profile/capability, pause and current
intent deadline. It cannot authenticate the product preparation or authorize
project roles. The product authority must attest to this exact action before an
execution grant is created, and the receiver must recheck roles, policy, versions
and deadline when committing the action.

`verify_artifact` checks COSE and mathematical validity only: it does not independently
reconstruct the product intent, fetch files, authenticate account linkage or
verify current authority. The independent SDK exposes shape/commitment, COSE and
IC certificate verification as separate checks.

The document-only `dmsg_user.attest` rejects `AppAction` content with
`UnsupportedProtocol`. `attest_app_action` takes `AppActionAttestRequest { account_id,
issuer, action, signature, approval }`, calls the fixed product authority for the
exact prepared body/account and rechecks account/device/configuration after the
await. It uses the same monthly quota, original request replay (`get_attestation`)
and certified receipt (schema 2) machinery as document attestation. A generic
document cannot bypass it.

## Regression evidence

`dmsg_protocol` tests sign all four variants with a real deterministic Ed25519
fixture and reject changed origin, receiver, actor, command, display name,
file hash/version/representation or intent expiry. Invalid schema/duplicate files,
paused registration, expired admission and the generic-document bypass are tested.
`integration_vectors.json` includes each complete action commitment, checked by
an independent TypeScript encoder and validator. Existing document vectors remain
byte-for-byte unchanged.
