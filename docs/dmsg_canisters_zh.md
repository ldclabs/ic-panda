# dMsg canisters：公开接口与参考实现

[English](dmsg_canisters.md) | 简体中文

本目录记录实际实现。公开协议及语言无关的字节规则见 [protocol/README_zh.md](protocol/README_zh.md)，声明的 CDDL 见 [statements.cddl](protocol/statements.cddl)。内部设计依据的定位方式见 [AGENTS.md](../AGENTS.md)。

## 商业化接口增量

新增共享 `membership`（PANDA 资格、独占、持久产品决定）及 `dmsg_commerce`（现金订单、合同、退款、资源租约）。实际协议、权限、生命周期和验证边界见 [commerce_zh.md](protocol/commerce_zh.md)。user 新增精确商业批准、真实账户创建时间及独立 UTC 月执行账；COSE 隔离正式签名与安全操作预算；delivery 使用版本化费率和 v2 报价/收据域。旧名称价格不受影响。

当前实现采用共享会员与 dMsg 产品商业服务分离的结构，共六类 canisters。扩展商业 UI 已完成本地接线，TokenList 项目与独立账号 adapter 已实现；生产钱包/SNS 验收单独执行，不将本地测试视为上线证明。

COSE schema 8 区分仍在途、已返回未知和确定终态：未知结果保留原请求与商业占用，但不再阻止后续终态清理。明确未发出的管理调用返回失败及成本 0，并撤回预算预留；结果清理不解码旧正文，固定签名派生前缀在初始化和升级时缓存。入口统一验证固定 user home，公开 Candid、派生路径与密钥身份保持不变。执行结果的 `cycles_cost_upper_bound` 是管理调用的保守成本上界，不是实际账单。详见 [COSE 实现与实测](../src/dmsg_cose/README.md)。

## 子库

| 包 | 职责 | 文档 |
| --- | --- | --- |
| dmsg_types | 公开数据合同，包含基础签名、ICP 接口和可选应用协议 | [README](../src/dmsg_types/README_zh.md) |
| dmsg_protocol | 确定性编码、批准构造、标准 COSE 验签、CTT 摘要 | [README](../src/dmsg_protocol/README_zh.md) |
| dmsg_runtime | 参考实现共用的稳定记录、认证树、账本和调用工具 | [README](../src/dmsg_runtime/README.md) |
| dmsg_user | 主体、认证、设备、恢复、根承诺与执行批准 | [README](../src/dmsg_user/README.md) / [Candid](../src/dmsg_user/dmsg_user.did) |
| dmsg_cose | 标准签名产物、受限 vetKD、密钥来源和执行状态 | [README](../src/dmsg_cose/README.md) / [Candid](../src/dmsg_cose/dmsg_cose.did) |
| dmsg_handle | 名称权属、导入、收费和转移 | [README](../src/dmsg_handle/README.md) / [Candid](../src/dmsg_handle/dmsg_handle.did) |
| membership | PANDA SNS 资格、跨产品占用与授益决定 | [README](../src/membership/README.md) / [Candid](../src/membership/membership.did) |
| dmsg_commerce | 套餐、现金订单、退款与认证资源权益 | [README](../src/dmsg_commerce/README.md) / [Candid](../src/dmsg_commerce/dmsg_commerce.did) |
| dmsg_payment | 固定条款托管、入金验证、互斥资金决定与出金 | [README](../src/dmsg_payment/README.md) / [Candid](../src/dmsg_payment/dmsg_payment.did) |
| dmsg_directory | Agent Delegation principal 文档的认证 HTTP 发布 | [README](../src/dmsg_directory/README.md) / [Candid](../src/dmsg_directory/dmsg_directory.did) |

扩展与云端的公开 wire 合同见 [cloud_zh.md](protocol/cloud_zh.md)。P0 已加入设备命令、HTTP PoP、完整设备证据桥与真实扩展/PocketIC/workerd 的 profile 互操作探针；A1 已接入账户/设备/恢复和根提交 UI，见 [账户与根合同](protocol/account_root_zh.md)；A2 已接通内容同步、冲突/墓碑及完整密文导出。其它配套文档保留的早期描述应按明确的版本修订核对。

