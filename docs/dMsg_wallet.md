# dMsg Wallet product specification v0.2 · Team wallets

English | [简体中文](dMsg_wallet_zh.md)

First version: 2026-10-16; revised: 2026-10-24. This document consolidates implementation and security revisions against public repository `b8b8b799`, describing current behavior and listing future capabilities separately. Product documentation is now unified here; protocols and component READMEs own exact interfaces and implementation boundaries. Source implementation is distinct from production deployment or desktop release.

Desktop flows also incorporate the 2026-10-24 UX01–UX20 revisions; components, test entry points and platform boundaries are in the [desktop README](../src/dmsg_desktop/README.md).

Companions: [public contract](protocol/wallet.md), [wallet implementation](../src/dmsg_wallet/README.md), [factory implementation](../src/dmsg_wallet_hub/README.md), [desktop README](../src/dmsg_desktop/README.md).

## 0. Baseline and scope

| Area | Reviewed state |
| --- | --- |
| Public code | Local `ic-panda` `main`, `b8b8b799` (2026-10-24); clean workspace at the start of that revision; remote not checked |
| Private design | Located through [AGENTS.md](../AGENTS.md): PRD v1.1, implementation v1.1, commercialization/implementation v1.0 and shared-membership recommendation; local `dmsg-cloud` was `801940d4` (2026-10-23), clean, remote not checked. Private design text is not reproduced here |
| Wallet services | One `dmsg_wallet` instance per team, with `dmsg_wallet_hub` as factory; reserved ID `ghcje-dyaaa-aaaas-qg7va-cai` is used by the hub |
| Clients | Rust core, daemon, CLI, MCP and Electron team-wallet module are implemented; desktop creation is paused, while existing wallets and historical activation jobs remain usable |
| Verification and release | Unit, PocketIC, dual-daemon, Electron and some local external-chain checks exist; these do not establish production deployment, real-hardware acceptance or completion of every external-chain check |

This document focuses on flows, authority and product boundaries. Public Candid, types and protocols govern exact fields/methods. Early research on incident losses, competitors, network throughput and dollar costs is not the current implementation baseline, and commercial recommendations are not configured entitlements.

## 1. Product definition

**dMsg Wallet is a multichain team wallet jointly controlled by on-chain policy and members.** Each team owns an ICP canister storing membership, policy, proposals and audit records. It spends ICP/ICRC assets directly through ledgers; for BTC, EVM and Solana it constructs transactions from approved intent and signs with ICP chain keys, then member clients broadcast them.

Chain-key addresses do not require a team seed phrase. Account devices, local wallet identities and optional cosigner keys still have storage/recovery duties. No seed phrase does not mean no keys or independence from ICP, wallet code and governance.

It serves managers of team operating funds, bringing multichain receipts, roles, multiple approvals, delays, cosigning and audit into one flow. Funds requiring SNS proposals remain in the SNS treasury.

## 2. Current capabilities

| Capability | Implementation | Boundary |
| --- | --- | --- |
| Factory | Quotes, activation payments, creation/resume, aborting empty instances, refunds, revenue collection, versions and member index | Quotas/prices come from hub configuration; paused desktop creation does not pause the hub |
| Policy and proposals | Member thresholds, cumulative per-asset tiers, delays/veto, address-book cooldown, freeze, Recovery and audit | Exceeding `daily_limits` requires every active approver; it is not an absolute spending prohibition |
| ICP / ICRC | Single and sequential batch transfers, allowlisted canister calls, cycles top-ups and unknown-transfer reconciliation | Batches are not atomically rolled back; generic calls are not arbitrary dapp signing |
| Bitcoin | P2TR, P2WPKH, same-input replacement, Taproot dual control and timelock paths | Bitcoin regtest broadcast acceptance and hardware-wallet integration remain incomplete |
| EVM | EIP-1559 native/ERC-20 transfers, same-nonce replacement, cancellation and release of pending nonces | No generic calldata, EVM batches or Safe dual control |
| Solana | SOL, SPL, optional ATA creation, batching and explicit durable-nonce initialization | Default addresses have no durable nonce; ordinary signed transactions can only rebroadcast original bytes |
| Device cosigning | Canister Raw/FIDO2/WebAuthn verification; client Secure Enclave P-256 on macOS and iOS, and software P-256; NC device soft keys sign Taproot L1; collection for the same plan across devices, with packets exchanged between the desktop and iOS | TPM, CTAP2, bridge WebAuthn and chain hardware-wallet adapters are not implemented; on-chain regtest acceptance of NC soft keys is pending |
| Desktop and collaboration | Four main workspace areas, separate approvals, certified reads, receive checks, audit, encrypted channel hints and RPC configuration | Channels only notify/navigate; public transparency pages and on-chain `attest_*` are not implemented |

