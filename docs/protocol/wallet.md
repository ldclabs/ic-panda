# dMsg team wallets: public dmsg-wallet/1 contract

English | [简体中文](wallet_zh.md)

This describes public interfaces, approval bytes, address derivation and external-chain transaction rules for `dmsg_wallet` (one instance per team) and its creation/upgrade factory `dmsg_wallet_hub`. Types are in [dmsg_types::wallet](../../src/dmsg_types/src/wallet.rs); digests, policy evaluation, address encoding and cosigner verification in [dmsg_protocol::wallet](../../src/dmsg_protocol/src/wallet/mod.rs). ICP interfaces follow [dmsg_wallet.did](../../src/dmsg_wallet/dmsg_wallet.did) and [dmsg_wallet_hub.did](../../src/dmsg_wallet_hub/dmsg_wallet_hub.did). Time uses Unix milliseconds; amounts use smallest asset units: e8s, satoshi, wei, lamport or token atomic units.

## Roles

| Role | Implementation | Responsibility |
| --- | --- | --- |
| Wallet | `dmsg_wallet` | Membership, policy, proposals/audit; direct ICP/ICRC transfers and BTC/EVM/SOL chain-key signing |
| Hub | `dmsg_wallet_hub` | Activation quotes/payments, creation, Wasm registry, accepted-version upgrades, code locking and member index |
| User home | `dmsg_user` | Verifies account-member device approvals through `verify_application_authorization` |
| Commerce | `dmsg_commerce` | Registers dmsg-wallet app/product; refresh_entitlement supplies creator plan |
| Client | `dmsg_client` | Reads external-chain inputs, independently decodes plans, cosigns, broadcasts and reports |

Wallets make no HTTPS outcalls. Member clients read balances, UTXOs, nonces and blockhashes; wrong inputs can make transactions invalid on chain but cannot make the wallet sign a transaction differing from intent.

## Members and roles

Teams manage wallets through dMsg account members (`MemberId::Account`) or direct-calling Principals (`MemberId::Principal`). SNS-governed funds remain in the SNS treasury for SNS proposals, not team-wallet operations. Roles:

| Role | Proposals | Counted approvals | Veto / freeze |
| --- | --- | --- | --- |
| Admin | All | Administration and spending | Yes |
| Approver | All | Spending | Yes |
| Proposer | All | No | No |
| Viewer | No | No | No |
| Recovery | Membership changes only | None required for its own proposals; 7-day delay with Admin/Approver veto | No |

Added members are Invited and must join before acting. Before joining, clients ask hub sync_wallet to refresh the member index, including post-creation invitees. Removing a member also removes its cosigners, except keys committed in dual-control Taproot scripts remain so addresses stay spendable. Account members bind calling Principals through WalletJoin device approval, at most 4 each. Bound Principals propose/execute/report for that account, but decisions still require fresh device approval. Each client device generates a local Ed25519 wallet key, LDK-encrypted in its workspace, since login delegations target fixed canisters. One Principal represents only one member; repeated join by an already joined/bound Principal returns its record. If a home is absent from wallet records, fetch hub get_user_homes once before verification.

## Digests and device approvals

All digests are `digest(domain, value) = SHA-256(CBOR([1, domain, value]))` using deterministic CBOR.

| Purpose | Domain | Value |
| --- | --- | --- |
| Proposal | `dmsg-wallet/proposal/v1` | `(environment, wallet, op_id, intent, policy_version, proposer, via_agent)` |
| Decision | `dmsg-wallet/decision/v1` | `(proposal_digest, decision)`, decision ∈ Approve / Reject / Veto |
| Join | `dmsg-wallet/join/v1` | `(wallet, account_id, actor)` |
| Activation | `dmsg-wallet/activation/v1` | `(quote, spec)` |
| Cosigner message | `dmsg-wallet/cosign/v1` | `(wallet, proposal_id, plan_seq, digest_index, digest)` |
| Audit | `dmsg-wallet/audit/v1` | `(prev, seq, at_ms, actor, proposal, kind)` |
| Activation deposit subaccount | `dmsg-wallet/deposit/v1` | `(hub, quote_id)` |

