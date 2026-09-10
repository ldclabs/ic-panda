# dmsg_types

公开数据合同的 Rust 定义。其他语言可以依据 [公开协议](../../docs/protocol/README.md)、CDDL 与各 canister 的 Candid 文件独立实现，不需要复制本库的 Rust 类型布局。

## 范围

| 模块 | 公开内容 |
| --- | --- |
| `signing` | `Statement`、`StatementContent`、`SignedArtifact`；可脱离 ICP 存储实现处理 |
| `account_id` | `ic_auth_types::Xid` 重导出为 `AccountId`，12 字节及规范文本编解码 |
| `protocol` | 固定字节、毫秒时间、批准、错误与 ICP 认证响应 |
| `cose` | ICP 签名/vetKD 请求、密钥来源、执行结果和部署参数 |
| `user` | 账户视图、设备、恢复与根承诺合同 |
| `handle` | 名称权属、注册、转移及导入合同 |
| `payment` | 付款授权、资金决定、托管视图和转账查询 |
| `profiles::delivery` | 可选的付费投递报价及受理收据 |
| `account` | ICRC Account 的明确 CBOR 字节表示 |

本库不包含 canister 调用、稳定内存、认证树、可变预算、账户内部状态或账本读取。那些实现分别属于 `dmsg_protocol`、`dmsg_runtime` 和具体 canister。对外定义也包含受限的跨 canister 合同；公开类型不代表允许任意 caller 使用。

## 使用

```rust
use dmsg_types::{Hash, StatementContent};
let body = StatementContent::Digest {
    sha256: Hash::new([1; 32]), // 示例摘要；实际使用原文件的 SHA-256
    content_type: Some("application/pdf".into()),
    location: None,
};
```

`Hash` 等固定值在 CBOR 中是字节串，在 Candid 中是 blob；长度由合同规定。`AccountId` 是 `ic_auth_types::Xid` 的别名，为 12 字节，JSON/显示为 20 字符 Xid，不复用 Hash。Statement 的可选 `issued_at` 为 Unix 秒，ICP 业务时间为 Unix 毫秒；ICRC 的 `created_at_time` 仍是纳秒。浏览器 JSON 桥按公开协议单独定义字段编码，不等同于 Rust serde JSON。

`Statement` 是标准 COSE 的准备/解析视图，文本 payload 直接为 UTF-8，摘要 payload 直接为 SHA-256；不序列化 Rust 枚举为签署正文。`ExecutionReceipt` 独立绑定执行编号、待签摘要和密钥，`VerificationReport` 区分数学签名与身份/授权/时间的验证结果。

`AccountInfo` 和 `EscrowInfo` 是公开视图，不是数据库记录。不要用协议编码或版本号替代内部 stable schema。编码、批准构造与验签见 [dmsg_protocol](../dmsg_protocol/README.md)。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_types
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