## 3. Product principles

1. **One instance per team.** Addresses, members, policy, audit and cycles are independent; each instance may accept upgrades or lock code.
2. **Approve intent before execution.** Device approval binds wallet, operation, policy version and decision. Clients read complete certified proposals and independently decode external-chain plans.
3. **Member votes and device cosigning are separate.** One account on two devices remains one member with at most one vote; independent cosigner keys may add a k-of-l threshold.
4. **Recover through original records.** Retry broadcasting original bytes first; reconcile unknown ledger payments with fixed arguments or actual blocks, never by creating a replacement payment.
5. **Existing funds do not depend on subscription renewal.** Running wallets do not check the creator's plan or disable addresses, cosigning, approvals or withdrawals by plan; team policy, cycles and network availability still apply.
6. **State each trust boundary.** Certified reads establish source, independent decoding checks transactions and audit records behavior. None proves external-chain settlement or replaces members' review of complete content.

## 4. Members, roles and identities

Members may be dMsg accounts or direct-calling Principals. The creator is an active Admin; new members start as `Invited` and must join before acting.

| Role | Proposals | Approval count | Veto / freeze |
| --- | --- | --- | --- |
| Admin | All intents | Administration and spending/operations | Yes |
| Approver | All intents | Spending/operations, not administration | Yes |
| Proposer | All intents | Does not count | No |
| Viewer | Cannot propose | Does not count | No |
| Recovery | Add, remove or update members only | Does not count; its own recovery proposals require no approval and wait 7 days | No; Admin/Approver may veto during the delay |

Each account-member device has a locally encrypted Ed25519 wallet key; `WalletJoin` device approval binds its Principal, one per device and at most 4 devices per member. A device joining may take over the bindings of devices it names, or of all others after a login recovery; an account approval also unbinds lost devices, while the member stays active. Login identity submits device approval at the user home; wallet identity calls dynamic wallet instances. They serve separate purposes.

Creation, joining and approve/reject/veto go through the approval center and require `WalletApprove` on account devices. Principal members decide as the caller directly. Proposals do not include the proposer's approval automatically; voting is separate. Non-Viewer members may execute approved operations, but execution grants no new approval authority.

Member additions/removals/updates and first joins advance the authorization version, making old unexecuted proposals `Stale`. Binding, replacing or unbinding a device of an active member does not advance it again. Account-device revocation is distinct from wallet membership: a revoked device keeps its wallet binding until it is unbound, and recorded votes do not disappear merely because the home's security epoch changes. Freeze, unbinding or membership changes must handle that situation.

## 5. Addresses, cosigners and policy

### 5.1 Addresses and assets

Default address IDs are unique across a wallet: 0 ICP, 1 BTC P2TR, 2 EVM, 3 Solana; new addresses start at 4, up to 256 total. EVM chains share derived keys/addresses and are distinguished by chain ID. ICP addresses are subaccounts under the wallet Principal.

Assets are identified by exact ledgers, EVM chain IDs/contracts and Solana mints, not names. Transfers support ICP, ICRC-1, BTC, EVM native/ERC-20 and SOL/SPL, subject to policy. Desktop amount conversion uses exact integers and explicitly requests atomic units when precision is unknown. Displaying ICP AccountIdentifier adds a receiving representation without changing the account model.

