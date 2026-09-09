# Statement v3 设计

状态：2026-09-09，已按本设计实现公开类型、两个文档 profile、Xid 账户、认证执行回执及 SDK；实际合同以 [协议说明](README.md) 和 [CDDL](statements.cddl) 为准。最初评估基于 `4fc3ca2`。完整生产 UI、TSA 信任验证与多分配器登记不在本轮实现范围。“v3”是本次设计迭代名称，线上协议版本由具体 profile 明确定义。

## 1. 设计决定

采用标准 COSE Signed Statement 作为交换对象，把 dMsg 自定义部分缩小为用途明确的 profile、身份适配和执行授权。取消当前固定的 `Statement { schema, subject, request_id, origin, audience, expires_at, body }` 总包装。

| 当前设计 | 提议 |
| --- | --- |
| BIP340 私有算法标签 | 从 dMsg 签名 profile 和对应入口移除；不删除共享密码库为其他使用者提供的能力 |
| request_id 位于声明内 | 仅保留在执行请求、去重状态和认证执行回执 |
| subject 表示署名账户且固定 32 字节 | 署名实体改用 `iss`；`sub` 表示被声明对象，二者采用 CWT 文本语义 |
| 内部 SubjectId 直接复用 Hash32 | 使用独立的 `AccountId`，按 Xid 保存 12 字节并由用户 canister 持久化发号 |
| origin 必填 | 归设备批准/执行上下文；确需表达业务来源时由专用 profile 定义 |
| audience 必填 | 仅受众受限的协议使用标准 `aud`，公开文档声明不强制包含 |
| expires_at 表示请求截止 | 留在执行上下文；声明本身的有效期另按 profile 定义 |
| schema 整数 + Rust 枚举形状 | 用受保护的 `typ` 识别完整签名对象的用途及版本，payload 遵守对应内容格式 |
| FileAttestation { sha256, size } | 简单摘要证明采用 COSE Hash Envelope；发布/验收等业务签署使用完整的业务声明 |
| kid 和 signature 都固定长度 | kid 为不透明可变长字节；签名和公钥长度由选定算法检查 |

Ed25519（COSE alg -19）为首个必需实现的签名算法；ES256K（-47）为 ICP 适配的可选算法。请求不能要求未启用算法，也不能在失败时自动替换算法。

## 2. 标准依据与应用选择

