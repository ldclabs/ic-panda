# dmsg_user

ICP 上的主体控制服务：认证绑定、设备能力、恢复、内容根承诺和敏感执行批准。主体独立于名称和当前登录 Principal。

## 接口

完整签名以 [dmsg_user.did](dmsg_user.did) 为准，类型在 [dmsg_types](../dmsg_types/README.md)。

- 创建和读取：`create_account`、`my_account`、`get_account`。
- 账户变更：`begin_auth_binding`、`mutate_account`，绑定设备批准与预期版本。
- 恢复：`request_recovery`、`get_recovery_request`、`reconfirm_recovery`、`complete_recovery`。
- 正式执行：`sign`、`derive_root`、`get_execution`、`reconcile_execution`、`prune_executions`。
- 认证查询：`security_snapshot_batch`、`get_device_bundle`、`get_execution_receipt`。
- 受限协作：名称授权核销及付款 offer 验证，只接受配置中的对应 canister。

`get_account` 返回 `AccountInfo`，不公开预算计数、执行窗口或内部授权记录。正常身份认证不自动批准新增设备或正式签名。频道、profile、联系列表和请求草稿不在此处存储。

## 账户与初始化

`UserInit` 指定 environment、固定 issuer_namespace、home_cose 及协作 canister、账户/每日创建配额。账户类型 `AccountId` 直接复用 `ic_auth_types::Xid`（12 字节），`AccountInfo` 和 schema 3 安全快照都返回 issuer；设备和操作 ID 仍是 32 字节。

配置直接持久化 `ic_auth_types::XidGenerator` 及完整命名空间 digest，私有模块 `xid.rs` 负责命名空间校验和错误映射；创建校验完成后在同一无 await 消息提交账户、认证索引、配额和计数器。重复认证创建返回原账户，拒绝不消耗编号。RemoveAuth 和恢复完成会删除被移除 Principal 的认证路由，它们随后可以创建或绑定其他账户。时钟回退继续计数；容量或时间溢出明确失败。升级校验完整命名空间 digest，不重置发号器。当前限固定单 home，未来多分配器必须先排除 5 字节指纹碰撞。

## 调用顺序

读取账户 issuer、当前设备序号、安全版本和选定公钥指纹，构造 `SignRequest`，用 `dmsg_protocol::SignRequestExt` 计算规范执行请求及批准摘要，由设备签署后调用 `sign`。执行授权前的拒绝返回 Err。授权后，传输确定未发出或 COSE 尚未记录终态的业务拒绝也可能返回 Err，但原 grant、执行序号和商业预留仍保留；调用者必须先查询/对账同一 request_id，不能自动换 ID 重签。只有 COSE 已记录的 Failed 才释放商业预留；ResultExpired 表示历史结果已清理，结算时仍计入原月已用额。

根派生使用独立 `DeriveRootRequest`，明确区分当前根和候选槽。设备撤销后新写必须等待换根。调整正式签名政策会失效旧批准及候选根槽，但保留当前内容根的写入状态。政策的日 cycles 上限与默认值均为 800,000,000,000，即 `FORMAL_DAILY_CYCLES`。恢复延迟不限制恢复码对已有离线备份的解密能力。

`get_execution_receipt` 仅向账户认证 caller 返回执行认证叶，绑定 issuer、设备、批准上下文、待签摘要、公钥指纹和签名摘要。叶路径及客户端认证规则见 [公开协议](../../docs/protocol/README.md)。它不把 request_id 加回通用 Statement。升级重建认证叶，清理结果时删除对应叶。

## 实现组织

`api.rs` 负责入口和异步协调；账户、执行、恢复规则与内部 `state.rs` 分离；`store.rs` 持有有类型的稳定表和认证视图。执行记录按 `(account_id, request_id)` 单条访问，普通账户读取不载入历史载荷。账户仅保存最多 64 项的保留索引；新正式签名在保留索引达到 56 项时暂停，根派生可继续使用余下槽位：`None` 固定未终结记录，时间戳表示终态结果可清理的时间。新授权在通过全部检查后原子更新预算、序号、索引和执行表；回调重新读取单条记录，只更新发生变化的结果及其认证叶。

schema 9 的私有 `stable_codec.rs` 递归使用 CBOR 整数 map key，并省略空窗口、空授权和 `None` 字段；公共账户、批准与认证编码不变。稳定 memory ID 只属于本实现。满设备、操作、名称授权和执行索引的旧 schema 4 测试样本编码为 17,642 字节；schema 5 另含创建时间、商业授权和安全预算；其中新增保留索引以少量常驻字节换取不扫描历史载荷，样本不包括独立执行表、认证树和稳定表节点开销。

账户操作和已终结执行的幂等重试不重复写入。同一名称 intent 通过新的设备批准可以续期；重放原批准不延长期限。名称服务负责业务去重。第三方应用批准保存在独立的有界记录中。月账仅保留用量计数、算法权重和完整权益响应的摘要；完整权益时间线在刷新时校验，不随每次预留或结算重复存储。测试月账样本不足 128 字节，不含稳定表节点开销。

