# dmsg_user

ICP 上的主体控制服务：认证绑定、设备能力、恢复、内容根承诺和敏感执行批准。主体独立于名称和当前登录 Principal。

## 接口

完整签名以 [dmsg_user.did](dmsg_user.did) 为准，类型在 [dmsg_types](../dmsg_types/README.md)。

- 创建和读取：`create_account`、`my_account`、`get_account`。
- 账户变更：`begin_auth_binding`、`mutate_account`，绑定设备批准与预期版本。
- 恢复：`request_recovery`、`get_recovery_request`、`reconfirm_recovery`、`complete_recovery`。
- 正式执行：`sign`、`derive_root`、`get_execution`、`reconcile_execution`。
- 认证查询：`security_snapshot_batch`、`get_device_bundle`、`get_execution_receipt`。
- 受限协作：名称授权核销及付款 offer 验证，只接受配置中的对应 canister。

`get_account` 返回 `AccountInfo`，不公开预算计数、执行窗口或内部授权记录。正常身份认证不自动批准新增设备或正式签名。频道、profile、联系列表和请求草稿不在此处存储。

## 账户与初始化

`UserInit` 指定 environment、固定 issuer_namespace、home_cose 及协作 canister、账户/每日创建配额。账户类型 `AccountId` 直接复用 `ic_auth_types::Xid`（12 字节），`AccountInfo` 和 schema 2 安全快照都返回 issuer；设备和操作 ID 仍是 32 字节。

配置直接持久化 `ic_auth_types::XidGenerator` 及完整命名空间 digest，私有模块 `xid.rs` 负责命名空间校验和错误映射；创建校验完成后在同一无 await 消息提交账户、认证索引、配额和计数器。重复认证创建返回原账户，拒绝不消耗编号。时钟回退继续计数；容量或时间溢出明确失败。升级校验完整命名空间 digest，不重置发号器。当前限固定单 home，未来多分配器必须先排除 5 字节指纹碰撞。

## 调用顺序

读取账户 issuer、当前设备序号、安全版本和选定公钥指纹，构造 `SignRequest`，用 `dmsg_protocol::SignRequestExt` 计算规范执行请求及批准摘要，由设备签署后调用 `sign`。执行授权前的拒绝返回 Err；授权后的在途、已知失败或未知结果通过 `ExecutionResult` 表达。超时后查询/对账同一 request_id，不能自动换 ID 重签。

根派生使用独立 `DeriveRootRequest`，明确区分当前根和候选槽。设备撤销后新写必须等待换根。恢复延迟不限制恢复码对已有离线备份的解密能力。

`get_execution_receipt` 仅向账户认证 caller 返回执行认证叶，绑定 issuer、设备、批准上下文、待签摘要、公钥指纹和签名摘要。叶路径及客户端认证规则见 [公开协议](../../docs/protocol/README.md)。它不把 request_id 加回通用 Statement。升级重建认证叶，清理结果时删除对应叶。

## 实现组织

`api.rs` 负责入口和异步协调；账户、执行、恢复规则与内部 `state.rs` 分离；`store.rs` 持有有类型的稳定表和认证视图。执行记录按 `(account_id, request_id)` 单条访问，普通账户读取不载入历史载荷。账户仅保存最多 64 项的保留索引：`None` 固定未终结记录，时间戳表示终态结果可清理的时间。新授权在通过全部检查后原子更新预算、序号、索引和执行表；回调重新读取单条记录，只更新发生变化的结果及其认证叶。

schema 4 的私有 `stable_codec.rs` 递归使用 CBOR 整数 map key，并省略空窗口、空授权和 `None` 字段；公共账户、批准与认证编码不变。稳定 memory ID 只属于本实现。满设备、操作、名称授权和执行索引的测试样本编码为 17,642 字节；其中新增保留索引以少量常驻字节换取不扫描历史载荷，样本不包括独立执行表、认证树和稳定表节点开销。

账户操作和已终结执行的幂等重试不重复写入。名称授权核销只核对已批准的完整 intent 与期限，名称服务负责业务去重；删除原先从不参与判断的 `consumed` 标记。恢复、设备撤销和预算限制仍在授权阶段执行。

## cycles 实测

2026-09-10 使用 PocketIC 16.0.0、同一 release/wasm32 构建配置比较优化前 `3584293` 的实现与本实现。两次构建使用同一工作区依赖。单账户分别保留 0、4、8 条已完成 Ed25519 签名，每条正文 4 KiB；记录 user canister 的调用前后余额差。以下为保留 8 条历史时的结果：

| 操作 | 优化前 cycles | 优化后 cycles | 降低 |
| --- | ---: | ---: | ---: |
| 验证付款 offer | 14,131,472 | 11,750,321 | 16.9% |
| 修改账户政策 | 31,546,313 | 15,033,801 | 52.3% |
| 新签名的 user 侧处理 | 84,518,534 | 50,704,863 | 40.0% |
| 重试已完成签名 | 39,289,743 | 19,432,357 | 50.5% |

无执行历史时，修改账户政策约 14.20M cycles，新签名的 user 侧处理从 48.73M 降至 47.93M。优化主要消除随历史载荷增长的重复反序列化、复制、哈希和认证树更新。

该测试不包含 COSE canister 和管理 canister 的阈值签名费用，也不是整条签名链路或生产账单的节省比例。基准使用确定性本地时钟和单账户，不能推断分片吞吐。可对各自的 Wasm 目录重复运行：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane user_cycles_profile -- --ignored --nocapture
```

实现遵循 ICP 的[稳定结构](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[性能测量](https://docs.internetcomputer.org/guides/canister-management/optimization/)与[跨 canister 回调](https://docs.internetcomputer.org/guides/security/inter-canister-calls/)开发指引。仍需单独验证的容量边界：认证树驻留 heap，升级按账户和执行记录总量重建，本次仅将根发布合并为一次；未实施稳定内存认证树或分批重建。终态结果仍在后续有效授权时惰性清理，闲置账户不会主动释放结果。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_user
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
