# dMsg canisters：开发、接口与运行边界

本目录说明开源控制服务的实际实现和验证方式。内部 PRD、Worker 实现、生产凭据及用户数据不属于这些工件。

## 组件

| 包             | 职责                                                             | Candid                                                   |
| -------------- | ---------------------------------------------------------------- | -------------------------------------------------------- |
| `dmsg_user`    | 稳定 subject、认证绑定、设备能力、恢复、根承诺 CAS、敏感执行授权 | [dmsg_user.did](../src/dmsg_user/dmsg_user.did)          |
| `dmsg_handle`  | 全局名称、冻结快照预留/认领、PANDA 注册、双方确认转移、认证解析  | [dmsg_handle.did](../src/dmsg_handle/dmsg_handle.did)    |
| `dmsg_cose`    | 固定 key home、生产 key 校验、受限签名/vetKD、执行去重与成本上限 | [dmsg_cose.did](../src/dmsg_cose/dmsg_cose.did)          |
| `dmsg_payment` | 固定报价托管、账本入金核验、结算/退款互斥、可恢复出金 outbox     | [dmsg_payment.did](../src/dmsg_payment/dmsg_payment.did) |
| `dmsg_types`   | 公开 Rust/Candid 类型、确定性编码、验签、稳定存储和账本适配器    | [源码](../src/dmsg_types/src/lib.rs)                     |

所有新实例与旧 `ic_message`/Profile/Channel/COSE 独立。这里没有频道 ACL、内容 grant、消息、文件、profile 正文或定期 checkpoint 更新入口。普通内容同步不调用这些 canister。

## 构建和验证

工具链：Rust 1.97.1、`wasm32-unknown-unknown` target、Node.js 22+、`candid-extractor` 0.1.6、PocketIC server 16.0.0。依赖版本由 `Cargo.lock` 固定。

```sh
rustup target add wasm32-unknown-unknown
cargo install candid-extractor --version 0.1.6 --locked
make build-dmsg

# 可复用已安装的 PocketIC；未指定时测试库下载其固定版本。
POCKET_IC_BIN=/absolute/path/to/pocket-ic make test-dmsg
```

`build-dmsg` 生成四个 Wasm 和相应 `.did`；`test-dmsg` 运行模型测试、Clippy、Wasm 构建、Candid 一致性检查、Rust/JavaScript 编码与验签向量、PocketIC 联调。可以设置 `CARGO_TARGET_DIR` 与 `DMSG_WASM_DIR`；后者仅指定测试读取 Wasm 的位置。

PocketIC 测试通过 `dmsg_integration/pocketic-tests` feature 显式启用，普通 `cargo test --workspace` 不要求预先构建 Wasm。测试用 `dmsg_test_ledger` 包不在 `dfx.json` 中，含公开铸造与故障注入入口，不能作为真实资产账本部署。

测试覆盖根 CAS、失败回滚、候选根代不复用、设备能力、批准重放/载荷替换、恢复争议与预算；以及真实 Wasm 跨 canister 调用、认证树、Ed25519 阈值验签、vetKD 解封/错误 transport 和 input、名称认领/转移/注册扣款、重复收据、转账提交后丢失响应、BadFee 修复、超额款、直接到期退款和升级恢复。资金模型另跑生成的入金/退款序列守恒检查。

这些是本地实现证据，不代表完成生产账本适配验收、真实费用测量、Chrome/Worker 联调、容量压测或独立安全审计。

## 编码和调用约定