### 5.2 Three control modes

| Mode | Execution requirements | Protection boundary |
| --- | --- | --- |
| CK chain key | Member approvals/policy; ICP chain-key signatures for external chains | Depends on ICP, wallet code and controller governance while unlocked |
| CS cosigning | Additional key threshold over exact plan digests; ICP operations bind proposal digest | Enforced by wallet code; Admin threshold plus 24 hours can change/remove ordinary address cosigning, and malicious upgrades may bypass it |
| NC on-chain dual control | Currently Bitcoin Taproot only: L1 requires chain key plus BIP-340 cosigner | L2 permits chain-key-only spending after the UTXO's configured block age; optional L3 uses a recovery key after another timelock. Not every path is permanently 2-of-2 |

Ordinary CS key generation supports Secure Enclave P-256 on macOS and iOS, and software P-256 encrypted under local LDK. A hardware request fails when hardware is unavailable and never silently downgrades: the desktop then asks whether to make a software key, the CLI makes one only with `--software`, and iOS makes Secure Enclave keys only. The canister's `hardware` flag is a registration claim, not remote attestation. A non-exportable private key is not a trusted transaction display.

The two clients differ: the macOS Secure Enclave signs at most 32 digests per presence check and iOS at most 64; turning off the iPhone passcode invalidates its Secure Enclave keys, so the wallet needs another cosigner to stay usable.

NC cosigner keys are BIP-340 Taproot keys, which the Secure Enclave cannot hold, so a device soft key holds them: each device has a random root sealed under one or more dedicated Secure Enclave keys (protectors). Creating it asks nothing; opening it needs the person. A device without such hardware keeps the root in software under its local key only when the person explicitly chooses that. Each wallet's key derives from the root and registers as a `Bip340` cosigner with `hardware: false`, so wallets that require hardware cosigners refuse it. Before signing an L1 spend the client's own decoder confirms the plan takes L1 with this device's registered key in the leaf; the person then opens the root, and the client signs each input's script-path sighash directly with fresh auxiliary randomness, refusing when the derived key differs from the registered one. Packets with native signatures are version 2, which older clients refuse; packets with ordinary signatures stay version 1.

The L1 leaf commits to one device's key, so a new device migrates an NC address: it registers its soft key (`AddCosigner`), adds a Taproot address with the old address's chain-key delay and recovery key (`AddAddress`), adds the new address to the address book and waits out its cooldown when the asset's tier is book-only, then sweeps the old address's UTXOs to it in one transfer without change: through L1 while the old device can still sign, otherwise through L2, spending only UTXOs at least `chain_key_after` blocks deep and showing each one's earliest spendable height, possibly in batches. Every step is a proposal the team approves as usual; once swept, the new device marks the old address archived locally.

Devices must cosign the same plan. Package import only collects signatures, never executes; each cosigner must cover every digest, and repeated import adds no count. Before importing and executing, clients check collected signatures against the cosigners' current keys; those made with a key that has since changed are set aside and their cosigners asked to sign again. A cosigner key changes in place: its member rotates it with the old key at once, or an Admin proposal replaces it after 24 hours when the old key is lost, also while frozen; signatures collected with the old key must be made again. Adjust cosigner policy before deleting a referenced key/member. A Taproot-committed key does not disappear when its member is removed.

### 5.3 Policy semantics