## 当前合同

- Statement v3 设计已实现三个文档 profile v1（纯文本、纯摘要、针对文件的文本声明），执行批准域为 `dmsg/execute/v3`；其他独立域的准确版本见协议说明和向量。与早期实验编码不兼容。
- `sign` 输出 `SignedArtifact { cose_sign1, cose_key }`：RFC 9052 COSE_Sign1 和 public-only COSE_Key。ICP 密钥来源另存于结果的 `key` 描述，不能把公钥查询本身当作身份/授权证明。
- 内部账户的 `AccountId` 直接复用 `ic_auth_types::Xid`（12 字节），用户服务使用共享 `XidGenerator` 同步原子发号；初始化增加固定 issuer_namespace，密码派生版本为 2，使用新开发实例。
- `get_execution_receipt` 提供认证执行叶，绑定请求 ID、待签字节、公钥和签名；请求元数据不再进入可移植 Statement。
- `get_account` 返回 `AccountInfo`，支付返回 `EscrowInfo`。内部预算、去重窗口、ID 分配器不进入这些视图。
- `dmsg_types` 不含稳定存储、认证树或网络调用。各 canister 使用自己的 `store.rs`、StableCell 和有类型的 StableBTreeMap；用户和 COSE 执行记录独立保存。
- 四个 canister 的稳定布局使用独立 compact representation：结构字段以显式 CBOR 整数 map key 保存，稀疏可选字段省略；标量、tuple 和原始字节索引保持原编码。该 representation 只存在于 `dmsg_runtime::stable_types` 和各 canister 私有 `stable_codec.rs`，不改变 `dmsg_types` 的公共 CBOR、签名摘要、认证叶或 Candid。`dmsg_user` schema 7 进一步采用有界执行保留索引，普通账户操作不扫描历史执行载荷；实测及容量限制见其 README。
- 文本签署原始 UTF-8，摘要签署 RFC 9995 Hash Envelope；issuer/subject 使用标准 CWT 文本语义，kid 可变长，BIP340 入口已删除。浏览器消息合同为 `dmsg-extension/4`。
- 付费投递的 Quote/AdmissionReceipt 位于公开的 `profiles::delivery`，它们不是所有签名实现必须支持的基础类型。
- payment 对开单与查账中的重复请求返回 `Pending`，内部退款/费用修订不重复更新未变化的认证叶。有界配置和预算在 heap 中更新，初始化及 `pre_upgrade` 写入 StableCell，因此升级不可跳过该 hook；资金记录仍直接保存到稳定表。cycles 实测和认证树重建的容量边界见 [payment README](../src/dmsg_payment/README.md)。

## 构建与验证

需要 Rust stable、wasm32-unknown-unknown target、candid-extractor 0.1.6、didc 和 Node.js；PocketIC server 与测试 crate 固定为 16.0.0。依赖以 Cargo.lock 为准。

```sh
rustup target add wasm32-unknown-unknown
make build-dmsg
pnpm --dir src/dmsg_app bindings
POCKET_IC_BIN=/path/to/pocket-ic make test-dmsg
pnpm --dir src/dmsg_app check
pnpm --dir src/dmsg_app test
```

`test-dmsg` 检查 Rust 测试、Clippy、Wasm、Candid 一致性、独立 Rust/JavaScript 协议向量与 PocketIC 调用。测试账本支持故障注入和公开 mint，仅用于测试，不在 dfx.json 中。详见 [集成测试说明](../tests/dmsg_integration/README.md)。

## 状态与保证

六类 canister 分别维护账户批准、名称权属、固定密钥执行、投递资金终态、共享会员资格和产品商业权益。账户批准先本地提交再跨 canister 执行；管理调用之前保存执行状态。未知结果查询原请求，不能自动新建请求重签或刷新未知转账的时间戳。设备撤销、恢复争议、根 CAS、结果清理后的重放保护、结算/退款互斥和资金守恒继续由本地状态机执行。

