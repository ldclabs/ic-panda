# dmsg_cose

受限 ICP 阈值签名和 vetKD 服务。正式签名输出可独立解析的 COSE_Sign1；原始任意 signHash、任意派生路径和 namespace 权限接口不开放。

## 接口

见 [dmsg_cose.did](dmsg_cose.did)。

| 入口 | 调用者及结果 |
| --- | --- |
| `initialize_keys` | controller；核对配置与真实公钥后启用 |
| `key_state` | 查询当前初始化与根描述 |
| `public_key(account_id, selector)` | 公开查询，离线派生公钥；不证明主体存在或具备执行权 |
| `execute(grant)` | 仅配置中的 user home；生成签名或 encrypted VetKey |
| `get_execution` | 仅配置中的 user home；查询原请求 |
| `prune_executions(after)` | controller；每批清理最多 8 个账户的过期终态结果，返回继续游标 |

应用通过 `dmsg_user.sign` / `derive_root` 提交设备批准。`Signature` 结果内的 `artifact` 含标准 COSE_Sign1 和 COSE_Key，`key` 保存 ICP 派生来源。`EncryptedRootKey` 保留独立的 vetKD 结果。

## 密码实现

`cose2` 负责 RFC 9052 待签结构和封装，`ic_cose_chain_key` 负责管理调用、公钥派生和费用。Ed25519 签 Sig_structure 原字节；ES256K 签其 SHA-256 摘要；dMsg 不再提供 BIP340 入口。文件摘要签署不受云端文件上传上限约束。

`CoseInit` 指定与 user home 相同的固定 issuer_namespace；account_id 为 12 字节 Xid。签署端同时检查 issuer、完整待签结构和实际派生公钥指纹。签名 kid 使用 RFC 9679 SHA-256 指纹，通用文档 profile 的 kid 仍为可变长字节。

所有用途保留固定 key home，正式签名使用 `dmsg/formal/v2` 派生域，derivation_version=2。Production 配置拒绝测试根并核对 fingerprint；升级不能暗中换根；只允许调整 `daily_executions` 和 `daily_cycles`，保留当天已用计数。执行在管理调用前落盘，未知结果保留原请求，重试不再次签名。

## TSA

`dmsg_protocol::timestamp_imprint` 提供 RFC 9921 CTT 接点。当前没有 TSA 网络客户端、TSA 信任验证或 `anchor_snapshot` 入口；这些能力不能由普通签名成功推断。

## 代码

`api.rs` 负责入口和密码调用，`model.rs` 管理有界执行状态，`store.rs` 保存配置、公钥缓存和内部记录。schema 6 的私有 `stable_codec.rs` 使用 CBOR 整数 map key；执行 grant 增加商业预留，摘要域为 `dmsg/cose-execution/v3`；正式 Statement 和设备执行批准字节不变。

每个 home 只保存最多 64 条执行元数据（request_id、完整 grant 的摘要、过期时间、终态及签名标志），结果正文按 `(account_id, execution_sequence)` 单独存储。执行、回调和查询只访问目标结果，不再扫描、解码或比较整个历史窗口的正文，也不重复持久化完整 grant。64 条元数据的编码小于 6 KiB；连续终结高水位和未完成空洞仍共同约束清理及防重放。

总预算和正式签名预算合并为一个独立 StableCell，和配置共享 memory 0 的既有 128 页区块：配置占页 `[0,127)`，预算占页 `[127,128)`，不额外分配 8 MiB。home 和结果分别使用 memory 1、2。预算更新不重写根公钥配置；升级恢复不扫描账户或结果表。

执行先检查 caller、重放、序号窗口和期限；首次送达的过期请求记录 `Failed(Expired)`，已清理的旧序号返回 `ResultExpired`。签名校验得到的不可变 `PreparedSignature` 和公钥描述复用于结果封装，回调不重复解析载荷或编码公钥。等待管理调用时释放 grant 和旧账户快照，回调仍重新读取最新账户。只有实际管理调用需要先提交 `Executing`；同步失败在同一消息中直接提交终态。管理调用之后重新读取 home，避免覆盖并发执行的元数据；unknown 和 in-flight 记录不会因到期清理而重新执行。

user 在保留记录达到 56 条时暂停新的正式签名批准，根派生仍可使用 64 条总窗口。COSE 分别限制正式签名记录为 56 条、总记录为 64 条，以接收乱序到达的已授权请求；重试先查原记录，不再次占位。正式签名预算为总上限扣除向上取整的 20%，小部署也保留安全操作名额。预算是保守预留，不因执行失败自动释放 cycles 额度。

结果默认在同账户的新执行中惰性清理。controller 可用 `prune_executions(None)` 启动维护，将返回的 `next_after` 传给下一次调用，直到其为 None；游标可在升级后继续使用。每批最多删除 512 条结果，仅清理已越过连续终结高水位且超过 `expires_at + DAY` 的记录；未完成空洞、账户高水位和预算保留。删除使存储空间可复用，不承诺物理 stable memory 缩小。

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

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_cose
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

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
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
