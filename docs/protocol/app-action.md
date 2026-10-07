# Application action profile v1

Status (2026-10-07): implemented. Browser `signAction`, product-authority admission,
account linkage, device signature and approval, the existing execution quota/journal,
schema-rendered confirmation UI and the certified receipt are connected. TokenList is
the first integration; it still consumes the earlier TokenList-specific command
variants and must adopt the registered schema below before its next release.

## Division of responsibility

dMsg is the platform and does not define any application's commands. It owns:

- the closed value model (`ActionValue`) and schema format (`ActionSchema`);
- the canonical encoding and the digests below;
- generic validation, and a confirmation page that renders any registered schema.

Each application owns its command vocabulary. It writes an `ActionSchema`, registers
it through governance as `AppRegistration.action_schema`, and keeps the projection
from its native input to `ActionCommand` in its own code. dMsg proves that an approved
device signed exactly this command, as rendered from the certified schema. The
receiver proves that the input it executes projects to exactly the signed command.
A new application therefore needs a governance registration, not a dMsg release.
Only a value type that no existing type expresses is a dMsg change, and then every
application can use it.

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

`AppAction` carries:

- `app_id`, `app_config_version`, `environment`, `origin` and `receiver`;
- `actor`, the product's own actor identity (1–64 opaque bytes), and
  `signing_account`, the dMsg account; the two are distinct signed identities;
- `operation_id` and `intent_hash`, the product's request identity and the
  commitment to its complete intent, including any subject, precondition, role
  or policy commitments the product keeps;
- `schema_hash = SHA256(CBOR([1, "dmsg/action-schema/v1", schema]))`, naming the
  exact schema that gives the command its meaning;
- `command = { name, args }` and the file manifest `files`.

A signature does not by itself establish that `actor` and `signing_account` are
linked. The registered product authority confirms the linkage before execution;
issuer/key provenance and the certified execution receipt are checked separately.

## Value model and schema

`ActionValue` is closed: `Nat` (u64), `Bool`, `Text`, `Hash` (nonzero 32 bytes),
`Principal`, `Choice` (a declared option), `Artifact` (an exact external reference),
`Null` (an absent optional value), `List` and `Record`. There is no opaque byte
value, method name, executable input, independent `display_summary` or asset
transfer value. Command arguments and record fields (`ActionArgs`) are named and
must equal the schema fields in order, so a projection cannot silently swap two
fields of the same type.

`ActionSchema { version: 1, commands }` declares each command's name, title and
fields (`SchemaFields`). A field has a name, a label and a `FieldType`:
`Nat { min, max }`, `Bool { yes, no }` (labels for both values),
`Text { max_bytes, multiline }`, `Hash`, `Principal`, `Choice { options }`,
`Artifact`, `Optional { item }`, `List { item, max_items }` or `Record { fields }`.
Titles and labels are plain text with one entry per locale. Every field is always
shown; a schema cannot hide a field or substitute a summary.

The following example is the shape of a review decision:

```text
DecideReview  "Record review decision"
  project_id  Nat{1..}            case_id  Nat{1..}          round  Nat{1..=2^32-1}
  outcome     Choice{Approved | Rejected | ChangesRequested}
  changes     List{64, Record{locator Text{128}, detail Text{4096, multiline},
                              blocking Bool{"Must be resolved" | "Optional"}}}
  rationale   Text{4096, multiline}
```

## Bounds and verification

- Schema: at most 16 KiB, 32 commands, 32 fields per command or record, 32 choice
  options and 8 locales per label; names are ASCII identifiers of at most 64 bytes;
  labels are single-line text of at most 256 bytes; `Text` allows at most 4096 bytes
  and `List` at most 64 items. A top-level argument is nesting level one, and types
  and values nest at most three levels. `Optional` cannot wrap `Optional`. The
  labels that tell apart the commands, the fields of one command or record, the
  options of a `Choice` or the two values of a `Bool` share no text in any locale,
  so the confirmation page never renders two of them alike.
- Action body: at most 48 KiB; total Sig_structure remains at most 64 KiB.
- Intent: positive validity interval of at most 300,000 milliseconds; historical
  signature parsing validates the interval shape without expiring the signature.
- Files: at most 64, strictly ascending unique ASCII file locators, positive
  revision, SHA-256, byte length up to 256 MiB, media type and explicit original
  or encrypted representation. Display names are also signed.
- Artifact URIs are canonical, printable ASCII HTTPS/IPFS URLs without credentials,
  at most 4096 bytes. They are never fetched automatically.

`validate_action_schema` runs when governance registers the application:
`validate_app` requires a schema exactly when the app has `SignAction`.
`validate_app_action` checks content without a clock or schema, so a historical
signature can be parsed anywhere. `validate_action_command` checks `schema_hash`,
the command name (`UnsupportedProtocol` if unknown) and every argument against the
schema. `validate_action_admission` additionally checks the registration, exact
environment/origin/config version, profile/capability, pause and current intent
deadline. None of them authenticates the product preparation or authorizes project
roles: the product authority must attest to this exact action before an execution
grant is created, and the receiver must recheck roles, policy, versions and deadline
when committing the action.

`verify_artifact` checks COSE and mathematical validity only: it does not reconstruct
the product intent, fetch files, authenticate account linkage or verify current
authority. A verifier that holds a schema whose digest equals `schema_hash` can
render and check the command offline.

The confirmation page reads the certified registration, checks `schema_hash`, and
renders the title and labels only from the schema; the request carries no display
text. The page states that the application defined the wording and governance
registered it. Only a new approval requires the current registration: after a
pause or a new registration version, an action that was already approved still
opens for reconciliation, without the schema-rendered details if the registration
no longer matches.

The document-only `dmsg_user.attest` rejects `AppAction` content with
`UnsupportedProtocol`. `attest_app_action` takes `AppActionAttestRequest { account_id,
issuer, action, signature, approval }`, admits the action against the registration,
calls the fixed product authority's `verify_dmsg_action` for the exact action and
account, and rechecks account/device/configuration after the await. It uses the
same monthly quota, original request replay (`get_attestation`) and certified
receipt (schema 2) machinery as document attestation.

## Integrating an application

1. Write the schema and the projection from the application's input to
   `ActionCommand`. Use one projection for preparation, for the authority's
   `verify_dmsg_action` check and for evidence verification.
2. Register the schema with `register_integration_app` through governance. Its
   validation preview lists the schema digest and command names. Changing the schema
   requires the next `config_version`, so in-flight actions prepared against the old
   version fail admission.
3. Build `AppAction` with the schema digest and call `signAction` from the
   registered origin.

## Regression evidence

`dmsg_protocol` tests sign every command of a sample review schema with a real
deterministic Ed25519 fixture and reject a changed origin, receiver, actor, schema
digest, command, argument, display name, file hash/version/representation or intent
expiry. Schema bounds, ambiguous labels, unknown commands, swapped or renamed
arguments, out-of-range numbers, undeclared choices, misplaced `Null`, depth,
unknown value kinds, registration without a schema, paused registration and
expired admission are tested. `integration_vectors.json` includes the registration,
each action and its signature, checked by an independent TypeScript encoder and
validator, and the extension renders the same vectors in a server-side render test.
Existing document vectors remain byte-for-byte unchanged.