稳定布局版本由各 canister 的 store.rs 自己维护，使用新实例联调，不读取此前 schema 的开发状态。`dmsg_handle` schema 7 使用 StableLog 保存事件；名称锁和账户锁只在注册扣款进行中或结果未知时存在，未开始扣款的请求不会占住名称。冻结旧名改为普通查询，由认领在链上重新核对，不进入堆上认证树；只认证快照承诺和活跃名称。保留新订单手续费维护和小分配桶；cycles 对比和容量边界见其 [README](../src/dmsg_handle/README.md)。整数 key 与代表样本字节由 round-trip、大小阈值、StableBTreeMap 分配和 SHA-256 golden 测试固定。相同 schema 代码升级后的执行恢复由 PocketIC 覆盖。认证树继续使用公共协议编码并由稳定记录重建，因此 compact stable representation 不改变认证响应。

生产部署须固定六类 canister ID；共享 membership 可复用经过核验的权威实例，再用各自 Init 参数配置引用。所有 user home 与 handle/cose/payment/directory 的 environment、issuer_namespace 必须一致且固定；各服务按账户 ID 内嵌的分配器指纹把账户路由到分配它的 home，新 home 须经各服务的 `admin_add_user_home` 登记，指纹不得碰撞。COSE 由 controller 或 governance 初始化并核对生产 key 与 fingerprint；公钥未就绪不接受执行，不能降级为测试根。生产 ledger/归档、扩展完整批准流程、私有服务协议、容量和审计仍需单独验收。

签名产物可在非保护头 270 携带不透明的 RFC 9921 CTT token，验证只限制其大小，不检查其可信性。没有 TSA 网络客户端、CMS/X.509 信任验证、完整证据包归档或 anchor_snapshot 入口；普通签名成功不表示已取得时间戳。频道/profile/普通 grant/消息/文件正文不在这些 canister 中存储，也没有周期 checkpoint 写入。

## 2026-09-22 客户端接线增量

账户新增 `SetDeviceCapabilities`，由现有管理员的精确批准更新能力，保留设备公钥/角色，防止移除最后的根管理员，并推进安全版本及必要换根。支付新增配置、报价签署钥和费用政策的认证读取；旧托管的条款与原资金决策保持固定。

共享认证模块在生成查询证明时读取一次 batch time，避免 replica 缓存返回长期不变的旧证书；新鲜度与防回退窗口未放宽。新增真实扩展集成验证覆盖正式签名、现金/SNS 客户端、付费来信、频道换代/历史/文件/设备撤销、旧快照与共享继承。使用合成账户、账本和旧密文样本，仍不替代正式 origin、真实旧用户或真实资金验收。

共享 runtime 的紧凑适配器直接保存 representation，避免表写入前的领域对象克隆；商业预留与费用政策也使用整数键。该次开发布局为 user schema 6、payment schema 5；后续版本见各服务实现及下文增量。认证批量响应检查完整 Candid 成功响应的 256 KiB 上限，包含证书与封装。

2026-09-23 payment 使用 schema 6：授权尝试与成功开单额度分开；默认认证查询按时间选择当前费率；controller 在固定上限内维护预期网络费，旧报价和已准备出金保持冻结。预算更新不重算配置认证叶，历史费率表使用紧凑表示，稳定内存改为 1 MiB 分配桶。转账增加有界 `last_failure` 诊断；容量和 cycles 结果见 payment README。

2026-09-25 payment 使用 schema 7：出金腿的并发锁改为内存占用键，升级或回调 trap 丢失账本回复后仍为 `InFlight` 的腿可用冻结参数重发，由账本去重；`Superseded` 腿返回 `VersionConflict`。开单只约束 user 观察时间与本地时间的差值，入金块时间不再与本地时间比较。付款方索引只保存键，出金块索引保存 `(escrow_id, leg_id)`，升级重建认证树只发布一次根哈希。

