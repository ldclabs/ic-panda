# dmsg_protocol

公开协议的确定性编码、请求验证和独立签名验证实现。依赖公开数据类型和标准密码库，不调用 ICP，也不读写 canister 内存。

## 使用

```rust
use dmsg_protocol::verify_artifact;
use dmsg_types::{Result, SignedArtifact, Statement};
fn verify(download: &SignedArtifact) -> Result<Statement> {
    verify_artifact(download)
}
```

`verify_artifact` 检查 COSE 数学签名、受保护的算法/公钥标识、typ、CWT claims 与对应文档 profile。它不把随包提供的公钥当身份凭证，不验证历史授权，也不把时间戳头的存在当 TSA 验证成功。

## 远程签名

`prepare_cose(statement, algorithm, kid)` 返回 `Sign1Message` 和 RFC 9052 `Sig_structure` 字节。调用实际签名器后用 `finish_cose(to_be_signed, public_key, signature)` 返回 `SignedArtifact { cose_sign1, cose_key }`。内部使用固定版本的 `cose2`，新消息为 tagged COSE_Sign1，公钥为 public-only COSE_Key。

基础算法为 Ed25519（COSE -19）；可选 ES256K（-47）。已移除 BIP340 私有 profile。vetKD 不属于这套签名验证接口。

`SignRequestExt` 等扩展 trait 提供 ICP 请求的规范转换与批准摘要；平台原语仍由 canister 适配器调用。`canonical` 使用 RFC 8949 core deterministic CBOR；签名验证保留外部 COSE 的原始 protected-header 字节。

离线命令行验证示例：`cargo run -p dmsg_protocol --example verify -- artifact.cbor`。输入是包含两个字节串字段的 SignedArtifact CBOR 记录；输出仅报告数学验证。

## TSA 接点

`timestamp_imprint(cose_sign1)` 实现 RFC 9921 CTT 的 SHA-256 message imprint，包括 signature 字节串的 CBOR 头。`attach_unverified_timestamp_token` 可以组装 token 到非保护头 270；它和此函数都不申请令牌、不验证 CMS/X.509、不判断 TSA 资格。TSA 客户端、信任策略、完整证据归档和链上锚定尚未交付。

## 身份、指纹与认证回执

`account_issuer` / `principal_issuer` 按明确命名空间构造身份 URI，不根据字节长度猜类型。`parse_account_issuer` 严格检查命名空间与 Xid 规范文本。`key_thumbprint` 使用 RFC 9679 SHA-256，EC2 压缩坐标先展开，kid/alg/key_ops 不进入指纹。

`verification_report(artifact, original_content)` 区分 signature/content/issuer_binding/authorization/timestamp/current_status。`match_execution_receipt` 只匹配签名产物与回执；调用者必须先验证 IC certificate、canister、路径与 witness。不能直接信任随包提供的回执。

## 互操作

[CDDL 和字节规则](../../docs/protocol/README.md) 是实现依据。向量见 [protocol_vectors.json](../dmsg_types/tests/protocol_vectors.json)，独立 JS 编码器和 Node Ed25519 验证器分别复核编码、COSE Sig_structure、公钥和 CTT 输入：

```sh
cargo run -p dmsg_types --example protocol_vectors > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_protocol
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
