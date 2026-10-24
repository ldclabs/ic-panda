# dMsg Canisters: Implementation and Verification

English | [简体中文](dmsg_canisters_zh.md)

Reviewed against public source `9fd70dca` on 2026-10-24. This is a guide to the current implementation, not a deployment inventory or an audit report. Architecture and measured capacity are in [dmsg_architecture.md](dmsg_architecture.md); exact interfaces and bytes are in the canister Candid files and [public protocols](protocol/README.md).

## Components

The control plane has seven service roles, plus a team-wallet factory and per-team wallet instances. The client, bridge page and private cloud have separate responsibilities.

| Package | Responsibilities | Documentation |
| --- | --- | --- |
| dmsg_types | Public data contracts, including base signatures, ICP interfaces, and optional application protocols | [README](../src/dmsg_types/README.md) |
| dmsg_protocol | Deterministic encoding, approval construction, standard COSE verification, and CTT digests | [README](../src/dmsg_protocol/README.md) |
| dmsg_runtime | Shared stable records, certified trees, ledger helpers, and call utilities for reference implementations | [README](../src/dmsg_runtime/README.md) |
| dmsg_user | Identities, authentications, devices, login recovery, device unlock secrets, root commitments, and device-signed attestations | [README](../src/dmsg_user/README.md) / [Candid](../src/dmsg_user/dmsg_user.did) |
| dmsg_cose | Content-root vetKD public key and recovery derivations | [README](../src/dmsg_cose/README.md) / [Candid](../src/dmsg_cose/dmsg_cose.did) |
| dmsg_handle | Name ownership, imports, pricing, and transfers | [README](../src/dmsg_handle/README.md) / [Candid](../src/dmsg_handle/dmsg_handle.did) |
| membership | PANDA SNS qualification, cross-product occupancy, and durable entitlement decisions | [README](../src/membership/README.md) / [Candid](../src/membership/membership.did) |
| dmsg_commerce | Tier plans, cash orders, refunds, and certified resource entitlements | [README](../src/dmsg_commerce/README.md) / [Candid](../src/dmsg_commerce/dmsg_commerce.did) |
| dmsg_payment | Fixed-term escrow, deposit verification, mutually exclusive fund decisions, and disbursements | [README](../src/dmsg_payment/README.md) / [Candid](../src/dmsg_payment/dmsg_payment.did) |
| dmsg_directory | Certified HTTP publication of Agent Delegation principal documents | [README](../src/dmsg_directory/README.md) / [Candid](../src/dmsg_directory/dmsg_directory.did) |
| dmsg_wallet_hub | Team wallet activation, creation, version registry, upgrades, locks and member index | [README](../src/dmsg_wallet_hub/README.md) / [Candid](../src/dmsg_wallet_hub/dmsg_wallet_hub.did) |
| dmsg_wallet | A team wallet: members, policy, proposals, ICP/ICRC transfers, chain-key signing and audit | [README](../src/dmsg_wallet/README.md) / [Candid](../src/dmsg_wallet/dmsg_wallet.did) |

## Current contracts

- **Accounts and devices:** `dmsg_user` issues 12-byte Xid accounts, controls login bindings, device capabilities, content-root commitments and delayed recovery. Clients discover user homes through `dmsg_handle` and query them in parallel. Cross-home login Principal uniqueness is intentionally not enforced.
- **Content and recovery:** devices generate content roots and wrap them for authorized devices and recovery. `dmsg_cose` performs vetKD only for recovery after all devices are lost; it does not sign ordinary documents or Agent Delegation events.
- **Signatures:** devices sign COSE statements and application actions locally. `attest` and `attest_app_action` verify the signature and device approval, charge the execution allowance and publish a certified receipt. A bare public key is not proof of account authority. See [statements and receipts](protocol/README.md).
- **Public identity:** user homes store the official PrincipalDocument display fields and controller records; `dmsg_directory` publishes certified JSON. Display edits write on chain; public avatar bytes remain in cloud storage. Agent controllers are self-held Ed25519 keys in the client vault. See [Agent Delegation](protocol/agent.md).
- **Names:** the registry owns canonical names, preserves optional display casing and handles registration, legacy claims and account-approved transfers. Registration fees go directly to the configured revenue account.
- **Payments and subscriptions:** `dmsg_payment` settles paid-letter escrows; `dmsg_commerce` handles commerce-2 cash checkout, resource entitlements and automatic settlement; shared `membership` handles membership-2 PANDA qualification and cross-product commitments. See [commerce](protocol/commerce.md). These are separate from everyday encrypted delivery.
- **Team wallets:** the hub activates instances and manages registered code versions; each wallet controls its own proposals, approvals, funds, chain-key signatures and audit. Upgrades require the wallet's prior acceptance. Desktop creation is currently paused; existing-wallet operations remain available. See [wallet protocol](protocol/wallet.md) and [desktop scope](dMsg_wallet.md).

Ordinary messages, files, vault edits and channel control do not add per-item ICP updates. The desktop app, CLI and MCP use the Rust daemon; web applications use [dmsg-connect/1](protocol/connect-v1.md). Cloud interoperability is described by [cloud.md](protocol/cloud.md), not by private implementation details.

## Persistence and upgrades

Public Candid/CBOR and private stable encodings are distinct. `dmsg_runtime::stable_types` and each canister's `stable_codec.rs` use compact integer-key maps without changing signed bytes or certified values. Current schema constants and supported predecessors live in each canister's storage and upgrade code; a version number alone does not establish upgrade compatibility.

**`dmsg_user` and `dmsg_directory` already hold production user data.** Layout changes must preserve it through an in-place upgrade and an old-Wasm-to-new-Wasm test. The implemented user 14 → 15 and directory 6 → 7 upgrades preserve existing data; directory's first conversion builds the additional document certification tree. Reinstallation is not a migration. The [principal upgrade check](../scripts/test-dmsg-principal-upgrade.sh) exercises these transitions and the associated handle changes.

For other canisters, follow their documented supported layouts and recovery procedures. Never use a reinstall to repair an instance containing accounts, escrow, accounting or irreplaceable records. Unknown cross-canister outcomes retain their original request and ledger parameters for reconciliation. Canister configuration, module hashes, controllers and operational readiness must be checked against the target deployment separately.

## Build and verification

Use the repository's pinned dependencies and the PocketIC setup in the [integration guide](../tests/dmsg_integration/README.md):

```sh
rustup target add wasm32-unknown-unknown
make build-dmsg
POCKET_IC_BIN=/path/to/pocket-ic make test-dmsg
python3 scripts/generate-integration-sdk.py --check
```

The test target covers Rust tests, Clippy, Wasm builds, Candid consistency, independent protocol vectors and PocketIC calls. Client and desktop checks, platform credential storage, native mounts and real application integration have their own test paths. This documentation update did not rerun those suites.

Each component README records its measurements and test boundaries. Historical test counts and Wasm sizes are not current acceptance criteria. Capacity limits are guards; only the explicitly measured workloads support capacity claims. Synthetic ledgers, CMC doubles, local relays and browser fixtures do not establish real-fund, mainnet, signed-distribution or independent audit acceptance.