2026-09-25 membership 使用新的开发布局（配置单元合并服务配置与每小时申请计数，申请表改为 memory 1–4），不读取旧实例。移除 USD 补贴预算：`PandaRatePolicy`、`ProductRegistration` 与 `PandaQuote` 不再含补贴字段，`set_panda_subsidy_budget`/`panda_budgets` 删除；`max_claims` 只计仍占用神经元的申请。会员到期 E 取报价固定的 offer 终点，神经元最早解锁不得早于 E。同一申请一分钟内复用资格观察；固定 governance 模块时经 `canister_info` 同时核对模块 hash 与唯一 controller 为 SNS root。每分钟调用计数只在 heap 中，不再有 `pre_upgrade`；申请只在索引、占用或公开视图变化时更新对应表和认证叶。

2026-09-25 commerce 使用 schema 4：配置单元只保存服务配置与调用预算，初始目录只在目录表中；暂停状态立即持久化，不读取旧实例。升级沿用原年度起点，升级差价和存储包按完整年度折算；过期存储包不占 64 个上限。适配器在账本或账户调用之外的交付失败都写入确定拒绝回执，结算方无需等到激活截止即可退款；预留时适配器依赖暂不可用则订单保持 `Reserving` 以便对账重试。共享调用预算在确认存在需要外部调用的记录后才扣减，`verify_settlement_asset` 仅限治理调用。已知拒绝的出金腿可由接收者以原 fee 或账本给出的 fee 重发，恢复超过 24 小时才派发的腿。SNS 调用失败与 membership 返回不可核验同样暂停付费执行时间，已知失格视图带修复截止时间。只影响内部状态的订单保存不再重算认证叶，重建认证树只发布一次根哈希。

`max_claims` 包含尚未对账完成的 `Applying` 申请，不使用到期索引长度代替占用数量。占用计数随申请状态更新，升级时在重建认证树的同一次遍历中恢复；Apply 回执丢失不会腾出新申请容量。PocketIC 回归覆盖回执丢失后的准入拒绝、升级恢复、成功对账继续占用及承诺到期后的容量释放。

2026-10-02 membership 修复产品预留暂时失败被永久拒绝、费率发布重试和旧版本复用问题；激活必须使用冷却结束后的资格观察。恢复调用按全局/actor 限流，并阻止同一申请的并发产品调用；SNS 配置核验合并在途请求。费率逻辑独立到 `rate.rs`，申请表、索引与认证维护集中到 `store.rs`，完整记录未变化时不写稳定表。

新的开发布局在 memory 5 保留最高政策版本，在 memory 6–8 增加终态保留、幂等摘要和读者索引；不迁移旧实验布局。终态在申请截止及终态时间两者较晚值之后保留 30 天，再有界压缩；未知 Apply 和未到 E 的占用不清理。完整记录和历史操作分别设置 10 万/100 万防护上限。实际返回语义、原生 1,000/10,000 条样本、索引的存储开销及未验证的生产容量边界见 [membership README](../src/membership/README.md)。

公开实现仍采用固定 offer E、无提前退出、终止不可恢复的 commerce 2 合同。私有目标设计与此不一致的部分待单独修订设计基线；本次不据旧提案恢复已移除的生命周期分支。私有设计定位继续由 AGENTS.md 维护。

## 2026-09-29 Agent Delegation 增量

新增 `dmsg_directory`，按 [agent_zh.md](protocol/agent_zh.md) 发布 principal 文档。user schema 8 新增 principal 稳定表（memory 8）与 `principal_updated_at`，`SecuritySnapshot` 升为 schema 3；账户命令新增 principal 启用、托管 controller 登记/退役/泄露/改名，新增 `register_controller`、`sign_agent_event`、`publish_principal`、`get_principal`。默认 `SensitivePolicy` 与 `SetPolicy` 允许 `AgentController`（上限 4 个用途）。COSE 新增 `KeyPurpose::AgentController`、`ExecutionKind::AgentEvent` 与 `ExecutionOutput::AgentSignature`；Agent 事件计入正式签名的商业额度与预算，不产生执行回执认证叶。