| Rule | Behavior |
| --- | --- |
| Member thresholds | Admin and Approver count for spending/operations; only Admin for administration; thresholds are independent |
| Tier amount | Sum per asset: valid approved-but-unexecuted reservations + amounts whose execution/signing started in the past 24 hours + this proposal. Calculated on submission/approval; requirements can only increase during approval |
| Reservation | Persists after threshold is met even beyond one day; cancellation, invalidation or definite no-spend releases it. Failed signing without a signed transaction restores the reservation |
| Daily amount | Exceeding `daily_limits` requires all active approvers and the asset's longest tier delay; not a hard limit |
| Address book | `book_only` accepts only entries past cooldown; no automatic lower tier for address-book destinations |
| Delay/veto | Delay starts at threshold; any Admin/Approver can veto during it. Veto terminates the proposal and requires a new proposal |
| Freeze | Admin/Approver may freeze directly. Only unfreeze and membership changes proceed; batches/signing stop before the next step. Signed transactions cannot be recalled |
| Automatic refuel | Configured `refuel()` is independently preauthorized maintenance, unaffected by freeze; balance, per-call ICP and daily-count limits apply |
| Version/deadline | First execution rechecks current votes, authorization version, expiry and freeze; Recovery must still be an active recovery member |
| Execution budget | `daily_signatures` bounds threshold signatures per UTC day, counted by digest before signing; it is not a ceiling on all wallet cycles consumption |

`SetPolicy`, `RemoveMember`, `SetCosign`, `RemoveCosigner`, `ReplaceCosigner`, `AcceptUpgrade`, `LockCode` and `ReleaseEvmNonce` require Admin threshold plus 24 hours; unfreeze delay follows policy. Adding addresses, cosigners or members and updating members require Admin threshold without a fixed delay. Recovery membership changes always wait 7 days.

## 6. User flows

### 6.1 Creation and activation payments

**Desktop creation is currently paused.** The [shared switch](../src/dmsg_desktop/src/shared/wallet-availability.ts) guards both controls and main-process RPC, blocking creation, quotes and quote previews. Existing wallets, joins, activation resumption/abort and refunds remain available. CLI/daemon/hub creation still exists, with a separate governance pause on the hub.

Available creation routes check device approval eligibility and payer balance, obtain a 15-minute quote, present name/members/policy/activation fee, verify user-home approval before payment, pay the quoted subaccount, then let the hub check the balance and create/install the wallet. The hub pays creation and initial cycles from its own pool, later replenished from activation revenue.

Paid jobs may continue their original activation. The quote caller may abort `Paid/Created` jobs with no installed code, deleting empty instances and releasing quota before refund; installed instances cannot use that refund path. Revenue collection takes only the quoted amount. Excess, late and refundable failed/expired payments are claimed separately, less ledger fees. Clients retain paid quotes to detect late funds and hide them when no balance is displayable.

Invitees confirm their own join. A notification channel may reference an existing channel; `channel_hint` is an initial hint overridden per local account. No channel is created automatically.

### 6.2 Everyday spending

1. Select source address, exact asset, full recipient, amount and maximum fee. Preview shows balance, fees and policy requirements without reserving funds.
2. In a separate approval window, members review the certified proposal, current wallet state and decision. Administrative operations show their changes; unknown/incomplete intents cannot be approved.
3. Execute after threshold and delay. The wallet sends ICP/ICRC ledger calls directly. External chains prepare a plan, decode it independently in the client, collect any cosignatures, obtain wallet signatures and broadcast from the client.
4. Inspect the actual ledger block or external-chain record. External-chain `Settled/Failed` is a member's RPC-based report; the wallet does not independently verify settlement. UI, channels and audit cannot turn it into an on-chain proof.

### 6.3 Execution failures

| Situation | Recovery and boundary |
| --- | --- |
| External broadcast failure or pending confirmation | Rebroadcast original bytes first; BTC replacements retain inputs, EVM replacements retain nonce, and fees remain within the original proposal ceiling |
| Ordinary signed Solana transaction | No new blockhash/re-signing, including after a fixed wait. Later durable-nonce initialization does not change old-proposal rules |
| Replaceable Solana transactions | Explicitly initialize a durable nonce in assets/addresses; later replacements preserve the first signature's nonce mode and value |
| Pending EVM nonce | Create cancellation/release proposals binding the exact original proposal and nonce. Release does not invalidate the signed transaction; cancellation reports do not erase its signed status or spending count |
| Unknown ICP/ICRC outcome | Retry original parameters; `TooOld` still leaves an unknown job. Supply an actual block and batch index for full transfer matching; a declaration of nonpayment cannot authorize a new transfer |
| Partially successful batch | Preserve completed blocks and continue remaining items. An unsent item's first call returning TooOld is a definite failure; completed transfers are not undone |

