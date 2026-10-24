# dMsg 团队钱包：dmsg-wallet/1 公开合同

[English](wallet.md) | 简体中文

本文描述 `dmsg_wallet`（每个团队一个实例）与 `dmsg_wallet_hub`（创建与升级实例的工厂）的公开接口、批准字节、地址派生与外链交易规则。类型定义在 [dmsg_types::wallet](../../src/dmsg_types/src/wallet.rs)，摘要、策略评估、地址编码与协签验证在 [dmsg_protocol::wallet](../../src/dmsg_protocol/src/wallet/mod.rs)，ICP 接口以 [dmsg_wallet.did](../../src/dmsg_wallet/dmsg_wallet.did) 与 [dmsg_wallet_hub.did](../../src/dmsg_wallet_hub/dmsg_wallet_hub.did) 为准。时间单位为 Unix 毫秒，金额为资产最小单位（e8s、satoshi、wei、lamport、代币原子单位）。

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

钱包由团队成员管理：成员是团队成员的 dMsg 账户（`MemberId::Account`）或以调用者身份直接行动的 Principal（`MemberId::Principal`）。需要 SNS 治理的资金留在 SNS 国库，由 SNS 提案处理，不经团队钱包。角色：

| 角色 | 提案 | 批准计数 | 否决 / 冻结 |
| --- | --- | --- | --- |
| Admin | 全部 | 管理类与支出类 | 是 |
| Approver | 全部 | 支出类 | 是 |
| Proposer | 全部 | 否 | 否 |
| Viewer | 否 | 否 | 否 |
| Recovery | 仅成员变更 | 否；其提案无需批准，延时 7 天，期间任一 Admin/Approver 可否决 | 否 |

被添加的成员处于 `Invited`，`join` 后才行动；客户端加入前先请 hub `sync_wallet` 刷新成员索引，创建后才加入的成员由此出现在索引中。移除成员时其协签人一并移除，但双控 Taproot 地址脚本承诺的协签人保留，该地址仍可花费。账户成员以 `WalletJoin` 设备批准绑定调用 Principal（每成员最多 4 个）；之后该 Principal 代表该账户提案、执行与回报，决定仍需每次的设备批准。dMsg 客户端为此在每台设备生成一把“钱包密钥”（Ed25519，LDK 加密存于本机工作区），因为登录委托只到达固定 canister。一个 Principal 只能代表一个成员；已加入并已绑定的 Principal 再次 `join` 直接返回成员记录。账户所在的 user home 不在钱包记录中时，钱包向 hub `get_user_homes` 补取一次后再核验。

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

地址协签策略分别适用于转账、批量支出、充值及对应外链操作的来源地址。`CanisterCall` 无法从任意 Candid 参数推断唯一支出子账户，执行时要求所有已配置协签的 ICP 地址分别满足阈值；不能通过通用调用绕过地址协签。其签名同 ICP/ICRC 转账一样，使用计划编号 0 和提案摘要。共享协签人只验证一次，FIDO2/WebAuthn 计数器按摘要编号递增验证，再分别计算各地址阈值。

删除协签人或成员不得隐式降低地址阈值：被地址策略引用的密钥必须先通过 `SetCosign` 调整引用。`SetPolicy` 启用仅硬件协签时，现有地址仍须有足够合格密钥。`SetCosign` 拒绝与当前值相同的空操作；`None` 明确表示移除地址协签要求。

`prepare_execution` 重复调用会复用同一提案当前有效的未签计划；过期或已经签过后才分配新编号。所有设备应对同一计划收集签名，替换交易继续使用独立计划与原意图约束。

`preview_intent(intent)` 为已加入成员提供当前策略的只读预检，检查成员、意图、冻结、地址簿与滚动窗口后返回 `Requirement`；不写提案、不占用预算。该结果只是提示，提交、批准和执行仍按当时状态校验。

