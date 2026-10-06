# dmsg_runtime

六个参考 canister 共用的实现工具，不是客户端协议库。第三方实现 canister 可选择自己的存储、账本适配和认证树。

## 模块

- `storage::Stored<T>`：在稳定内存中保存已经紧凑的标量、tuple 或原始字节记录。
- `storage::StableCodec` / `CompactStored<T>`：通过独立 representation 为结构化记录提供整数 map key；适配器直接持有 representation，表写入从借用值构造一次，避免先克隆完整领域对象；存储字节不会改变领域类型的公开 CBOR、摘要或 Candid。
- `storage::MapExt<V>`：同时覆盖 `Stored<V>` 和 `CompactStored<V>` 的便利操作；`V` 在表声明时固定，读取不能临时指定任意类型。
- `cert_map::CertMap`：user、payment、commerce、membership 和 directory 的认证 map，结构存放在 stable memory。每个 key 仍以 `labeled(key, child)` 按字节序认证，客户端照旧用 `lookup_path([key])` 查找，无需分桶标签。内部是 crit-bit 前缀树：内部节点为 `fork(left, right)`，在其 key 首个不同的位分叉；每个 key 字节先有一个存在位再接八个数据位，所以前缀排在其扩展之前。树形只由 key 集合决定，一次写入只重算一条根到叶的路径，升级只发布根；key 分布均匀时 witness 嵌套约 log₂(n)+2 层。map 只存叶子表（key → child 哈希）和内部节点，认证值由调用者从自己的记录重新生成，构造 witness 时与已认证哈希核对；一个 witness 可同时披露多个 key 的值或不存在证明。
- `certified_batch` / `query_certificate`：按调用者给出的值与 witness 组装认证响应，`CertMap` 和 `name_tree` 共用同一套大小限制。
- `admin`：管理方法的公共规则。`check_admin` 接受 controller 和固定的 governance；`validation` 把检查结果转换成 SNS 通用函数验证方法的 `Result<String, String>` 回复，`unchanged`、`user_home_payload` 和 `hex` 统一提案说明的写法。
- `name_tree::NameTree`：handle 的名称认证树。名称按 `handle_bucket` 分桶，桶号位组成二叉标签树，节点哈希存放在 stable memory 的定长数组；写入只重算一个桶和一条路径，升级无需重建。
- `ledger`：读取受信账本及其归档，解析支持的 ICRC-3 转账格式。
- `Budget`：内部有界预算。`WINDOW`、`FORMAL_EXECUTION_WINDOW` 和正式签名/根派生日上限由 user 与 COSE 共用，避免两侧口径漂移。
- `call` / `call_classified`：有界跨 canister 调用。后者区分本次确定未执行与结果未知；先前未知的尝试不能由本次确定拒绝消除。签名未发出的批准保留原序号供重试，出金仅在没有历史未知结果时进入已拒绝状态。

各 canister 自己持有 MemoryManager、memory ID、StableCell、StableBTreeMap 和 stable schema。结构化 stable representation 的字段使用显式正整数 key，0 保留；既有 key 不得改义或复用，新增字段使用新 key。可选字段可以按默认值省略。本库没有共享全局内存管理器，也没有无类型 `Table`。StableCell 使用 `CompactStored::new(&value)` 构造，通过 `value()` 取得领域值；表仍使用相同的 `MapExt`。底层 StableBTreeMap 覆盖或删除时仍解码旧 representation，不引入延迟解码层。

## 使用

```rust
use dmsg_runtime::storage::Stored;
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap};
type Records = StableBTreeMap<Vec<u8>, Stored<u64>, DefaultMemoryImpl>;
```

结构化业务记录改用 `CompactStored<T>`；`StableCodec` 的 implementation 位于共用 `stable_types` 或所属 canister 的私有 `stable_codec.rs`，调用者仍只经过 `MapExt` interface。stable representation 使用 `#[derive(cbor2::Cbor)]`，不能同时重复 derive Serde 的 `Serialize` / `Deserialize`。

认证响应限制为完整成功 `Result<CertifiedBatch>` 的 Candid 编码不超过 256 KiB，包含证书及类型封装。构造过程中先累计原始字段并提前拒绝超限，返回前再检查精确编码长度。

`MapExt::page` 的游标是排他的原始键，复合键分页由调用模块检查前缀。它不提供数据库事务；跨 await 前后的提交、重新读取和幂等由业务模块负责。账本适配当前只接受已定义的转账 schema，不能仅凭 ICRC-1 转账能力启用任意资产。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_runtime
```

六个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
