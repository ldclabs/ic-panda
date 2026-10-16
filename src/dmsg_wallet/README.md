# dmsg_wallet

dMsg 团队钱包：每个团队一个 canister，由 [dmsg_wallet_hub](../dmsg_wallet_hub/README.md) 创建并作为唯一 controller。成员提案、按策略批准，钱包直接用自己的 ICRC 账户转出 ICP 与 ICRC 代币，并用链密钥（threshold ECDSA、BIP-340、Ed25519）为 Bitcoin、EVM、Solana 交易签名，由成员客户端广播。钱包不做 HTTPS outcall。

公开合同见 [wallet_zh.md](../../docs/protocol/wallet_zh.md)，类型见 [dmsg_types::wallet](../dmsg_types/src/wallet.rs)，摘要、策略、地址与协签验证见 [dmsg_protocol::wallet](../dmsg_protocol/src/wallet/mod.rs)，接口见 [dmsg_wallet.did](dmsg_wallet.did)。产品与技术设计见 [misc/dMsg_wallet_product_2026-10-16_zh.md](../../misc/dMsg_wallet_product_2026-10-16_zh.md) 与 [misc/dMsg_wallet_implementation_2026-10-16_zh.md](../../misc/dMsg_wallet_implementation_2026-10-16_zh.md)。

## 接口

| 类别 | 方法 | 调用者 |
| --- | --- | --- |
| 读取 | `get_wallet`、`get_wallet_certified`、`get_proposal`、`get_proposal_certified`、`list_proposals`、`get_plan`、`address_book`、`audit`、`member_ids` | 已绑定的成员 Principal、hub、controller；`policy.public` 时任何人 |
| 运行 | `wallet_stats`、`accepted_upgrade` | 任何人 |
| 引导 | `bootstrap`（读取三个链密钥根并建默认地址 1–3，安装后定时器自动执行，失败可重试）、`refresh_homes`（从 hub 追加 user home） | 任何人 |
| 成员 | `join(MemberApproval)` | 被邀请或已加入的成员 |
| 提案 | `propose(ProposeArgs)`、`cancel_proposal` | 非 Viewer 成员；Recovery 只提成员变更 |
| 决定 | `approve_proposal`、`reject_proposal`、`veto_proposal`（`MemberApproval::Account` 或 `Principal`） | Admin / Approver |
| 执行 | `prepare_execution`、`execute(ExecuteArgs)`、`report(ChainReport)`、`reconcile` | 非 Viewer 成员 |
| 冻结 | `freeze()` | Admin / Approver，立即生效 |
| cycles | `refuel()` | 任何人，按策略规则 |
| 维护 | `prune()` | 任何人；终态提案超过 10,000 条时按序删除，每次最多 100 条 |

`inspect_message` 拒绝未绑定任何成员的调用者（`join`、`bootstrap`、`refuel`、`refresh_homes`、`prune` 除外）与匿名调用。所有 update 在入口读取一次 caller 与时间，跨 canister 返回后重读提案、成员、策略与时间。

## 状态

| 区域 | 结构 | 说明 |
| --- | --- | --- |
| 钱包记录 | `StableCell<Stored<Wallet>>` | 设置、成员（≤ 64）、策略、地址（≤ 256）、协签人（≤ 64）、冻结、代码接受、链密钥根、审计链头、当日签名与充值计数、未结算 EVM nonce、Solana 最近 nonce、24 小时支出窗口 |
| 提案 | `StableBTreeMap<u64, Stored<Proposal>>` | 另有 `op_id → id` 去重表 |
| 计划 | `StableBTreeMap<(id, seq), PlanRecord>` | 计划、签名规格与组装所需数据 |
| 执行作业 | `StableBTreeMap<u64, Job>` | ICRC 转账的确切参数、CMC 充值、在途调用与签名 |
| 地址簿 | `StableBTreeMap<canonical(Destination), BookEntry>` | ≤ 4,096 条 |
| 审计 | `StableLog<AuditEvent>` | 哈希链，`hash = digest("dmsg-wallet/audit/v1", ...)` |
| 认证树 | `CertMap` | 叶 `wallet`（`WalletView` 的确定性 CBOR）与 `proposal/<id>` |