提案时按当前策略算出 `Requirement { approvals, admin, delay_ms, window_amount }`。支出类在每次批准时按当时的 24 小时窗口再算一次，更严时提高 `approvals` 与 `delay_ms`，`expires_at_ms` 随延时延长：一笔支出拆成多个同时挂起的提案，后批准的仍落入合计金额的档。策略版本、成员增删、角色/Agent 授权变化及邀请首次激活都会推进授权版本，使未执行提案变为 `Stale`；绑定同一成员的另一设备不重复推进版本。首次执行重验当前有效票数，零批准恢复提案须由仍有效的 Recovery 发起。

- **支出类**（`Transfer`、`Batch`、`Refuel`）：资产须在 `spendable_assets`。`window_amount` = 该资产所有尚有效的未执行预留 + 过去 24 小时开始执行/签名的金额 + 本提案金额；取第一个 `up_to ≥ window_amount` 的档（`approvals`、`delay_ms`、`book_only`）；超出最高档为最高档 + 1 人（不超过活跃批准人数）；超过 `daily_limits` 时需要全部活跃批准人。`book_only` 档要求每个收款方是冷却期已过的地址簿条目。没有该资产的档时用 `approver_threshold`、无延时。
- **操作类**（`CanisterCall`、`CancelEvmNonce`、`InitSolNonce`、`AddressBook`）：`approver_threshold`，无延时。
- **管理类**：`admin_threshold`，只计 Admin。`SetPolicy`、`RemoveMember`、`SetCosign`、`RemoveCosigner`、`AcceptUpgrade`、`LockCode`、`ReleaseEvmNonce` 延时 24 小时；`Unfreeze` 延时 `unfreeze_delay_ms`；其余无延时。
- 达到要求后进入 `Delayed`（有延时，期间可否决）或 `Ready`。只改钱包记录的意图在 `Ready` 时即生效；其余由成员调用 `execute`。
- 任一 Admin/Approver 可直接 `freeze()`；冻结时只能提案、批准与执行 `Unfreeze` 和成员变更（拒绝与否决不受限），进行中的批量转账停在下一笔之前，签名停在下一个签名之前。
- 部分转出后失败的批量转账、成员回报失败的已签交易仍计入 24 小时窗口；只有确定没有转出的失败才释放。

## 状态

`Open → Delayed → Ready → Executing → Signed → Broadcast → Settled`，终态另有 `Failed`、`Rejected`（剩余可批准人数不足）、`Vetoed`、`Cancelled`（提案人撤回）、`Expired`（超过 `expires_at_ms = created + delay + proposal_ttl_ms`）、`Stale`。ICP/ICRC 转账在 `Executing` 时结果未知，由 `reconcile` 用相同参数重发，账本去重返回原区块。

终态提案超过 10,000 条时，每个新提案顺带删除最旧的 2 条终态提案，`prune()` 每次最多删除 100 条；期限已过的提案先按期限关闭，仍在执行或等待回报的跳过。

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

实例安装与每次升级后，定时器执行 `bootstrap`（失败每分钟重试，任何人也可提前调用），读取三个根公钥（空路径的公钥与 chain code，见 `WalletView.key_roots`）并建立默认地址 1（Bitcoin）、2（EVM）、3（Solana）。客户端用 `ic-secp256k1` / `ic-ed25519` 的 `derive_subkey_with_chain_code` 从根公钥离线派生每个地址的公钥并重算地址文本，不一致的地址标记为未核对。

链上双控 Taproot 的叶脚本：`L1 = <ck_x> OP_CHECKSIGVERIFY <cs_x> OP_CHECKSIG`（链密钥与协签人同时签），`L2 = <n> OP_CSV OP_DROP <ck_x> OP_CHECKSIG`（UTXO 满 `chain_key_after` 个区块后链密钥单独花费），可选 `L3 = <m> OP_CSV OP_DROP <recovery_x> OP_CHECKSIG`。协签人须为 `Bip340` 密钥，对每个输入的脚本路径 sighash 直接签名。

## 外链执行

