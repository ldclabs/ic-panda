# dmsg_runtime

四个参考 canister 共用的实现工具，不是客户端协议库。第三方实现 canister 可选择自己的存储、账本适配和认证树。

## 模块

- `storage::Stored<T>`：在稳定内存中保存内部 Rust 记录。
- `storage::MapExt<V>`：原生 `StableBTreeMap<Vec<u8>, Stored<V>, Memory>` 的便利操作；`V` 在表声明时固定，读取不能临时指定任意类型。
- `Certification`：构造 ICP 认证响应；认证值由调用者选择公开视图，支持在执行清理时删除对应叶。
- `ledger`：读取受信账本及其归档，解析支持的 ICRC-3 转账格式。
- `Budget`：内部有界预算。
- `call`：有界跨 canister 调用与未知结果分类。

各 canister 自己持有 MemoryManager、memory ID、StableCell、StableBTreeMap 和 stable schema。本库没有共享全局内存管理器，也没有无类型 `Table`。

## 使用

```rust
use dmsg_runtime::storage::Stored;
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap};
type Records = StableBTreeMap<Vec<u8>, Stored<u64>, DefaultMemoryImpl>;
```

`MapExt::page` 的游标是排他的原始键，复合键分页由调用模块检查前缀。它不提供数据库事务；跨 await 前后的提交、重新读取和幂等由业务模块负责。账本适配当前只接受已定义的转账 schema，不能仅凭 ICRC-1 转账能力启用任意资产。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_runtime
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
