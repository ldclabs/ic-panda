# dmsg_wallet

dMsg 团队钱包：每个团队一个 canister，由 [dmsg_wallet_hub](../dmsg_wallet_hub/README.md) 创建并作为唯一 controller。成员提案、按策略批准，钱包直接用自己的 ICRC 账户转出 ICP 与 ICRC 代币，并用链密钥（threshold ECDSA、BIP-340、Ed25519）为 Bitcoin、EVM、Solana 交易签名，由成员客户端广播。钱包不做 HTTPS outcall。

公开合同见 [wallet_zh.md](../../docs/protocol/wallet_zh.md)，类型见 [dmsg_types::wallet](../dmsg_types/src/wallet.rs)，摘要、策略、地址与协签验证见 [dmsg_protocol::wallet](../dmsg_protocol/src/wallet/mod.rs)，接口见 [dmsg_wallet.did](dmsg_wallet.did)。产品与技术方案见 [产品方案](../../docs/dMsg_wallet_product_2026-10-16_zh.md) 与 [技术方案](../../docs/dMsg_wallet_implementation_2026-10-16_zh.md)。

## 接口

| 类别 | 方法 | 调用者 |
| --- | --- | --- |
| 读取 | `preview_intent`（只读预检）、`get_wallet`、`get_wallet_certified`、`get_proposal`、`get_proposal_certified`、`list_proposals`、`get_plan`、`address_book`、`audit`、`member_ids` | 已绑定的成员 Principal、hub、controller；`policy.public` 时任何人 |
| 运行 | `wallet_stats`、`accepted_upgrade` | 任何人 |
| 代码执行领取 | `claim_code`、`release_code` | 仅 hub |
| 引导 | `bootstrap`（读取三个链密钥根并建默认地址 1–3；安装与升级后由定时器执行，失败每分钟重试） | 任何人 |
| 成员 | `join(MemberApproval)` | 被邀请或已加入的成员 |
| 提案 | `propose(ProposeArgs)`、`cancel_proposal` | 非 Viewer 成员；Recovery 只提成员变更 |
| 决定 | `approve_proposal`、`reject_proposal`、`veto_proposal`（`MemberApproval::Account` 或 `Principal`） | Admin / Approver |
| 执行 | `prepare_execution`、`execute(ExecuteArgs)`、`report(ChainReport)`、`reconcile`、`reconcile_transfer` | 非 Viewer 成员 |
| 冻结 | `freeze()` | Admin / Approver，立即生效 |
| cycles | `refuel()` | 任何人，按策略规则 |
| 维护 | `prune()` | 任何人；终态提案超过 10,000 条时从最旧的开始删除，每次最多 100 条；每个新提案也顺带删除 2 条 |

`inspect_message` 拒绝未绑定任何成员的调用者（hub 及 `join`、`bootstrap`、`refuel`、`prune` 除外）与匿名调用。所有 update 在入口读取一次 caller、本 canister ID 与时间，跨 canister 返回后重读提案、成员、策略与时间。账户成员的 user home 不在钱包记录中时，`join` 与决定向 hub `get_user_homes` 补取一次。

## 状态

| 区域 | 结构 | 说明 |
| --- | --- | --- |
| 钱包记录 | `StableCell<CompactStored<Wallet>>` | 存储版本、设置、成员（≤ 64）、策略、地址（≤ 256）、协签人（≤ 64）、冻结、代码接受、链密钥根、审计链头、当日签名与充值计数、未结算 EVM nonce、Solana 最近 nonce、24 小时支出窗口 |
| 提案 | `StableBTreeMap<u64, CompactStored<Proposal>>` | 另有 `op_id → id` 去重表 |
| 计划 | `StableBTreeMap<(id, seq), PlanRecord>` | 计划、整份计划共用的签名规格与组装所需数据 |
| 执行作业 | `StableBTreeMap<u64, Job>` | ICRC 转账的确切参数、CMC 充值、在途调用与签名；键 0 为策略自动充值 |
| 地址簿 | `StableBTreeMap<canonical(Destination), CompactStored<BookEntry>>` | ≤ 4,096 条 |
| 审计 | `StableLog<CompactStored<AuditEvent>>` | 哈希链，`hash = digest("dmsg-wallet/audit/v1", ...)` |
| 认证树 | `CertMap` | 叶 `wallet`（`WalletView` 的确定性 CBOR）与 `proposal/<id>` |

stable 记录用整数键的紧凑编码：协议类型的表示在 `dmsg_runtime::stable_types::wallet`，钱包记录在 [stable_codec.rs](src/stable_codec.rs)，计划与作业的本地类型直接以整数键派生；认证叶与摘要仍用协议的确定性 CBOR。满载的钱包记录（64 成员、256 地址）约 38.6 KB，成员与地址列表比字段名编码小约三分之一。

heap 只缓存解码后的钱包记录；`post_upgrade(Option<u32>)` 校验存储版本，只在 hub 传入的版本等于已接受版本时清除接受记录，重新认证钱包视图，并在链密钥根或默认地址不全时挂上 `bootstrap` 定时器。

## 执行

