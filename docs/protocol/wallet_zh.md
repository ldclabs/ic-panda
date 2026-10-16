# dMsg 团队钱包：dmsg-wallet/1 公开合同

简体中文 | 本文描述 `dmsg_wallet`（每个团队一个实例）与 `dmsg_wallet_hub`（创建与升级实例的工厂）的公开接口、批准字节、地址派生与外链交易规则。类型定义在 [dmsg_types::wallet](../../src/dmsg_types/src/wallet.rs)，摘要、策略评估、地址编码与协签验证在 [dmsg_protocol::wallet](../../src/dmsg_protocol/src/wallet/mod.rs)，ICP 接口以 [dmsg_wallet.did](../../src/dmsg_wallet/dmsg_wallet.did) 与 [dmsg_wallet_hub.did](../../src/dmsg_wallet_hub/dmsg_wallet_hub.did) 为准。时间单位为 Unix 毫秒，金额为资产最小单位（e8s、satoshi、wei、lamport、代币原子单位）。

## 角色

| 角色 | 实现 | 职责 |
| --- | --- | --- |
| 钱包 | `dmsg_wallet` | 成员、策略、提案、审计；ICP/ICRC 自行转账；BTC/EVM/SOL 用链密钥签名 |
| hub | `dmsg_wallet_hub` | 激活报价与收款、创建实例、登记 Wasm 版本、按实例接受的版本升级、锁定代码、成员索引 |
| user home | `dmsg_user` | 核验账户成员的设备批准（`verify_application_authorization`） |
| commerce | `dmsg_commerce` | 登记应用与产品 `dmsg-wallet`；`refresh_entitlement` 提供创建者的套餐 |
| 客户端 | `dmsg_client` | 读取外链参数、独立解码计划、协签、广播与回报 |

钱包不做 HTTPS outcall。外链余额、UTXO、nonce 与 blockhash 由成员客户端读取；读错只会让交易在链上无效，不会让钱包签出与意图不同的交易。

## 成员与角色

成员是 dMsg 账户（`MemberId::Account`）或 Principal（`MemberId::Principal`，例如 SNS governance）。角色：

| 角色 | 提案 | 批准计数 | 否决 / 冻结 |
| --- | --- | --- | --- |
| Admin | 全部 | 管理类与支出类 | 是 |
| Approver | 全部 | 支出类 | 是 |
| Proposer | 全部 | 否 | 否 |
| Viewer | 否 | 否 | 否 |
| Recovery | 仅成员变更 | 否；其提案无需批准，延时 7 天，期间任一 Admin/Approver 可否决 | 否 |

被添加的成员处于 `Invited`，`join` 后才行动。账户成员以 `WalletJoin` 设备批准绑定调用 Principal（每成员最多 4 个）；之后该 Principal 代表该账户提案、执行与回报，决定仍需每次的设备批准。dMsg 客户端为此在每台设备生成一把“钱包密钥”（Ed25519，LDK 加密存于本机工作区），因为登录委托只到达固定 canister。

## 摘要与设备批准

所有摘要为 `digest(domain, value) = SHA-256(CBOR([1, domain, value]))`（确定性 CBOR）。

| 用途 | domain | value |
| --- | --- | --- |
| 提案 | `dmsg-wallet/proposal/v1` | `(environment, wallet, op_id, intent, policy_version, proposer, via_agent)` |
| 决定 | `dmsg-wallet/decision/v1` | `(proposal_digest, decision)`，decision ∈ `Approve` / `Reject` / `Veto` |
| 加入 | `dmsg-wallet/join/v1` | `(wallet, account_id, actor)` |
| 激活 | `dmsg-wallet/activation/v1` | `(quote, spec)` |
| 协签消息 | `dmsg-wallet/cosign/v1` | `(wallet, proposal_id, plan_seq, digest_index, digest)` |
| 审计 | `dmsg-wallet/audit/v1` | `(prev, seq, at_ms, actor, proposal, kind)` |
| 激活收款子账户 | `dmsg-wallet/deposit/v1` | `(hub, quote_id)` |

账户成员的决定是 `ApplicationApproval`：`app_id = "dmsg-wallet"`，`beneficiary = { product_id: "dmsg-wallet", authority_canister: hub, subject_schema: "dmsg-wallet-v1", subject_bytes: service 的 10 字节 Principal }`，`service` 为钱包（创建时为 hub），`actor` 为调用钱包的 Principal，`action_digest` 取上表。用途：

| `ApprovalPurpose` | service | action_digest |
| --- | --- | --- |
| `WalletCreate` | hub（必须等于产品登记的 `adapter`） | 激活摘要 |
| `WalletProposal` | 钱包 | 决定摘要 |
| `WalletJoin` | 钱包 | 加入摘要 |

设备须持有新能力 `WalletApprove`（与 `FormalApprove` 分离）。user home 对这三种用途要求 `subject_bytes == service`，`verify_application_authorization` 只接受 caller 等于 `service`；home 不判断钱包策略。钱包与 hub 用 `check_wallet_approval` 核对全部字段后再请 home 核验，跨 canister 返回后重读提案、成员与时间。

## 策略与要求

