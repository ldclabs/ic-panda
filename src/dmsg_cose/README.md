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

应用通过 `dmsg_user.sign` / `derive_root` 提交设备批准。`Signature` 结果内的 `artifact` 含标准 COSE_Sign1 和 COSE_Key，`key` 保存 ICP 派生来源。`EncryptedRootKey` 保留独立的 vetKD 结果。

## 密码实现

`cose2` 负责 RFC 9052 待签结构和封装，`ic_cose_chain_key` 负责管理调用、公钥派生和费用。Ed25519 签 Sig_structure 原字节；ES256K 签其 SHA-256 摘要；dMsg 不再提供 BIP340 入口。文件摘要签署不受云端文件上传上限约束。

`CoseInit` 指定与 user home 相同的固定 issuer_namespace；account_id 为 12 字节 Xid。签署端同时检查 issuer、完整待签结构和实际派生公钥指纹。签名 kid 使用 RFC 9679 SHA-256 指纹，通用文档 profile 的 kid 仍为可变长字节。

所有用途保留固定 key home 和 `dmsg/formal/v2` 派生域，derivation_version=2。Production 配置拒绝测试根并核对 fingerprint；升级不能暗中换根。执行在管理调用前落盘，未知结果保留原请求，重试不再次签名。

## TSA

`dmsg_protocol::timestamp_imprint` 提供 RFC 9921 CTT 接点。当前没有 TSA 网络客户端、TSA 信任验证或 `anchor_snapshot` 入口；这些能力不能由普通签名成功推断。

## 代码

`api.rs` 负责入口和密码调用，`model.rs` 管理有界执行状态，`store.rs` 保存配置、公钥缓存和内部记录。schema 4 的私有 `stable_codec.rs` 使用 CBOR 整数 map key；公开执行授权、结果和密码协议编码不变。

每个 home 只保存最多 64 条执行元数据（request_id、完整 grant 的摘要、过期时间和终态标志），结果正文按 `(account_id, execution_sequence)` 单独存储。执行、回调和查询只访问目标结果，不再扫描、解码或比较整个历史窗口的正文，也不重复持久化完整 grant。64 条元数据的编码小于 6 KiB；连续终结高水位和未完成空洞仍共同约束清理及防重放。

全局预算使用独立 StableCell，和配置共享 memory 0 的既有 128 页区块：配置占页 `[0,127)`，预算占页 `[127,128)`，不额外分配 8 MiB。home 和结果分别使用 memory 1、2。预算更新不重写根公钥配置；升级恢复不扫描账户或结果表。

执行先检查 caller、重放、序号窗口和期限；过期请求直接记录 `ResultExpired`。签名校验得到的公钥描述复用于结果封装，避免再次做椭圆曲线派生。只有实际管理调用需要先提交 `Executing`；同步失败在同一消息中直接提交终态。管理调用之后重新读取 home，避免覆盖并发执行的元数据；unknown 和 in-flight 记录不会因到期清理而重新执行。

这些选择依据 ICP 的 [stable structures](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[重试与幂等](https://docs.internetcomputer.org/guides/canister-calls/idempotency/)及[性能优化](https://docs.internetcomputer.org/guides/canister-management/optimization/)实践。

## Cycles 对比

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

当前计费边界：共享 `ic_cose_chain_key::charged_cycles` 将 `cost_call` 加入返回值，而该系统 API 包含最大响应传输和回调执行的预留。它不是实际余额扣减，不能直接用作实际网络费用账单。本次保留这一共享计费合同与保守预算；实际费用口径仍需在共享库单独修正，不能用跨 `await` 的余额相减分摊并发请求。[ICP cost_call 定义](https://docs.internetcomputer.org/references/ic-interface-spec/canister-interface/#cycle-cost-calculation)

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