- **批准**：支出类提案在每次批准时按当时的 24 小时窗口重算要求，更严则提高批准数与延时（期限随之延长），拆单不能落入低档。未执行金额持续预留，执行/签名时开始计入 24 小时窗口；签名中断且没有已签交易则恢复预留。成员增删、角色/Agent 授权变化和首次加入推进策略版本，使旧未执行提案失效。冻结时只接受 Unfreeze 与成员变更的批准。
- **ICP/ICRC**：同一消息内置 `Executing` 并保存作业，读取账本手续费（超过 `max_fee` 即失败）后固定 `created_at_time`，逐笔 `icrc1_transfer`；批量转账每笔的 memo 为 `SHA-256(proposal_digest ‖ index)`，避免相同收款相同金额被账本去重合并；每笔发出前核对冻结，冻结后批量停在下一笔之前。结果未知时保持 `Executing`，`reconcile` 以完全相同的参数重发。部分转出后被拒的批量、成员回报失败的已签交易仍计入 24 小时窗口。
- **canister 调用**：目标须在 `call_targets`，提案时先核对白名单，再尝试读取目标的 ICRC-21 通用同意文本存入意图。执行前必须满足所有受保护 ICP 地址的协签策略，覆盖直接转账和额度授权等通用调用；签名绑定计划 0 与提案摘要。执行用有界等待调用，回复原样保存（≤ 64 KiB）及其 SHA-256；`SYS_UNKNOWN` 记为结果未知。
- **外链**：`prepare_execution` 从意图构造交易并核对参数（见合同文档），`execute` 核对协签后在开始签名时按摘要数计入当日签名数，再逐个签名，期间冻结即停止，签完重读状态并组装保存 `SignedTx`。重签规则以提案的首个已签计划为准，构建计划时、签名前与签完后各核对一次：签名前先构建的多个计划只有与先签者相容的能再签，两个提案同时为同一 EVM 地址或同一 Solana nonce 签名时后签完者被拒绝。sighash 的整笔交易哈希只算一次，各输入共用。编码器手写：BIP-341/342 与 BIP-143 sighash、bech32/bech32m、RLP、Solana legacy message。
- **出错**：跨 canister 返回后出现的错误把提案退回原状态（签名、取公钥）或记为失败（新地址的协签人已移除），不会留在 `Executing`。同一计划的 `Broadcast` 回报只记录一次。
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
- PocketIC（`tests/dmsg_integration`，控制面 `wallet::*`）：hub 付费激活与配额、成员以设备批准加入、ICP 转账与拒绝、`key_1` 真实阈值签名的 BTC（含同输入替换）、EVM（含 nonce 取消）、SOL 交易（分别用 `bitcoin`、`alloy`、`ed25519-dalek` 验签；先于首签构建的第二个计划在签名时被拒），客户端解码器复核每个真实计划，P-256 协签、冻结与解冻延时（冻结中不能批准地址簿等内部变更）、Recovery 否决与生效、移除协签人所属成员后双控 Taproot 地址仍可走时间锁叶花费、经 hub 升级与锁定、经 CMC 替身充值、过期报价与创建失败（hub cycles 不足）退到付款人账户、收入归集；延时档的外链支出在延时过后可直接构建计划；同时挂起的两笔转账，后批准者按合计金额提高批准数；广播回报幂等；重复 `join` 不写审计；两个提案并发签同一 EVM 地址时后签完者被拒绝。
- 桌面端到端 `desktop_wallet`（`tests/dmsg_integration/tests/desktop.rs`，需私有中继）：两个 daemon 经审批中心创建并付款、加入、批准与执行 ICP 转账，核对地址与审计链；创建失败后以 `dmsg wallet refund` 退回付款身份。

## 未实现

- 审计模式的链上核对（`attest_btc` / `attest_evm` / `attest_sol`）。
- EVM Safe 与 Solana Squads 的链上双控；ICP 账本的 AccountIdentifier 收款地址。
- Taproot 双控的 L3 恢复叶由恢复密钥持有人自行构造并签名交易，钱包不构造这类交易。

## 2026-10-21 桌面接线

新增 `preview_intent` query：按当前成员、冻结、地址簿、滚动窗口和策略返回提案要求，不写状态。桌面、Hold 执行、多设备协签和验收记录见 [dmsg_wallet_desktop_zh.md](../../docs/dmsg_wallet_desktop_zh.md)。ICP AccountIdentifier 在客户端收款视图计算，不新增链上账户。


## 2026-10-24 执行与恢复修订

无 durable nonce 的 Solana 已签提案不允许新 blockhash 重签；只能重广播原交易。持久 nonce 的替换同时绑定首次签名的账户模式与 nonce 值。

EVM 取消和释放须指明原提案与 nonce。成员报告取消已结算不会释放原付款额度，也不会把原付款强行标为失败；旧回报不能影响后续 nonce。

新增 `reconcile_transfer(id, item, block)`：在原作业仍 Executing 时，核对指定账本的真实 ICRC-3 区块，找回未知转账结果。TooOld 不再删除作业或显示为可重做的普通失败。

`claim_code`/`release_code` 仅 hub 可用，领取期间拒绝变更接受记录。新增地址在公钥返回后重新检查所有协签引用与当前授权。详见 [公开合同](../../docs/protocol/wallet_zh.md)。回归入口为 `wallet::review::*`，实际外链用例需显式运行 ignored 测试并提供回环 RPC。