普通文档签名在商业缓存有效时只做一次本地授权；AppAction 还需远程业务核验及回调复查。确需跨服务刷新时，仍在调用前预检查，返回后重读时间、账户与执行记录。显式 `refresh_execution_entitlement` 始终获取当前商业权益，并保留当月已用和预留额度。每次认证叶写入都重新发布根，消息结束时以最后一次为准；升级重建整批设置叶后只发布一次。`reconcile_execution` 对 COSE 查询的传输失败直接返回错误，不改写执行记录。

## cycles 实测

2026-09-23 使用 PocketIC 16.0.0、相同 release/wasm32 配置和工作区依赖，对比 `917a3ac` 与本次修复。单账户分别保留 0、4、8 条已完成 Ed25519 签名，每条正文 4 KiB，每次 max_cycles 为 80B，使九次批准可处于 800B 日预算内。记录 user canister 的调用前后余额差。

| 操作 | 修改前 cycles | 修改后 cycles | 变化 |
| --- | ---: | ---: | ---: |
| 新签名，商业缓存有效，8 条历史 | 59,477,880 | 53,720,925 | -9.7% |
| 重取已完成签名，8 条历史 | 20,244,808 | 19,850,729 | -1.9% |
| 首次签名，需获取商业权益，0 条历史 | 64,264,134 | 64,463,012 | +0.3% |

主要收益来自商业缓存命中时省去重复授权，以及缩小月账重写范围；首次获取权益的路径仍需两次权限检查和完整权益验证，成本基本持平。

该测试不包含 COSE canister 和管理 canister 的阈值签名费用，也不是整条签名链路或生产账单的节省比例。基准使用确定性本地时钟和单账户，不能推断分片吞吐。可对各自的 Wasm 目录重复运行：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane user_cycles_profile -- --ignored --nocapture
```

实现遵循 ICP 的[稳定结构](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[性能测量](https://docs.internetcomputer.org/guides/canister-management/optimization/)与[跨 canister 回调](https://docs.internetcomputer.org/guides/security/inter-canister-calls/)开发指引。仍需单独验证的容量边界：认证树驻留 heap，升级按账户、执行记录和历史月账总量重建；未实施稳定内存认证树或分批重建。终态结果在后续有效授权时惰性清理，也可调用 `prune_executions(account_id)` 独立释放闲置账户的过期结果。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_user
```

真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。

商业账户批准与正式执行月账见 [commerce contract](../../docs/protocol/commerce.md)。初始化必须固定 commerce 和 membership canister；月账独立持久化，恢复/根派生不消费商业单位。`get_execution_usage` 和认证用量查询只允许账户本人。

内容根安全派生独立限制为每天 20 次 / 300,000,000,000 cycles，按每次批准的 max_cycles 预留且未知结果不退回；单次硬上限为 100,000,000,000。此开发调整支持约 68.3B cycles 的实测 vetKD 派生完成初始化、设备读取和必要换根；实际部署费用仍需核对。四次 70B、第五次拒绝、重放不重复扣额及跨日恢复已有回归测试。

商业执行 grant 的预留字段采用递归紧凑表示，未携带预留时省略。确定未发出的跨 canister 执行保留原授权、执行序号和商业预留供同请求重试；不会误标 Unknown，也不会覆盖并发返回的执行结果。

