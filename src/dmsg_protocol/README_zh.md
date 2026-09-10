# dmsg_protocol

[English](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_protocol/README.md) | 简体中文

dMsg 的确定性编码、文档签名验证和请求辅助库。本库在 [dmsg_types](https://github.com/ldclabs/ic-panda/tree/main/src/dmsg_types) 上实现公开协议，只执行本地计算：不调用 ICP、不查询账本、不操作 canister 存储、不进行网络发现，也不执行账户授权。

可用于准备可移植 COSE 文档、验证返回的签名产物、构造设备批准摘要，以及将文档与已经独立认证的执行回执进行绑定检查。所有公开 API 都在 crate 根重导出；源码模块仅用于组织实现，不是公开导入路径。

## 安装与依赖方向

开发期间，按应用 Cargo.toml 的相对位置使用路径依赖：

```toml
[dependencies]
dmsg_types = { path = "../ic-panda/src/dmsg_types" }
dmsg_protocol = { path = "../ic-panda/src/dmsg_protocol" }
```

两个版本正式发布后，对应的 registry 依赖为：

```toml
[dependencies]
dmsg_types = "0.1"
dmsg_protocol = "0.1"
```

正式依赖方向是 **dmsg_protocol → dmsg_types**。仓库中反方向的引用只是开发依赖，用于 dmsg_types 的合同测试和向量生成；仅使用类型的应用不会引入本协议库。使用方直接从 dmsg_types 导入公开 DTO 和错误类型。下文示例还使用 `ed25519-dalek = "3"`，批准示例另外使用 `candid = "0.10"`。

发布配置不表示版本已经上架 crates.io。当前 checkout 中两个包的版本均为 0.1.0；首次发布顺序见文末说明。

## API 导航

| 任务 | 入口 | 仍由调用方负责的部分 |
| --- | --- | --- |
| 编码协议记录 | `canonical`, `decode_canonical`, `sha256`, `digest` | 使用准确的公开类型、域和数据结构，并进行业务验证 |
| 准备文档 | `validate_statement`, `statement_purpose`, `prepare_cose`, `parse_signing_input` | 取得用户同意、选择可信密钥并调用签名器 |
| 组装与验证 | `finish_cose`, `verify_artifact`, `verification_report` | 确认 issuer 身份、授权与当前状态 |
| 描述签名公钥 | `cose_algorithm`, `public_cose_key`, `key_thumbprint` | 认证公钥来源，不把指纹当成所有权证明 |
| 标识签署者 | `account_issuer`, `principal_issuer`, `parse_account_issuer`, `validate_uri`, `validate_namespace` | 配置可信命名空间，并与已认证证据绑定 |
| 准备 ICP 批准 | `SignRequestExt`, `ExecuteRequestExt`, `approval_message`, `execution_request_id` | 获取当前账户/设备状态、签署摘要并提交到 user home |
| 验证请求输入 | `DeviceInputExt`, `CoseInitExt`, `KeyRequestExt`, `validate_origin`, `validate_transport_key` | 执行服务端授权、私钥持有证明检查与状态转换 |
| 核对执行证据 | `artifact_signing_bytes`, `match_signing_result`, `execution_receipt_key`, `match_execution_receipt` | 先验证 IC certificate、预期 canister、witness、路径和叶值 |
| 处理恢复与名称 | `recovery_confirmation_message`, `normalize_handle`, `price`, `charge_terms_digest` | 在对应服务中执行恢复政策、名称与账本操作 |
| 附加时间戳证据 | `timestamp_imprint`, `attach_unverified_timestamp_token`, `signature_digest` | 获取并独立验证 TSA token 及其信任链 |
| 检查基础约束 | `authenticated`, `nonzero`, `expiry`, `check_sequence`, `verify` | 提供可信 caller、时间和状态；辅助函数不读取或修改它们 |

Rustdoc 为各入口说明参数、失败行为和信任边界。`verify` 是原始严格 Ed25519 验签；`verify_artifact` 还验证 COSE 文档 profile。`authenticated` 仅排除匿名和管理 canister Principal，不证明账户成员身份。

## 本地签署并验证文档

这个完整示例使用确定性的**测试密钥**。生产签名应使用安全生成或外部管理的密钥。

```rust
use dmsg_protocol::{
    account_issuer, finish_cose, key_thumbprint, match_signing_result,
    prepare_cose, public_cose_key, sha256, verification_report,
};
use dmsg_types::{cose::Algorithm, AccountId, Statement, StatementContent, VerificationStatus};
use ed25519_dalek::{Signer, SigningKey};

let signer = SigningKey::from_bytes(&[7; 32]); // 仅用于测试。
let public = signer.verifying_key().to_bytes();
let algorithm = Algorithm::Ed25519;
// 此处允许空 kid，以便先计算公钥指纹。
let fingerprint = key_thumbprint(&public_cose_key(&algorithm, &[], &public).unwrap()).unwrap();
let original = b"Release 1.0 specification";
let statement = Statement {
    issuer: account_issuer("https://example.org/u/", &AccountId([1; 12])).unwrap(),
    subject: Some("release/specification".into()),
    issued_at: Some(1_800_000_000), // 声明的 Unix 秒，不是可信时间。
    content: StatementContent::Digest {
        sha256: sha256(original),
        content_type: Some("text/plain".into()),
        location: None,
    },
};
let (_, tbs) = prepare_cose(&statement, &algorithm, fingerprint.as_slice()).unwrap();
let signature = signer.sign(&tbs).to_bytes().to_vec();
let artifact = finish_cose(&tbs, &public, signature).unwrap();
match_signing_result(&artifact, &tbs, fingerprint).unwrap();
let report = verification_report(&artifact, Some(original)).unwrap();
assert_eq!(report.signature, VerificationStatus::Verified);
assert_eq!(report.content, VerificationStatus::Verified);
assert_eq!(report.issuer_binding, VerificationStatus::NotChecked);
assert_eq!(report.timestamp, VerificationStatus::NotProvided);
```

`prepare_cose` 返回未签名消息和 `Sig_structure = CBOR(["Signature1", protected_bstr, h'', payload_bstr])`。文本使用原始 UTF-8（1..4096 字节），摘要文档使用原文字节的 32 字节 SHA-256。Rust Statement 枚举不是额外的 wire payload。issuer、可选 subject 和声明的 issued_at 是受保护 CWT claims；请求 ID、浏览器 origin 和执行截止时间是单独的执行元数据。

| 算法 | COSE 标签 | 签名器输入 | 传给 `finish_cose` 的签名/公钥 |
| --- | --- | --- | --- |
| Ed25519 | -19 | 完整 tbs 字节 | 64 字节签名；原始 32 字节公钥 |
| ES256K | -47 | 使用 prehash API 时传 SHA-256(tbs) | 64 字节 r\|\|s，不是 DER；SEC1 secp256k1 公钥 |

使用内部计算哈希的 API 时，应传入 tbs，避免重复哈希。vetKD 不是文档签名算法。`finish_cose` 组装并校验结构，但**不验证签名**；`match_signing_result` 或 `verify_artifact` 才执行验签。`public_cose_key` 返回编码后的 COSE_Key，输入则是原始公钥。`key_thumbprint` 对必需的公开 COSE 参数计算摘要，排除 kid/alg/key_ops，并展开压缩 EC y 坐标；它不是原始公钥 SHA-256，也不是完整公钥验证器。

## 构造 ICP 设备批准

以下示例构造本地请求和设备签名，不联系 canister，也不授予权限。真实账户 ID、设备 ID、epoch、序号和签名公钥引用必须从已配置、已认证的服务取得。示例 ID 和密钥仅用于演示。

```rust
use candid::Principal;
use dmsg_protocol::{execution_request_id, ExecuteRequestExt, SignRequestExt};
use dmsg_types::{
    cose::{SignRequest, SigningAlgorithm, SigningKeyRef},
    AccountId, Approval, Hash, Statement, StatementContent,
};
use ed25519_dalek::{Signer, SigningKey};

let account_id = AccountId([1; 12]);
let device_id = Hash::new([2; 32]);
let security_epoch = 1;
let sequence = 0;
let request_id = execution_request_id(&account_id, security_epoch, device_id, sequence);
let request = SignRequest {
    account_id,
    key: SigningKeyRef {
        algorithm: SigningAlgorithm::Ed25519,
        kid: vec![3; 32].into(),
        public_key_fingerprint: Hash::new([3; 32]),
    },
    statement: Statement {
        issuer: "https://example.org/signers/alice".into(),
        subject: None,
        issued_at: None,
        content: StatementContent::Text("Approve release 1.0".into()),
    },
    origin: "https://example.org".into(),
    max_cycles: 1_000_000_000,
    approval: Approval {
        device_id, security_epoch, sequence, request_id,
        expires_at: 1_800_000_060_000, // Unix 毫秒；实际请求应使用有效期限。
        signature: Vec::new().into(), // 不进入批准摘要。
    },
};
let mut execution = request.into_execution().unwrap();
let home_user = Principal::from_slice(&[1, 1]); // 部署演示值。
let digest = execution.approval_message(home_user);
let device_key = SigningKey::from_bytes(&[9; 32]); // 仅用于测试。
execution.approval.signature = device_key.sign(digest.as_slice()).to_bytes().to_vec().into();
assert_eq!(execution.approval.request_id, request_id);
```

`SignRequestExt::into_execution` 验证 origin 和声明，以签名 generation 1 准备字节。它保留传入的指纹和批准，不认证它们。`ExecuteRequestExt::approval_message` 在 `dmsg/execute/v3` 下绑定 kind 和 max_cycles，再包装到 `dmsg/device-approval/v2`。任何已批准字段变化都需要新批准。类型化 `sign` 入口接收 SignRequest；低层 ExecuteRequest 还表示根派生，不是无限制的原始字节签名入口。

账户变更使用 `approval_message`、`dmsg/account/v2` 域和 `(expected_version, command)`。恢复再次确认使用 `recovery_confirmation_message` 和恢复密钥，不使用设备密钥。序号消耗、期限检查和权限决定发生在服务中。结果未知时，对账原请求，不自动创建新的签名操作。

## 编码与身份辅助函数

```rust
use dmsg_protocol::{canonical, decode_canonical, digest, normalize_handle};
use dmsg_types::Hash;

let value = Hash::new([0x42; 32]);
let encoded = canonical(&value);
assert_eq!(decode_canonical::<Hash>(&encoded).unwrap(), value);
assert_ne!(digest("example/a/v1", &value), digest("example/b/v1", &value));
assert_eq!(normalize_handle("Alice_01").unwrap(), "alice_01");
```

`canonical` 使用 RFC 8949 core deterministic CBOR。`digest(domain, value)` 为 `SHA256(CBOR([1, domain, value]))`。两者面向可信、可序列化的值；Serialize 实现失败时 panic，不检查业务语义。`decode_canonical` 限制输入为 65,536 字节并要求重新编码完全一致，对畸形或非规范记录返回错误。它不是签名产物解码器：外部 COSE 验签必须保留原始 protected 字节。

AccountId 为 12 字节二进制和 20 字符规范 Xid 文本，Hash/OpId 为 32 字节串。身份适配器要求明确的规范 URI 命名空间，以 `/` 或 `:` 结尾且无 query/fragment；不会发现服务或确认账户存在。`validate_origin` 接受精确 HTTPS origin 或 32 个 a..p 字母的 Chrome extension ID，不允许尾随路径。语法检查不能证明真实浏览器来源。

业务时间戳和时长使用毫秒，Statement.issued_at 使用秒，ICRC created_at_time 使用纳秒。`expiry` 要求截止时间严格晚于当前时间且不超过给定的最大剩余时长。`check_sequence` 对旧序号返回 ResultExpired，对未来或耗尽序号返回 VersionConflict，不更新计数器。`price` 使用固定 PANDA 价格表和 8 位小数，要求名称已经验证；它不是通用代币价格预言机。

## 证据与时间戳边界

`verification_report` 对有效签名返回 Verified。内嵌文本为 Verified；传入的原文必须逐字节匹配或 SHA-256 匹配。没有原文的摘要为 NotProvided。issuer 绑定、授权和当前状态保持 NotChecked。时间戳不存在时为 NotProvided，附加不透明 token 后为 NotChecked。验证失败返回错误，而不是返回一个带成功签名标记的报告。

调用 `match_execution_receipt` 前，先独立验证可信 IC 根、预期 user canister、certificate、witness、请求路径和叶值。`execution_receipt_key` 构造单段原始路径 `b"execution/" || account_id[12] || request_id[32]`。匹配器要求 schema 1 和 Completed，然后匹配 issuer、待签字节摘要、公钥指纹和原始签名摘要。它不独立检查回执的账户/请求 ID、origin、截止时间或外部项目权限；应认证预期路径，并单独执行其他政策检查。

| 辅助函数 | 精确操作 | 不执行的工作 |
| --- | --- | --- |
| `signature_digest` | 对原始签名字节计算 SHA-256，用于执行回执 | 验证签名或 dMsg profile |
| `timestamp_imprint` | SHA-256(CBOR(signature bstr))，包含字节串头，要求外层规范编码 | 验证签名、获取 TSA token 或核对 TSA 信任 |
| `attach_unverified_timestamp_token` | 验证产物后，将不透明 token 字节附加到非保护头 270 | 验证 CMS、imprint、证书链、TSA 政策或撤销状态 |

token 上限为 131,072 字节，编码后的 COSE_Key 上限为 2,048 字节，COSE_Sign1 产物上限为 196,608 字节。已有 token 时，附加函数返回 VersionConflict。可信时间戳验证、TSA 网络调用、SCITT 透明服务、归档和链上锚定不属于本库范围。

## 错误、协议依据与验证

函数返回 `dmsg_types::Result<T>`。常见错误包括：语义输入错误为 InvalidInput；字节、签名或绑定不匹配为 IntegrityFailed；不支持的算法或 profile 语义为 UnsupportedProtocol；大小越界为 QuotaExceeded。准确映射见各 API 的 rustdoc。诊断字符串不是稳定的机器错误码。网络错误由应用的传输层处理，不由这个本地库处理。

对接时使用同一提交的[公开协议](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README.md)、[CDDL](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/statements.cddl)、[类型指南](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/README.md)和[测试向量](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/tests/protocol_vectors.json)。链接指向 main，可能变化。文档 profile 媒体类型是项目实验名称，不是已注册的标准。

从 workspace 根目录执行：

```sh
cargo test -p dmsg_protocol -p dmsg_types --locked
RUSTDOCFLAGS="-D warnings" cargo doc -p dmsg_protocol --no-deps --locked
cargo clippy -p dmsg_protocol --all-targets --locked -- -D warnings
cargo run -p dmsg_types --example protocol_vectors --locked > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

英文 README 作为 crate 文档，其 Rust 示例会执行 doctest。两版示例代码保持一致，只翻译注释。`missing_docs` 警告帮助维护 API 注释覆盖率。

离线验签运行 `cargo run -p dmsg_protocol --example verify -- artifact.cbor`。输入是包含 cose_sign1 和 cose_key 字节串的 CBOR SignedArtifact 记录，不是单独的 COSE_Sign1 文件。输出只报告数学验证。性能基准使用 `cargo bench -p dmsg_protocol --bench validation --locked`，衡量宿主机 Rust 执行，不代表 canister 指令数或端到端延迟。真实 Wasm 联调使用 `POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh`。

## 维护者发布说明

先发布 dmsg_types，再发布 dmsg_protocol。后者的依赖同时指定本地路径与版本 0.1.0；Cargo 在发布包中使用 registry 版本。应让版本约束与公开合同保持一致。

Cargo 会从规范化后的 dmsg_types 发布包 manifest 中省略仅含 path 的 dmsg_protocol 开发依赖，从而避免发布依赖循环；但仓库合同测试和向量示例仍需要 checkout 的开发依赖。应在 workspace 执行它们，发布包中的 dmsg_types 测试集不等同于仓库测试环境。

先验证 dmsg_types 发布包。其版本在 registry 可用后，用 `cargo package -p dmsg_protocol --locked` 和 `cargo publish -p dmsg_protocol --dry-run --locked` 检查实际协议包，再发布。正常打包依赖解析中，本地存在 dmsg_types 不能代替其 registry 可用性。开发接口不承诺兼容早期实验编码；生产部署、外部服务、容量和审计仍需单独验收。