Account decisions use ApplicationApproval: `app_id = "dmsg-wallet"`; beneficiary = `{ product_id: "dmsg-wallet", authority_canister: hub, subject_schema: "dmsg-wallet-v1", subject_bytes: service's 10-byte Principal }`; service is wallet, or hub at creation; actor is the calling wallet Principal, with action_digest above. Purposes:

| `ApprovalPurpose` | service | action_digest |
| --- | --- | --- |
| `WalletCreate` | Hub, equal to product registration adapter | Activation digest |
| `WalletProposal` | Wallet | Decision digest |
| `WalletJoin` | Wallet | Join digest |

Devices require independent WalletApprove, separate from FormalApprove. Homes require subject_bytes == service for these purposes and verify_application_authorization accepts only caller == service; homes do not evaluate wallet policy. Wallet/hub check all fields with check_wallet_approval before home verification and reread proposal, members and time after the await.

## Policy and requirements

Address cosigning applies to source addresses of transfers, batches, refuels and external-chain operations. CanisterCall cannot infer one spending subaccount from arbitrary Candid, so all protected ICP addresses must individually meet their thresholds; generic calls cannot bypass them. Signatures bind plan 0 and proposal digest as with ICP/ICRC transfers. Shared cosigners verify once, FIDO2/WebAuthn counters increase per digest index, then thresholds are evaluated per address.

Removing keys/members must not implicitly lower thresholds; referenced keys first need SetCosign changes. Enabling hardware-only SetPolicy requires enough qualified keys on existing addresses. SetCosign rejects no-ops equal to current state; None explicitly removes the requirement.

Repeated prepare_execution reuses the proposal's valid unsigned plan; only expiry or prior signing allocates a new sequence. All devices collect signatures for the same plan. Replacement transactions use separate plans under original intent constraints.

preview_intent(intent) is joined-member read-only preflight, checking member, intent, freeze, address book and rolling window before returning Requirement. It writes/reserves nothing and is only a hint; submission, approval and execution revalidate current state.

Proposal creation computes `Requirement { approvals, admin, delay_ms, window_amount }`. Each spending approval recomputes the current 24-hour window and raises approvals/delay if stricter, extending expires_at_ms accordingly. Splitting spending among simultaneous proposals still puts later approvals in the cumulative tier. Policy version, membership, role/Agent authority and first invitation activation advance authorization version, making unexecuted proposals Stale; additional devices of one active member do not. First execution rechecks valid votes; zero-approval recovery proposals require a still-active Recovery originator.

- **Spending** (Transfer/Batch/Refuel): asset must be in spendable_assets. window_amount is valid unexecuted reservations + amounts whose execution/signing began in the past 24 hours + this proposal. Choose the first tier with up_to ≥ window_amount, using approvals/delay_ms/book_only. Above the highest tier requires one more approver, capped by active approvers; exceeding daily_limits requires all active approvers. book_only requires each recipient in the address book past cooldown. No asset tier means approver_threshold without delay.
- **Operations** (CanisterCall/CancelEvmNonce/InitSolNonce/AddressBook): approver_threshold without delay.
- **Administration:** admin_threshold, counting only Admin. SetPolicy/RemoveMember/SetCosign/RemoveCosigner/AcceptUpgrade/LockCode/ReleaseEvmNonce wait 24 hours; Unfreeze uses unfreeze_delay_ms; others have no delay.
- Meeting requirements enters Delayed, with veto during the wait, or Ready. Intents modifying only wallet records take effect at Ready; others require a member execute call.
- Any Admin/Approver may freeze directly. While frozen only Unfreeze/membership changes may be proposed, approved or executed; reject/veto remain available. Batches stop before the next transfer and signing before the next signature.
- Partially paid failed batches and signed transactions reported failed still count in the 24-hour window. Only definite no-spend failures release it.

## States

`Open → Delayed → Ready → Executing → Signed → Broadcast → Settled`; other terminal states are Failed, Rejected (insufficient remaining eligible approvers), Vetoed, Cancelled (proposer withdrawal), Expired after `expires_at_ms = created + delay + proposal_ttl_ms`, and Stale. Unknown ICP/ICRC results remain Executing; reconcile resends identical parameters and ledger deduplication returns the original block.

