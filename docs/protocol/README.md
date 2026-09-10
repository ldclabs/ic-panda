# dMsg 公开协议：声明与 ICP 控制接口

当前实现采用 [Statement v3 设计](statement-v3-design.md)：两个文档 profile v1、12 字节 Xid 账户、执行批准域 v3。具体字节格式见 [statements.cddl](statements.cddl)，ICP 接口以各 canister 的 `.did` 为准。profile 媒体类型为本项目实验名称，尚未注册或被标准组织采纳。

`dmsg_types` 只定义公开数据；`dmsg_protocol` 实现标准编码、profile 验证和身份适配；`dmsg_runtime` 与各 canister 保存内部状态。第三方独立验签无需实现账户数据库、名称或付费投递业务。

## 交换对象

交换对象是 RFC 9052 tagged COSE_Sign1：`18([protected_bstr, unprotected_map, payload_bstr, signature_bstr])`。Rust `Statement { issuer, subject, issued_at, content }` 是准备/解析视图，**不是额外的 wire payload 包装**。`SignedArtifact { cose_sign1, cose_key }` 是可选传输 DTO。

| 内容 | 保护头 typ（16） | payload | 其他保护头 |
| --- | --- | --- | --- |
| 文本 | `application/vnd.dmsg.text-statement+cose;v=1` | 原始 UTF-8，1..4096 字节 | 3=`text/plain;charset=utf-8`，crit=[15,16] |
| 摘要 | `application/vnd.dmsg.digest-statement+cose;v=1` | 原文 SHA-256，32 字节 | 258=-16，可选 259=原文媒体类型、260=原文 URI，crit=[15,16,258] |

二者均要求保护头 alg（1）、kid（4）、CWT claims（15）和 typ（16）。claims 包含必需 `iss`（1）、可选 `sub`（2）和可选 `iat`（6）。摘要 profile 按 RFC 9995 禁止头 3，文件长度不是必填字段。两种 profile 均没有 request_id、origin、audience 或执行截止时间。

`iss` 表示签署者；`sub` 表示被声明对象，允许省略。`iat` 是 i64 Unix **秒**，是签署者的时间声明，不是可信时间戳。普通文档签名不因执行批准到期而失效。含受众、授权期限或新业务语义的声明需要单独定义 profile，当前入口拒绝未知 typ、claims、保护头或关键语义。

## 身份与字节规则

`iss` 使用规范的绝对 ASCII URI，1..8192 字节，无凭据、控制字符或错误的百分号转义。创建端必须预先确定规范字符串；签署和验证不会隐式改写 URL。`sub` 为 1..8192 UTF-8 字节的 StringOrURI，无控制字符；含冒号时须符合相同 URI 规则。普通文本不作 Unicode 归一化，也不裁剪空白。URI 只作标识，不触发网络发现。

| 类型 | 二进制 | 文本 |
| --- | --- | --- |
| dMsg `AccountId` | `ic_auth_types::Xid` 别名，Candid blob / CBOR bstr，12 字节 | 规范 Xid：20 字符小写 base32hex，末字符为 0 或 g |
| ICP Principal | Candid principal / CBOR 原始 bstr，0..29 字节 | 标准 Principal 文本；空字节管理 canister 也是合法表示 |
| 设备、操作、SHA-256 | 各自语义类型，32 字节 | 桥接口明确指定编码，不根据长度推断身份类型 |
| COSE kid | 不透明 bstr，当前 profile 限 1..256 字节 | 无隐含账户语义 |

身份适配器接收明确命名空间：`account_issuer("https://dmsg.example/u/", account)` 或 `principal_issuer("https://id.example/ic/mainnet/", principal)`。命名空间为固定 URI 前缀，以 `/` 或 `:` 结束，无 query/fragment，最多 8128 字节。账户迁移不改变身份 URI；不补零、截断或哈希身份来适配长度。

