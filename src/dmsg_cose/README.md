# dmsg_cose

受限 ICP 阈值签名和 vetKD 服务。正式签名输出可独立解析的 COSE_Sign1；原始任意 signHash、任意派生路径和 namespace 权限接口不开放。

## 接口

见 [dmsg_cose.did](dmsg_cose.did)。

| 入口 | 调用者及结果 |
| --- | --- |
| `initialize_keys` | controller 或 governance；核对配置与真实公钥后启用 |
| `admin_add_user_home(home)` | controller 或 governance；追加 user home，见“多 user home 与治理” |
| `admin_set_daily_budget(executions, cycles)` | controller 或 governance；调整全局每日预算，保留当天已用计数 |
| `validate_*` | 与上面三个管理方法同参数的 query，供 SNS 通用提案预演并渲染载荷 |
| `key_state` | 查询当前初始化与根描述 |
| `public_key(account_id, selector)` | 公开查询，离线派生公钥；不证明主体存在或具备执行权 |
| `execute(grant)` | 仅账户所属的 user home；生成签名或 encrypted VetKey |
| `get_execution` | 仅账户所属的 user home；查询原请求 |
| `prune_executions(after)` | 公开维护；每批清理最多 8 个账户的过期终态结果，返回继续游标 |

应用通过 `dmsg_user.sign` / `derive_root` 提交设备批准。`Signature` 结果内的 `artifact` 含标准 COSE_Sign1 和 COSE_Key，`key` 保存 ICP 派生来源。`EncryptedRootKey` 保留独立的 vetKD 结果。

## 多 user home 与治理

`dmsg_user` 按实例分片，多个 user home 可以共用一个 COSE。`CoseInit.user_homes` 列出允许提交执行的 home；每个账户 ID 的第 4..9 字节是分配它的 home 的分配器指纹（`account_allocator_digest(environment, issuer_namespace, home)` 的前 5 字节），`execute` 和 `get_execution` 只接受该 home 对自己账户的调用。密钥派生只取决于 environment、derivation_version 和账户，与 home 无关，所以增加 home 不改变任何已有公钥。

新 home 由 `admin_add_user_home` 追加：列表只增不减，最多 64 个，指纹不能与已有 home 重复，重复添加同一 home 不报错。新 `dmsg_user` 必须以本 canister 为 `home_cose`，并使用相同的 environment 和 issuer_namespace。

所有管理方法接受 controller 和初始化时固定的 `governance`。controller 用于本地和 SNS 之前的部署；登记到 SNS 后，提案执行时由 SNS governance 调用。每个管理方法都有同参数的 `validate_*` query：它按当前状态执行与方法相同的检查，通过时返回给投票者看的说明，否则返回方法将给出的错误；与当前状态相同时说明末尾标注 “no change”。升级不读取参数，也不修改配置。

## 密码实现

`cose2` 负责 RFC 9052 待签结构和封装，`ic_cose_chain_key` 负责管理调用、公钥派生和费用。Ed25519 签 Sig_structure 原字节；ES256K 签其 SHA-256 摘要；dMsg 不再提供 BIP340 入口。文件摘要签署不受云端文件上传上限约束。

`CoseInit` 指定与 user home 相同的固定 issuer_namespace；account_id 为 12 字节 Xid。签署端同时检查 issuer、完整待签结构和实际派生公钥指纹。签名 kid 使用 RFC 9679 SHA-256 指纹，通用文档 profile 的 kid 仍为可变长字节。

所有用途保留固定 key home，正式签名使用 `dmsg/formal/v2` 派生域，derivation_version=2。Production 配置拒绝测试根并核对 fingerprint；升级不读取参数，不能换根；`daily_executions` 和 `daily_cycles` 只经 `admin_set_daily_budget` 调整，保留当天已用计数。执行在管理调用前落盘，未知结果保留原请求，重试不再次签名。

## TSA

产物可在非保护头 270 携带不透明 RFC 9921 CTT token，验证不检查其可信性。当前没有 TSA 网络客户端、TSA 信任验证或 `anchor_snapshot` 入口；这些能力不能由普通签名成功推断。

## 代码