Above 10,000 terminal proposals, each new proposal removes the oldest 2 terminal entries; prune removes at most 100 per call. Expired proposals close first, while executing/awaiting-report proposals are skipped.

## Addresses

`derivation_path = [b"dmsg-wallet/v1", chain_tag, id.to_be_bytes()]`, where id is wallet-unique. ICP subaccounts left-pad the four-byte big-endian ID to 32 bytes; `id = 0` is the default subaccount.

| Chain / control | chain_tag | Key | Address | Signature |
| --- | --- | --- | --- | --- |
| ICP | `icp` | None | ICRC-1 text of (wallet, subaccount) | Direct ledger call |
| Bitcoin `ChainKey` | `btc` | Schnorr BIP-340 | P2TR：`Q = P + H_TapTweak(P)·G`，bech32m | `sign_with_schnorr`，`aux = Bip341 { merkle_root_hash: [] }` |
| Bitcoin `Segwit` | `btc-segwit` | ECDSA secp256k1 | P2WPKH，bech32 | `sign_with_ecdsa`（BIP-143 sighash），low-s DER + SIGHASH_ALL |
| Bitcoin Taproot dual control | `btc-native` | Schnorr BIP-340 | BIP-341 NUMS internal key, tree `{L1, L2}` or `{L1, {L2, L3}}` | Script path, aux = None |
| EVM | `eth` | ECDSA secp256k1 | `keccak256(x‖y)[12..]`, EIP-55, shared across EVM chains | `sign_with_ecdsa(keccak256(0x02‖rlp(...)))`, low-s, derive y-parity |
| Solana | `sol` | Schnorr Ed25519 | Base58 of 32-byte public key | sign_with_schnorr over the entire message |

After install/every upgrade, a timer runs bootstrap, retrying each minute on failure; anyone may also call it. It reads three root keys with empty-path public keys/chain codes in WalletView.key_roots and creates defaults 1 Bitcoin, 2 EVM, 3 Solana. Clients use ic-secp256k1/ic-ed25519 derive_subkey_with_chain_code offline to derive keys and recompute address text; mismatches remain unverified.

Taproot leaf scripts: `L1 = <ck_x> OP_CHECKSIGVERIFY <cs_x> OP_CHECKSIG` requires both keys; `L2 = <n> OP_CSV OP_DROP <ck_x> OP_CHECKSIG` permits chain-key-only spending once the UTXO reaches chain_key_after blocks; optional `L3 = <m> OP_CSV OP_DROP <recovery_x> OP_CHECKSIG`. Cosigners must be Bip340 and directly sign each input's script-path sighash.

## External-chain execution

1. A member reads chain parameters and calls prepare_execution(id, ExecutionParams). The wallet constructs from intent, verifies inputs and returns `ExecutionPlan { unsigned, digests, summary }` without signing.
2. Client independently decodes unsigned, recomputes each digest (BIP-341/BIP-143 sighash, keccak256, SHA-256(message)) and compares against intent/summary; mismatch fails.
3. Required cosigners sign `cosign_message(wallet, id, seq, i, digests[i])`, then call `execute(ExecuteArgs { proposal, plan: Some(seq), cosigs })`. Wallet verifies and calls management signing sequentially, stopping if frozen, then assembles `SignedTx { raw, txid }`.
4. Member broadcasts raw and reports `report(ChainReport { outcome: Broadcast | Settled | Failed })`. Wallet records reporter without chain verification. Broadcast is recorded once per plan; Settled/Failed ends the proposal.