新消息使用 RFC 8949 §4.2.1 core deterministic CBOR：最短编码、按键编码字节排序、无浮点、重复键或尾字节。现有 COSE 验签保留原 protected 字节，不能重排后验签。本地签署入口只接受按 profile 规范准备的输入。待签结构最多 65536 字节，COSE_Key 最多 2048 字节，签名产物最多 196608 字节；TSA token 最多 131072 字节。库解码器设有递归限制，SDK 另外限制 24 层和 50000 个节点。

ICP 业务时间、批准期限为 u64 Unix **毫秒**，使用 `now < expires_at`。certificate 和 ICRC `created_at_time` 为纳秒；账本重试保留原值。CBOR u128 超过 u64 时用 tag 2 最短大端 bstr；不能经 JS Number 转换。ICRC Account 是 `{owner: bstr, subaccount: bstr .size 32 / null}`。

浏览器 JSON 协议为 `dmsg-extension/3`：accountId 为 Xid 文本，SHA-256/requestId/nonce 为小写 hex，大整数为十进制字符串；statement 的 issuedAt 为秒，外层 expiresAt 为毫秒。普通 Rust serde JSON 输出不等同于此桥协议。

## 签名与密钥

待签字节严格为 `CBOR(["Signature1", protected_bstr, h'', payload_bstr])`，external_aad 为空。

| 算法 | COSE alg | 签名 | 公钥 |
| --- | --- | --- | --- |
| Ed25519（基础） | -19 | 直接签 Sig_structure | OKP，crv=6，x=32 字节 |
| ES256K（可选） | -47 | SHA-256(Sig_structure)，ECDSA r\|\|s | EC2，crv=8，x/y 各 32 字节；验证也接受压缩 y |

dMsg 不再提供 BIP340 入口或私有算法标签。失败不会自动切换算法。public-only COSE_Key 禁止私钥 d（-4）；alg 必须匹配，key_ops 存在时须允许 verify，kid 存在时须与保护头匹配。

公钥指纹采用 RFC 9679 SHA-256：只编码所需公开参数，排除 kid、alg、key_ops；EC2 y 必须先展开为完整坐标。同一密钥的压缩和非压缩表示因此得到同一指纹。ICP 签名适配器用该指纹作为 kid；通用 profile 不要求所有实现这样选 kid。

ICP 固定派生域升级为 `dmsg/formal/v2`，derivation_version=2；vetKD input 为 `[account_id, generation]`，context 为 `["dmsg/content-root/v2", environment, 2]`。Xid 改变密码派生输入，使用新开发实例，不把旧 ID 截短后当成同一密钥。

## ICP 执行批准与回执

`digest(domain, value) = SHA256(CBOR([1, domain, value]))`。

```text
request_id = digest("dmsg/execution-request/v2",
  [account_id, security_epoch, device_id, sequence])

kind = {Sign: {
  key: {purpose, algorithm, generation: 1},
  to_be_signed: Sig_structure_bstr,
  public_key_fingerprint: RFC9679_SHA256,
  origin: checked_browser_origin
}}

digest("dmsg/device-approval/v2", [
  target_user_principal_bytes, account_id, "dmsg/execute/v3",
  device_id, security_epoch, sequence, request_id, expires_at,
  digest("dmsg/execute/v3", [kind, max_cycles])
])
```

设备严格 Ed25519 签署此摘要。purpose 根据内容选 `Statement` 或 `FileAttestation`，algorithm 为 `Ed25519` 或 `EcdsaSecp256k1`。根派生 kind 为 `{Derive: {generation, root_op_id: bstr/null, transport_key: bstr .size 48}}`。账户变更使用独立 `dmsg/account/v2` 域，命令为 `[expected_version, AccountCommand]`。CBOR 无负载枚举为名称字符串，有负载枚举为单项 map，Option 为值或 null。

签署前冻结完整保护头、payload、密钥指纹与批准上下文。user home 检查 issuer 是否属于当前账户；cose home 核对实际派生密钥的 kid 和指纹。浏览器 origin 最多 256 字节，为精确 HTTPS origin 或 Chrome extension origin；由扩展核实，设备签名不独立证明浏览器来源。