heap 不保存业务状态；`post_upgrade(Option<u32>)` 只在 hub 传入的版本等于已接受版本时清除接受记录，并重新发布认证根。

## 执行

- **ICP/ICRC**：同一消息内置 `Executing` 并保存作业，读取账本手续费（超过 `max_fee` 即失败）后固定 `created_at_time`，逐笔 `icrc1_transfer`；批量转账每笔的 memo 为 `SHA-256(proposal_digest ‖ index)`，避免相同收款相同金额被账本去重合并。结果未知时保持 `Executing`，`reconcile` 以完全相同的参数重发。
- **canister 调用**：目标须在 `call_targets`；提案时尝试读取目标的 ICRC-21 通用同意文本存入意图。执行用有界等待调用，回复原样保存（≤ 64 KiB）及其 SHA-256；`SYS_UNKNOWN` 记为结果未知。
- **外链**：`prepare_execution` 从意图构造交易并核对参数（见合同文档），`execute` 核对协签后逐个签名，期间冻结即停止，签完组装并保存 `SignedTx`。编码器手写：BIP-341/342 与 BIP-143 sighash、bech32/bech32m、RLP、Solana legacy message。
- **新地址**：`AddAddress` 执行时调用 `*_public_key` 取得该路径的公钥。

## 验证

```sh
cargo test -p dmsg_protocol --lib wallet
cargo test -p dmsg_wallet --lib
POCKET_IC_BIN=/path/to/pocket-ic make test-dmsg
```

- `dmsg_protocol` 单测：Taproot 调整与 `ic-secp256k1` 的 BIP-341 签名互验，bech32/bech32m/base58check 参考向量，EIP-55，Taproot 树与控制块，三种协签封装（含计数器、RP、来源与 UV），策略分档、滚动窗口、日限额、Recovery 与管理类要求，审计链。
- `dmsg_wallet` 单测用独立实现核对编码器：`bitcoin` 0.32 核对 P2TR 密钥路径、P2WPKH、双控 Taproot 脚本路径的 sighash、控制块、已签交易、txid 与 vsize；`alloy` 解码 EIP-1559 交易并恢复签名人；`solana-message` 与本实现编译出的 message 逐字节一致，含 ATA 与 durable nonce。
- 向量 [tests/vectors/wallet](../../tests/vectors/wallet/README.md)：`dmsg_protocol` 生成 `protocol-v1.json`（全部承诺的原像与摘要、审计链、地址与收款脚本），本 crate 的 `chains::tests::vectors` 以构建器生成 `plans-v1.json`（11 个计划、确定性签名与已签交易、6 个篡改样例），`dmsg_client` 的解码器逐个验收并拒绝篡改样例。
- PocketIC（`tests/dmsg_integration`，控制面 `wallet::*`）：hub 付费激活与配额、成员以设备批准加入、ICP 转账与拒绝、`key_1` 真实阈值签名的 BTC（含同输入替换）、EVM（含 nonce 取消）、SOL 交易（分别用 `bitcoin`、`alloy`、`ed25519-dalek` 验签），客户端解码器复核每个真实计划，P-256 协签、冻结与解冻延时、Recovery 否决与生效、经 hub 升级与锁定、经 CMC 替身充值、过期报价退款、收入归集。
- 桌面端到端 `desktop_wallet`（`tests/dmsg_integration/tests/desktop.rs`，需私有中继）：两个 daemon 经审批中心创建并付款、加入、批准与执行 ICP 转账，核对地址与审计链。

## 未实现

- 审计模式的链上核对（`attest_btc` / `attest_evm` / `attest_sol`）。
- EVM Safe 与 Solana Squads 的链上双控；ICP 账本的 AccountIdentifier 收款地址。
- Taproot 双控的 L3 恢复叶由恢复密钥持有人自行构造并签名交易，钱包不构造这类交易。