| Chain | prepare_execution rules |
| --- | --- |
| Bitcoin | Version 2; all inputs belong to source, no duplicates, ≤32. Outputs ≥546 sat. Remainder must cover no-change fee_rate × vsize; add change only if ≥546 sat remains after with-change fees, otherwise add remainder to fee. fee ≤ max_fee. Input sequence 0xFFFFFFFD for RBF, or chain_key_after for L2. Later plans spend exactly the first signed plan's inputs with the same amounts |
| EVM | chain_id ∈ evm_chains; native to=recipient/data=empty; ERC-20 transfer(address,uint256). gas_limit × max_fee_per_gas ≤ max_fee and max_priority ≤ max_fee_per_gas. One unsettled signed transaction per address/chain; re-signing preserves nonce. CancelEvmNonce/ReleaseEvmNonce explicitly bind original proposal/nonce; cancellation is zero-value self-transfer with that nonce. Reports unlock only the matching nonce lock without erasing original signing facts or spending allowance |
| Solana | Instructions derive only from intent: ComputeBudget, System Transfer, SPL TransferChecked with optional ATA CreateIdempotent, CreateAccountWithSeed + InitializeNonceAccount for InitSolNonce (seed dmsg-wallet-nonce, rent 1,447,680 lamports). With a nonce account, first instruction is AdvanceNonceAccount and blockhash is its nonce; re-signing keeps the first signed value. Without one, signed proposals cannot re-sign a new blockhash, only rebroadcast bytes. First signing fixes nonce mode; later initialization cannot repay an old proposal. `5000 + ⌈price × limit / 10⁶⌉ ≤ max_fee` |

Plans last 10 minutes, at most 8 per proposal. daily_signatures counts plan digests before signing, including interrupted attempts. Replacement rules are checked at preparation, before signing and after signing: among prebuilt plans, the first signed governs; concurrent proposals signing for the same address reject the later completion without returning its signed transaction.

## Cosigner envelopes

| CosignerEnvelope | Signed bytes | Extra checks |
| --- | --- | --- |
| Raw | Cosigner message; P-256/secp256k1 use ECDSA-SHA256, BIP-340/Ed25519 sign directly | — |
| `Fido2 { rp_id_hash, user_verification }` | authenticator_data ‖ cosigner message, which is CTAP2 clientDataHash | RP hash, UP=1, policy-required UV=1, monotonic counter |
| `WebAuthn { rp_id_hash, origins, user_verification }` | `authenticator_data ‖ SHA-256(client_data_json)` | `type = "webauthn.get"`, challenge=base64url(cosigner message), origin ∈ origins; remaining checks as FIDO2 |

Signatures are 64-byte r‖s; clients convert DER. hardware is client-declared and unverifiable by the wallet; hardware_cosigners policy rejects keys not declared hardware-backed.

## Hub activation

1. quote_activation_preview(account_id), a query, reads hub pricing/current version/account creation usage and returns price, cycles, ledger and remaining quota per plan. It writes/reserves nothing. Clients separately read plan and ledger fee; quota 0 displays remaining 0 rather than an error.
2. quote_activation(account_id) calls commerce refresh_entitlement (no record means Free), subtracts account usage from quotas and issues a 15-minute quote for `(hub, deposit_subaccount(hub, quote_id))`. No quote is issued if current fees make the two revenue transfers unaffordable. Exhausted quota still returns `remaining = 0` so users see the plan limit; create_wallet rejects creation.
3. Payer sends price_e8s to the deposit account; creator device approves WalletCreate.
4. `create_wallet(CreateWallet { quote_id, spec, approval_id, approval })` requires the original quote caller. After checking approval/balance and rereading expiry/refunds/pause/quota after awaits, mark Paid, create a canister with hub-only controller, 365-day freezing threshold and cycles_to_wallet, and chunk-install the quoted version: Created → Ready. Creation failure is Failed. Interrupted installs resume with the same create_wallet request or anyone's reconcile_activation. Quota is rechecked; `remaining = 0` cannot create.
5. claim_activation_refund(quote_id, to) lets the quote caller claim unreserved balance, less current ledger fees, to an account outside the hub. It covers failed/expired payments, excess on successful wallets and late deposits. Uncollected quote amounts/unsent revenue legs stay reserved. Each refund fixes destination/fee/memo/time and reuses them if unknown; new money after completion creates a new refund. Clients retain paid quotes after creation/refund to detect late funds.
6. collect_revenue takes only quote.price_e8s, not the entire subaccount balance. get_activation_reserve returns remaining protection. abort_activation permits only the quote caller to end uninstalled Paid/Created activations; deleting an empty instance releases quota and enables refund. Installed instances cannot use this path.
7. collectable_activations(after, take) lists ready revenue, collectible by anyone. Quoted cycles_share_e8s goes to CMC with TPUP memo; the quote remainder after fees goes to revenue.

