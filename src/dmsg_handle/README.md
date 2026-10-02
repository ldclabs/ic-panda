# dmsg_handle

名称权属的独立 ICP 注册表。handle 是到稳定主体的映射，转移名称不转移账户、内容、签名身份或外部应用权限。

## 接口与流程

完整接口见 [dmsg_handle.did](dmsg_handle.did)。先通过用户服务批准具体 `HandleIntent`，并给 handle canister 足额 ICRC-2 allowance，再调用一次 `register_handle`：同一调用内核对授权、加锁并扣款，成功即提交权属。金额和 fee 已由 `terms_digest` 固定，approve 可以在注册前完成。转移由双方针对同一名称、版本和操作 ID 分别批准；handle 通过 user 的固定双意图接口一次验证两份授权，回调后仍重新检查权属版本。

`resolve_handle_certified` 返回名称的 ICP 包含或不存在证明；验证者核对 canister ID、信任根、certificate、witness 和所需新鲜度。名称事件独立编号，可通过 `get_handle_event` 查询。

## 收费和导入

ASCII 规范化、PANDA 定价及旧名称导入属于此注册表的业务规则，不是基础签名协议。新注册在旧快照封存前关闭。导入分 begin/import/seal；未认领旧名保持预留，争议记录不自动释放。冻结管理员会作为认领 caller，导入时拒绝匿名和管理 canister principal。

扣款前固定付款账户、金额、fee、memo 和账本纳秒时间。名称锁和账户锁只在扣款进行中（`Charging`）或结果未知（`ChargeUnknown`）时存在，未开始扣款的请求不会跨消息占住名称，也不占用 `max_pending` 容量。未知扣款保持锁：内存调用 guard 区分正在执行的调用与升级/回调取消后遗留的 `Charging`，`commit_handle` 用原参数重试无运行中调用的 `Charging` 或 `ChargeUnknown`，`reconcile_handle_charge` 用真实账本块对账；不能因为本地超时而再次售卖名称。查询 `get_handle_operation` 可恢复原流程。重试和对账共用同一操作 guard，正在执行时返回 `Pending`；升级不会靠扫描历史操作清理状态。遗留 `Charging` 按已有不确定性处理，后续拒绝也不能证明前次未扣款。

账本明确拒绝（含 `TemporarilyUnavailable`）或本次调用确定未执行时，操作保存为 `Rejected { reason }` 并立即释放锁；该操作 ID 作废，重放返回同一终态，重新注册需要新的操作 ID 和授权。此前未知的扣款不会因为后续拒绝而变成未付款。持久状态只有 `Charging/ChargeUnknown/Committed/Rejected`。历史操作 ID 和终态继续保存，以维持幂等语义。

`get_handle_config` 返回当前部署参数。controller 可通过 `update_ledger_fee` 更新新订单手续费，费用必须低于最低名称价格；ledger 和 home 不通过此接口变更。已存在操作优先按请求摘要返回，费用更新不改变旧订单的金额、fee、memo 或账本时间。授权回调后重新检查最新配置，旧报价需重新准备。

`get_legacy_reservation(name)` 和 `snapshot_progress` 是普通 query。认领在链上重新核对封存状态、调用者角色和 `terms_digest` 绑定的确切冻结记录，伪造的查询结果只会让认领失败，因此旧名不进入认证树。`snapshot_certified` 证明 `_legacy_snapshot` 导入进度；当前权属仍通过 `resolve_handle_certified` 查询。

旧名认领和转移统一保存 `(account_id, op_id)` 的请求摘要与原始权属回执。认领重试仍校验冻结 caller 身份，名称后续转移或同版本升级不会改变原回执；另一个操作 ID 不能借当前 owner 相同而被当作成功认领。

名称拥有者、转移目标和授权引用均为 12 字节 `AccountId`；handle 与 issuer URI 分别维护，名称转移不会改变 Xid。

## 实现

`api.rs` 保存入口与名称状态转换，`store.rs` 保存有类型的名称、操作、锁和事件。schema 7 的私有 `stable_codec.rs` 为配置、名称、导入、操作、事件和权属回执递归使用 CBOR 整数 map key；现有名称摘要和权属认证叶编码保持不变；公开接口及状态以 Candid 为准。部署参数由 `HandleInit` 定义。

