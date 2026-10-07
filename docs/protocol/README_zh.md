# dMsg 公开协议：声明与 ICP 控制接口

[English](README.md) | 简体中文

> 应用动作：[app-action v1](app-action.md) 定义闭合载荷、专用授权签署、浏览器确认及独立执行回执。通用文档接口拒绝该类型。
当前实现采用 [Statement v3 设计](statement-v3-design_zh.md)：三个文档 profile v1、12 字节 Xid 账户、执行批准域 v3。具体字节格式见 [statements.cddl](statements.cddl)，ICP 接口以各 canister 的 `.did` 为准。profile 媒体类型为本项目实验名称，尚未注册或被标准组织采纳。

`dmsg_types` 只定义公开数据；`dmsg_protocol` 实现标准编码、profile 验证和身份适配；`dmsg_runtime` 与各 canister 保存内部状态。第三方独立验签无需实现账户数据库、名称或付费投递业务。

## 交换对象

交换对象是 RFC 9052 tagged COSE_Sign1：`18([protected_bstr, unprotected_map, payload_bstr, signature_bstr])`。Rust `Statement { issuer, subject, issued_at, content }` 是准备/解析视图，**不是额外的 wire payload 包装**。`SignedArtifact { cose_sign1, cose_key }` 是可选传输 DTO。

| 内容 | 保护头 typ（16） | payload | 其他保护头 |
| --- | --- | --- | --- |
| 文本 | `application/vnd.dmsg.text-statement+cose;v=1` | 原始 UTF-8，1..4096 字节 | 3=`text/plain;charset=utf-8`，crit=[15,16] |
| 摘要 | `application/vnd.dmsg.digest-statement+cose;v=1` | 原文 SHA-256，32 字节 | 258=-16，可选 259=原文媒体类型、260=原文 URI，crit=[15,16,258] |
| 文件声明 | `application/vnd.dmsg.file-statement+cose;v=1` | 确定性 CBOR：文本与一个文件的 SHA-256，可选文件元数据 | 3=`application/cbor`，crit=[15,16]；禁止 258/259/260 |

`StatementContent::FileStatement { text, sha256, content_type, location }` 表示针对一个确定文件的声明。payload 为封闭整数键 map `{1: text, 2: sha256, ?3: content_type, ?4: location}`。文本保留原始 1..4096 UTF-8 字节；摘要为原文件精确字节的 SHA-256，编码为 32 字节 bstr。可选媒体类型与 URI 沿用摘要 profile 的校验边界，但位于此 payload 内。缺失可选键必须省略，不能编码为 null。payload 必须采用 RFC 8949 core deterministic CBOR（确定长度、最短编码、键排序），拒绝重复/未知键、tag 和尾随字节，编码上限为 16,384 字节。验签保留 COSE 的原始 protected 字节，同时要求此 payload 采用规范编码。

文本与文件摘要共同受签名保护。文本可表达赞成、反对或观察；此 profile 本身不授予发布/验收权限，也不证明签名人已审阅文件。`subject` 可关联项目或事项，SHA-256 字段确定精确文件版本。验证不会自动获取 `location`。需要机器可判定动作、角色或项目权限的业务应另设明确 profile。扩展桥接格式为 `{kind: "file_statement", text, sha256, contentType?, location?}`，SHA-256 使用小写十六进制；确认界面并列显示文本、摘要与原文件核对状态。

三者均要求保护头 alg（1）、kid（4）、CWT claims（15）和 typ（16）。claims 包含必需 `iss`（1）、可选 `sub`（2）和可选 `iat`（6）。摘要 profile 按 RFC 9995 禁止头 3，文件长度不是必填字段。三种 profile 均没有 request_id、origin、audience 或执行截止时间。

`iss` 表示签署者；`sub` 表示被声明对象，允许省略。`iat` 是 i64 Unix **秒**，是签署者的时间声明，不是可信时间戳。普通文档签名不因执行批准到期而失效。含受众、授权期限或新业务语义的声明需要单独定义 profile，当前入口拒绝未知 typ、claims、保护头或关键语义。

## 身份与字节规则

`iss` 使用规范的绝对 ASCII URI，1..8192 字节，无凭据、控制字符或错误的百分号转义。创建端必须预先确定规范字符串；签署和验证不会隐式改写 URL。`sub` 为 1..8192 UTF-8 字节的 StringOrURI，无控制字符；含冒号时须符合相同 URI 规则。普通文本不作 Unicode 归一化，也不裁剪空白。URI 只作标识，不触发网络发现。

| 类型 | 二进制 | 文本 |
| --- | --- | --- |
| dMsg `AccountId` | `ic_auth_types::Xid` 别名，Candid blob / CBOR bstr，12 字节 | 规范 Xid：20 字符小写 base32hex，末字符为 0 或 g |
| ICP Principal | Candid principal / CBOR 原始 bstr，0..29 字节 | 标准 Principal 文本；空字节管理 canister 也是合法表示 |
| 设备、操作、SHA-256 | 各自语义类型，32 字节 | 桥接口明确指定编码，不根据长度推断身份类型 |
| COSE kid | 不透明 bstr，当前 profile 限 1..256 字节 | 无隐含账户语义 |