1. 成员读取外链参数，调用 `prepare_execution(id, ExecutionParams)`；钱包从意图构造交易，核对参数，返回 `ExecutionPlan { unsigned, digests, summary }`，不签名。
2. 客户端独立解码 `unsigned`，重算每个待签摘要（BIP-341/BIP-143 sighash、`keccak256`、`SHA-256(message)`），与意图和 `summary` 逐项比对；不一致即拒绝。
3. 地址有协签策略时，协签人对 `cosign_message(wallet, id, seq, i, digests[i])` 签名；`execute(ExecuteArgs { proposal, plan: Some(seq), cosigs })`。钱包验签后逐个调用管理 canister，期间若被冻结则停止；签完组装交易并返回 `SignedTx { raw, txid }`。
4. 成员广播 `raw`，以 `report(ChainReport { outcome: Broadcast | Settled | Failed })` 回报；钱包记录回报人，不核对链上状态。同一计划的 `Broadcast` 只记录一次，`Settled` 与 `Failed` 结束提案。

| 链 | `prepare_execution` 规则 |
| --- | --- |
| Bitcoin | 版本 2；输入全部属于来源地址、不重复、最多 32 个；输出不低于 546 sat；剩余额须够付无找零交易的 `fee_rate × vsize`，扣除含找零的手续费后仍不低于 546 sat 时才加找零，否则全部并入手续费；`fee ≤ max_fee`；输入序号 `0xFFFFFFFD`（RBF），走 L2 时为 `chain_key_after`。同一提案的后续计划必须以相同金额花费与首个已签计划完全相同的输入 |
| EVM | `chain_id ∈ evm_chains`；原生转账 `to = 收款方, data = 空`；ERC-20 为 `transfer(address,uint256)`；`gas_limit × max_fee_per_gas ≤ max_fee`，`max_priority ≤ max_fee_per_gas`。同一地址同一链同时只能有一笔未结算已签交易，重签必须同 nonce；`CancelEvmNonce` 与 `ReleaseEvmNonce` 必须明确绑定原 `proposal` 和 `nonce`；取消以同 nonce 零值自转。成员回报只解开匹配对象的 nonce 锁，不改变原提案的签名事实、不释放其支出额度 |
| Solana | 指令只来自意图：ComputeBudget、System `Transfer`、SPL `TransferChecked`（可选 ATA `CreateIdempotent`）、`CreateAccountWithSeed` + `InitializeNonceAccount`（`InitSolNonce`，seed `dmsg-wallet-nonce`，租金 1,447,680 lamports）；地址有 nonce 账户时第一条为 `AdvanceNonceAccount`，`blockhash` 为 nonce 值，重签必须与首个已签计划同值；无 nonce 账户时已签提案不能换 blockhash 重签，只能重广播原字节；首签计划同时固定是否使用 nonce 账户，后续初始化 nonce 不能让旧提案再次付款；`5000 + ⌈price × limit / 10⁶⌉ ≤ max_fee` |

每计划有效 10 分钟，每提案最多 8 个计划。每日阈值签名数受 `daily_signatures` 限制，签名开始时即按计划的摘要数计入，中途停止也计入。重签规则在构建计划时、签名前和签完后各核对一次：任何计划签名前构建的多个计划，以先签名者为准；两个提案同时为同一地址签名时，后签完者被拒绝，不返回已签交易。

## 协签封装

| `CosignerEnvelope` | 签名覆盖 | 额外核对 |
| --- | --- | --- |
| `Raw` | 协签消息（P-256、secp256k1 为 ECDSA-SHA256；BIP-340、Ed25519 直接签） | — |
| `Fido2 { rp_id_hash, user_verification }` | `authenticator_data ‖ 协签消息`（CTAP2 的 clientDataHash 即协签消息） | rp id hash、UP=1、按策略 UV=1、计数器单调 |
| `WebAuthn { rp_id_hash, origins, user_verification }` | `authenticator_data ‖ SHA-256(client_data_json)` | `type = "webauthn.get"`、`challenge = base64url(协签消息)`、`origin ∈ origins`，其余同 FIDO2 |