- 配置在 heap 中保留已解码值供只读借用，每次变更仍同步写 stable cell；不会跨 await 复用旧的可变配置。操作和权属回执的索引均使用 `[u8; 32]`，省去变长 key 转换。
- 名称事件使用 `StableLog`，序号来自日志长度，配置只保存事件摘要 tip；不再为顺序追加维护 B-tree 和重复计数器。
- 名称锁保存持锁操作的 32 字节 key，扣款回调和对账提交前核对锁仍属于该操作；账户锁是只记录未决扣款账户的集合。没有到期索引和后台回收。
- `MemoryManager` 使用 16 页（1 MiB）分配桶，减少默认每个活跃区域 8 MiB 的预分配。当前库最多 32,768 桶，对应约 32 GiB 可分配空间；这是布局上限，不是容量验收结果。桶不会因为删除记录自动缩回。
- 注册在跨 user 调用前检查全局和账户额度，回调后再次检查，幂等重试先返回已有操作。加锁、`Charging` 操作和配置在发起账本调用前一起持久化；扣款回调只重读一次操作，将事件、权属、最终操作状态和锁释放作为同一个消息中的提交。
- 已完成的快照导入重试与封存重试不重复写配置和认证根。升级时只重建活跃名称和导入进度的认证叶，最后发布一次根哈希。

