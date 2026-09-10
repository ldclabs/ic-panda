# dmsg_types

[English](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/README.md) | 简体中文

`dmsg_types` 定义 dMsg 的公开 Rust 数据合同，用于文档签名、账户与设备控制、名称权属和可选的付费投递对接。dMsg 将可独立验证的 COSE 文档与 ICP 上的账户授权、密钥执行和资金状态分开；集成文档验签不需要实现完整账户、收件箱或支付系统。

本库提供类型、Serde/Candid 表示和少量转换/结果访问方法，**不执行签名、验签、网络调用或业务授权**。构造一个类型或成功反序列化，不代表请求合法、签名有效或调用者有权限。编码、校验和批准摘要构造由配套 [dmsg_protocol](https://github.com/ldclabs/ic-panda/tree/main/src/dmsg_protocol) 提供；存储和平台调用由运行时与 canister 实现。

## 获取与文档入口

仓库中的包版本为 `0.1.0`。发布配置以 `Cargo.toml` 为准；启用发布不表示该版本已经上架 crates.io。开发时可使用路径依赖：

```toml
[dependencies]
dmsg_types = { path = "../ic-panda/src/dmsg_types" }
```

路径相对于调用方的 `Cargo.toml`，按实际 checkout 位置调整。版本正式发布后可使用 `dmsg_types = "0.1"`；如需协议编码与验签，还要引入配套协议库。

- [公开协议与字节规则](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README.md)：独立语言实现的入口。
- [声明 CDDL](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/statements.cddl)：COSE 文档结构。
- [互操作测试向量](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/tests/protocol_vectors.json)：固定编码与签名样本。
- [canisters 实际实现与验证边界](https://github.com/ldclabs/ic-panda/blob/main/docs/dmsg_canisters_zh.md)：服务职责、构建与部署状态。

以上链接指向公开仓库的 main 分支，可能随开发变化；对接时应固定同一提交的类型、协议、Candid 和测试向量。Rust API 注释使用英文，便于 IDE 与 rustdoc 使用。英文 README 同时作为 crate 首页；中英文版本应同步维护。

## 先理解这些术语

| 术语 | 含义与边界 |
| --- | --- |
| `AccountId` | 稳定的 dMsg 账户身份，由 user canister 分配；12 字节 Xid，不是 ICP Principal。更换登录绑定或设备不应替换账户身份。 |
| Principal / auth binding | ICP 调用者身份及其账户登录绑定。登录成功不自动获得设备批准或解密权限。 |
| ICRC `Account` | 账本收付款地址，由 owner Principal 和可选 32 字节 subaccount 组成，与 dMsg 账户 ID 不同。 |
| handle | 可注册、转移的名称，指向 `AccountId`；名称权属和登录认证分别管理。 |
| issuer / subject | `Statement.issuer` 是签署者的规范 URI；`subject` 是被声明对象，可省略，不是旧版账户 ID 字段。 |
| device / capability | 登记的设备公钥与明确权限。设备角色 `Administrator` 不等于 ICP canister controller。 |
| approval | 设备对特定操作、账户安全状态、序号、期限等上下文的签名批准，不是通用登录凭证。 |
| home user / home COSE | 分别负责账户授权与密钥执行的固定 canister。密钥派生身份依赖 home COSE 和派生参数。 |
| content root / generation | 内容根的代次及外部加密 bundle 的承诺；这些类型不包含明文根密钥。vetKD 返回加密派生结果。 |
| security epoch / account version | 前者使旧安全批准失效，后者用于账户变更的乐观并发检查；不能互换。 |
| artifact / execution receipt | 前者是可移植 COSE 签名文档；后者是 ICP 记录的执行证据，单独绑定请求、待签字节和密钥。 |
| offer / quote / admission receipt | 分别是收款方授权、固定投递报价、服务受理证明；均不表示对方已阅读或回复。 |
| escrow / transfer leg | 前者管理托管资金及结算/退款决定；后者记录一笔具体的账本出金。 |

## 模块导航

| 模块 | 主要类型 | 使用场景 |
| --- | --- | --- |
| `signing` | `Statement`, `StatementContent`, `SignedArtifact`, `VerificationReport` | 准备、交换、解析文档签名，可独立于 ICP 使用 |
| `account_id` | `AccountId` | 稳定账户身份；重导出 `ic_auth_types::Xid` |
| `protocol` | `Hash`, `OpId`, `Approval`, `Error`, `CertifiedBatch` | 共享字节、时间、设备批准、错误和认证查询 |
| `cose` | `SignRequest`, `DeriveRootRequest`, `KeyDescriptor`, `ExecutionResult`, `ExecutionReceipt` | ICP 正式签名、vetKD、密钥来源和执行对账 |
| `user` | `AccountInfo`, `AccountMutation`, `Device`, `SecuritySnapshot` | 账户、设备、恢复与根承诺 |
| `handle` | `HandleIntent`, `HandleRecord`, `HandleOperation` | 名称注册、转移与冻结名称导入 |
| `payment` | `PaymentOffer`, `EscrowInfo`, `TransferLeg` | 收款授权、托管会计与账本转账 |
| `profiles::delivery` | `Quote`, `OpenEscrow`, `SignedReceipt` | 可选付费投递应用协议 |
| `account` | `account_cbor` | ICRC Account 的明确 Serde/CBOR 表示 |

`protocol`、`signing` 中的类型以及 `AccountId` 在 crate 根重导出；其他类型按模块导入。`profiles` 的“可选”指集成方可以不使用该业务，不是 Cargo feature。

## Rust 示例

### 准备文档内容

```rust
use dmsg_types::{Hash, Statement, StatementContent};

let statement = Statement {
    issuer: "https://example.org/signers/alice".into(),
    subject: Some("Release approval".into()),
    issued_at: Some(1_800_000_000), // 声明的 Unix 秒，不是执行截止时间
    content: StatementContent::Text("I approve release 1.0.".into()),
};
assert!(matches!(statement.content, StatementContent::Text(_)));

// 实际对接时须自行计算原始文件字节的 SHA-256；这里仅演示类型构造。
let digest_content = StatementContent::Digest {
    sha256: Hash::new([0x42; 32]),
    content_type: Some("application/pdf".into()),
    location: None,
};
assert!(matches!(digest_content, StatementContent::Digest { .. }));
```

`Statement` 是准备/解析视图，不能将其 Rust 枚举序列化后直接当签署正文。线上对象是 tagged COSE_Sign1：文本 payload 为原始 UTF-8（1..4096 字节），摘要 payload 为原文的 32 字节 SHA-256。issuer、subject 和 issued_at 进入受保护的 CWT claims。文本不裁剪空白、不自动做 Unicode 归一化；URI 是标识，不会触发自动网络发现。

### 账户文本与时间转换

```rust
use dmsg_types::{AccountId, millis_to_nanos, nanos_to_millis, MINUTE};

// 示例字节仅用于编码演示；真实账户 ID 从 user canister 的创建结果获得。
let account = AccountId([1; 12]);
let text = account.to_string();
assert_eq!(text.len(), 20);
assert_eq!(text.parse::<AccountId>().unwrap(), account);

let deadline_ms = 1_800_000_000_000u64 + MINUTE;
let ledger_time_ns = millis_to_nanos(deadline_ms).unwrap();
assert_eq!(nanos_to_millis(ledger_time_ns), deadline_ms);
assert!(millis_to_nanos(u64::MAX).is_err()); // 溢出不会静默回绕
```

### 区分完成、等待和未知结果

```rust
use dmsg_types::{cose::{ExecutionOutcome, ExecutionResult}, Error, Hash};

let result = ExecutionResult {
    request_id: Hash::new([7; 32]),
    outcome: ExecutionOutcome::Unknown(Error::ExecutionUnknown),
    charged_cycles: 0,
};
assert!(!result.is_terminal());
assert_eq!(result.output(), Err(Error::ExecutionUnknown));
// 保留原 request_id 查询和对账，不为未知结果自动新建签名请求。
```

`output()` 对等待中的操作返回 `Error::Pending`，对已清理输出返回 `Error::ResultExpired`。`is_terminal()` 只对 Completed、Failed、ResultExpired 返回 true；Unknown 不表示未执行。

## 按场景对接

### 独立文档验签

取得 `SignedArtifact`，用配套 `dmsg_protocol::verify_artifact` 检查 COSE profile 和数学签名。需要核对文件时，将原文件字节交给 `verification_report`；摘要原文未提供时，content 为 `NotProvided`。随包 `cose_key` 只是公钥，不能自行证明 issuer 身份。`issuer_binding`、`authorization`、`timestamp`、`current_status` 应按各自证据检查，未检查时保持 `NotChecked`。

当前支持 Ed25519（COSE -19）和 ES256K（-47）。vetKD 用于派生内容根，不属于文档签名算法。普通签名不自带可信时间戳；`Statement.issued_at` 是签署者的时间声明，批准期限到期也不会自动使已完成的文档签名失效。

### ICP 正式签名

1. 从部署配置确定 user/COSE home，查询账户及已认证的密钥描述；确认设备权限、`security_epoch` 和设备序号。
2. 冻结完整 `Statement`、`SigningKeyRef`、经扩展核实的 origin 和 `max_cycles`。使用协议库 `SignRequestExt` 等辅助接口转换请求、生成 request ID 和批准摘要，再由设备 Ed25519 key 签名。
3. 调用 user canister 的 `sign`，按 `ExecutionResult` 查询原请求，完成后提取 `ExecutionOutput::Signature`。
4. 如需证明 dMsg 执行授权，查询 `get_execution_receipt`，验证 IC certificate/witness 后，再用 `match_execution_receipt` 匹配产物与回执。该 Rust helper 只检查绑定，不代替 IC 证书验证。

可移植声明不包含 request_id、origin 或执行期限；这些属于执行上下文。设备签名绑定 origin，但不能独立证明该字符串确实来自浏览器。低层 `ExecutionGrant` 是受限跨 canister 合同，公开类型不意味着任意 caller 可以执行它。

### 账户、设备和内容根

`AccountMutation` 绑定 `expected_version`、完整 `AccountCommand` 和 `Approval`。版本冲突后重新读取状态并重新准备批准，不直接替换已签请求的版本字段。新增设备需要对应私钥的持有证明；登录 Principal、设备签名 key 和 HPKE 加密 key 各有职责。

换根通过 `ReserveRoot` → 派生候选根/准备外部加密 bundle → `CommitRoot` 完成，操作 ID、期望代次和安全状态须匹配。`ContentRootRef` 只保存 bundle 承诺和派生信息。`VaultWriteState::RekeyRequired` 表示不能继续用旧根写入。恢复材料的版本、恢复等待窗口和争议确认由 user 合同表达，不等同于本地 UI 解锁状态。

### 可选付费投递

收款设备签署 `PaymentOffer`，服务生成并签署 `Quote`；付款方用 `OpenEscrow` 固定条款，然后向托管 subaccount 入金。服务受理后返回 `SignedReceipt`，payment canister 核验后提交结算或退款决定。对接依据见 [payment Candid](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_payment/dmsg_payment.did)。

`FundsDecision::SettlementCommitted` 与 `RefundCommitted` 互斥，但二者都不保证出金已成功，应继续检查各 `TransferLeg.status`。未知账本结果先对账，重试保留原 memo 和纳秒 `created_at_time`。`Quote.amount = recipient_net + service_fee + fee_reserve`；`EscrowInfo` 满足 `confirmed_in = liabilities + transferred + network_fees`，单位均为该 ledger 的最小整数单位。

## 编码、单位和认证约定

| 数据 | 合同 |
| --- | --- |
| `AccountId` | 12 字节；CBOR bstr / Candid blob；人类可读 Serde/显示为 20 字符规范小写 base32hex Xid，末字符为 0 或 g |
| `Hash` / `OpId` | 32 字节；CBOR bstr / Candid blob，解码校验长度。它们是别名，不会在 Rust 类型层面阻止混用不同语义的 32 字节值 |
| COSE `kid` | 不透明字节串，文档 profile 限 1..256 字节；不能根据长度猜账户身份 |
| ICRC Account | `{owner: bstr, subaccount: bstr(32) / null}`；合同字段使用 `account::account_cbor` 保留明确表示 |
| 业务时间 | `u64` Unix 毫秒；duration 也是毫秒，截止条件通常为 `now < expires_at` |
| 声明 `issued_at` | 可选 `i64` Unix 秒；与 `PaymentOffer.issued_at` 的毫秒不同 |
| IC certificate / ICRC `created_at_time` | Unix 纳秒；`nanos_to_millis` 丢弃不足一毫秒的部分 |
| 金额 / cycles | 金额为 `u128` ledger 最小单位；cycles 是 ICP 执行成本，二者不同 |

Serde 只定义数据表示，确定性编码仍须使用协议库或独立遵循公开规范：RFC 8949 core deterministic CBOR；大于 u64 的 u128 用 tag 2 最短大端字节串。不要通过 JavaScript Number 中转金额。验签须保留原 COSE protected-header 字节，不能重排后再验签。

浏览器桥是独立的 `dmsg-extension/3` JSON 合同：accountId 为 Xid 文本、摘要/requestId/nonce 为小写 hex、大整数为十进制字符串。普通 Rust Serde JSON 不等同于桥协议，也不要把 Rust 枚举的内存布局当成 wire 格式。

认证查询返回 `CertifiedBatch`，必须核对可信 IC 根、预期 canister、certificate 时间、witness 路径和原始 leaf 值。账户安全叶路径为单段原始 AccountId，escrow 叶为单段原始 escrow_id；执行回执为 `b"execution/" || account_id || request_id`。`SecuritySnapshot.devices_root` 提交完整设备 map，包括撤销/序号字段，不是删减后的设备列表。当前账户安全快照使用证书时间 +60 秒的新鲜度边界；历史执行回执不能机械套用这一窗口。`AccountInfo` 或 `DeviceEvidence` 单独出现不构成认证证明。

## 错误处理与接口依据

| 情况 | 集成方处理 |
| --- | --- |
| `AuthRequired`, `DeviceNotApproved`, `Forbidden` | 检查登录绑定、设备状态、角色和 capability；不要盲重试 |
| `VersionConflict`, `PolicyStale` | 重新获取状态并重新准备授权 |
| `IdempotencyConflict` | 相同 ID 的参数发生变化；修正客户端重试逻辑 |
| `Pending`, `ExecutionUnknown` | 保留原操作标识，查询/对账，不假定执行失败 |
| `ResultExpired` | 输出已超出保留期；不能据此重放旧批准 |
| `FeeBlocked` | 处理账本费用与批准上限，不能擅自提高费用 |
| `InvalidInput`, `UnsupportedProtocol` | 修正请求字段或协议版本；诊断字符串不适合作为稳定错误码 |

传输超时和 Candid 解码错误不属于本库 `Error`；收到 `Result<T>` 前的网络失败也可能需要按原 ID 对账。准确的方法签名、caller 限制和参数顺序以对应服务的 Candid 与公开实现为准：

- [user](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_user/dmsg_user.did)
- [COSE](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_cose/dmsg_cose.did)
- [handle](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_handle/dmsg_handle.did)
- [payment](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_payment/dmsg_payment.did)

## 本地验证与发布准备

在 workspace 根目录运行：

```sh
cargo test -p dmsg_types --locked
RUSTDOCFLAGS="-D warnings" cargo doc -p dmsg_types --no-deps --locked
cargo run -p dmsg_types --example protocol_vectors --locked > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

英文 README 的 Rust 代码块会作为 doctest 执行；两版示例保持相同代码，仅翻译注释。crate 启用 `missing_docs` 警告，新增公开项时应同步补充注释。协议/真实 canister 联调测试另见公开实现文档。

发布前核对 `Cargo.toml` 的发布配置、仅用于仓库测试的本地 `dmsg_protocol` dev-dependency 在发布包中的处理方式，并对实际发布包执行 package/publish dry-run。本库的文档完整性不代表生产部署、容量、外部服务或审计已经验收。当前为开发接口，不承诺兼容早期实验编码或稳定存储布局。
