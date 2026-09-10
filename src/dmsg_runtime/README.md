# dmsg_runtime

四个参考 canister 共用的实现工具，不是客户端协议库。第三方实现 canister 可选择自己的存储、账本适配和认证树。

## 模块

- `storage::Stored<T>`：在稳定内存中保存已经紧凑的标量、tuple 或原始字节记录。
- `storage::StableCodec` / `CompactStored<T>`：通过独立 representation 为结构化记录提供整数 map key；存储字节不会改变领域类型的公开 CBOR、摘要或 Candid。
- `storage::MapExt<V>`：同时覆盖 `Stored<V>` 和 `CompactStored<V>` 的便利操作；`V` 在表声明时固定，读取不能临时指定任意类型。
- `Certification`：构造 ICP 认证响应；认证值由调用者选择公开视图，支持在执行清理时删除对应叶。
- `ledger`：读取受信账本及其归档，解析支持的 ICRC-3 转账格式。
- `Budget`：内部有界预算。
- `call`：有界跨 canister 调用与未知结果分类。

各 canister 自己持有 MemoryManager、memory ID、StableCell、StableBTreeMap 和 stable schema。结构化 stable representation 的字段使用显式正整数 key，0 保留；既有 key 不得改义或复用，新增字段使用新 key。可选字段可以按默认值省略。本库没有共享全局内存管理器，也没有无类型 `Table`。

## 使用

```rust
use dmsg_runtime::storage::Stored;
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap};
type Records = StableBTreeMap<Vec<u8>, Stored<u64>, DefaultMemoryImpl>;
```

结构化业务记录改用 `CompactStored<T>`；`StableCodec` 的 implementation 位于共用 `stable_types` 或所属 canister 的私有 `stable_codec.rs`，调用者仍只经过 `MapExt` interface。stable representation 使用 `#[derive(cbor2::Cbor)]`，不能同时重复 derive Serde 的 `Serialize` / `Deserialize`。

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
