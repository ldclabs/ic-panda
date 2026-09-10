# dmsg_handle

名称权属的独立 ICP 注册表。handle 是到稳定主体的映射，转移名称不转移账户、内容、签名身份或外部应用权限。

## 接口与流程

完整接口见 [dmsg_handle.did](dmsg_handle.did)。先通过用户服务批准具体 `HandleIntent`，再调用 `reserve_handle` / `commit_handle`。转移由双方针对同一名称、版本和操作 ID 分别批准。

`resolve_handle_certified` 返回名称的 ICP 包含或不存在证明；验证者核对 canister ID、信任根、certificate、witness 和所需新鲜度。名称事件独立编号，可通过 `get_handle_event` 查询。

## 收费和导入

ASCII 规范化、PANDA 定价及旧名称导入属于此注册表的业务规则，不是基础签名协议。新注册在旧快照封存前关闭。导入分 begin/import/seal；未认领旧名保持预留，争议记录不自动释放。

扣款前固定付款账户、金额、fee、memo 和账本纳秒时间。未知扣款保持名称锁，使用原参数或真实账本块对账；不能因为本地超时而再次售卖名称。查询 `get_handle_operation` 可恢复原流程。

名称拥有者、转移目标和授权引用均为 12 字节 `AccountId`；handle 与 issuer URI 分别维护，名称转移不会改变 Xid。

## 实现

`api.rs` 保存入口与名称状态转换，`store.rs` 保存有类型的名称、操作、锁和事件。schema 4 的私有 `stable_codec.rs` 为配置、名称、导入、操作、事件和转移回执递归使用 CBOR 整数 map key；公开名称摘要、认证叶编码和 Candid 不变。部署参数由 `HandleInit` 定义。

- 名称事件使用 `StableLog`，序号来自日志长度，配置只保存事件摘要 tip；不再为顺序追加维护 B-tree 和重复计数器。
- 名称锁直接保存 32 字节操作 key；主体活跃操作只需 `StableBTreeSet<[u8; 12]>`，不重复保存操作摘要。
- `MemoryManager` 使用 16 页（1 MiB）分配桶，减少默认每个活跃区域 8 MiB 的预分配。当前库最多 32,768 桶，对应约 32 GiB 可分配空间；这是布局上限，不是容量验收结果。桶不会因为删除记录自动缩回。
- 注册在跨 user 调用前检查全局和主体额度，回调后再次检查，幂等重试先返回已有操作。扣款回调只重读一次操作，将事件、权属、最终操作状态和锁释放作为同一个消息中的提交，合并配置写入。`Paid` 不作为额外稳定写入阶段；账本调用前的 `Charging` 仍需持久化，未知扣款继续保留锁。
- 已完成的快照导入重试与封存重试不重复写配置和认证根。升级重建名称认证叶后只发布一次认证根。

选择依据包括 ICP 官方的 [Stable structures](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[消息执行原子性](https://docs.internetcomputer.org/references/message-execution-properties/)和[性能测量建议](https://docs.internetcomputer.org/guides/canister-management/optimization/)。认证树仍驻留 heap，升级仍需遍历所有已激活名称；大规模升级的指令和内存上限尚未验证。操作去重回执和事件历史也持续增长，本次未引入可能破坏重试语义的历史删除策略。

## cycles 实测

2026-09-10，以 `14ae7b3` 为修改前基线，使用 Rust 1.98.1、Cargo.lock、相同 release 配置和 PocketIC 16.0.0；其他 canister 使用同一份 Wasm。测试导入 256 个旧名，再依次注册 17 个新名。以下调用费用取已有 16 个注册记录时的样本，封存重试在注册前测量，升级在 17 个名称时测量。

| 项目 | 修改前 | schema 4 | 变化 |
| --- | ---: | ---: | ---: |
| 新预留 cycles | 15,064,418 | 15,057,478 | -0.05% |
| 主体额度拒绝 cycles | 14,598,522 | 8,343,618 | -42.85% |
| 成功扣款并提交 cycles | 16,087,787 | 15,576,169 | -3.18% |
| 封存重试 cycles | 7,285,593 | 7,040,867 | -3.36% |
| 样本已分配 stable memory | 56.0625 MiB | 8.0625 MiB | -85.62% |
| Wasm 文件字节数 | 1,527,488 | 1,554,953 | +1.80% |
| 同版本代码升级 cycles | 3,139,191,345 | 3,195,030,739 | +1.78% |

余额差仅统计 `dmsg_handle`，不包含 user/ledger 的执行费与 PANDA 扣款；内存项是小样本实际分配空间，不代表每个名称减少 85.62%。新增存储类型让 Wasm 稍大，小样本总升级费用也增加；不能把减少认证根发布次数解读为已测得总升级费下降。测试不代表生产吞吐或百万名称容量。

复现时为两次构建各保留 Wasm 目录，运行同一测试：

```sh
DMSG_WASM_DIR=/path/to/wasm POCKET_IC_BIN=/path/to/pocket-ic \
  cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane handle_cycles_profile -- --ignored --nocapture
```

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_handle
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

`tests/dmsg_integration/tests/control_plane/handle.rs` 额外覆盖批量导入失败无部分写入、重叠导入重试、并发预留/扣款、额度提前拒绝、锁与日志升级恢复、未知扣款不释放、确定失败释放、名称包含/不存在证明及查询批量上限。

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