## 7. Desktop, agents and notifications

The desktop has overview, assets/addresses, transactions/proposals and team/security areas. History/audit belong under transactions and operations under team/security. It supports pagination, ID lookup, exact amounts, receive QR codes, cosignature packages, periodic refresh, resource status and RPC diagnostics. Account/wallet switches discard stale responses; network/hardware waits do not hold the global daemon Client lock.

MCP tools are `dmsg_wallet_list`, `dmsg_wallet_balances`, `dmsg_wallet_propose_transfer`, `dmsg_wallet_proposal_status`, `dmsg_wallet_execute` and `dmsg_wallet_report_broadcast`, governed by `wallet.read`, `wallet.propose:<wallet>` and `wallet.execute:<wallet>`. Agent proposals additionally require the member to register that Agent ID. There is no approval tool; human cosigning goes through the approval center.

Channels carry encrypted `dmsg-wallet-event/1` hints. Recipients reread the wallet; messages and `dmsg://wallet/...` links only navigate. Notification failure does not roll back completed financial actions. Audit exports identify scope, head and verification time, never presenting one page as full history or external-chain reconciliation proof.

## 8. Pricing, plans and operating costs

| Item | Implemented rule | Configuration or recommendation |
| --- | --- | --- |
| Creation quota | Hub reads the plan through commerce `refresh_entitlement` and limits creation with `HubPricing.quotas`; unconfigured plans get 0 | Plus 1 / Pro 3 / Max 10 was an initial recommendation, not a hard-coded table; Free quotas may be configured |
| Activation fee | Fixed ICP e8s; governance configures initial cycles, the ICP share replenishing the hub and revenue account | The suggested roughly 10 USD starting value is not a peg or automatic exchange mechanism |
| Operating costs | Each wallet pays its own cycles; external fees use the respective chain's assets; refunds pay ledger fees | No fixed dollar signing cost or guaranteed operating lifetime; assess actual configuration/consumption |
| Advanced features | No runtime Pro/Max gate on CS, NC or batch transfers | Advanced-plan tiers and custom domains for transparency pages are not implemented |

The hub checks eligibility only at creation; plan expiry does not automatically remove members' permissions or prevent new addresses. Replenishing hub cycles and the new instance's initial cycles are separate configuration items, not one immediate exchange in UI or copy.

## 9. Seed wallet and governance

An ICPanda DAO team wallet may serve as the initial user for operating funds, using member thresholds, address book, delays and optional cosigning, with an optionally offline-kept Recovery member. Governance may direct hub revenue to it; other business revenue requires separate review of receiving configuration and collection authority.

This does not mean a seed wallet exists, revenue has moved or transparency pages are live. Funds requiring SNS governance remain in the SNS treasury; member approval is not SNS voting.

## 10. Security and recovery boundaries