`api.rs` 负责入口和密码调用，`model.rs` 管理有界执行状态，`store.rs` 保存配置、公钥缓存和内部记录。schema 9 的私有 `stable_codec.rs` 使用 CBOR 整数 map key，内部执行元数据直接以自身整数 key 形式保存；执行 grant 增加商业预留，摘要域为 `dmsg/cose-execution/v3`；正式 Statement 和设备执行批准字节不变。

每个 home 只保存最多 64 条执行元数据（request_id、完整 grant 的摘要、过期时间、InFlight/Unknown/Terminal 状态及签名标志），结果正文按 `(account_id, execution_sequence)` 单独存储。执行、回调和查询只访问目标结果，不扫描或比较整个历史窗口的正文，也不重复持久化完整 grant。结果表保存已编码 CBOR，只有读取目标结果才解码；替换和删除仍读取底层字节，但不再解码被丢弃的旧正文。64 条元数据的当前回归样本编码小于 6 KiB；连续关闭高水位和仍在途的空洞共同约束清理及防重放。user home 列表只由配置保存，execute 入口统一核对实际 caller 是账户所属的 home，并等于 grant.home_user，且 home_cose 是本 canister；不逐账户存储 home。

总预算和正式签名预算合并为一个独立 StableCell，和配置共享 memory 0 的既有 128 页区块：配置占页 `[0,127)`，预算占页 `[127,128)`，不额外分配 8 MiB。home 和结果分别使用 memory 1、2。预算更新不重写根公钥配置；升级恢复不扫描账户或结果表。

执行先检查 caller、重放、序号窗口和期限；首次送达的过期请求记录 `Failed(Expired)`，已清理的旧序号返回 `ResultExpired`。签名校验得到的不可变 `PreparedSignature` 和公钥描述复用于结果封装，回调不重复解析载荷或编码公钥。等待管理调用时释放 grant 和旧账户快照，回调仍重新读取最新账户。派发管理调用前提交 `Executing` 并预留单账户和全局预算；过期或前置校验失败在同一消息中直接提交终态，不受当日额度影响。签名响应无法封装时记为 `Failed`，管理调用不会重试。管理调用之后重新读取 home，避免覆盖并发执行的元数据。明确未发出的调用同步返回，不能读取只允许在回调中调用的退款 API：返回 `Failed`、成本 0，并撤回本次单账户和全局预留；原失败结果保留，充值后重试同一请求也不会重新派发。已发出的管理调用仍按保守成本处理。

user 在保留记录达到 56 条时暂停新的正式签名批准，根派生仍可使用 64 条总窗口。COSE 分别限制正式签名记录为 56 条、总记录为 64 条，以接收乱序到达的已授权请求；重试先查原记录，不再次占位。正式签名预算为总上限扣除向上取整的 20%，小部署也保留安全操作名额。单账户上限为每天 125 次、1.1T cycles，按同一规则覆盖 user 可授权的正式签名（100 次、800B）与根派生（20 次、300B）；两侧共用 `dmsg_runtime` 常量，单测核对覆盖关系。已发出管理调用的预留是保守值，不因执行失败自动释放。

结果默认在同账户的新执行中惰性清理。任何人可用 `prune_executions(None)` 启动维护，将返回的 `next_after` 传给下一次调用，直到其为 None；满页即返回游标，恰好整页结束时多一次空调用；游标可在升级后继续使用。每批最多删除 512 条结果，仅清理已越过连续关闭高水位、状态为 Terminal 且超过 `expires_at + DAY` 的记录。管理调用已经返回的 Unknown 可以推进关闭高水位，让后续终态记录正常清理，但自身结果和商业占用继续保留，不重签、不按超时退款。尚未返回的 InFlight 仍阻止高水位跨越；未决记录继续受 64 条总窗口和 56 条正式执行窗口约束。账户高水位和预算保留。删除使存储空间可复用，不承诺物理 stable memory 缩小。

初始化核对原始根公钥 pin 后，在 heap 中缓存各签名算法的固定两级派生前缀（`dmsg/formal/v2`、environment）。公开查询和执行准备只派生账户、用途、generation 三个后缀；管理签名调用仍发送原完整路径。升级从已核对的根公钥重建前缀，缓存不成为新的持久密钥权威。vetKD 直接使用原 context 公钥，不构造未使用的签名路径。