`dmsg_protocol` 依赖 crates.io 的 `agent-protocols =0.10.0`（`default-features = false`）做严格 I-JSON、JCS 与 Agent Delegation 校验。本地 release 构建中 user Wasm 由 3,771,117 增至 4,244,709 字节，cose 由 2,165,748 增至 2,412,506 字节，directory 为 1,496,953 字节（未经 ic-wasm shrink）。directory 容量与升级重建开销尚未实测，home 迁移尚未实现。


## 2026-10-02 user 审查修复

`dmsg_user` 开发布局升级到 schema 9，安全快照保持 schema 3。恢复完成绑定请求 ID 并保留最近完成回执；绑定容量不足时回收过期项；固定服务 caller、外部批准配额与 Agent 本地授权在 await 前检查，回调后复查。新增公开 `prune_executions(account_id)`，按最多 64 项索引清理过期终态并返回数量，保留未终结执行、防重放状态与历史结算；执行回执查询可返回认证的不存在证明，原样重放已清理的执行请求返回 `ResultExpired`。

执行路径拆分只读预检与同步提交，复用不可变解析结果；controller 注册不再预先复制整个账户。认证树保留实测占用更低的整批重建方式，只发布一次根；逐项流式候选未保留。具体边界、回归及同配置 cycles 比较见 [user README](../src/dmsg_user/README.md)。开发接口与扩展绑定一起更新，不读取旧实验布局；生产部署与大规模容量仍未验收。

## 2026-10-02 handle 审查修复

schema 7 用内存调用 guard 区分执行中的扣款和升级遗留的 `Charging`，后者使用原 ledger 参数恢复；对账与重试共用 guard，保留此前扣款的不确定性。认领和转移保存精确请求回执，原认领重放不受后来名称转移影响。配置只读路径借用已解码值，变更仍同步持久化；操作索引改用固定 32 字节键。公开 Candid 和认证叶不变。恢复、对账证据、并发回归及活跃名称容量测量见 [handle README](../src/dmsg_handle/README.md)。

## 2026-10-02 payment 审查修复

payment 使用 schema 8：授权、查账、出金预算分别按调用方和全局限流；预留覆盖结算最高网络费，退款支持最多 32 笔同源入金与已释放预留合并，并提供入金分页/退款预览。总订单容量限制只阻止新开单；去重索引保留，归零开放计数删除。实现、测量和真实资产验证边界见 [payment README](../src/dmsg_payment/README.md)。

## 2026-10-02 directory 审查修复

directory schema 2 持久化文档 SHA-256 摘要，发布摘要与 HTTP 查询复用摘要并转移正文缓冲区；升级逐条读取稳定记录构建认证树。home 与目录共享保守的 64 KiB 文档预算，预留后续退役、泄露标记和最大名称空间，超预算注册在 home 提交前被拒绝。公开配置 URL 增加长度边界；HTTP 路由与认证库采用相同的路径规范化规则，principal 权威解析仍要求精确的规范 URL。公开 Candid 不变；开发期拒绝 schema 1。

定向单测、PocketIC 回归及可复现的 [directory profile](../src/dmsg_directory/README.md) 覆盖这些改动。35,360 字节文档的本地样本中，HTTP 复制查询 cycles 降低 31.87%；130 条记录升级后的 Wasm 内存由 6,356,992 降至 1,966,080 字节，升级 cycles 降低 15.14%。这些样本不代表最大容量或网关吞吐。本次未运行 dMsg 全套验证脚本。


## 2026-10-03 commerce 审查修复

commerce 开发布局升级为 schema 5：出金、对账与修订共用在途 guard，升级遗留的 `InFlight` 仍以冻结参数恢复；开单精确匹配价格权威的历史快照，支持仍有效的旧报价。目录生效边界终止旧资源租约，Free 月额度按实际政策时间线分段；当前资源公开字段保留，账户内部移除重复调用状态并裁剪已结束的历史月份。