幂等作用域为 account_id/request_id；同 ID 不同参数拒绝。未知结果对账原请求。严格设备序号和执行水位在结果清理后继续阻止重放。相同内容可以产生相同签名，不能把签名摘要当作所有业务操作的唯一 ID。

`get_execution_receipt(account_id, request_id)` 返回 ICP 认证叶，查询要求账户认证。路径为 `b"execution/" || account_id[12] || request_id[32]`。`ExecutionReceipt` 保存 issuer、设备/epoch、批准时间/期限、origin、费用上限、状态、待签字节 SHA-256、公钥指纹和签名原始字节 SHA-256。它不改变可移植签名产物。升级从稳定执行记录重建叶，清理结果时删除叶。

验证回执必须先验证指定 user canister 的 IC certificate、witness、路径和值，再匹配 Completed 状态、issuer、待签摘要、公钥指纹和签名摘要。SDK `verifyExecutionReceipt` 完成这两步；Rust `match_execution_receipt` 仅做绑定检查，调用者负责认证 certificate。历史回执不套用账户安全快照的 60 秒新鲜度；当前授权状态另行查询。回执证明本服务记录的执行授权，不自动证明外部项目权限或当前设备状态。

## Xid 发号

用户 canister 使用 `ic_auth_types::XidGenerator` 持久化发号，其输出为 `timestamp_seconds[4] || allocator_fingerprint[5] || counter[3]`。指纹取 `digest("dmsg/account-id-generator/v1", ["dmsg", environment, issuer_namespace, creating_canister])` 前 5 字节，并在配置中单独保存完整 namespace digest 用于升级校验。同秒/时钟回退继续计数，新秒从 0 开始；时间溢出或计数耗尽明确失败。

认证、设备 PoP、配额和唯一绑定校验通过后，同一无 await 消息提交账户、认证索引、配额和分配器。已有认证创建重试返回原账户。无 `raw_rand` 或异步创建暂存表。当前固定单 user home；未来多分配器必须先登记并排除指纹碰撞，不能把截断哈希当成绝对全局唯一保证。

## 时间戳与验证结果

RFC 9921 CTT 的 SHA-256 MessageImprint 为 `SHA256(CBOR(signature_bstr))`，包括 bstr 头，区别于执行回执中的原始签名摘要。`timestamp_imprint` 要求规范外层编码；`attach_unverified_timestamp_token` 只组装头 270，不申请或信任 TSA。CMS 签名、imprint、证书链、用途、政策与状态需要独立验证。

`verify_artifact` / SDK `verifyDocumentArtifact` 检查 profile 和数学签名；`verification_report` 分别报告 signature、content、issuer_binding、authorization、timestamp、current_status。摘要原文未提供时 content 为 NotProvided，未认证身份或 TSA 时为 NotChecked。不能把随包公钥或单个成功布尔值当作完整证明。

当前不提供 TSA 网络/CMS 验证、SCITT 透明服务、长期归档或链上 anchor 入口。可选付费投递、名称与 ICP 控制合同分别维护。

## 验证与依据

```sh
cargo test -p dmsg_types -p dmsg_protocol
cargo run -p dmsg_types --example protocol_vectors > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

签名、身份头和 profile 依据：[RFC 9052](https://www.rfc-editor.org/rfc/rfc9052.html)、[RFC 9597](https://www.rfc-editor.org/rfc/rfc9597.html)、[RFC 8392](https://www.rfc-editor.org/rfc/rfc8392.html)、[RFC 9596](https://www.rfc-editor.org/rfc/rfc9596.html)。摘要、公钥指纹和时间戳分别依据 [RFC 9995](https://www.rfc-editor.org/rfc/rfc9995.html)、[RFC 9679 §4.2](https://www.rfc-editor.org/rfc/rfc9679.html#section-4.2)、[RFC 9921](https://www.rfc-editor.org/rfc/rfc9921.html)。SCITT 的声明/证据分工参考 [RFC 9943](https://www.rfc-editor.org/rfc/rfc9943.html)，当前普通文档 profile 不宣称实现其透明服务。