身份适配器接收明确命名空间：`account_issuer("https://dmsg.example/u/", account)`，或在命名空间后追加规范 Principal 文本构成 Principal issuer，例如 `https://id.example/ic/mainnet/aaaaa-aa`。命名空间为固定 URI 前缀，以 `/` 或 `:` 结束，无 query/fragment，最多 8128 字节。账户迁移不改变身份 URI；不补零、截断或哈希身份来适配长度。

新消息使用 RFC 8949 §4.2.1 core deterministic CBOR：最短编码、按键编码字节排序、无浮点、重复键或尾字节。现有 COSE 验签保留原 protected 字节，不能重排后验签。本地签署入口只接受按 profile 规范准备的输入。待签结构最多 65536 字节，COSE_Key 最多 2048 字节，签名产物最多 196608 字节；TSA token 最多 131072 字节。库解码器设有递归限制，SDK 另外限制 24 层和 50000 个节点。

ICP 业务时间、批准期限为 u64 Unix **毫秒**，使用 `now < expires_at`。certificate 和 ICRC `created_at_time` 为纳秒；账本重试保留原值。CBOR u128 超过 u64 时用 tag 2 最短大端 bstr；不能经 JS Number 转换。ICRC Account 是 `{owner: bstr, subaccount: bstr .size 32 / null}`。

浏览器 JSON 协议为 `dmsg-extension/4`：accountId 为 Xid 文本，SHA-256/requestId/nonce 为小写 hex，大整数为十进制字符串；statement 的 issuedAt 为秒，外层 expiresAt 为毫秒。普通 Rust serde JSON 输出不等同于此桥协议。

## 签名与密钥

待签字节严格为 `CBOR(["Signature1", protected_bstr, h'', payload_bstr])`，external_aad 为空。

| 算法 | COSE alg | 签名 | 公钥 |
| --- | --- | --- | --- |
| Ed25519 | -19 | 直接签 Sig_structure | OKP，crv=6，x=32 字节 |

dMsg 只支持 Ed25519；2026-10-07 起不再提供 ES256K、BIP340 入口或私有算法标签。失败不会自动切换算法。public-only COSE_Key 禁止私钥 d（-4）；alg 必须匹配，key_ops 存在时须允许 verify，kid 存在时须与保护头匹配。

公钥指纹采用 RFC 9679 SHA-256：只编码所需公开参数 `{1: 1, -1: 6, -2: x}`，排除 kid、alg、key_ops。dMsg 的正式文档由账户的设备 Ed25519 密钥签名，kid 就是该设备公钥的指纹；设备是否属于账户、签名是否经账户服务授权，由认证执行回执证明，不由公钥本身证明。

vetKD 只用于内容根的恢复信封：context 为 `["dmsg/content-root/v2", environment, 2]`，所有账户共享同一派生公钥；每一代根的 IBE 身份为 `[account_id, generation]`。它不是文档签名算法。

## ICP 执行批准与回执

`digest(domain, value) = SHA256(CBOR([1, domain, value]))`。

```text
request_id = digest("dmsg/execution-request/v2",
  [account_id, security_epoch, device_id, sequence])

signature = Ed25519(device_key, Sig_structure)          // kid = RFC9679(device_key)

digest("dmsg/device-approval/v2", [
  target_user_principal_bytes, account_id, "dmsg/attest/v1",
  device_id, security_epoch, sequence, request_id, expires_at,
  digest("dmsg/attest/v1", [statement, checked_browser_origin, signature])
])
```

设备用同一把 Ed25519 密钥先签 Sig_structure，再签此批准摘要；`statement` 为 Rust `Statement` 的 CBOR（Option 为值或 null，无负载枚举为名称字符串，有负载枚举为单项 map）。应用动作用 `AppActionAttestRequest { account_id, issuer, action, signature, approval }`，statement 为 `{issuer, content: {AppAction: action}}`、origin 为 `action.origin`。恢复设备的根派生使用 `dmsg/derive-root/v1`，命令为 `[generation, transport_key (bstr .size 48), max_cycles]`。账户变更使用独立 `dmsg/account/v2` 域，命令为 `[expected_version, AccountCommand]`。

签署前冻结完整保护头、payload 与批准上下文。user home 检查 issuer 属于当前账户、设备是账户的未撤销设备且具备 `FormalApprove`、用途在账户政策内、当月额度有余，验签 Sig_structure 与批准，然后在同一消息内扣减额度、保存产物并写入认证回执。浏览器 origin 最多 256 字节，为精确 HTTPS origin 或 Chrome extension origin（Local 部署还接受精确的环回 HTTP origin）；由扩展核实，设备签名不独立证明浏览器来源。

幂等作用域为 account_id/request_id；同 ID 不同参数拒绝。丢失回复用 `get_attestation(account_id, request_id)` 取回同一产物。严格设备序号和执行水位在结果清理后继续阻止重放，原样重放已清理的请求返回 `ResultExpired`。

