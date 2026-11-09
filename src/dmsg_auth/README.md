# dmsg_auth

First-party Passkey identities for dMsg. One global instance issues fixed **12-hour** IC delegations to independent P-256 hardware device keys. A remembered device can renew on demand without another Passkey prompt; grants have no scheduled expiration. Account bindings, device approval, content keys and recovery remain in the existing account services.

The complete interface is [dmsg_auth.did](dmsg_auth.did), with Rust types in [auth.rs](../dmsg_types/src/auth.rs). Browser/native transport and deployment steps are described in [the public login contract](../../docs/protocol/passkey-v1_zh.md).

## State and limits

- Stable memory IDs 0/1/2/3 hold configuration, credential-ID-hash identities, device-principal sessions and the registration-caller index. Records use compact integer-key CBOR. A persistent random salt determines identity seeds; upgrading preserves it and the canister ID.
- Each identity has one ES256 Passkey and at most 16 sessions. Reaching that limit refuses a new session with `SessionLimitExceeded`; rate limits and other capacity limits return `QuotaExceeded`. A fresh Passkey assertion can end all sessions. Clients offer this action for the session limit only. There is no automatic eviction or session inventory API.
- Challenges expire after five minutes, one per caller. They bind the operation and the P-256 public key. Successful authentication consumes the challenge. Ending all sessions also invalidates challenges prepared before that operation for this identity, including devices that had not yet logged in.
- Heap challenge indexes and the standard canister-signature witness table are transient. Upgrades do not enumerate identities or sessions; an unfinished authentication must start again. A client already holding a delegation can use it until expiration.
- Installation sets identity, daily registration, interactive request, renewal and pending-challenge budgets. Each session may renew at most once per minute. Interactive budgets include both challenge preparation and completion. `admin_set_limits` and `admin_set_admission_key` are controller/governance operations with matching validation queries.
- Registering can require the existing `AdmissionTicket`, bound to this canister and the hardware caller. Each hardware caller may register only one identity, including after logout, so its admission ticket cannot create more identities. This index is bounded by the identity count. Neither a ticket nor a login grants account/device permissions.

`renew` has no inter-canister calls. It looks up the authenticated caller, checks the interval/global renewal budget, updates that stable record and publishes a canister signature. `get_delegation` returns its certified witness. The short-lived witness must be fetched promptly; its retention is independent of the signed 12-hour delegation expiration.

## Validation

```sh
cargo test --locked -p dmsg_auth --lib
cargo test --locked -p dmsg_protocol --lib passkey
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_auth
candid-extractor target/wasm32-unknown-unknown/release/dmsg_auth.wasm
cargo test --locked -p dmsg_integration --features pocketic-tests --test passkey -- --nocapture
```

The PocketIC test measures complete renewal update cycles and stable pages, checks actual P-256 ingress and nested delegations, upgrades the Wasm and renews after 180 simulated days. These measurements are local fixtures, not mainnet throughput or deployment capacity claims. Apple hardware/Passkey origin checks and production rollout require separate device evidence. Current results are recorded in [validation](../../docs/passkey-validation_zh.md).

The 2026-11-09 fixture measured 7,213,730 cycles for a complete renewal update and 513 stable pages (including allocator granularity) after one identity and sixteen sessions. This is not a scale or throughput estimate; the exact Wasm hash and remaining gates are in the linked validation record.
