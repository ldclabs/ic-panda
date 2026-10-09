# dmsg_protocol

> 第三方集成的新公共契约与实现边界见 [integration](../../docs/protocol/integration.md)。公共契约、登记和专用认证已实现；通用 v2 checkout/membership 消费者仍待后续工作包，不自动回退或混用。

[English](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_protocol/README.md) | 简体中文

dMsg 的确定性编码、文档签名验证和请求辅助库。本库在 [dmsg_types](https://github.com/ldclabs/ic-panda/tree/main/src/dmsg_types) 上实现公开协议，只执行本地计算：不调用 ICP、不查询账本、不操作 canister 存储、不进行网络发现，也不执行账户授权。

可用于准备可移植 COSE 文档、验证返回的签名产物、构造设备批准摘要，以及将文档与已经独立认证的执行回执进行绑定检查。通用辅助函数在 crate 根重导出；商业与共享会员辅助函数分别从公开的 `billing`、`membership` 模块导入。

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
dmsg_types = "0.2"
dmsg_protocol = "0.3"
```

正式依赖方向是 **dmsg_protocol → dmsg_types**。仓库中反方向的引用只是开发依赖，用于 dmsg_types 的合同测试和向量生成；仅使用类型的应用不会引入本协议库。使用方直接从 dmsg_types 导入公开 DTO 和错误类型。下文示例还使用 `ed25519-dalek = "3"`，批准示例另外使用 `candid = "0.10"`。

发布配置不表示版本已经上架 crates.io。当前 checkout 中 dmsg_types 为 0.2.0，dmsg_protocol 为 0.3.0；发布顺序见文末说明。

## API 导航

| 任务 | 入口 | 仍由调用方负责的部分 |
| --- | --- | --- |
| 编码协议记录 | `canonical`, `decode_canonical`, `sha256`, `digest` | 使用准确的公开类型、域和数据结构，并进行业务验证 |
| 准备文档 | `validate_statement`, `statement_purpose`, `prepare_cose` | 取得用户同意、选择可信密钥并调用签名器 |
| 组装与验证 | `PreparedAttestation::finish`, `verify_artifact` | 确认 issuer 身份、授权与当前状态 |
| 描述签名公钥 | `cose_algorithm`, `public_cose_key`, `key_thumbprint` | 认证公钥来源，不把指纹当成所有权证明 |
| 标识签署者 | `account_issuer`, `validate_uri`, `validate_namespace` | 配置可信命名空间，并与已认证证据绑定 |
| 准备 ICP 批准 | `prepare_attestation`, `app_action_statement`, `approval_message`, `ATTEST_APPROVAL_DOMAIN`, `attest_approval_command`, `DERIVE_APPROVAL_DOMAIN`, `derive_approval_command`, `execution_request_id` | 获取当前账户/设备状态，用设备密钥签署 Sig_structure 与摘要并提交到 user home |
| 验证请求输入 | `DeviceInputExt`, `CoseInitExt`, `validate_origin`, `validate_transport_key` | 执行服务端授权、私钥持有证明检查与状态转换 |
| 配置 COSE 执行器 | `content_root_context`、`cose_pins::master_key_pin`（feature `cose-pins`） | 先创建 canister：pin 依赖其 ID；初始化后核对取得的指纹 |
| 核对执行证据 | `signature_digest`, `execution_receipt_key`, `match_execution_receipt` | 先验证 IC certificate、预期 canister、witness、路径和叶值 |
| 绑定根与密钥 | `root_recipients_digest`, `root_bundle_digest`, `recovery_device_message`, `controller_pop_message` | 客户端构造根包与证明，服务重算并比对 |
| 处理名称 | `normalize_handle`, `price`, `charge_terms_digest` | 在对应服务中执行名称与账本操作 |
| 检查基础约束 | `authenticated`, `nonzero`, `expiry`, `check_sequence`, `verify` | 提供可信 caller、时间和状态；辅助函数不读取或修改它们 |

Rustdoc 为各入口说明参数、失败行为和信任边界。`verify` 是原始严格 Ed25519 验签；`verify_artifact` 还验证 COSE 文档 profile。`authenticated` 仅排除匿名和管理 canister Principal，不证明账户成员身份。

商业辅助函数使用 `dmsg_protocol::billing::monthly_allowance` 和 `dmsg_protocol::{membership::mul_div, integration::required_panda_stake}` 等路径。金额使用整数原子单位，商业时间使用 UTC Unix 毫秒；报价和本金门槛向上取整，月度额度累计加权时长后统一向下取整。摘要构造不执行授权，具体字段校验范围见 rustdoc。

## 本地签署并验证文档

这个完整示例使用确定性的**测试密钥**。生产签名应使用安全生成或外部管理的密钥。

```rust
use dmsg_protocol::{account_issuer, key_thumbprint, prepare_attestation, sha256, verify_artifact};
use dmsg_types::{AccountId, Hash, Statement, StatementContent};
use ed25519_dalek::{Signer, SigningKey};

let signer = SigningKey::from_bytes(&[7; 32]); // 仅用于测试。
let public = Hash::new(signer.verifying_key().to_bytes());
let original = b"Release 1.0 specification";
let statement = Statement {
    issuer: account_issuer("https://example.org/u/", &AccountId([1; 12])),
    subject: Some("release/specification".into()),
    issued_at: Some(1_800_000_000), // 声明的 Unix 秒，不是可信时间。
    content: StatementContent::Digest {
        sha256: sha256(original),
        content_type: Some("text/plain".into()),
        location: None,
    },
};
// kid 是公钥的 RFC 9679 指纹。
let prepared = prepare_attestation(&statement, &public).unwrap();
let thumbprint = prepared.thumbprint;
let signature = signer.sign(&prepared.to_be_signed).to_bytes();
let artifact = prepared.finish(&signature).unwrap();
// 验证后的声明承诺了原文字节的 SHA-256。
assert_eq!(verify_artifact(&artifact).unwrap(), statement);
assert_eq!(key_thumbprint(&artifact.cose_key).unwrap(), thumbprint);
```

`prepare_cose` 返回未签名消息和 `Sig_structure = CBOR(["Signature1", protected_bstr, h'', payload_bstr])`。文本使用原始 UTF-8（1..4096 字节），摘要文档使用原文字节的 32 字节 SHA-256。`FileStatement` 采用确定性 CBOR payload，联合包含原样文本、文件 SHA-256 和可选媒体类型/位置，使用 `FILE_STATEMENT_PROFILE` 与 `Statement` 密钥用途；封闭 schema 和边界见[公开协议](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README_zh.md)。Rust Statement 枚举不是额外的 wire payload。issuer、可选 subject 和声明的 issued_at 是受保护 CWT claims；请求 ID、浏览器 origin 和执行截止时间是单独的执行元数据。

| 算法 | COSE 标签 | 签名器输入 | 公钥（`prepare_attestation`）/ 签名（`finish`） |
| --- | --- | --- | --- |
| Ed25519 | -19 | 完整 tbs 字节 | 原始 32 字节公钥；64 字节签名 |

使用内部计算哈希的 API 时，应传入 tbs，避免重复哈希。vetKD 不是文档签名算法。`PreparedAttestation::finish` 组装并校验结构，但**不验证签名**；`verify_artifact` 才执行验签。`public_cose_key` 返回编码后的 COSE_Key，输入则是原始公钥。`key_thumbprint` 对必需的公开 COSE 参数计算摘要，排除 kid/alg/key_ops；它不是原始公钥 SHA-256，也不是完整公钥验证器。

`prepare_attestation` 以签名公钥的 RFC 9679 指纹作为 kid，返回 Sig_structure、编码后的公钥、指纹和密钥用途。需要其他 kid 时，签署 `prepare_cose` 返回的消息，并用 `public_cose_key` 编码公钥。接收产物时仍需数学验签。

## 构造 ICP 设备批准

以下示例构造本地认证请求和设备签名，不联系 canister，也不授予权限。真实账户 ID、设备 ID、epoch 和序号必须从已配置、已认证的服务取得。示例 ID 和密钥仅用于演示。

```rust
use candid::Principal;
use dmsg_protocol::{
    approval_message, attest_approval_command, execution_request_id, prepare_attestation,
    ATTEST_APPROVAL_DOMAIN,
};
use dmsg_types::{AccountId, Approval, AttestRequest, Hash, Statement, StatementContent};
use ed25519_dalek::{Signer, SigningKey};

let account_id = AccountId([1; 12]);
let device_id = Hash::new([2; 32]);
let device_key = SigningKey::from_bytes(&[9; 32]); // 仅用于测试。
let device_public: Hash = device_key.verifying_key().to_bytes().into();
let security_epoch = 1;
let sequence = 0;
let statement = Statement {
    issuer: "https://example.org/signers/alice".into(),
    subject: None,
    issued_at: None,
    content: StatementContent::Text("Approve release 1.0".into()),
};
// 设备签署精确的 Sig_structure；kid 是设备公钥的 RFC 9679 指纹。
let prepared = prepare_attestation(&statement, &device_public).unwrap();
let mut request = AttestRequest {
    account_id,
    statement,
    origin: "https://example.org".into(),
    signature: device_key.sign(&prepared.to_be_signed).to_bytes().into(),
    approval: Approval {
        device_id,
        security_epoch,
        sequence,
        request_id: execution_request_id(&account_id, security_epoch, device_id, sequence),
        expires_at: 1_800_000_060_000, // Unix 毫秒；实际使用有效的截止时间。
        signature: Default::default(), // 不进入批准摘要。
    },
};
let home_user = Principal::from_slice(&[1, 1]); // 部署示例值。
let digest = approval_message(
    home_user,
    &request.account_id,
    ATTEST_APPROVAL_DOMAIN,
    &attest_approval_command(&request.statement, &request.origin, &request.signature),
    &request.approval,
);
request.approval.signature = device_key.sign(digest.as_slice()).to_bytes().into();
assert_eq!(prepared.thumbprint, dmsg_protocol::key_thumbprint(&prepared.cose_key).unwrap());
```

批准把 `ATTEST_APPROVAL_DOMAIN`（`dmsg/attest/v1`）和 `attest_approval_command`（即 `(statement, origin, signature)` 三元组）传给 `approval_message`，再包装到 `dmsg/device-approval/v2`；user canister 校验同一组值、设备对 Sig_structure 的签名，并写入认证回执。任何已批准字段变化都需要新批准。应用动作用 `app_action_statement` 由 `AppActionAttestRequest` 构造声明，并绑定 `action.origin`。

账户变更使用 `approval_message`、`dmsg/account/v2` 域和 `(expected_version, command)`。恢复设备的根派生使用 `DERIVE_APPROVAL_DOMAIN` 与 `derive_approval_command`，即 `(generation, transport_public_key, max_cycles)`。序号消耗、期限检查和权限决定发生在服务中。结果未知时，对账原请求，不自动创建新的签名操作。

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

AccountId 为 12 字节二进制和 20 字符规范 Xid 文本，Hash/OpId 为 32 字节串。身份适配器要求明确的规范 URI 命名空间，以 `/` 或 `:` 结尾且无 query/fragment；不会发现服务或确认账户存在。`validate_origin` 接受精确 HTTPS origin 或 32 个 a..p 字母的 Chrome extension ID，不允许尾随路径；Local 部署还接受精确的环回 HTTP origin。user 与 COSE home 按各自部署环境检查。语法检查不能证明真实浏览器来源。

业务时间戳和时长使用毫秒，Statement.issued_at 使用秒，ICRC created_at_time 使用纳秒。`expiry` 要求截止时间严格晚于当前时间且不超过给定的最大剩余时长。`check_sequence` 对旧序号返回 ResultExpired，对未来或耗尽序号返回 VersionConflict，不更新计数器。`price` 使用固定 PANDA 价格表和 8 位小数，要求名称已经验证；它不是通用代币价格预言机。

## 证据与时间戳边界

`verify_artifact` 只检查 profile 和数学签名。原文需由调用方对照验证后的声明自行核对：内嵌文本必须逐字节匹配，摘要或文件声明必须匹配原文件的 SHA-256；内嵌意见文本不代表已核对文件。issuer 绑定、授权、当前状态和时间戳信任都不在检查范围内。

调用 `match_execution_receipt` 前，先独立验证可信 IC 根、预期 user canister、certificate、witness、请求路径和叶值。`execution_receipt_key` 构造单段原始路径 `b"execution/" || account_id[12] || request_id[32]`。匹配器要求 schema 2，然后匹配 issuer、待签字节摘要、公钥指纹和原始签名摘要。它不独立检查回执的账户/请求 ID、origin、截止时间或外部项目权限；应认证预期路径，并单独执行其他政策检查。

`signature_digest` 对执行回执使用的原始签名字节计算 SHA-256，不验证签名或 dMsg profile。

验证接受非保护头 270 中不超过 131,072 字节的不透明 token，但不检查它。编码后的 COSE_Key 上限为 2,048 字节，COSE_Sign1 产物上限为 196,608 字节。可信时间戳验证、TSA 网络调用、SCITT 透明服务、归档和链上锚定不属于本库范围。

## 错误、协议依据与验证

函数返回 `dmsg_types::Result<T>`。常见错误包括：语义输入错误为 InvalidInput；字节、签名或绑定不匹配为 IntegrityFailed；不支持的算法或 profile 语义为 UnsupportedProtocol；大小越界为 QuotaExceeded。准确映射见各 API 的 rustdoc。诊断字符串不是稳定的机器错误码。网络错误由应用的传输层处理，不由这个本地库处理。

对接时使用同一提交的[公开协议](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/README_zh.md)、[CDDL](https://github.com/ldclabs/ic-panda/blob/main/docs/protocol/statements.cddl)、[类型指南](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/README_zh.md)和[测试向量](https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_types/tests/protocol_vectors.json)。链接指向 main，可能变化。文档 profile 媒体类型是项目实验名称，不是已注册的标准。

从 workspace 根目录执行：

```sh
cargo test -p dmsg_protocol -p dmsg_types --locked
RUSTDOCFLAGS="-D warnings" cargo doc -p dmsg_protocol --no-deps --locked
cargo clippy -p dmsg_protocol --all-targets --locked -- -D warnings
cargo run -p dmsg_types --example protocol_vectors --locked > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

英文 README 作为 crate 文档，其 Rust 示例会执行 doctest。两版示例代码保持一致，只翻译注释。`missing_docs` 警告帮助维护 API 注释覆盖率。

COSE 部署 pin 运行 `cargo run -p dmsg_protocol --features cose-pins --example cose_pins -- <canister-id> Production`，按主网 master 公钥离线派生并打印 `CoseInit` 的 `master` 字段与内容根公钥（第三个参数 `pocketic` 改用 PocketIC 与本地 dfx 的 key）。离线验签运行 `cargo run -p dmsg_protocol --example verify -- artifact.cbor`。输入是包含 cose_sign1 和 cose_key 字节串的 CBOR SignedArtifact 记录，不是单独的 COSE_Sign1 文件。输出只报告数学验证。性能基准使用 `cargo bench -p dmsg_protocol --bench validation --locked`，衡量宿主机 Rust 执行，不代表 canister 指令数或端到端延迟。真实 Wasm 联调使用 `POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh`。

## 维护者发布说明

先发布 dmsg_types，再发布 dmsg_protocol。后者的依赖同时指定本地路径与版本 0.2.0；Cargo 在发布包中使用 registry 版本。应让版本约束与公开合同保持一致。

0.2.0 移除了 0.1.x 中没有生产代码使用的公开项。`finish_cose` 改为 `parse_signing_input(tbs)?.into_signature(public)?.finish(signature)`；`ExecuteRequestExt::approval_message` 改为调用 `approval_message`：文档证明用 `ATTEST_APPROVAL_DOMAIN` 和 `attest_approval_command`，根派生用 `DERIVE_APPROVAL_DOMAIN` 和 `derive_approval_command`；`ExecutionResult::output()` 改为匹配 `ExecutionOutcome::Completed`。`dmsg_types::handle::HandleInit` 新增必填字段 `governance`，并以 `environment`、`issuer_namespace` 和只能追加的 `user_homes` 取代 `home_user`，按账户 ID 的分配器指纹路由。新增的 `handle_bucket` 与 `HANDLE_BUCKET_BITS` 规定注册表在何处认证名称。`CoseInit` 以 `user_homes` 取代 `initial_home_user` 并新增 `governance`；`PaymentInit` 以 `environment`、`issuer_namespace` 和 `user_homes` 取代 `home_user`，`PaymentConfiguration` 改列 `user_homes`；`DirectoryInit` 新增 `governance`。新增的 `agent::check_user_home`、`validate_user_homes`、`account_home`、`is_account_home`、`validate_custom_domains` 和 `MAX_USER_HOMES`（64）让各服务使用同一套分配器指纹路由。`MIN_HANDLE_PRICE`（即 7–20 字节名称的 `price`）从 5,000 PANDA 改为 100 PANDA，与旧注册表的现行价格一致。 `ProductRegistration` 以只能追加的 `beneficiary_authorities` 取代 `beneficiary_authority`，`validate_subject` 要求主体的 authority 在列表中；`CommerceInit` 以 `limits: CommerceLimits` 取代 `max_subjects` 与 `daily_orders`。新增 `dmsg_types::integration::LEASE_RENEW_WINDOW_MS`（10 分钟），是 commerce 与 membership 共用的租约续期窗口。`CommerceLimits` 新增 `calls_per_caller`，新增的 `CommerceStats` 报告 commerce 的实时记录数；`PandaServiceConfig` 以只能追加的 `commerce_homes: Vec<CommerceHome>`（每个 user home 的 commerce）取代 `commerce_canister`，并新增 `qualifications_per_minute`。`ExecutionResult` 新增 `cycles_charged`，即返回的管理调用实际消耗的阈值费用，COSE 与 user 的预算都结算到它；新增的 `CoseStats` 报告 COSE 执行器的实时计数。`content_root_context` 构造 vetKD 内容根 context，可选 feature `cose-pins` 提供 `cose_pins::master_key_pin` 与 `cose_pins` 示例，用于离线计算 COSE master key pin。

0.2.0 同时移除阈值文档签名与托管 controller 签名：删除 `SignRequest`、`AppActionSignRequest`、`AgentEventSignRequest`、`SigningKeyRef`、`KeySelector`、`ExecutionKind`、`ExecutionOutput`、`RootTarget`、`RecoveryPolicy`、`RecoveryConfirmation`、`SignRequestExt`、`KeyRequestExt`、`recovery_confirmation_message`、ES256K 支持（`k256`）与 `AgentController` 用途。文档由设备密钥签名：`prepare_attestation` 返回 Sig_structure 与指纹，`PreparedAttestation::finish` 附上设备签名，`AttestRequest` / `AppActionAttestRequest` 携带设备签名，批准在 `ATTEST_APPROVAL_DOMAIN` 下绑定 `(statement, origin, signature)`；`ExecutionReceipt` 为 schema 2，不含 `max_cycles`。`ContentRootRef` 改为 `{ generation, suite: "dmsg-root-v2", bundle_digest, recipients_digest, body_digest }`，新增 `root_recipients_digest` 与 `root_bundle_digest`；`DeriveRootRequest` 指定已提交代次，只接受登录恢复登记的设备（`DERIVE_APPROVAL_DOMAIN`、`derive_approval_command`）。恢复改为登录授权：`RecoveryRequest` 去掉 `generation`，`request_recovery` 参数为 `(account_id, request, device_proof)` 并用 `recovery_device_message`，`DisputeRecovery { op_id }` 即取消，`SetRecoveryDelay` 取代 `SetRecovery`/`ConfirmRecovery`，`AccountInfo` 暴露 `recovery_delay_ms`、`pending_recovery` 与 `recovered_device`。`RegisterController` 携带 `controller_pop_message` 构造的 `proof`。`CoseInit` 只有一个 `master: MasterKey`，`KeyDescriptor` 描述共享的内容根公钥，`ExecutionGrant` 携带 `generation` 与 `transport_key`，`SecuritySnapshot` 为 schema 4、不含账户状态与恢复公钥。`HandleInit` 新增 `registration_homes`；`AppRegistration` 去掉 `user_homes` 与 `cose_homes`；`ExecutionWeights` 去掉 `ecdsa_secp256k1`。

0.2.0 同时让应用动作改由应用定义：删除 `AppActionCommand`、`ActionReviewOutcome`、`ActionRequestedChange` 与 `action_input_hash`，dMsg 不再包含 TokenList 的命令，也不再复刻 TokenList 的输入编码。应用登记 `AppRegistration.action_schema: Option<ActionSchema>`，有 `SignAction` 能力时必须提供，由 `validate_action_schema` 校验。`AppAction.command` 改为封闭 `ActionValue` 值模型上的 `ActionCommand { name, args: ActionArgs }`；`AppAction` 以不透明的 `actor` 字节取代 `actor_id`，以 `schema_hash`（`action_schema_hash`）取代 `input_hash`、`subject_hash`、`precondition_hash`、`role_snapshot_hash`、`signing_policy_hash` 与 `rule_set_hash`，产品自己的上下文由 `intent_hash` 承诺。`validate_action_command(action, schema)` 按 schema 校验命令，`validate_action_admission` 使用登记中的 schema。执行输入与签名命令一致由接收方校验，不由 dMsg 校验。

0.3.0 移除 `parse_signing_input`、`PreparedStatement`、`PreparedSignature`、`match_signing_result` 和 `agent::delegation_id_prefix`：文档改由设备签名后，已没有生产代码调用它们。用 `prepare_attestation(statement, signing_pub)?.finish(signature)` 组装产物；需要其他 kid 时，签署 `prepare_cose` 返回的消息并用 `public_cose_key` 编码公钥。执行回执用 `match_execution_receipt` 绑定。0.3.0 还删除了 `billing::usage_key`：user home 改以账户与月份为月账键后已没有服务使用它；新增 `account_admission_message`，用于 user home 核验的注册准入票据；`validate_ed25519_key` 改为公开。配套的 dmsg_types 改动新增 `user::AdmissionTicket`、`UserInit.admission_key`、`CreateAccount.admission` 和常量 `MAX_HOME_ACCOUNTS`（21,000,000）、`MAX_OPERATION_RECEIPTS`、`MAX_PENDING_BINDINGS`、`BINDING_ACCEPT_MS`、`MAX_ADMISSION_TTL_MS`、`MAX_HOURLY_ACCOUNT_CALLS`，并删除 `ExecutionUsage.held_units`。随 agent-protocols 0.11.3 取消 controller 继承、改由受限 controller 管理其上限内的凭证，`controller_pop_message` 不再接收被接管的代次，`HostedController` 与 `AccountCommand::RegisterController` 去掉 `supersedes`，`DirectoryInit.delegation_query_url` 改为 delegation 服务的 HTTPS origin `delegation_service`。

Cargo 会从规范化后的 dmsg_types 发布包 manifest 中省略仅含 path 的 dmsg_protocol 开发依赖，从而避免发布依赖循环；但仓库合同测试和向量示例仍需要 checkout 的开发依赖。应在 workspace 执行它们，发布包中的 dmsg_types 测试集不等同于仓库测试环境。

先验证 dmsg_types 发布包。其版本在 registry 可用后，用 `cargo package -p dmsg_protocol --locked` 和 `cargo publish -p dmsg_protocol --dry-run --locked` 检查实际协议包，再发布。正常打包依赖解析中，本地存在 dmsg_types 不能代替其 registry 可用性。开发接口不承诺兼容早期实验编码；生产部署、外部服务、容量和审计仍需单独验收。


应用动作 v1 是独立的签名 profile，使用 `AppAction` 密钥用途；各应用登记自己命令的 schema，详见
[profile 与实现边界](../../docs/protocol/app-action.md)。Rust COSE 准备与验证支持它；
`dmsg_user.attest` 与文档浏览器流程明确拒绝它，`attest_app_action` 是唯一入口。