选择依据包括 ICP 官方的 [Stable structures](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[消息执行原子性](https://docs.internetcomputer.org/references/message-execution-properties/)和[性能测量建议](https://docs.internetcomputer.org/guides/canister-management/optimization/)。认证树仍驻留 heap，升级需遍历所有活跃名称，成本随活跃名称数量增长，见下方容量样本。操作去重回执和事件历史也持续增长，本次未引入可能破坏重试语义的历史删除策略。

## schema 7 容量与恢复边界

2026-10-02，Rust 1.98.1、PocketIC 16.0.0、仓库 release 配置。`handle_active_name_capacity_profile` 从真实初始化并封存的 canister 取得稳定内存，在 host 中用相同 `StableBTreeMap` 和 `CompactStored<HandleRecord>` 填入 20 字节名称，再用正式 Wasm 升级、验证认证查询。它测量活跃名称表的升级和查询成本，未模拟同数量的授权、扣款、事件或回执历史；这些业务路径由公开接口回归和 `handle_scale_profile` 覆盖。

当前准入上限为 **100,000 个活跃名称**。新注册和旧名认领在授权前后均检查 `活跃名称数 + 未决扣款数`，为已发出的扣款保留提交位置。达到上限返回 `QuotaExceeded`，已有名称转移、原操作重放、扣款恢复与对账继续可用。该上限取已实测规模，提升前需重新测量升级预算与内存；不由小样本外推百万名称容量。

升级日志记录 `handle_upgrade names=… instructions=…`，指令数直接来自本次 `post_upgrade` 的 `performance_counter(0)` 差值。64 名称查询的 Candid 字节数包含证书和成功 Result 封装；host 耗时含 PocketIC/HTTP 开销，不能当作主网延迟或吞吐。

| 活跃名称数 | 升级指令数 | 升级 cycles | heap 字节 | stable 字节 | 64 名称响应字节 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 657,682,610 | 4,022,926,949 | 1,572,864 | 3,211,264 | 50,118 |
| 10,000 | 8,636,811,833 | 12,002,056,319 | 3,997,696 | 4,259,840 | 63,038 |
| 100,000 | 106,721,845,931 | 110,087,090,634 | 28,114,944 | 23,134,208 | 75,426 |

上述每个实例另有 1 条未认领旧名。单次 64 名称查询的 host 耗时为 6–19 ms，不代表生产延迟。100,000 名称的测试还验证满容量注册/认领的本地拒绝，以及满容量下的双方授权转移。heap/stable 是已分配量；表内 stable 不包含同等数量的真实收费操作或事件历史。

`handle_cycles_profile` 使用同一 host 程序及 user/ledger Wasm，分别装载修改前保存的 schema 6 与本轮 schema 7 handle Wasm；256 条旧名、逐步注册至 17 条活跃名称：

| 项目 | 修改前 | schema 7 |
| --- | ---: | ---: |
| 首次注册 cycles | 22,700,188 | 22,689,030 |
| 已有 8 次注册后的新注册 cycles | 22,803,734 | 22,761,180 |
| 已有 16 次注册后的新注册 cycles | 22,919,303 | 22,859,037 |
| 同一请求重放 cycles 范围 | 8,358,503–8,429,106 | 8,355,829–8,428,322 |
| 17 活跃名称升级 cycles | 3,308,105,352 | 3,371,517,203 |
| stable 字节 | 8,454,144 | 8,454,144 |
| Wasm 字节 | 1,611,468 | 1,642,673 |

注册 cycles 下降约 0.05%–0.26%，重放基本持平。新增恢复保护、回执及容量检查使 Wasm 增加 31,205 字节，小规模升级总费用增加约 1.9%；未把它描述为全面性能提升。cycles 只统计 handle，升级费用也包含 Wasm 安装等成本，不能替代 hook 指令数。

公开接口规模回归另完成了 `1,000 旧名 + 17 活跃`、`10,000 旧名 + 17 活跃`、`1,000 旧名 + 1,017 活跃` 三组写入、转移和重复升级。最后一组注册为 24,530,642 cycles、转移为 16,319,681 cycles，heap 为 1,572,864 字节、stable 为 9,502,720 字节；重复升级为 4,022,247,361 cycles，hook 为 657,003,021 条指令。

比较用 Wasm SHA-256：修改前 `ad9525aef886ec6c54e9c10776eaeb1385a29109f5c5622deeb678fc38ec77fa`；schema 7 `41c07a0c83b9efe0bbeb389b14783523736a8e6cfbc7a7603deb3a4317619ef0`。

```sh
POCKET_IC_BIN=/path/to/pocket-ic \
  cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane handle_active_name_capacity_profile -- --ignored --nocapture
```

生产部署与真实 PANDA 账本仍需独立验收。若未知扣款已超出账本时间窗口，保留原参数并通过真实交易块对账，不能刷新时间戳或直接释放未知付款的名称。

## schema 6 历史容量样本

2026-09-25，Rust 1.98.1、PocketIC 16.0.0、当前 Cargo.lock 和 release 配置。通过公开导入接口分批写入旧名，再逐个注册活跃名称、转移一次；没有绕过授权的测试播种入口。`dmsg_handle.wasm` 为 1,612,950 字节。

| 项目 | 1,000 旧名 + 17 活跃 | 10,000 旧名 + 17 活跃 | 1,000 旧名 + 1,017 活跃 |
| --- | ---: | ---: | ---: |
| 注册（授权 + 扣款 + 提交）cycles | 23,028,233 | 23,066,848 | 24,562,968 |
| 双方授权转移 cycles | 15,552,195 | 15,554,193 | 16,315,677 |
| Wasm heap 字节 | 1,376,256 | 1,376,256 | 1,572,864 |
| 已分配 stable memory 字节 | 9,502,720 | 10,551,296 | 9,502,720 |
| 单条旧名查询 Candid 字节 | 285 | 285 | 285 |
| 同版本升级 cycles | 3,311,117,793 | 3,311,117,826 | 9,120,749,583 |

旧名不进入认证树，升级费用与旧名数量无关。活跃名称仍在升级时重建：间隔后重复升级，17 个活跃名称为 3,311,117,793 cycles，1,017 个为 3,961,853,643 cycles。临时用 `performance_counter` 测得 1,017 名时 `post_upgrade` 约 6.6 亿条指令（17 名时约 640 万），其中约 94% 用于认证树插入，即每个活跃名称约 65 万条指令。按此线性外推，单次升级 300B 指令上限约对应四十余万活跃名称；树深随规模增加，实际上限更低，需要按目标规模实测。

表中 1,017 名一列的"同版本升级"是长时间运行后的首次升级，另含约 5.2B cycles 的一次性费用：关闭重建后仍然出现，间隔后重复升级不再出现，来源尚未确认。该次升级后立即再次升级会被 install_code 速率限制拒绝，稍后重试成功。

256 旧名 / 17 活跃名称的 `handle_cycles_profile` 中，新注册约 22.7–22.9M cycles，同一请求重放约 8.4M，名称已被占用的本地拒绝约 8.4M，封存重试约 7.2M。cycles 只统计 handle，不包含 user/ledger；heap 和 stable 是分配量，不是有效载荷大小。除上述临时 `post_upgrade` 计数外，没有采集 Wasm 指令数，也不由 cycles 反推指令数。样本不代表生产吞吐、并发延迟或百万名称容量。

```sh
POCKET_IC_BIN=/path/to/pocket-ic \
  cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane handle_scale_profile -- --ignored --nocapture
```

## 验证

2026-10-02，本轮完整 `scripts/test-dmsg.sh` 通过，含 Rust 单元/文档测试、Clippy `-D warnings`、Wasm/Candid 比对、跨语言协议向量、84 项 PocketIC 回归和 26 项 SDK 测试。默认忽略的 10 项 profile 中，3 项 handle profile 已另外运行通过，其余服务的 profile 未在本轮运行。

从仓库根目录运行：

```sh
cargo test -p dmsg_handle
```

真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

`tests/dmsg_integration/tests/control_plane/handle.rs` 额外覆盖 allowance 不足、临时失败和确定拒绝后立即释放名称/账户/全局额度、作废操作 ID 的重放、手续费更新、冻结管理员/隔离名称及匿名管理员拒绝、双授权过期/并发转移，以及批量导入失败无部分写入、重叠导入重试、并发注册只扣一次、额度提前拒绝、锁与日志升级恢复、未知扣款跨时间和升级不释放、名称包含/不存在证明、快照认证叶及查询批量上限。

新增回归覆盖成功/拒绝扣款回复到达前升级、运行中重试与对账互斥、认领回执跨转移/升级重放、错误 payer/收款方/金额/memo/时间戳/spender 的对账拒绝，以及账本去重窗口过期后的并发对账。

测试账本只模拟 allowance 校验/扣减、固定参数去重和 sender 时间窗口；`approve_test` 是测试设置入口，不模拟真实批准手续费与批准事件。

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。schema 7 不读取旧实验布局；公开 Candid 和认证叶格式保持不变。生产部署、容量和真实外部服务仍需单独验收。