提案时按当前策略一次算出 `Requirement { approvals, admin, delay_ms, window_amount }` 并冻结；之后策略版本变化使未执行提案变为 `Stale`。

- **支出类**（`Transfer`、`Batch`、`Refuel`）：资产须在 `spendable_assets`。`window_amount` = 该资产过去 24 小时已批准或执行的金额 + 本提案金额；取第一个 `up_to ≥ window_amount` 的档（`approvals`、`delay_ms`、`book_only`）；超出最高档为最高档 + 1 人（不超过活跃批准人数）；超过 `daily_limits` 时需要全部活跃批准人。`book_only` 档要求每个收款方是冷却期已过的地址簿条目。没有该资产的档时用 `approver_threshold`、无延时。
- **操作类**（`CanisterCall`、`CancelEvmNonce`、`InitSolNonce`、`AddressBook`）：`approver_threshold`，无延时。
- **管理类**：`admin_threshold`，只计 Admin。`SetPolicy`、`RemoveMember`、`SetCosign`、`RemoveCosigner`、`AcceptUpgrade`、`LockCode`、`ReleaseEvmNonce` 延时 24 小时；`Unfreeze` 延时 `unfreeze_delay_ms`；其余无延时。
- 达到要求后进入 `Delayed`（有延时，期间可否决）或 `Ready`。只改钱包记录的意图在 `Ready` 时即生效；其余由成员调用 `execute`。
- 任一 Admin/Approver 可直接 `freeze()`；冻结时只能提案与执行 `Unfreeze` 和成员变更。

## 状态

`Open → Delayed → Ready → Executing → Signed → Broadcast → Settled`，终态另有 `Failed`、`Rejected`（剩余可批准人数不足）、`Vetoed`、`Cancelled`（提案人撤回）、`Expired`（超过 `expires_at_ms = created + delay + proposal_ttl_ms`）、`Stale`。ICP/ICRC 转账在 `Executing` 时结果未知，由 `reconcile` 用相同参数重发，账本去重返回原区块。

## 地址

`derivation_path = [b"dmsg-wallet/v1", chain_tag, id.to_be_bytes()]`，`id` 是钱包内唯一的地址编号（ICP 地址的子账户为 `id` 的大端 4 字节左填充到 32 字节，`id = 0` 为默认子账户）。

| 链 / 控制 | chain_tag | 密钥 | 地址 | 签名 |
| --- | --- | --- | --- | --- |
| ICP | `icp` | 无 | `(钱包, 子账户)` 的 ICRC-1 文本 | 钱包直接调用账本 |
| Bitcoin `ChainKey` | `btc` | Schnorr BIP-340 | P2TR：`Q = P + H_TapTweak(P)·G`，bech32m | `sign_with_schnorr`，`aux = Bip341 { merkle_root_hash: [] }` |
| Bitcoin `Segwit` | `btc-segwit` | ECDSA secp256k1 | P2WPKH，bech32 | `sign_with_ecdsa`（BIP-143 sighash），low-s DER + SIGHASH_ALL |
| Bitcoin `Taproot`（链上双控） | `btc-native` | Schnorr BIP-340 | 内部密钥为 BIP-341 NUMS 点，脚本树 `{L1, L2}` 或 `{L1, {L2, L3}}` | 脚本路径，`aux = None` |
| EVM | `eth` | ECDSA secp256k1 | `keccak256(x‖y)[12..]`，EIP-55；所有 EVM 链共用 | `sign_with_ecdsa(keccak256(0x02‖rlp(...)))`，low-s，试算 y-parity |
| Solana | `sol` | Schnorr Ed25519 | 32 字节公钥的 base58 | `sign_with_schnorr` 签完整 message |

实例创建后 `bootstrap` 读取三个根公钥（空路径的公钥与 chain code，见 `WalletView.key_roots`）并建立默认地址 1（Bitcoin）、2（EVM）、3（Solana）。客户端用 `ic-secp256k1` / `ic-ed25519` 的 `derive_subkey_with_chain_code` 从根公钥离线派生每个地址的公钥并重算地址文本，不一致的地址标记为未核对。

链上双控 Taproot 的叶脚本：`L1 = <ck_x> OP_CHECKSIGVERIFY <cs_x> OP_CHECKSIG`（链密钥与协签人同时签），`L2 = <n> OP_CSV OP_DROP <ck_x> OP_CHECKSIG`（UTXO 满 `chain_key_after` 个区块后链密钥单独花费），可选 `L3 = <m> OP_CSV OP_DROP <recovery_x> OP_CHECKSIG`。协签人须为 `Bip340` 密钥，对每个输入的脚本路径 sighash 直接签名。

## 外链执行

