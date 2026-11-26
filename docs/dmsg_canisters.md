# dMsg Canisters: Implementation and Verification

English | [简体中文](dmsg_canisters_zh.md)

This index covers public canister responsibilities, interfaces and upgrades. See the [documentation index](README.md) for the source-review baseline and [architecture](dmsg_architecture.md) for topology and capacity. Deployment state needs separate verification.

## Components

The table separates shared libraries, control services and wallet services. Clients and the private cloud are outside the canister control plane.

| Package | Responsibilities | Documentation |
| --- | --- | --- |
| dmsg_types | Public data contracts, including base signatures, ICP interfaces, and optional application protocols | [README](../src/dmsg_types/README.md) |
| dmsg_protocol | Deterministic encoding, approval construction, standard COSE verification, and CTT digests | [README](../src/dmsg_protocol/README.md) |
| dmsg_runtime | Shared stable records, certified trees, ledger helpers, and call utilities for reference implementations | [README](../src/dmsg_runtime/README.md) |
| dmsg_auth | Passkey identities, hardware login renewal, local/all-login logout | [README](../src/dmsg_auth/README.md) / [Candid](../src/dmsg_auth/dmsg_auth.did) |
| dmsg_user | Identities, authentications, devices, login recovery, device unlock secrets, root commitments, and device-signed attestations | [README](../src/dmsg_user/README.md) / [Candid](../src/dmsg_user/dmsg_user.did) |
| dmsg_cose | Content-root vetKD public key and recovery derivations | [README](../src/dmsg_cose/README.md) / [Candid](../src/dmsg_cose/dmsg_cose.did) |
| dmsg_handle | Name ownership, imports, pricing, and transfers | [README](../src/dmsg_handle/README.md) / [Candid](../src/dmsg_handle/dmsg_handle.did) |
| membership | PANDA SNS qualification, cross-product occupancy, and durable entitlement decisions | [README](../src/membership/README.md) / [Candid](../src/membership/membership.did) |
| dmsg_commerce | Plans, cash orders, refunds, fixed-duration grants and certified resource entitlements | [README](../src/dmsg_commerce/README.md) / [Candid](../src/dmsg_commerce/dmsg_commerce.did) |
| dmsg_payment | Fixed-term escrow, deposit verification, mutually exclusive fund decisions, and disbursements | [README](../src/dmsg_payment/README.md) / [Candid](../src/dmsg_payment/dmsg_payment.did) |
| dmsg_directory | Certified HTTP publication of Agent Delegation principal documents | [README](../src/dmsg_directory/README.md) / [Candid](../src/dmsg_directory/dmsg_directory.did) |
| dmsg_wallet_hub | Multisig wallet activation, creation, version registry, upgrades, locks and member index | [README](../src/dmsg_wallet_hub/README.md) / [Candid](../src/dmsg_wallet_hub/dmsg_wallet_hub.did) |
| dmsg_wallet | A multisig wallet: members, policy, proposals, ICP/ICRC transfers, chain-key signing and audit | [README](../src/dmsg_wallet/README.md) / [Candid](../src/dmsg_wallet/dmsg_wallet.did) |

## Current contracts

- **Accounts and devices:** `dmsg_user` issues 12-byte Xid accounts, controls login bindings, device capabilities, content-root commitments and delayed recovery. Clients discover user homes through `dmsg_handle` and query them in parallel. Cross-home login Principal uniqueness is intentionally not enforced.
- **Content and recovery:** devices generate content roots and wrap them for authorized devices. On-chain recovery is disabled by default for new accounts; recovery envelopes are added only when enabled. `dmsg_cose` performs vetKD only for recovery after all devices are lost; it does not sign ordinary documents or Agent Delegation events.
- **Signatures:** devices sign COSE statements and application actions locally. `attest` and `attest_app_action` verify the signature and device approval, charge the execution allowance and publish a certified receipt. A bare public key is not proof of account authority. See [statements and receipts](protocol/README.md).
- **Public identity:** user homes store the official PrincipalDocument display fields and controller records; `dmsg_directory` publishes certified JSON. Display edits write on chain; public avatar bytes remain in cloud storage. Agent controllers are self-held Ed25519 keys in the client vault. See [Agent Delegation](protocol/agent.md).
- **Names:** the registry owns canonical names, preserves optional display casing and handles registration, legacy claims and account-approved transfers. Registration fees go directly to the configured revenue account.
- **Payments and subscriptions:** `dmsg_payment` settles paid-letter escrows; `dmsg_commerce` handles commerce-2 cash checkout, resource entitlements, automatic settlement, admitted Apple/Google Play facts and authorized fixed-duration grants; shared `membership` handles membership-2 PANDA qualification and cross-product commitments. See [commerce](protocol/commerce.md). These are separate from everyday encrypted delivery.
- **Multisig wallets:** the hub activates instances and manages registered code versions; each wallet controls its own proposals, approvals, funds, chain-key signatures and audit. Upgrades require the wallet's prior acceptance. Desktop creation is currently paused; existing-wallet operations remain available. See [wallet protocol](protocol/wallet.md) and [desktop scope](dMsg_wallet.md).

Ordinary messages, files, vault edits and channel control do not add per-item ICP updates. Desktop, CLI and MCP use the Rust daemon; iOS and Android directly reuse the core through UniFFI ([mobile](protocol/mobile_zh.md), [Android](protocol/android_zh.md)). Web applications use [dmsg-connect/1](protocol/connect-v1.md). Cloud interoperability is described by [cloud.md](protocol/cloud.md), not by private implementation details.

## Persistence and upgrades

Public Candid/CBOR and private stable encodings are distinct. `dmsg_runtime::stable_types` and each canister's `stable_codec.rs` use compact integer-key maps without changing signed bytes or certified values. Current schema constants and supported predecessors live in each canister's storage and upgrade code; a version number alone does not establish upgrade compatibility.

**Existing production state must survive upgrades.** Current user schema 18 supports in-place upgrades from 14–17. Legacy accounts without an explicit recovery switch retain enabled recovery and schema 4 security leaves; new accounts and explicit policies use schema 5 leaves. The first directory 6 → 7 conversion builds its document tree; handle 10 → 11 builds its owner index; wallet 1 → 2 preserves member bindings. Unknown layouts are rejected. Reinstallation is not migration.

Regression entry points: [principal](../scripts/test-dmsg-principal-upgrade.sh), [hardware approval](../scripts/test-dmsg-hardware-upgrade.sh), [device succession](../scripts/test-dmsg-succession-upgrade.sh), [wallet](../scripts/test-dmsg-wallet-upgrade.sh). Check each script's old-module baseline separately from the current stable version; a passing test does not establish direct upgrades from every historical version.

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