签名为 64 字节 `r‖s`（DER 由客户端转换）。`hardware` 由客户端声明，钱包无法核实；策略 `hardware_cosigners` 为真时不接受未声明硬件的协签人。

## hub 激活

1. `quote_activation_preview(account_id)`（query）：hub 只读自己的定价、当前版本与该账户已占用的创建数，返回价格、cycles、账本和每个套餐的剩余额度；不写报价、不占配额。客户端另行读取自己的套餐与账本手续费，所以“获取报价”是快速查询，套餐额度为 0 时显示剩余 0 而不是报错。
2. `quote_activation(account_id)`：hub 向 commerce `refresh_entitlement` 读取账户套餐（无记录为 Free），按 `quotas` 减去该账户已占用的创建数；报价 15 分钟有效，收款账户为 `(hub, deposit_subaccount(hub, quote_id))`。按当前账本手续费，收入的两笔转账付不出时不出报价。额度已用完时仍返回报价（`remaining = 0`），由 `create_wallet` 拒绝，用户能看到套餐限制。
3. 付款人向收款账户转入 `price_e8s`；创建者设备批准 `WalletCreate`。
4. `create_wallet(CreateWallet { quote_id, spec, approval_id, approval })`：调用者必须是报价时的调用者；hub 核对批准与余额，返回后重查报价期限、退款、暂停与配额，记为 `Paid`，再创建 canister（controller 仅 hub，冻结阈值 365 天，附 `cycles_to_wallet`）并以分块安装报价中的版本，状态 `Created → Ready`；创建 canister 失败为 `Failed`。安装中断时，付款人以同一请求再调 `create_wallet`，或任何人调 `reconcile_activation`，都会继续这次激活。配额在此时再次核对，`remaining = 0` 的报价不能创建钱包。
5. `claim_activation_refund(quote_id, to)`：报价调用者领取未预留余额，扣当前账本手续费后转到 hub 之外的账户；失败/过期款、成功钱包的超额款及迟到款都适用。尚未归集的报价金额与未发送的收入腿仍被预留。每次退款固定目的地、费用、memo 和时间，未知结果复用原参数；已完成退款后再收到的款项形成新的一笔退款。客户端保留已付款报价以继续发现迟到款，不能因创建或退款完成而删除记录。
6. `collect_revenue` 只归集 `quote.price_e8s`，不归集子账户全部余额。`get_activation_reserve` 返回尚需预留的金额。`abort_activation` 仅供报价调用者终止 Paid/Created 且尚未安装代码的激活；空实例删除后释放配额并允许退款，已经安装的实例不能通过此路径退款。
7. `collectable_activations(after, take)` 列出待归集订单，任何人可执行归集；固定报价内的 `cycles_share_e8s` 以 memo `TPUP` 转入 CMC，扣除手续费后的报价余款转入 `revenue`。

## 升级与锁定

钱包以 `AcceptUpgrade { version, module_hash }` 管理类提案（24 小时延时）接受 hub 登记的版本；任何人调用 hub `upgrade_wallet`，hub 读取 `accepted_upgrade()`、核对版本与哈希后停止实例、分块安装（`post_upgrade` 参数为该版本号，匹配时清除接受记录）并启动；停止失败时立即重新启动实例；读不到 `accepted_upgrade()` 时先启动实例再报错。`LockCode` 生效后 `lock_wallet` 把 controllers 设为空，之后不能升级或删除。hub 只安装治理以 `admin_register_version` 登记过哈希的模块；上传由 controller、governance 或其指定的上传者完成。已退役的版本只有在未过期报价与未完成激活均不再引用时才能由 `admin_remove_version` 删除，版本号不复用。hub 持久维护版本引用，安装前重查模块；安装成功但回复丢失时按该激活唯一控制的实例状态继续。

## cycles