| Boundary | Scope of the guarantee |
| --- | --- |
| Upgrades/controller | Current hub code requires acceptance of exact version/hash and serializes upgrade/lock, but the hub remains the sole controller of unlocked instances. Its governance can change code, so it remains a trust root |
| Code lock | After Admin threshold/delay, the hub clears controllers while Running. Normal upgrade repair is then lost; existing vulnerabilities remain and permanent operation is not guaranteed |
| CS/recovery | Authorized delayed administrator proposals may change ordinary cosigning; it supplements member approvals rather than providing a permanent lock independent of wallet governance |
| Taproot NC | Bitcoin script enforces L1 dual signatures, but L2 permits chain-key-only spending after delay; optional L3 requires the recovery-key holder to construct the transaction. Relative locks use UTXO age |
| Cycles | A 365-day `freezing_threshold` calculates freeze reserve, not guaranteed one-year retention after exhaustion. Preauthorized top-up still needs ICP, CMC and an external trigger |
| Signed transactions | Freeze, revocation, UI cancellation or nonce release cannot recall valid signatures already delivered |
| Certification/external chains | Certificates establish returned state and audit hashes establish consistency; external balances, fees, metadata and reports depend on RPC nodes, without canister-side chain verification |
| Client/hardware | Independent decoding rejects mismatched plans, but full recipients/intent still need review. Hardware claims, user presence and trusted display are separate capabilities |
| Privacy | `policy.public` governs ordinary reads; canister members, amounts and addresses are not end-to-end encrypted secrets, and on-chain transactions remain observable |

If all members are unavailable, a preconfigured Recovery member may change membership after 7 days; without an available recovery path, funds cannot be promised recoverable. This product has no complete fiat, exchange, automated-trading, generic WalletConnect or ICP dapp-signer implementation.

## 11. Verification and remaining work

Development evidence through `b8b8b799` includes wallet/protocol Rust unit tests, PocketIC wallet regressions, real local Solana-validator and Anvil broadcasts, dual-daemon flows, 92 desktop Node tests, type checks/build, Electron smoke, Wasm/Candid consistency and Clippy. That Electron run skipped native macOS fullscreen and used a CMC double. Details and rerun entry points are in the [integration guide](../tests/dmsg_integration/README.md) and [desktop README](../src/dmsg_desktop/README.md).

Separate remaining delivery work:

- Bitcoin regtest broadcasts and on-chain acceptance of Taproot recovery paths.
- Real-device Secure Enclave acceptance; Windows/Linux TPM, CTAP2, bridge WebAuthn and chain hardware-wallet adapters.
- EVM Safe, Solana Squads, public transparency pages and on-chain `attest_*` verification.
- Real CMC, SNS governance/production deployment, platform releases and reopening desktop creation.

These remain unfinished capabilities or release acceptance; desktop forms, protocol verification and offline vectors do not complete them.

## 12. Current disposition of initial decisions

| Decision | Current disposition |
| --- | --- |
| D-1 Naming | `dmsg_wallet` instances and `dmsg_wallet_hub` factory; payer identity uses `payer/*`, team wallets use `wallet/*` |
| D-2 Price/quota | Fixed ICP activation and configurable plan quotas implemented; recommendations are not production configuration |
| D-3 Control | Hub-only controller, instance acceptance of upgrades, optional controller removal to lock code; hub governance remains trusted |
| D-4 Chains | BTC P2TR/P2WPKH, allowlisted EVM chain IDs and SOL implemented; no default claim of deployment acceptance for any L2 |
| D-5 Amounts | Policy uses atomic asset units; USD display prices are not policy inputs |
| D-6 SNS/members | Members manage operating funds; SNS-governed funds stay in the treasury |
| D-7 Personal wallets | Policy allows one member; multiple devices add no votes. No implemented personal cold-storage template is claimed |
| D-8 Cosigners | Secure Enclave and locally encrypted software keys implemented; other hardware remains pending, without silent downgrade |
| D-9 External verification | Clients read/report; canister `attest_*` audit mode remains unimplemented |

## 13. Documentation responsibilities

- [Public contract](protocol/wallet.md): identities, digests, execution and financial recovery rules.
- [Wallet README](../src/dmsg_wallet/README.md) and [factory README](../src/dmsg_wallet_hub/README.md): public interfaces and service behavior.
- [Desktop README](../src/dmsg_desktop/README.md): client flows, implementation entry points and platform boundaries.
- [Canister implementation](dmsg_canisters.md) and [architecture](dmsg_architecture.md): component relationships and deployment structure.
- Private product design remains located and reviewed through [AGENTS.md](../AGENTS.md). This document neither replaces private-design version management nor presents future commercial plans as implemented interfaces.