`control_plane/user.rs` 覆盖早期 COSE 拒绝后的序号补齐、跨月历史结果对账、名称续期、政策变更及商业回调期间的并发冻结。`user_upgrade_profile` 对 1/16/64 个账户及最多 12 个已刷新月份测量升级 cycles 和稳定内存，并验证升级后的认证用量；该小样本不构成生产容量验收：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane user_upgrade_profile -- --ignored --nocapture
```

同环境对比中，64 个账户、64 条月账的升级总 cycles 从 7,405,017,311 降至 7,032,438,568；768 条历史月账时从 18,206,890,495 降至 17,536,817,436。总数包含模块安装和认证树重建开销。上述样本的稳定内存均为 42,008,576 字节，受 MemoryManager 分桶分配粒度影响，不能据此推算生产存储节省比例或最大可升级账户数。


## 第三方应用批准与认证

`approve_authentication` 签发独立认证叶，`authentication_certificate` 只允许当前账户本人读取。
认证绑定完整产品 challenge、临时 session key、目的、origin、receiver、app 配置版本及账户 epoch，
最长五分钟，依赖证书另需六十秒新鲜度。已交付证明的撤销传播以此有界窗口为准，不宣称即时撤销。
`approve_application` 将批准账号与产品 beneficiary 分离；固定服务通过
`verify_application_authorization` 重新检查精确批准。设备需要 FormalApprove，不要求读取秘密库，
不消费正式文档签署月额度。冻结、争议、撤销、陈旧序号和配置不匹配会拒绝新的批准。

应用/产品配置由固定 commerce canister 的治理登记提供。批准前后均检查设备，外部 await 后重读
时间、账户和操作记录；成功提交序号、批准记录及认证叶。申请限额是每账号 32 条未过期操作、
每 UTC 小时 60 次成功批准；拒绝和相同批准重取不消耗成功额度。`prune_external_approvals`
每批最多处理 64 个账号，只清除过期批准，不解除商业承诺。

这部分已通过原生测试和真实 PocketIC Wasm 的升级、重放、暂停、跨主体批准与证书验证。
当前公开入口为 `approve_application`、`verify_application_authorization`、
`authorize_product_billing` 和 `verify_product_account`，v2 checkout/membership 的公开产品互操作已有测试。
真实外部产品、扩展端到端和生产部署仍需独立验收。
精确字节和验证边界见 [integration](../../docs/protocol/integration.md)。


## 2026-10-02 审查修复

- `complete_recovery(account_id, request_id)` 绑定原恢复操作；账户只保留最近一次完成的请求 ID 与恢复 caller。原 caller 重试返回成功，不再更改设备、认证路由、版本或预算；同 schema 升级后仍可重试。新恢复不能复用最近完成的 ID。重复提交待恢复请求按已存有效期限判断，不重新要求一个完整延迟窗口，也不延长已有等待期。
- `begin_auth_binding` 在全局 1024 条容量已满时回收过期记录，保留尚有效的绑定；无需等待外部维护调用。`prune_auth_bindings` 仍可用于提前清理。
- 固定服务身份、第三方批准容量与小时限额，以及 Agent 事件/controller/nonce 检查在外部调用之前执行；await 返回后重新检查当前账户、序号、时间和 principal。预检只构造候选预算与 nonce，实际批准与商业预留仍在最后的同步执行段提交。
- 授权阶段复用不可变签名载荷与 Agent 事件的一次解析结果，保留返回结果校验；重放直接使用载入的执行记录，controller 注册预检不再复制整份账户。
- `prune_executions(account_id)` 是公开维护入口，每次最多检查 64 项保留索引，返回删除数量。只删除超过保留期限的终态结果及认证叶，保留 Authorized/Executing/Unknown、设备序号、操作回执和历史月账；不重复结算。删除后的 `get_execution_receipt` 返回可核验的不存在证明，`get_execution` 仍返回 `ResultExpired`；原样重放同一执行请求也返回 `ResultExpired`，同 ID 不同参数仍返回 `IdempotencyConflict`。删除让空间可复用，不承诺 stable memory 物理缩小。
- 升级继续整批构造认证叶并只发布一次根。对逐项流式插入进行了同配置对比，实测分配高水位反而增加 128 KiB，因此未保留该候选优化。升级日志记录重建完成时的指令数与 Wasm 线性内存页数；后者是分配高水位，不是存活对象的精确 heap 大小。月账及未清理的外部批准仍参与全量重建，生产容量边界需要独立负载验证。

本次开发稳定布局为 schema 9；安全快照仍为 schema 3。使用新开发实例，不兼容旧实验布局。恢复完成的 Candid 参数与扩展生成绑定已同步。


### 本次同配置测量

对比基线 `917a88e3` 与本次修复，使用相同 Cargo.lock、完整 canister 构建命令、release/wasm32 配置和 PocketIC 16.0.0。每个版本建立新实例并用自身 Wasm 升级。以下只统计 user canister，未包含 COSE/管理 canister 的阈值签名费用。

| 操作 | 修改前 cycles | 修改后 cycles | 变化 |
| --- | ---: | ---: | ---: |
| 首次文档签名，4 KiB，刷新商业权益 | 65,267,931 | 64,276,174 | -1.5% |
| 文档签名，8 条历史，商业缓存有效 | 54,473,388 | 54,241,849 | -0.4% |
| 重取文档签名，8 条历史 | 22,027,619 | 22,010,726 | -0.1% |
| AppAction，商业缓存有效 | 63,285,385 | 62,315,778 | -1.5% |
| AppAction 已完成重放 | 12,866,215 | 12,860,365 | 基本不变 |
| controller 注册 | 34,036,863 | 34,010,008 | -0.1% |

64 个账户、768 条历史月账与 2048 条当前认证叶的最终批量重建，记录 2,975,053,139 条重建完成前指令，Wasm 内存分配高水位 4,456,448 字节。升级总 cycles 为 25,917,775,295 → 25,954,770,824（+0.1%，包含模块安装及其他修复），stable memory 均为 50,397,184 字节。

逐项流式候选的高水位为 4,587,520 字节（+128 KiB），升级总 cycles 为 25,947,529,468，相对最终批量方式仅减少约 0.03%。提前释放临时记录的复测仍无内存改善，因此最终保留原批量方式，不为消除中间数组引入新的批处理框架。历史月账仍需保留以处理跨月责任，本次没有加入新的归档或分片机制。