| 标准 | 使用部分 |
| --- | --- |
| [RFC 9052](https://www.rfc-editor.org/rfc/rfc9052.html) | COSE_Sign1、COSE_Key、保护头、Sig_structure |
| [RFC 9597](https://www.rfc-editor.org/rfc/rfc9597.html) | 在保护头 15 中携带 CWT claims，可用于非 CWT payload |
| [RFC 8392](https://www.rfc-editor.org/rfc/rfc8392.html) | iss/sub/aud 及时间 claim 的已有类型和语义 |
| [RFC 9596](https://www.rfc-editor.org/rfc/rfc9596.html) | 保护头 16：完整 COSE 对象的 typ |
| [RFC 9943](https://www.rfc-editor.org/rfc/rfc9943.html) | SCITT 的 issuer、subject、声明与证据分工；按需提供对应声明 profile |
| [RFC 9995](https://www.rfc-editor.org/rfc/rfc9995.html) | 文件摘要的 Hash Envelope |
| [RFC 9679](https://www.rfc-editor.org/rfc/rfc9679.html) | 可移植的 COSE 公钥指纹 |
| [RFC 9921](https://www.rfc-editor.org/rfc/rfc9921.html) | COSE 签名上的 RFC 3161 时间戳证据 |

RFC 9943 已于 2026 年 6 月发布，RFC 9995 已于 2026 年 7 月发布。当前提议复用它们的相关格式，不宣称现有 canister 已实现 SCITT 透明服务或受监管 TSA。

本草案选择必需的受保护字段、启用算法、URI 约定、用途 profile 和实现资源限制。这些选择应明确标为应用约束，不混称为所有 COSE 消息的要求。具体媒体类型及版本参数已在 CDDL 中冻结；采用实验名称时不宣称已经注册。

## 3. 身份标识

### 3.1 区分 issuer 与 subject

- `iss`：对声明负责的署名实体，可以是用户、组织、设备或服务。它与实际签名 key 的绑定需要可验证证据。
- `sub`：这份声明谈论的对象，如文件、项目、账户或设备。它也可用于关联同一对象的多份声明。
- `kid`：选择验证密钥的提示，不等于账户身份，也不保证全局唯一。

普通文档 profile 的 `sub` 可以省略；面向 SCITT 的 profile 必须按照其要求提供 `iss` 和 `sub`。不要为了填满字段而把签署者重复填为被声明对象。

### 3.2 推荐：签名对象使用文本身份引用

CWT 的 iss/sub 是文本字符串。本草案进一步建议 issuer 使用规范的绝对 URI，以同时表达身份命名空间和标识。subject 可以是绝对 URI，或由 profile 明确定义、在 issuer 范围内解释的文本标识。

以下仅为格式示例，不表示相应服务已部署：

| 输入标识 | 适配后的 issuer 示例 |
| --- | --- |
| TokenList Xid，12 原始字节 | `https://tokenlist.example/users/<规范的20字符xid>` |
| ICP Principal，0–29 原始字节 | `https://identity.example/ic/mainnet/principals/<规范principal文本>` |
| dMsg AccountId，12 原始字节的 Xid | `https://dmsg.example/u/<规范的20字符xid>` |

不能根据长度猜测标识类型：12 字节也可能是一个 Principal。适配器必须知道所属命名空间、编码方案以及适用网络。Principal 的空字节形式是管理 canister 的合法标识；某个签署入口是否允许它作为账户，应由该入口的权限规则决定。

SDK/ICP 适配接口可以继续接收原生字节，由对应适配器无损转成规范身份引用。无需补零、截断或先做 SHA-256。URL 只作标识，验证器不根据不可信 issuer/sub 自动访问网络。

身份 URI 的创建规则必须固定。已有签名按原字符串验证，不能验签前重新拼 URL、统一改大小写或更换命名空间。身份命名空间不能跟着可迁移的存储分片或当前密钥变化。签署端也不能把任意调用者提供的 issuer 当作已证明的身份。

### 3.3 原生二进制方案的取舍

若跨语言消费者明确要求签名对象内保留原始字节，可单独设计 `{ namespace: uri, id: bstr }`。它支持 Xid、Principal 和其他标识，但属于新增的数据格式，需要定义命名空间、规范化和验证规则。

本草案优先采用已有 CWT 文本规则。不能把标准 iss/sub 的值直接换成 bstr 后仍宣称遵循同一 CWT claim 定义。dMsg 内部账户 ID 的长度也不应再成为通用声明的类型约束。

### 3.4 dMsg 内部账户采用 Xid

使用独立语义类型 `AccountId`，不再定义 `SubjectId = Hash`。Rust 表达可采用 `AccountId([u8; 12])`，复用 `ic_auth_types::Xid` 的规范文本编解码，并在 JSON 解码时检查规范形式。

| 使用位置 | 表示 |
| --- | --- |
| Candid / CBOR 账户字段 | 12 字节 blob / bstr |
| 稳定存储主键 | 12 字节；Storable/索引实现留在 canister 内部 |
| JSON / URL / 日志 | 规范的 20 字符 Xid 文本 |
| COSE `iss` | 稳定身份命名空间下的 URI，包含该 Xid 文本 |

摘要、密钥标识、设备标识、操作标识分别遵循自己的合同。此次更换账户类型不是对所有 32 字节值的批量替换。

发号实现参考公开项目 `ldclabs/token-listing` 的 `canisters/user/src/accounts/xid.rs`。本次核对的该文件对应仓库基线 `9981d7f`，文件本身无本地修改；没有核对远端。复用其 canister 场景下的持久化分配机制，而不是依赖进程随机状态的通用客户端生成器。

```text
AccountId = timestamp_seconds[4] || allocator_fingerprint[5] || counter[3]
```

三个分段按参考实现采用大端编码。分配器保存 `profile_version`、完整 `namespace_digest`、`fingerprint`、`last_second` 和 `next_counter`。fingerprint 的来源包含固定应用命名空间、环境及创建账户的 canister；准确域与编码已在公开协议和互操作向量中冻结。

发号规则：

1. 仅在分配器边界将业务毫秒时间换算为整数秒，检查能否表示为 u32。
2. 时间进入新的秒时从计数器 0 开始；同秒或时钟回退时沿用已保存的秒和下一个计数器。
3. 24 位计数器耗尽时拒绝发号，直到时间超过已保存的秒；不取模或覆盖已有 ID。时间超出 u32 时明确拒绝，不截断。
4. `allocate` 返回候选 ID 和候选新状态，错误不改变原状态。提交前检查本地账户键尚不存在。
5. 创建入口先完成认证、设备 PoP、唯一绑定和配额检查，再在同一个无 await 的本地执行段中写入分配器、账户、认证索引和计费/配额状态。所有可返回 Err 的检查在写入前完成，trap 由 canister 消息事务回滚。
6. 重试已有创建请求返回已创建的账户，不再次分配。首次创建不再依赖 `raw_rand`；现有为该异步步骤设置的创建暂存记录可随流程重构移除。

分配器是内部状态，不进入公开类型库。升级恢复原有秒和计数器，并核对命名空间、环境、创建 canister 和分配器版本；初始化不能因为配置变化或解码失败而静默重置。恢复旧备份时也不能默认把旧发号水位当成当前水位重新发号。

单一分配器的持久化秒/计数器提供不复用保证；5 字节截断哈希本身不提供跨分配器的绝对唯一性。当前固定单 user home 可以沿用本地发号。未来扩展到多个 user home 前，需要在既有账户分配权威中登记并校验 fingerprint，使同一账户命名空间中的分配器指纹不重复，且不会分配给另一个独立发号者。指纹冲突在启用新分配器前处理，不能等到两个分片各自完成同 ID 账户创建后再对账。

账户移动到其他存储位置时保留 AccountId；目标位置使用自己的发号器创建新账户，不复制并重置原发号器。issuer URI 的命名空间保持稳定，不改成当前分片地址。

Xid 是公开的结构化标识，会反映分配时间及同一分配器内的次序。权限仍由主体绑定、设备及策略验证；Xid 不承担秘密凭据或可信时间戳的作用。配套设计中的随机账户标识描述需同步为该生成方案。

实施时同步检查四个 canister 的账户引用、认证叶、索引和分页前缀、设备批准摘要、名称权属、付款 offer、签名密钥路径及 vetKD input。账户字节进入密码派生，所以从旧实验 ID 换成 Xid 会改变相应派生输入，不能把截断旧 ID 当作保持同一密钥的转换。按开发阶段约定使用新实例和新测试向量。

## 4. 声明与执行分离

以下字段继续由执行层保存并校验：account_id、request_id、目标 canister、实际请求 origin、设备、安全版本、批准序号、执行截止和费用上限。

批准必须同时绑定这些上下文，以及最终 COSE 待签字节的摘要和不可变的目标密钥描述。保护头和 payload 在用户确认前冻结，重试不得刷新内容或时间。执行后通过查询或经认证的回执，把 request_id 与待签摘要、签名结果绑定起来。

移除声明中的 request_id 不等于移除去重：同一次执行仍使用同一操作 ID，未知结果仍查询原请求，结果清理后仍保留不能重放的序号状态。也不应把 request_id 换名为 nonce 或 cti 后强制塞回普通文档声明。

相同 issuer、密钥、保护头和内容可能产生相同签名产物。需要区分“同内容的两次签署事件”时，事件 ID 或时间属于明确的业务声明/执行证据。文件摘要和签名产物摘要不能代替所有业务操作的幂等 ID。

## 5. 两类基础文档 profile

### 5.1 内嵌内容

保护头包含签名算法、typ、身份 claims 和密钥引用。content type（3）说明 payload 的格式；payload 可以是明确编码的文本、CBOR 或其他已支持格式。未知格式不自动进入 dMsg 正式签名入口。

纯文本可直接签署原始 UTF-8 字节，不再包装成 `{ Statement: { text } }`。如果内容本身是带业务语义的结构化声明，应按它自己的公开 schema 编码。不能默认重排外部 JSON 或修改空白后签署；需要 JCS 或确定性 CBOR 的 profile 必须显式声明这种规则。

### 5.2 文件摘要

采用 RFC 9995：payload 是摘要，保护头 258 标识摘要算法，259 可标识原始内容格式，260 可提供原始内容位置。该格式禁止 content type（3），不能同时沿用普通内嵌消息的头部要求。

第一版可只启用 SHA-256（258 的值 -16），这时 payload 长度为 32 字节。此长度来自摘要算法，与 issuer/sub/kid 的长度无关。无需增加一个通用 size 字段；取得原文件后摘要校验同时绑定了确切的字节序列。需要签署文件长度、发布版本、许可或验收含义时，应签完整 manifest/业务声明。

“对这些字节作摘要证明”“以项目发布者身份发布该文件”“接收并验收该交付”应有不同的业务含义。TokenList 的文件发布声明仍可携带项目、文件版本和权限依据，并由 TokenList 自己校验角色；不把它们强加给普通文件证明。

## 6. 候选 CDDL 结构

下面用于表达设计结构，实施时需补齐 profile 的媒体类型、语义约束和互操作向量。不是已发布的新合同。

```cddl
signed-statement = inline-statement / digest-statement

inline-statement = #6.18([
  protected: bstr .cbor inline-headers,
  unprotected: evidence-headers,
  payload: bstr,
  signature: bstr
])

digest-statement = #6.18([
  protected: bstr .cbor digest-headers,
  unprotected: evidence-headers,
  payload: bstr .size 32,
  signature: bstr
])

common-headers = (
  1: -19 / -47,          ; enabled signature algorithms
  4: bstr,              ; variable-length kid in this key profile
  15: identity-claims,   ; RFC 9597
  16: tstr              ; recognized, versioned profile / typ
)

inline-headers = {
  common-headers,
  2: [15, 16],
  3: tstr               ; inline payload content type
}

digest-headers = {
  common-headers,
  2: [15, 16, 258],
  258: -16,             ; SHA-256
  ? 259: tstr,
  ? 260: tstr
}

identity-claims = {
  1: tstr,              ; issuer: canonical absolute URI in this profile
  ? 2: tstr,            ; subject: the object being described
  ? 6: int              ; optional claimed iat, integer Unix seconds
}

evidence-headers = {
  ? 270: bstr           ; optional RFC 9921 CTT token
}
```

这里的 crit（2）列出本 profile 要求理解的保护头。解码器必须拒绝重复字段和保护头/非保护头冲突，识别 typ 对应的规则，再验证必要字段。crit 中未知的参数必须拒绝；认识 CWT 容器不等于可以忽略其中未知的授权语义。涉及 aud/exp/nbf 或其他 claim 的 profile 单独声明规则，不通过一个任意 claims map 开启授权。

CDDL 中保护头字节使用 `.cbor` 真正关联对应结构。若另外定义外层证据包，其中 COSE/公钥的字节字段也应关联其实际结构；不能只定义未被引用的非终结符，然后把所有内容当任意 bstr。

## 7. 时间、用途与扩展

- 请求截止仍可用 ICP 接口现有的整数毫秒；CWT 时间 claim 按它自己的 NumericDate 规则。本 profile 若使用 iat，选择整数秒，不能直接塞原毫秒值。
- 普通文档签名不默认过期。授权的业务有效期与签名请求的执行截止分别表达，不能互相替代。
- iat 是签署者声明的时间，不构成可信授时。TSA 验证单独处理，不能通过一个本地时间字段完成。
- typ 标识完整签名对象的用途和版本，content type 标识内嵌 payload 的格式，二者职责不同。
- 协议允许扩展不等于 dMsg 允许任意签名。dMsg 只启用可解析、可向用户展示、策略允许的 profile；不根据 URI 下载可执行的解释器。
- 发布新的关键语义需要新的明确 profile/version。未知关键语义拒绝，不能降级解释为普通文本签名。
- URI、头部、payload、证据、嵌套深度和条目数的资源上限由具体实现/profile 明确公布。它们与某种账户 ID 的固定长度分开维护。

## 8. 公钥、TSA 和证据

公钥使用 COSE_Key。需要跨实现重算的公钥指纹时采用 RFC 9679 并固定摘要算法；kid 仍是可变长的选择提示，不强制等于某种平台派生 ID。ICP 密钥来源、主网配置及账户绑定放在独立的身份/密钥证据中。

TSA 沿用 RFC 9921 CTT，覆盖 COSE 签名。文件摘要和 TSA 的 message imprint 是不同的输入，不互相替换。追加时间戳或其他证据会改变外层证据对象的字节，因此需区分签署内容、执行记录和证据包版本的标识。

COSE 保护头不加密；不默认放私密文件名、下载地址或不必要的账户信息。敏感证据可以整体加密保存。若需要对原文作带盐承诺，应定义正确的 preimage/profile，不能把 salted commitment 冒充原文件 SHA-256。

ICP certificate/witness、TSA token 和 SCITT receipt 使用各自的验证规则。普通 ICP 锚定不能仅因放入一个字段就称为 SCITT receipt。验证结果分别报告内容、签名、身份绑定、历史授权、时间证明和当前状态，不合并成一个含义模糊的成功布尔值。

## 9. 实施范围与验收

1. 冻结两种文档 profile、URI 构造规则和支持算法；为 Xid、Principal、dMsg ID 提供明确适配器。
2. 将通用签名数据与 ICP 执行请求分离，内部账户统一改为 AccountId/Xid，并落实第 3.4 节的持久化分配与原子提交规则。通用声明使用身份 URI，不导入账户存储类型。
3. 删除 dMsg BIP340 入口和私有标签，保持 Ed25519/ES256K 的独立验证；不修改共享库对其他使用者的支持。
4. 复用 cose2 的消息和异步签名接口，在协议层处理新保护头、CWT 语义和 crit。当前通用 Header map 可承载这些标签，不必再造 COSE 编解码器。
5. 验证请求元数据移出后的批准绑定、幂等、未知结果、升级恢复及跨语言字节一致性。
6. 正向向量覆盖 Xid/Principal 的无损映射、内嵌文本、文件摘要、TSA 添加前后；负向向量覆盖错误身份命名空间、算法、typ、关键字段、时间单位和身份/密钥绑定。
7. Xid 发号测试覆盖同秒并发、时钟回退、升级恢复、计数器与时间范围耗尽、配置不一致、重复认证创建、拒绝后的状态不变，以及账户/索引/分配器的提交一致性；分片启用前另验证分配器指纹冲突处理。

验收重点是第三方能用现有 COSE/CWT 工具理解消息，并只为业务 profile 和信任策略编写少量代码。采用标准格式不自动完成身份验证、项目授权、透明日志或时间戳服务运营。
