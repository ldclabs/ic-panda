# dMsg documentation

English | [简体中文](README_zh.md)

dMsg is an ICPanda DAO product for private collaboration between people and agents. Start with the [product overview and downloads](../README.md#-dmsg--private-collaboration), then choose the implementation or protocol guide below.

This index was reviewed against public source `9fd70dca` on 2026-10-24. Documents describe source behavior unless they identify a specific release, deployment or measurement. Source changes, installable releases, production deployment and independent audits are separate facts. Internal design documents are located through [AGENTS.md](../AGENTS.md); their contents are not published here.

## Use and build

| Topic | Guide |
| --- | --- |
| Install and update | [Downloads](https://github.com/ldclabs/ic-panda/releases/latest) · [Release scripts](../scripts/dmsg/README.md) |
| Desktop workspace | [Desktop app](../src/dmsg_desktop/README.md) · [Current design system](../src/dmsg_desktop/DESIGN.md) |
| Credentials and file references | [Desktop IDs, references, developer workflows and verification (简体中文)](dmsg_secrets_tutorial_zh.md) |
| Command line and MCP | [CLI and daemon](../src/dmsg_cli/README.md) · [Agent connector](../src/dmsg_agent/README.md) |
| Reusable Rust core | [dmsg_client](../src/dmsg_client/README.md) |
| Web and application integration | [SDK](../packages/dmsg-sdk/README.md) · [Account adapter example](../examples/dmsg-account-product/README.md) · [Bridge page](../src/dmsg_connect/README.md) |
| Local verification | [Integration tests](../tests/dmsg_integration/README.md) · [Client vectors](../tests/vectors/client/README.md) · [Agent vectors](../tests/vectors/agent-protocols/README.md) |

## Architecture and contracts

| Topic | Maintained documentation |
| --- | --- |
| Architecture, capacity and multiple instances | [English](dmsg_architecture.md) · [简体中文](dmsg_architecture_zh.md) |
| Canister roles, upgrades and validation | [English](dmsg_canisters.md) · [简体中文](dmsg_canisters_zh.md) |
| Protocol index, signatures and receipts | [English](protocol/README.md) · [简体中文](protocol/README_zh.md) |
| Encryption and recovery boundaries | [Encryption](dmsg_encryption.md) · [Accounts and roots](protocol/account_root.md) |
| Approved credential use and secret sharing | [Secrets contract and verification](protocol/secrets.md) |
| Team wallets | [Product scope](dMsg_wallet.md) · [Public contract](protocol/wallet.md) · [Desktop implementation](../src/dmsg_desktop/README.md) |
| Legacy data and migration | [Read-only inventory](legacy_inventory.md) · [Personal archives](protocol/legacy_archive.md) · [Freeze evidence](protocol/legacy_freeze.md) |

The [protocol index](protocol/README.md#protocol-map) also covers cloud delivery, channels, browser login, dmsg-connect, commerce 2, Agent Delegation and Mail events. Each canister README owns its detailed limits, measurements and operations; Candid and shared vectors define the executable interface.

## Current boundaries

- The main clients are the desktop app and CLI, sharing the Rust daemon. Web applications use dmsg-connect/1; the former browser extension is retired. There is no mobile app yet.
- `dmsg_user` and `dmsg_directory` already contain production user data. Stable-layout changes require data-preserving upgrades and old-Wasm verification, never reinstallation.
- Team-wallet creation is paused in the desktop app. Existing wallets remain accessible; this does not change the CLI, daemon or hub's creation interfaces. Native secret-file mounts have separate OS, packaging and validation requirements.
- Legacy personal archives remain readable and importable. The old shared-channel inheritance workflow was removed from the public clients; the current channel protocol and legacy evidence formats still serve different purposes.
- dMsg has not been independently audited. Recorded local tests and capacity samples have explicit scopes and are not blanket production acceptance.

## Historical records

Completed design iterations and one-time investigations live in `archived/docs/`:

- Statement v3 rationale: [English](../archived/docs/statement-v3-design.md) · [简体中文](../archived/docs/statement-v3-design_zh.md). Use the current protocol and CDDL for implementation.
- [Desktop design and implementation record](../archived/docs/dmsg_desktop_design_zh.md). Current component rules live beside the desktop source.
- [2026-10-22 unlock/performance investigation](../archived/docs/dmsg_desktop_performance_zh.md). Its samples and unrun checks retain their original scope.

Superseded daily canister increments are retained in Git history. New behavior belongs in its current topic section, with versioned evidence where needed; do not append a second, conflicting contract. Keep English/Chinese architecture section numbers aligned, update incoming links when moving a document, and regenerate `protocol/integration.cddl` from its script rather than editing it manually.

Every Markdown topic in this directory has an English `name.md` and Simplified Chinese `name_zh.md` counterpart with matching sections. Follow the language switch at the top of each page. CDDL files are shared machine-readable contracts; both languages link to the same definitions.
