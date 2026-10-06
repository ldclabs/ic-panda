# dmsg_directory

Publishes Agent Delegation 1.0 principal documents for every dMsg user home at one custom domain, as ICP-certified HTTP responses. User homes remain authoritative for controllers and signing; this canister only serves what they publish.

- `publish(account_id, PrincipalState)`: only the account's home publishes (first by the Xid allocator fingerprint, then the recorded home). Versions only increase; the same version must carry the same state.
- `get_publication`, `directory_config`: queries.
- `admin_add_user_home(home)`, `admin_set_custom_domains(domains)`: controllers or the fixed `governance` Principal. Homes are append-only (at most 64, distinct allocator fingerprints, repeating a listed home is a no-op); the new `dmsg_user` must name this canister as `directory_canister` and share the environment, issuer namespace and principal origin. Domains are replaced and recertified at once. Each method has a same-argument `validate_*` query that runs its checks against current state and renders the payload for an SNS generic-function proposal, or returns the error the method would return. Upgrades take no arguments and never change configuration.
- `http_request`: `GET /<account_id>` returns the exact JCS document; other paths return a certified 404; `/.well-known/ic-domains` lists custom domains. Responses are certified response-only with all headers. Routing follows the certification library: percent decoding and repeated slashes address the same path; a trailing slash stays distinct. Only the document's exact canonical `id` is authoritative for principal resolution.
- Documents remain limited to 64 KiB. Home and directory share a conservative byte budget that includes future retirement/compromise fields and maximum names. Oversized registrations fail before the home commits; reaching the byte budget can limit the account before 32 generations. Origins are limited to 512 bytes, query URL/profile prefix to 2 KiB; public configuration URLs must not contain raw JSON escape characters.
- Stable layout schema 4: config (memory 0), documents (memory 1) including a persisted SHA-256 body digest, and the certification map (memories 2 and 3). Under `http_expr`, the first segment of every certified expression path is a key of a `dmsg_runtime::cert_map` holding that segment's subtree hash: one per account document, `.well-known` for the domain list and `<*>` for the fallback 404. Only hashes are stored. A document response reveals its segment; the fallback also reveals the first segment of the exact request path and of every more specific wildcard path, which the HTTP gateway requires to be absent. Upgrades visit no documents: they recertify the fallback and domain responses, which this code renders, and publish the root. Changing the document response headers therefore needs every document recertified. Earlier development schemas are rejected; no migration is implemented.

Not implemented: home `handoff`, external-key reservation. Maximum capacity (documents per canister and upgrade instruction ceiling) has not been established. Contract: [docs/protocol/agent_zh.md](../../docs/protocol/agent_zh.md).

## Targeted verification

```sh
cargo test --locked -p dmsg_directory -p dmsg_protocol -p dmsg_user --lib
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_directory
cargo test --locked -p dmsg_integration --features pocketic-tests --test directory
cargo test --locked -p dmsg_integration --features pocketic-tests --test directory directory_cost_and_rebuild_profile -- --ignored --exact --nocapture
```

The standalone PocketIC fixture covers publication authorization/idempotency, atomic rejection, size limits and safety changes, certified paths and body tampering, governance-added homes and domains with their validators, permanent configuration and upgrade recovery. The explicit profile measures replicated-query cycles (without an HTTP certificate) and a 130-record upgrade. `DMSG_WASM_DIR` can select an independently built baseline.

Local PocketIC 16.0.0 samples on 2026-10-02, using the same inputs and release settings against the pre-review implementation and schema 2. They predate the stable certification map, so the upgrade rows describe the former heap rebuild:

| Measurement | Before | After |
| --- | ---: | ---: |
| Publication query, 291-byte document, cycles | 7,113,073 | 7,068,252 |
| HTTP replicated query, 291-byte document, cycles | 7,629,144 | 7,591,285 |
| Publication query, 35,360-byte document, cycles | 10,150,473 | 7,216,357 |
| HTTP replicated query, 35,360-byte document, cycles | 14,171,766 | 9,655,917 |
| Older-version publish, 16 controllers, cycles | 67,976,834 | 61,786,101 |
| Upgrade, 130 records, cycles | 3,843,188,472 | 3,261,319,160 |
| Wasm linear memory after that upgrade, bytes | 6,356,992 | 1,966,080 |

These are bounded local samples, not gateway latency, certified-query throughput or a maximum-capacity result. Both samples had 2,490,368 bytes of Wasm memory before the upgrade. Certification verification is tested separately with real non-replicated query certificates.