## Upgrades and code locking

An administrative AcceptUpgrade { version, module_hash } proposal with 24-hour delay accepts a hub-registered version. Anyone calls upgrade_wallet; hub reads `accepted_upgrade()`, checks version/hash, stops, chunk-installs with the accepted version as post_upgrade argument (clearing matching acceptance), then starts. Stop failure restarts immediately; inability to read acceptance attempts start before erroring. Effective LockCode lets lock_wallet clear controllers, preventing upgrades/deletion. Only modules whose hashes governance registered through admin_register_version are installed; controller/governance/designated uploader uploads them. Retired versions can be admin_remove_version-deleted only without unexpired quotes/unfinished activations, and version numbers are never reused. Hub persists references and rechecks modules before install; lost successful-install replies recover from the uniquely controlled instance's state.

## cycles

Anyone may call refuel(): under policy, when below min_cycles and within daily count, transfer from the specified ICP address to the CMC instance subaccount using ICRC-1 memo TPUP (`0x50555054` as 8-byte little-endian), then notify_top_up. Refuel intent does the same with approval. Watchdogs enumerate hub list_wallets(after, take).

## Vectors

[Wallet vectors](../../tests/vectors/wallet/README.md) fix bytes: protocol-v1.json provides CBOR, complete preimages/digests for proposals, decisions, joins, activation, cosigner messages, deposits and audit, plus chain addresses, Taproot leaves/control blocks and receiving scripts. plans-v1.json provides external-chain plans, signatures and signed transactions; clients must accept all and reject tampered cases. Rust generates both files and tests compare every byte.

## Desktop and local protocol

Structured desktop methods, multi-device packages, incremental audit and encrypted channel hints are described in the [team-wallet guide](../dMsg_wallet.md). Client creation verifies WalletCreate at the home before payment; importing cosignatures never executes automatically.


## Security revisions on 2026-10-24

- Hub upgrade/lock share one per-wallet mutex. Before execution, hub-only claim_code(expected) atomically claims acceptance; while claimed, instances cannot accept another upgrade/lock. Successful upgrade clears it; failed operations `release_code()` after restarting. Verify Running before clearing controllers.
- After AddAddress public-key retrieval, recheck authorization version, expiry, freeze, capacity and all cosigner references, for ordinary ChainKey and native Taproot alike.
- Unexecuted reservations do not expire by approval time; only cancellation, invalidation or definite no-spend releases them. Interrupted signing without a signed transaction restores reservation, preventing failed execution from bypassing limits across days.
- An attempted ICRC transfer retried with TooOld retains Executing/original parameters and returns ExecutionUnknown. A never-sent item first rejected TooOld fails definitively, preserving paid batch blocks. reconcile_transfer(proposal, item, block) reads ICRC-3 and matches payer/payee/amount/fee/memo/created_at_time/spender before recording the block; unproven absence cannot authorize another payment.
- Client approval/cosigning authenticates the full proposal through get_proposal_certified, deriving expiry/delay from freshly certified wallet version/time. Recomputing a supplied digest is not authoritative reading.
- Native EVM/ERC-20 preview and execution estimate gas using the same final call parameters; instances still enforce fee ceilings.

Deploy in order: register/upgrade wallet instances through the existing hub, then update the claim_code-aware hub, then clients. Old cancellation/release proposals without target binding must be recreated. These are source contracts, not production deployment claims.


## Policy usage and local workflows

`policy_usage() -> Result<Vec<SpendUsage { asset, reserved, executed }>>` is read-only under wallet read permissions. reserved covers valid approved/unexecuted commitments, executed covers starts in the past 24 hours, using approval-tier logic. It is not asset balance or a substitute for approval/execution rechecks.

Client wallet/queue and wallet/history read all retained proposals and use certified audit heads before/after reads and for cache invalidation; lists are not approval authority. wallet/notes encrypts bookkeeping text only in the current local account without changing Intent, proposal digests, approvals or on-chain memo. wallet/cosigner_test signs/verifies a random domain-separated local challenge, exports no signature and constructs no chain transaction. UI distinguishes process completion, target responses, ledger confirmation and external reports not independently verified by the wallet.