- dMsg 业务时间戳统一为 ICP 共识时间的 Unix **毫秒**，在 canister 入口以 `ic_cdk::api::time() / 1_000_000` 向下取整；时长也使用毫秒。`SECOND = 1_000`、`MINUTE = 60_000`、`DAY = 86_400_000`；恢复延迟和保留时长字段分别为 `delay_ms`、`recovery_delay_ms`、`retain_ms`。金额和 cycles 为整数。
- ICP 原始时间、证书 `/time`、身份委托期限和 ICRC 账本时间仍遵循平台的**纳秒**约定。证书时间在校验后换算为毫秒；账本外层 `block.ts` 在适配器中向下取整为毫秒后参与业务比较。ICRC `created_at_time` 及 `TransferLeg.created_at_time` 保留纳秒，用 `millis_to_nanos` 检查溢出并转换；账本重试和对账始终保留、比较原始纳秒值。
- `Hash`、`SubjectId`、`OpId`、摘要、设备 ID 和固定公钥均使用 `serde_bytes::ByteArray<32>`，Candid 表示为 `blob`，Rust 解码校验固定长度。客户端的 CBOR 字节值使用 `Uint8Array`，不能用普通数字数组代替。
- 签名使用 RFC 8949 core deterministic CBOR。map key 按编码字节排序；拒绝重编码后不一致的正式载荷、未知字段/版本和超长输入。
- CBOR 格式并不规定 Rust `[u8; 32]` 必须是整数数组：Serde 默认按通用数组序列化，需要通过 `#[serde(with = "serde_bytes")]` 或 `serde_bytes` 包装类型显式标记字节语义。这里用 `ByteArray<32>` 统一覆盖字段、元组、`Option`、集合及 map key，使固定字节值编码为 CBOR 字节串（32 字节以 `58 20` 开头）；`ByteBuf` 和 Principal 也编码为字节串。外部 ICRC `Account` 的 `subaccount` 通过 `ledger::account_cbor` 适配，名称付款摘要使用 `charge_terms_digest`。Candid 与 CBOR 的编码约定应分别核对。[Serde 字节编码说明](https://docs.rs/serde_bytes/latest/serde_bytes/)
- `digest(domain, value) = SHA256(CBOR([1, domain, value]))`。设备签署的是 `approval_message` 返回的 32 字节摘要，使用严格 Ed25519 验签。
- [`protocol.rs`](../src/dmsg_types/src/protocol.rs) 定义批准摘要：目标 canister、subject、操作域、device、security epoch、单调 sequence、request ID、expires_at、命令摘要全部绑定。
- [`protocol_vectors.json`](../src/dmsg_types/tests/protocol_vectors.json) 和 [`verify-dmsg-vectors.mjs`](../scripts/verify-dmsg-vectors.mjs) 是独立 Rust/JavaScript 互操作向量，包含毫秒时间戳、固定字节值及其容器、ICRC 子账户、大整数和 Principal；向量里的固定 seed 仅用于测试。

账户变更签名域为 `dmsg/account/v1`，命令值为 `[expected_version, AccountCommand]`；执行域为 `dmsg/execute/v1`，命令值为 `[ExecutionKind, max_cycles]`。正式执行的 request_id 必须通过 `execution_request_id(subject, security_epoch, device_id, device_sequence)` 计算，即 `digest("dmsg/execution-request/v1", [subject, security_epoch, device_id, device_sequence])`；不能自行选择随机 ID。user 检查设备序号，cose 校验同一绑定并保留全局执行水位，结果清理后不能使用新序号复用旧 ID。初始设备 PoP、设备新增、恢复登记/核对/请求/再确认、名称、报价和受理收据各有独立域，准确字段顺序以对应 model 和测试为准。

## 用户、设备和恢复

`create_subject` 验证实际 caller 与初始管理员设备 PoP，再从管理 canister 取得随机数，原子建立 subject 和认证路由。重复登录返回已有 subject，不给新设备自动授权。

`mutate_account` 使用 `expected_version` 和逐设备连续 sequence。每次成功变更保留有界操作回执。普通 `ContentSign` 设备不能登记根、添加设备或批准正式签名。`begin_auth_binding` 由新 Principal 自己调用，已有管理员再签署绑定的 subject/nonce；单一 user home 是认证路由权威，移除/恢复后也保留原 Principal 的路由，避免变成另一主体。

默认最多保存 16 个当前/近期设备描述、8 个认证绑定、64 个操作回执和 64 个执行记录。新设备加入时可回收最旧的已撤销设备描述；历史授权证据须随客户端导出保存。过期的待绑定记录可通过 `prune_auth_bindings` 有界清理。

设备新增/撤销和控制风险变更提升 `security_epoch`，使当前 vault 进入 `RekeyRequired`。`reserve_root` 和 `commit_root` 是 `AccountCommand` 变体，均经 `mutate_account` 调用。每个候选根取得独立、单调递增的 generation；超时、撤销和失败的 generation **不复用**。根提交校验 slot、当前 epoch、预期已提交代、固定 home_cose、恢复代和 bundle 摘要。根 bundle 字节及 VRK 始终由客户端/密文存储负责。

恢复 key 登记要求独立 PoP，恢复核对也需其签名。恢复请求绑定 subject、恢复 nonce、当前恢复代、新认证 Principal、新设备和有效期；默认延迟 24 小时，允许 1–7 天策略。公开认证安全叶包含恢复 nonce、公钥、延迟和待恢复记录摘要。`get_recovery_request` 仅允许当前认证主体或已提交申请的新 Principal 读取待恢复记录，新申请人不会因此获得完整 Subject 管理资料。第一份有效设备争议冻结高风险操作；申请人取得并验证争议摘要后，以 `RecoveryConfirmation { request_id, dispute, expires_at }` 和恢复 key 签名调用 `reconfirm_recovery`。签名域为 `dmsg/recovery-reconfirm/v2`，绑定 home、subject、恢复 nonce、原请求和该确认。新的有效期必须覆盖从再确认开始的完整延迟，且不超过当时起 14 天；不受原申请剩余时间截断。确认仅生效一次，同一确认重试和重复争议都不延长等待。待恢复期间旧管理员不能替换恢复政策来静默取消请求。完成恢复撤销旧设备，保留执行去重状态，并要求内容换根。

`security_snapshot_batch` 最多 64 叶、256 KiB，返回 ICP certificate 与逐叶 witness；`get_device_bundle` 的设备集合须重新计算 `devices_root` 并匹配安全叶。验证者必须核对 ICP 信任根、canister ID、证书时间、witness、schema 和已知较高 epoch；缓存截止最多为证书时间后 60 秒。响应不证明云端内容历史的最新性。

## 正式签名与 vetKD

先创建四个 canister ID，再按 Candid 配置互相引用的 home。`dmsg_cose` 的 `executing_canister` 必须等于自己的 ID。生产环境仅接受各启用算法的 `key_1`，并要求预先确定的非零 public-key fingerprint；通过管理接口预取公钥时必须显式绑定这个新 canister ID。Local/Staging 的测试 fingerprint 可为空，但这不构成生产根证据。

安装后由 controller 调用 `initialize_keys`；所有配置公钥验证通过才进入 Ready。失败不会降级，也不会生成或替换用户根。升级可以无参数或传相同 CoseInit，修改 environment、key name、执行位置、版本或已固定配置会拒绝升级。

适配层直接使用锁定版本的 `ic-cdk-management-canister`，没有继承旧 COSE namespace 权限。只有配置中的 user home 能登记 subject、发出 `ExecutionGrant` 或读取受限执行结果。客户端经 `dmsg_user.authorize_and_execute` 进入签名，user 在 await 前完成设备/政策/序号/预算检查并保存授权。

当前正式 schema 支持 `FileAttestation` 与 `Statement`，用途隔离且 key generation 固定为 1。载荷含 subject、request_id、准确 origin/audience、期限及结构化正文。允许规范 HTTPS origin 和正式格式的 Chrome extension origin。Ed25519 签署规范 CBOR；BIP340 和 ECDSA secp256k1 签署其 SHA-256 摘要。每种算法必须单独配置后才能使用。

正式路径为 `dmsg/formal/v1`、CBOR(environment)、subject、CBOR(purpose)、generation 的大端 8 字节；管理 canister 还隐含绑定执行 canister。vetKD context 为 CBOR(`["dmsg/content-root/v1", environment, 1]`)，input 为 CBOR(`[subject, generation]`)。只允许派生当前根或有效候选 slot，transport key 也包含在批准里；服务端会检查 transport key 的 BLS G1 压缩编码、子群和非单位元；扩展仍须验证并解开 encrypted VetKey。

默认每主体每天最多 20 次执行、`10^12` cycles 的预留预算；政策上限 100 次和 `10^12` cycles，单请求 max_cycles 不超过 `10^11`。user 按用户批准上限预留，cose 再检查并记录实际管理调用成本；失败和 Unknown 不自动退还预留额度，以防重试重新花费同一预算。全局预算由 CoseInit 硬限制。

结果按 subject/request ID 去重，caller 不参与唯一性域；有限序号窗口保留未完成空洞和连续终结水位。并发重试、超时、升级都不会发起同一操作的第二次管理签名。迟到的 pending 回调不能覆盖完成结果。`reconcile_execution` 可查询或补交同一已授权操作；过期且未开始的操作只终结序号，不重新签名。确定性管理拒绝和未发出的调用进入 Failed；响应损坏或真正未知的结果仍保留 Unknown，不能擅自重签。cose 已清理结果并返回 ResultExpired 时，user 同样保存 ResultExpired 终态，后续可以回收该槽位。

`Identity`/`ProviderController` 类型预留在合同中，但当前执行入口明确拒绝；普通身份认证使用设备签名。Agent Protocols/alink 的固定版本、双控制证明与互操作验收完成前不开启 provider controller。没有 wallet/raw signHash/任意派生路径入口。

## 名称快照与收费

新名称销售默认关闭。治理提供经过冻结与对账的旧名称快照后，调用 `begin_legacy_snapshot`，再按规范化 handle 升序分批 `import_legacy_handles`。每批最多 256 条；相同记录重传幂等，冲突拒绝。初始 accumulator 为 32 个零字节，每条按 `digest("dmsg/legacy-entry/v1", [previous, reservation])` 更新；数量和最终摘要全部匹配才能 `seal_legacy_snapshot`。

未认领旧名始终预留。`claim_legacy_handle` 同时要求冻结 owner 的实际 caller 和目标 subject 的一次性接受授权；若 owner 是旧名称 Principal，必须匹配冻结的独立管理员 Principal，名称账户普通委托登录不能认领。隔离条目拒绝自动激活。`snapshot_certified` 和分页 `list_legacy_reservations` 支持核验导入承诺；快照来源依然需要治理核验，导入者不能自行把清单称为旧 canister 的认证证明。

名称规则保持 ASCII 小写字母/数字/下划线、最长 20 字符、不能以下划线开始。PANDA 价格保持长度 1/2/3–4/5–6/7+ 对应 100万/20万/5万/2万/5000 PANDA；账本精度 `10^8`，与旧服务一样从标价扣除固定账本 fee 后转入服务。

注册先由 user 批准 `HandleIntent`，再 `reserve_handle` 原子锁定名称和主体操作，最后 `commit_handle`。新的 spender 是 dmsg_handle，旧 allowance 不迁移。付款参数和 memo 固定；业务 `created_at` 为毫秒，发送 ICRC 请求及对账时转换为固定的纳秒 `created_at_time`。未知扣款保持锁定，支持原参数重试/Duplicate 和指定账本块对账；不会到期后自动再出售。只有未发起扣款的普通预留可到期释放，旧名称预留不会释放。

`transfer_handle` 要求双方主体针对同一名称、版本、op_id 的授权。转移只改变 HandleRecord 和独立名称事件链；不会改变旧身份、subject、内容 key 或任何应用权限。旧 `ic_message` 的 canister ID、原 myIV、测试根和数据没有被本实现改写。

## 支付与对账

PaymentInit 固定单一受支持 ledger、user home、平台账户、服务费和网络 fee 上限。`open_escrow` 要求实际付款 caller、专用 signer 的 Quote、匹配受益账户/净额的 PaymentOffer，以及 user 对 offer 设备能力和安全版本的新鲜核验。验证前有硬预算，await 后重查本地 signer、期限、报价唯一性与未付款上限。用户可以直接调用 `list_my_escrows` 找回订单。

每单独立 subaccount；默认 `fund_by=created_at+15 分钟`，`accept_by=fund_by+30 分钟`。`check_funding` 通过固定账本的 `icrc3_get_blocks` 及其返回的归档 callback 核验 `1xfer`/`2xfer` 记录（btype 优先，缺失时才使用旧 tx.op），使用外层区块 `ts`，不采用付款者提供的交易创建时间。不支持该账本 schema 的资产不得启用。

一笔符合原出资账户、金额及 `floor(block.ts / 1_000_000) < fund_by` 的转账成为主入金；适配器输出的 `committed_at`、订单 `funded_at` 和期限均为毫秒。这与原始纳秒时间严格早于毫秒截止时刻等价；恰好到达截止时刻的入金属于迟到款。不合并多笔少付。每个 block 只认领一次，多付、额外付款和迟到入金归其实际原始出资账户。`finalize_receipt` 在已保存入金基础上本地提交唯一结算方向；`now < accept_by` 才能结算，`now >= accept_by` 任何人可提交 `expiry_refund`。未知入金也能先决定退款，迟到查询继续走退款。

`FundsDecision` 是资金方向，**不是到账完成**。固定转账腿保存在 outbox，通过 `process_transfer` 发送；Duplicate 归并为同一成功。Pending、明确失败及 Unknown 的重试均保持原始所有转账参数，也可用 `reconcile_transfer` 查询指定真实账本块，绝不刷新未知转账的 timestamp。只有付款方或本次收款账户的控制者能调用 `revise_rejected_transfer`。BadFee 修复必须使用账本已返回的 expected_fee，其他明确拒绝只能保持原 fee、更新时间戳；不能随意选择一个注定失败的手续费。BadFee 增量只消耗批准上限内的原 fee reserve，不减少 recipient_net。每条出金链最多保留 8 个当前/已拒绝版本，较早的 Superseded 版本折叠到 history_digest；成功、在途和 Unknown 记录不参与清理。leg_id 始终递增，清理不会使旧 ID 再次可执行。无法承担费用时保持 FeeBlocked。

`claim_deposit_refund` 处理实际来源的少付/多付/迟到款及到期主入金；`claim_fee_reserve` 在受益出金完成后处理余量。每次修复与出金都保持：

```text
confirmed_in = liabilities + transferred + network_fees
```

不足一笔 fee 的余额保留为可查询负债，不视为收入或已清算；当前版本没有跨订单合并微额退款。`list_transfers` 对实际保留记录分页，允许已清理版本产生的 ID 间隔。入金/出金对账与出金发送共享每分钟最多 400 次 ledger RPC 的硬预算。关闭订单或吊销 signer 不关闭查询、对账和到期退款。收据证明服务签过受理承诺，不能证明 Worker 持续可用或执行了最新屏蔽规则。

## 稳定布局与发布前工作

本轮毫秒时间、CBOR 字节编码、执行 ID 及恢复确认参数属于首版开发合同调整，配套客户端必须同步 helper、Candid 绑定和互操作向量。签名域和正式 schema 仍沿用首版版本号，但不能据此混用旧编码。固定字节值的编码变化会改变批准摘要、执行 ID、认证叶、事件链和快照承诺，旧签名与报价必须重新生成，不能静默重放。

正式 key 的原始字节派生路径保持不变；vetKD 的 `CBOR([subject, generation])` 则因 subject 改为字节串而改变，派生出的内容根 key 也会变化。已有开发根必须用旧输入解封后显式换根并更新 bundle，不能仅替换服务端编码。稳定存储版本升为 `STABLE_SCHEMA = 2`，四个 canister 在升级入口拒绝旧版本状态；尚未提供自动迁移器。旧开发实例需重建，或先完成保留原签名证据、根解封能力和账本原始转账参数的显式迁移。

所有状态按记录写入 `ic-stable-structures`，不依赖 pre_upgrade 序列化整个堆。各 canister 的 stable table memory ID 固定在其 `lib.rs`；后续版本只能显式迁移，不能复用 ID 或把旧服务状态直接装入新实例。认证树在升级后由稳定记录重建；大规模状态的升级指令量/内存仍须实测。

当前只支持单一认证分配权威和固定 user/key home。没有在线 user 分片迁移、双 home 写入或自动 key 迁移接口；不能通过部署另一个未接入权威的 user 实例宣称完成横向扩容。

生产启用需要继续完成：指定 ledger/归档部署的实际转账、去重窗口与 fee 演练；所有启用算法的主网 key 描述/fingerprint 核对；真实扩展和私有 Worker 协议联调；恢复包与旧样本迁移验收；性能、容量和独立审计。旧系统 Draining/ReadOnly 升级、OSS token 撤销和正式 cutover 是另一个受治理控制的迁移交付，当前提交未冻结旧生产服务。现阶段不要把这份新控制服务实现标为完整 v1.1 已上线。

平台合同依据：[ICP 管理接口](https://docs.internetcomputer.org/references/ic-interface-spec/management-canister/)、[ICRC-3](https://github.com/dfinity/ICRC-1/blob/main/standards/ICRC-3/README.md)、[PocketIC](https://github.com/dfinity/pocketic/blob/main/CHANGELOG.md)。