`refuel()` 任何人可调：策略 `refuel` 规则下余额低于 `min_cycles` 且当日次数未满时，从指定 ICP 地址以 ICRC-1 memo `TPUP`（`0x50555054` 小端 8 字节）转入 CMC 的实例子账户并 `notify_top_up`。`Refuel` 意图做同样的事但需要批准。看门狗以 hub 的 `list_wallets(after, take)` 遍历实例。

## 向量

[tests/vectors/wallet](../../tests/vectors/wallet/README.md) 固定本合同的字节：`protocol-v1.json` 给出每种承诺（提案、决定、加入、激活、协签消息、收款子账户、审计事件）的 CBOR 值、完整原像与摘要，以及各链地址、Taproot 叶与控制块和收款脚本；`plans-v1.json` 给出钱包为各类外链意图构建的计划、签名与已签交易，客户端解码器须全部接受，并拒绝其中的篡改样例。两份文件由 Rust 实现生成，每次测试逐字节比对。

## 桌面与本地协议

桌面结构化方法、多设备协签包、增量审计和加密频道提示的实现及边界见 [团队钱包桌面实现](../dMsg_wallet_zh.md)。客户端在创建付款前完成 `WalletCreate` 的 user home 核验；协签包导入只收集签名，不自动执行。


## 2026-10-24 安全修订

- hub 的升级和锁定共用每钱包互斥锁。执行前调用仅 hub 可用的 `claim_code(expected)` 原子领取已接受记录，领取期间实例不能接受另一次升级或锁定；成功升级清除领取，失败并重启后经 `release_code()` 释放。清空 controllers 前确认实例 Running。
- `AddAddress` 取公钥返回后重新验证授权版本、期限、冻结、地址容量和全部协签引用，普通 ChainKey 协签与原生 Taproot 都覆盖。
- 未执行额度预留不按批准时间自动过期；只有取消、失效、确定无出金等明确结果才释放。签名中断且没有已签交易时恢复预留，不能借失败执行跨日绕过额度。
- ICRC 已尝试发送的项在重试收到 `TooOld` 时保留 Executing 作业与原转账参数，返回 ExecutionUnknown；从未发送的项首次即被 `TooOld` 拒绝时明确失败，保留批量已付款区块。`reconcile_transfer(proposal, item, block)` 读取指定账本的 ICRC-3 记录，逐字段验证付款账户、收款账户、金额、手续费、memo、created_at_time 和 spender，再补记区块；未经证明的“不存在”不能被当作再次付款许可。
- 客户端批准与协签通过 `get_proposal_certified` 认证完整提案，再按新认证钱包版本与当前时间计算过期/延时展示；仅重算自带 digest 不再作为权威提案读取。
- 原生 EVM 与 ERC-20 预览、执行复用最终调用参数估算 gas，仍由实例强制执行费用上限。

部署顺序：先通过现有 hub 登记并升级钱包实例，再更新使用 `claim_code` 的 hub，最后更新客户端。旧版未绑定目标的取消/释放提案须重新创建。此处描述源码合同，不表示已部署生产。


## 策略用量与本地工作流

`policy_usage() -> Result<Vec<SpendUsage { asset, reserved, executed }>>` 是遵守钱包读取权限的只读 query：reserved 为仍有效的已批准未执行预留，executed 为过去 24 小时开始执行的金额，两者使用与批准分档相同的窗口逻辑。它不是资产余额，也不替代批准/执行时的重新校验。

客户端的 `wallet/queue` 与 `wallet/history` 完整读取保留的提案，认证钱包审计头用于校验读取前后和失效缓存；列表不作为批准权威。`wallet/notes` 仅在本机当前账户加密保存记账文本，不改变 Intent、提案摘要、成员批准或链上 memo。`wallet/cosigner_test` 只签名并验证随机的域隔离本地挑战，不导出签名、不构造链交易。客户端结果展示区分流程结束、目标回复、账本确认与未经钱包独立核验的外链回报。