新增商户读权限、订单/出金读者索引、完整入金游标分页，以及每 caller 的恢复预算。稳定存储和认证维护集中到 `checkout_store.rs`，账本回复分类在纯模型中，配置采用 1 MiB 分配桶及紧凑订单/出金记录。已结清终态保留后有界归档，保留原输入摘要、报价、账务、回执、读者索引和入金去重；迟到款恢复原订单原路退款，未知结果与未清资金不归档。验证命令、存储和性能样本的实际边界见 [commerce README](../src/dmsg_commerce/README.md)。

## 2026-10-06 handle 治理与收入提取

handle 开发布局升级为 schema 8：`HandleInit` 增加固定的 SNS `governance`，快照导入/封存、`update_ledger_fee` 和新增的 `admin_collect_token` 接受 controller 或 governance 调用。每个管理方法都有同参数的 `validate_*` query，按当前状态预演并返回提案说明，可登记为 SNS 通用函数的验证方法。`import_legacy_handles` 增加快照位置 `offset`，乱序执行的批次被拒绝，不会留下缺口；注册收入可用 `admin_collect_token` 从 handle 默认账户提取。7–20 字节名称价格改为 100 PANDA，整张价格表与旧 `ic_message` 现行价格一致。认证树只保留名称叶，移除没有调用方的 `snapshot_certified`；按升级指令实测，单实例活跃名称上限从 100,000 提高到 150,000（升级约用 300B 上限的 55%）。部署、治理和迁移流程见 [handle README](../src/dmsg_handle/README.md)。

## 2026-10-06 handle 千万级名称与多 user home

handle 开发布局升级为 schema 9，作为全局唯一的名称注册表承载千万级名称：认证树改存在 stable memory，名称按 `handle_bucket` 分进 2^20 个桶，桶号位组成二叉标签树，写入只重算一个桶和一条路径，升级只重新发布根哈希；名称的证明路径变为 20 个桶号位标签加名称。`HandleInit` 以 `environment`、`issuer_namespace` 和只能追加的 `user_homes` 取代单个 `home_user`，按账户 ID 的分配器指纹把授权核对发给账户所在的 user home，跨 home 转移分别核对；新增 `admin_add_user_home` 及其预演。分配桶改为 8 MiB，stable 可寻址 256 GiB。实测 1,000 至 1,000 万名称的升级约 115 万指令，64 名称证书响应不超过 81 KB，活跃名称上限提高到 1,000 万。数据与步骤见 [handle README](../src/dmsg_handle/README.md)。

## 2026-10-06 多 user home 与 SNS 治理

cose、directory、payment 与 handle 一样按账户 ID 的分配器指纹路由 user home：`CoseInit` 以只增不减的 `user_homes` 取代 `initial_home_user`，`execute`/`get_execution` 只接受账户所属 home 的调用；`PaymentInit` 以 `environment`、`issuer_namespace`、`user_homes` 取代 `home_user`，开单时把 `verify_payment_offer` 发给收款账户的 home，认证配置叶升为 schema 2 并改列 `user_homes`。commerce 的 `user_homes` 上限提高到 64；membership 的 user home 来自 commerce 应用登记，不需改动。新的 dmsg_user 分片只需在各服务登记，并用 `register_integration_app` 提交列有新 home 的应用登记新版本。

user、cose、directory 新增固定的 `governance`。七个 canister（user、handle、cose、directory、payment、commerce、membership）的管理方法统一接受 controller 和 governance，每个都有同参数的 `validate_*` query，按当前状态执行与方法相同的检查并渲染提案说明，可登记为 SNS 通用函数的验证方法。新增管理方法：各服务的 `admin_add_user_home`，user 的 `admin_set_account_limits`（账户上限与每日新建配额），cose 的 `admin_set_daily_budget`，directory 的 `admin_set_custom_domains`；这两个 canister 的升级不再读取参数，配置只经管理方法修改。cose 的 `prune_executions` 改为公开维护，与 user 的同名入口一致。稳定布局：user schema 10、cose schema 9、directory schema 3、payment schema 9。

前端仍只连接一个 `dmsg_user`；按账户指纹选择 home 的客户端路由、跨 home 的认证身份唯一性和已有账户迁移尚未实现。验证范围见各 canister README。