这些选择依据 ICP 的 [stable structures](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[重试与幂等](https://docs.internetcomputer.org/guides/canister-calls/idempotency/)及[性能优化](https://docs.internetcomputer.org/guides/canister-management/optimization/)实践。

## 2026-09-10 历史 Cycles 对比

2026-09-10，本地 PocketIC 16.0.0、Rust 1.98.1，使用相同 `--release` 配置（LTO、`opt-level=s`）比较 `14ae7b3` 的 COSE Wasm 与本次优化。正文为 4 KiB；正常执行/重试使用已完成签名历史，过期请求使用已过期历史。数字为 COSE canister 的实际余额差，不包含 user canister；正常执行包含阈值调用费用，不代表生产吞吐或长期存储成本。

| 已有历史 | 操作 | 优化前 cycles | 优化后 cycles | 降幅 |
| --- | --- | ---: | ---: | ---: |
| 0 | 新签名 | 26,199,489,025 | 26,191,476,739 | 0.031% |
| 4 | 新签名 | 26,209,154,248 | 26,192,463,344 | 0.064% |
| 8 | 新签名 | 26,218,788,950 | 26,193,081,803 | 0.098% |
| 0 | 同请求重试 | 18,880,590 | 18,691,379 | 1.00% |
| 4 | 同请求重试 | 20,131,035 | 18,892,331 | 6.15% |
| 8 | 同请求重试 | 21,308,592 | 18,984,858 | 10.91% |
| 0 | 过期请求 | 28,095,142 | 18,740,263 | 33.30% |
| 4 | 过期请求 | 33,219,069 | 18,899,010 | 43.11% |
| 8 | 过期请求 | 38,845,780 | 19,268,891 | 50.40% |

Wasm 从 1,937,175 降至 1,883,788 bytes（约 2.76%）；该小样本的 stable memory 分配均为 25,231,360 bytes，未因预算拆分增加区块。结果正文存储的减少需要更多数据才会体现为物理区块分配差异。

对比 Wasm SHA-256：

- 前：`44cf56160e0c001cbaf6cd633df1b39455715238ac04b45c780fd262677bae3f`
- 后：`f09921c250965e2d7efd9ca334858c3efc2ab012437ae5820ea091c41928724e`

当前成本字段为 `ExecutionResult.cycles_cost_upper_bound`，由共享 `ic_cose_chain_key::cost_upper_bound` 计算：管理请求付款扣除退回的附带 cycles，再加 `cost_call` 的完整预留。后者包含最大响应传输和回调执行成本，因此该字段是成本上界，不是实际余额扣减或用户账单。未发送的管理调用及前置失败返回 0，Executing 返回完整预留。实际费用不能用跨 `await` 的余额相减分摊并发请求。[ICP cost_call 定义](https://docs.internetcomputer.org/references/ic-interface-spec/canister-interface/#cycle-cost-calculation)

## 2026-09-23 修复与性能验证

同一 PocketIC 16.0.0、Rust 1.98.1 和 release 配置，对比 `9902e69` 与本次修改。下面均为 COSE 的实际余额差；签名正文 4 KiB，历史保留 0/4/8 条，成功签名包含管理调用费用，不包含 user canister。旧成本上界与新 `cycles_cost_upper_bound` 数值相同，不能从实际余额差中减去这个上界来计算业务指令成本。

| 历史条数 | 操作 | 修改前 cycles | 修改后 cycles |
| --- | --- | ---: | ---: |
| 0 | 新签名 | 26,192,381,029 | 26,192,081,022 |
| 4 | 新签名 | 26,193,330,824 | 26,193,148,305 |
| 8 | 新签名 | 26,193,936,802 | 26,193,854,444 |
| 0 | 同请求重试 | 19,476,276 | 19,485,294 |
| 4 | 同请求重试 | 19,635,920 | 19,666,203 |
| 8 | 同请求重试 | 19,735,382 | 19,778,433 |
| 0 | 过期请求 | 19,529,318 | 19,499,507 |
| 4 | 过期请求 | 19,766,450 | 19,792,771 |
| 8 | 过期请求 | 20,082,816 | 20,164,772 |

正常签名总成本基本持平；记录增加签名类型及分类计数后，部分重试/过期路径略增，本样本最多约 0.41%。全局预算合并使新实例稳定内存由 33,619,968 降至 25,231,360 字节，减少 8 MiB。新增维护入口允许闲置账户回收结果空间，账户高水位继续保留。

新实例单次 1/1024/4096 字节文本签名分别消耗 26,178,765,400 / 26,182,079,472 / 26,192,081,022 cycles；调用后 Wasm 线性内存分配分别为 1,507,328 / 1,507,328 / 1,572,864 字节。这是小样本，不代表并发峰值、生产吞吐或长期容量。

回归覆盖小预算的根派生预留、满签名窗口后根派生、乱序到达、预算增减升级、公钥与当日计数保持、控制权限、跨页/跨升级清理及旧请求防重放。协议测试对三个文档 profile、两种签名算法比较复用封装与原始字节封装并独立验签。

## 2026-10-02 修复与性能验证

开发稳定布局升级为 schema 8，公开 Candid 和批准字节保持不变。补充低可用 cycles 下的未派发失败、已返回 Unknown 的窗口推进、原请求重放、固定 home 权限、结果清理和前缀派生一致性回归。Unknown 的 PocketIC 用例在稳定记录中注入管理调用返回的未知结果，再验证升级、30 天后清理 63 条后续终态、原请求不重签和新根派生继续执行；它不是对真实网络故障的复现。普通管理调用仍使用原 `ic_cose_chain_key`，不增加生产测试入口。

使用 PocketIC 16.0.0、Rust 1.98.1、相同 release 配置（LTO、`opt-level=s`），对比修改前后 Wasm。公钥样本通过复制查询测量 COSE 余额差，对应 user 的跨 canister 读取路径，不是浏览器 query 延迟。清理样本包含 4 KiB 正式签名正文；满窗恢复另由状态与 Wasm 回归覆盖。

| 操作 | 修改前 cycles | 修改后 cycles | 降幅 |
| --- | ---: | ---: | ---: |
| Ed25519 公钥 | 14,519,518 | 12,011,828 | 17.27% |
| ES256K 公钥 | 21,900,368 | 13,535,583 | 38.19% |
| AgentController 公钥（generation 7） | 14,515,552 | 12,006,528 | 17.29% |
| ContentRoot 公钥（generation 7） | 7,744,488 | 7,737,773 | 0.09% |
| 清理 4 条签名结果 | 8,051,053 | 7,680,919 | 4.60% |
| 清理 8 条签名结果 | 9,094,299 | 8,369,983 | 7.96% |
| 新签名，4 KiB 正文、无历史 | 26,192,216,874 | 26,189,680,004 | 0.010% |

公钥字节在两版样本中完全一致，单测还覆盖三种环境、多个账户/用途/generation 的前缀与原完整路径派生等价。正式签名总成本仍主要来自管理服务；历史 0/4/8 条的执行、重试和过期请求样本均未出现成本增加。Wasm 从 2,413,465 增至 2,421,872 字节（约 0.35%），稳定内存分配保持 25,231,360 字节。样本不是生产吞吐、并发峰值或容量承诺。

对比 Wasm SHA-256：

- 前：`802eae29400398a627fb96cf09a316ab5fe636dfe9ebc0ba9f8e82032021ff31`
- 后：`dadf856fb881f6f2ec5e1eb8ca44e9e7287872d752cc38357a6197424def28f4`

复现时分别设置两个版本的 `DMSG_WASM_DIR`，运行下方两个显式成本测试。每个版本使用新实例，不跨 schema 升级。

本轮基于固定 Wasm 构建通过 161 项 Rust 测试（含文档测试，其中 COSE 单测 18 项）、严格 Clippy、Candid 一致性及完整控制面回归（87 项通过、11 项默认忽略）。两个 COSE 性能测试另行显式通过；其余容量/性能样本、浏览器端到端和生产验证不在本轮执行范围。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_cose
```

公开 canisters 的真实 Wasm 集成与协议检查：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

专门验证 COSE 并发回调、pending 重试、满窗口清理、邻接账户隔离以及去重/预算的升级恢复：

```sh
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane cose_optimization -- --test-threads=1
```

显式运行成本样本，分别令 `DMSG_WASM_DIR` 指向两个版本的 Wasm 目录：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane cose_cycles_profile -- --ignored --nocapture
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane cose_query_and_cleanup_cycles_profile -- --ignored --nocapture
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