1. 成员读取外链参数，调用 `prepare_execution(id, ExecutionParams)`；钱包从意图构造交易，核对参数，返回 `ExecutionPlan { unsigned, digests, summary }`，不签名。
2. 客户端独立解码 `unsigned`，重算每个待签摘要（BIP-341/BIP-143 sighash、`keccak256`、`SHA-256(message)`），与意图和 `summary` 逐项比对；不一致即拒绝。
3. 地址有协签策略时，协签人对 `cosign_message(wallet, id, seq, i, digests[i])` 签名；`execute(ExecuteArgs { proposal, plan: Some(seq), cosigs })`。钱包验签后逐个调用管理 canister，期间若被冻结则停止；签完组装交易并返回 `SignedTx { raw, txid }`。
4. 成员广播 `raw`，以 `report(ChainReport { outcome: Broadcast | Settled | Failed })` 回报；钱包记录回报人，不核对链上状态。

| 链 | `prepare_execution` 规则 |
| --- | --- |
| Bitcoin | 版本 2；输入全部属于来源地址、不重复、最多 32 个；输出不低于 546 sat；`fee = fee_rate × vsize`（含找零估算），找零低于 546 sat 时并入手续费；`fee ≤ max_fee`；输入序号 `0xFFFFFFFD`（RBF），走 L2 时为 `chain_key_after`。同一提案的后续计划必须花费与首个已签计划完全相同的输入 |
| EVM | `chain_id ∈ evm_chains`；原生转账 `to = 收款方, data = 空`；ERC-20 为 `transfer(address,uint256)`；`gas_limit × max_fee_per_gas ≤ max_fee`，`max_priority ≤ max_fee_per_gas`。同一地址同一链同时只能有一笔未结算已签交易，重签必须同 nonce；`CancelEvmNonce` 以同 nonce 零值自转替换它，结算后被替换的提案标记失败 |
| Solana | 指令只来自意图：ComputeBudget、System `Transfer`、SPL `TransferChecked`（可选 ATA `CreateIdempotent`）、`CreateAccountWithSeed` + `InitializeNonceAccount`（`InitSolNonce`，seed `dmsg-wallet-nonce`，租金 1,447,680 lamports）；地址有 nonce 账户时第一条为 `AdvanceNonceAccount`，`blockhash` 为 nonce 值，重签必须同值；无 nonce 账户时重签至少间隔 150 秒；`5000 + ⌈price × limit / 10⁶⌉ ≤ max_fee` |

每计划有效 10 分钟，每提案最多 8 个计划，每日阈值签名数受 `daily_signatures` 限制。

## 协签封装

| `CosignerEnvelope` | 签名覆盖 | 额外核对 |
| --- | --- | --- |
| `Raw` | 协签消息（P-256、secp256k1 为 ECDSA-SHA256；BIP-340、Ed25519 直接签） | — |
| `Fido2 { rp_id_hash, user_verification }` | `authenticator_data ‖ 协签消息`（CTAP2 的 clientDataHash 即协签消息） | rp id hash、UP=1、按策略 UV=1、计数器单调 |
| `WebAuthn { rp_id_hash, origins, user_verification }` | `authenticator_data ‖ SHA-256(client_data_json)` | `type = "webauthn.get"`、`challenge = base64url(协签消息)`、`origin ∈ origins`，其余同 FIDO2 |

签名为 64 字节 `r‖s`（DER 由客户端转换）。`hardware` 由客户端声明，钱包无法核实；策略 `hardware_cosigners` 为真时不接受未声明硬件的协签人。

## hub 激活

1. `quote_activation(account_id)`：hub 向 commerce `refresh_entitlement` 读取账户套餐（无记录为 Free），按 `quotas` 减去该账户已占用的创建数；报价 15 分钟有效，收款账户为 `(hub, deposit_subaccount(hub, quote_id))`。
2. 付款人向收款账户转入 `price_e8s`；创建者设备批准 `WalletCreate`。
3. `create_wallet(CreateWallet { quote_id, spec, approval_id, approval })`：调用者必须是报价时的调用者；hub 核对批准与余额后记为 `Paid`，再创建 canister（controller 仅 hub，冻结阈值 365 天，附 `cycles_to_wallet`）并以分块安装当前版本，状态 `Created → Ready`；失败可 `reconcile_activation`，创建失败为 `Failed`，付款人可 `claim_activation_refund`（扣账本手续费）。过期未用的报价同样可退款。
4. `collect_revenue(quote_id)`：已就绪订单的收款中，`cycles_share_e8s` 以 memo `TPUP` 转入 CMC 为 hub 充值 cycles，余额转入 `revenue` 账户。

## 升级与锁定

钱包以 `AcceptUpgrade { version, module_hash }` 管理类提案（24 小时延时）接受 hub 登记的版本；任何人调用 hub `upgrade_wallet`，hub 读取 `accepted_upgrade()`、核对版本与哈希后停止实例、分块安装（`post_upgrade` 参数为该版本号，匹配时清除接受记录）并启动。`LockCode` 生效后 `lock_wallet` 把 controllers 设为空，之后不能升级或删除。hub 只安装治理以 `admin_register_version` 登记过哈希的模块；上传由 controller、governance 或其指定的上传者完成。

## cycles

`refuel()` 任何人可调：策略 `refuel` 规则下余额低于 `min_cycles` 且当日次数未满时，从指定 ICP 地址以 ICRC-1 memo `TPUP`（`0x50555054` 小端 8 字节）转入 CMC 的实例子账户并 `notify_top_up`。`Refuel` 意图做同样的事但需要批准。