`get_execution_receipt(account_id, request_id)` 返回 ICP 认证叶，查询要求账户认证。路径为 `b"execution/" || account_id[12] || request_id[32]`。`ExecutionReceipt`（schema 2）保存 issuer、设备/epoch、批准时间/期限、origin、待签字节 SHA-256、设备公钥指纹和签名原始字节 SHA-256。它不改变可移植签名产物。升级从稳定执行记录重建叶，清理结果时删除叶，之后查询返回认证的不存在证明。

验证回执必须先验证指定 user canister 的 IC certificate、witness、路径和值，再匹配 issuer、待签摘要、公钥指纹和签名摘要。SDK `verifyExecutionReceipt` 完成这两步；Rust `match_execution_receipt` 仅做绑定检查，调用者负责认证 certificate。没有回执的 COSE_Sign1 只证明某把设备密钥签过这些字节，不证明 dMsg 授权。回执证明本服务记录的执行授权，不自动证明外部项目权限或当前设备状态。

## Xid 发号

用户 canister 使用 `ic_auth_types::XidGenerator` 持久化发号，其输出为 `timestamp_seconds[4] || allocator_fingerprint[5] || counter[3]`。指纹取 `digest("dmsg/account-id-generator/v1", ["dmsg", environment, issuer_namespace, creating_canister])` 前 5 字节，并在配置中单独保存完整 namespace digest 用于升级校验。同秒/时钟回退继续计数，新秒从 0 开始；时间溢出或计数耗尽明确失败。

认证、设备 PoP、配额和唯一绑定校验通过后，同一无 await 消息提交账户、认证索引、配额和分配器。已有认证创建重试返回原账户。一个部署可以运行多个 user home，各服务按账户 ID 内嵌的指纹路由到分配它的 home；`dmsg_handle` 的 `user_homes` 是权威列表，`registration_homes` 是当前接收新账户的子集，客户端对每个 home 并行查询 `my_account` 定位登录身份的账户。不实现跨 home 的 Principal 唯一性。

## 时间戳与验证结果

RFC 9921 CTT 的 SHA-256 MessageImprint 为 `SHA256(CBOR(signature_bstr))`，包括 bstr 头，区别于执行回执中的原始签名摘要。头 270 携带不透明 token；验证既不申请也不信任 TSA。CMS 签名、imprint、证书链、用途、政策与状态需要独立验证。

`verify_artifact` / SDK `verifyDocumentArtifact` 检查 profile 和数学签名；SDK 结果的 `checks` 字段分别报告 signature、content、issuerBinding、authorization、timestamp、currentStatus；顶层 `signature` 是原始签名字节。纯文本的内嵌内容为 verified；摘要或文件声明未提供原文件时 content 为 not_provided，未认证身份或 TSA 时为 not_checked。不能把随包公钥或单个成功布尔值当作完整证明。

当前不提供 TSA 网络/CMS 验证、SCITT 透明服务、长期归档或链上 anchor 入口。可选付费投递、名称与 ICP 控制合同分别维护。

## 验证与依据

```sh
cargo test -p dmsg_types -p dmsg_protocol
cargo run -p dmsg_types --example protocol_vectors > /tmp/dmsg-vectors.json
node scripts/verify-dmsg-vectors.mjs /tmp/dmsg-vectors.json
```

签名、身份头和 profile 依据：[RFC 9052](https://www.rfc-editor.org/rfc/rfc9052.html)、[RFC 9597](https://www.rfc-editor.org/rfc/rfc9597.html)、[RFC 8392](https://www.rfc-editor.org/rfc/rfc8392.html)、[RFC 9596](https://www.rfc-editor.org/rfc/rfc9596.html)。摘要、公钥指纹和时间戳分别依据 [RFC 9995](https://www.rfc-editor.org/rfc/rfc9995.html)、[RFC 9679 §4.2](https://www.rfc-editor.org/rfc/rfc9679.html#section-4.2)、[RFC 9921](https://www.rfc-editor.org/rfc/rfc9921.html)。SCITT 的声明/证据分工参考 [RFC 9943](https://www.rfc-editor.org/rfc/rfc9943.html)，当前普通文档 profile 不宣称实现其透明服务。

## Agent Delegation

dMsg 账户可作为 Agent Delegation 1.0 principal：controller 是客户端 vault 中自持的 Ed25519 key，注册时向 user home 证明持有，事件在本机签名；principal 文档由 `dmsg_directory` 以 ICP 认证 HTTP 发布。接口与验证范围见 [agent_zh.md](agent_zh.md)。

## 商业服务

产品中立的 `membership/1` 与 `dmsg-commerce/1` 合同、投递 profile 2 和执行授权 v3 详见 [commerce_zh.md](commerce_zh.md)，配套有 [commerce.cddl](commerce.cddl) 结构定义和独立的商业测试向量。它们不改变正式 Statement 或设备执行批准字节。
