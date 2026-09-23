# @dmsg/sdk

Development package for the third-party integration contract. Current exports
provide exact wire types, input validation, deterministic CBOR, commitments and
integer PANDA quotation arithmetic. These functions do not prove payment, SNS control or product delivery. The
`@dmsg/sdk/ic` export independently verifies a dedicated IC authentication
certificate against an explicitly trusted root, user home and pending challenge.
The `@dmsg/sdk/browser` export implements the v4 authenticated browser transport for authentication, documents and recovery. Action signing and checkout are not yet advertised as capabilities.

Use `bigint` for **all** wire integers, including versions. Principals, hashes and
account IDs are raw `Uint8Array` values, not display text. The codec rejects JS
Number, floats, undefined, invalid Unicode and unsupported object types. Public
types are generated from `dmsg_types/src/integration.rs` with
`python3 scripts/generate-integration-sdk.py`; validation rejects extra fields.

Run `node --test packages/dmsg-sdk/tests/*.test.ts` from the repository root with
Node >=22.18. The tests independently encode the committed Rust fixtures and
compare every CBOR and SHA-256 byte. The fixed BLS certificate and optional real PocketIC proof run through an
independent verifier. No Svelte, device database or wallet store is imported. A bundler supporting TypeScript consumes the source exports; this is
not yet a published npm release.

See [browser-v4](../../docs/protocol/browser-v4.md) for the exact frames, session-key ownership checks and durable recovery contract.
