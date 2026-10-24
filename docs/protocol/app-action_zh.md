# 应用动作 profile v1

[English](app-action.md) | 简体中文

状态：公开源码已实现。经 dmsg-connect/1 的 `signAction`、产品权威准入、账户关联、设备签名与批准、按 schema 渲染的桌面确认及认证回执已接通。外部应用必须登记并使用同一 schema；其发布状态由应用自己维护，不能从本合同推断。

## 职责划分

dMsg 作为平台不定义应用命令，负责：

- 封闭值模型 `ActionValue` 与 schema 格式 `ActionSchema`；
- 下文的规范编码和摘要；
- 通用校验，以及可渲染任意已登记 schema 的桌面审批窗口。

各应用拥有自己的命令词汇：编写 `ActionSchema`，经治理登记为 `AppRegistration.action_schema`，在自身代码中维护原生输入到 `ActionCommand` 的投影。dMsg 证明获准设备签署了由认证 schema 展示的精确命令；接收方证明将执行的输入恰好投影为已签命令。因此新应用只需治理登记，无须发布新版 dMsg。只有现有类型无法表达的新值类型才需要修改 dMsg，之后所有应用都可使用。

## 签署对象

`typ` 为 `application/vnd.dmsg.app-action+cose;v=1`。保护头严格为 `alg`（1）、`crit`（2）=`[15,16]`、content type（3）=`application/cbor`、`kid`（4）、CWT claims（15）=`{1: issuer}` 和 `typ`（16）。没有可与类型化动作矛盾的独立 `sub` 或声明 `iat`，external AAD 为空。批准设备以 Ed25519（-19）签名，与三个文档 profile 相同；`AppAction` 是账户 `SensitivePolicy` 中独立的用途。

payload 是 [integration.cddl](integration.cddl) 中 `AppAction` 的确定性 CBOR。与现有文档 profile 不同，应用 origin、接收者与意图有效期均进入签署内容。完整摘要为 `SHA256(CBOR([1, "dmsg/app-action/v1", action]))`。

`AppAction` 包含：

- `app_id`、`app_config_version`、`environment`、`origin` 和 `receiver`；
- 产品自己的操作者身份 `actor`（1–64 不透明字节）和 dMsg 账户 `signing_account`；两者是不同的已签身份；
- `operation_id` 和 `intent_hash`：产品请求标识及完整意图的承诺，包括产品保存的主体、前置条件、角色或策略承诺；
- `schema_hash = SHA256(CBOR([1, "dmsg/action-schema/v1", schema]))`，指定赋予命令含义的精确 schema；
- `command = { name, args }` 与文件清单 `files`。

签名本身不证明 `actor` 与 `signing_account` 已关联。登记的产品权威须在执行前确认关联；issuer/key 来源与认证执行回执另外核验。

## 值模型与 schema

`ActionValue` 为封闭模型：`Nat`（u64）、`Bool`、`Text`、`Hash`（非零 32 字节）、`Principal`、`Choice`（声明过的选项）、`Artifact`（精确外部引用）、`Null`（缺失可选值）、`List`、`Record`。没有不透明字节值、方法名、可执行输入、独立 `display_summary` 或资产转移值。命令参数与记录字段 `ActionArgs` 具有名称，必须与 schema 字段顺序完全一致，不能静默交换两个同类型字段。

`ActionSchema { version: 1, commands }` 声明每个命令的名称、标题与字段 `SchemaFields`。字段含 name、label 与 `FieldType`：`Nat { min, max }`、`Bool { yes, no }`（两个值的标签）、`Text { max_bytes, multiline }`、`Hash`、`Principal`、`Choice { options }`、`Artifact`、`Optional { item }`、`List { item, max_items }` 或 `Record { fields }`。标题和标签是按 locale 提供的纯文本。每个字段始终显示，schema 不能隐藏字段或用摘要替代。

下面是审查决定的结构示例：

```text
DecideReview  "Record review decision"
  project_id  Nat{1..}            case_id  Nat{1..}          round  Nat{1..=2^32-1}
  outcome     Choice{Approved | Rejected | ChangesRequested}
  changes     List{64, Record{locator Text{128}, detail Text{4096, multiline},
                              blocking Bool{"Must be resolved" | "Optional"}}}
  rationale   Text{4096, multiline}
```

## 边界与验证

- Schema：最多 16 KiB、32 个命令；每命令/记录最多 32 个字段、每 Choice 32 个选项、每标签 8 个 locale。名称是最多 64 字节的 ASCII 标识符，标签是最多 256 字节的单行文本；Text 最多 4096 字节，List 最多 64 项。顶层参数为第一级，类型和值最多嵌套三级；Optional 不能包 Optional。区分命令、同一命令/记录的字段、Choice 选项、Bool 两个值的标签，在任一 locale 中都不得重复，避免审批窗口显示成相同内容。
- 动作正文：最多 48 KiB；完整 Sig_structure 仍限 64 KiB。
- 意图：正长度有效区间，最多 300,000 毫秒；历史签名解析检查区间形状，不因此使签名过期。
- 文件：最多 64 个，ASCII locator 严格递增且唯一；revision 为正，含 SHA-256、最多 256 MiB 的字节长度、媒体类型及明确的原文/密文表示。展示名称也受签名保护。
- Artifact URI：规范、可打印 ASCII HTTPS/IPFS URL，无凭据，最多 4096 字节；不会自动获取。

治理登记应用时运行 `validate_action_schema`；`validate_app` 仅在应用拥有 `SignAction` 时要求 schema。`validate_app_action` 不依赖时钟/schema 地校验内容，使历史签名可在任意位置解析。`validate_action_command` 核对 `schema_hash`、命令名（未知时 `UnsupportedProtocol`）及所有参数。`validate_action_admission` 另检查登记、精确 environment/origin/config version、profile/capability、暂停状态及当前意图期限。这些函数不认证产品准备结果或授予项目角色：生成执行授权前，产品权威必须确认精确动作；提交动作时接收者重新核对角色、策略、版本和期限。

`verify_artifact` 只检查 COSE 与数学有效性，不重建产品意图、下载文件、认证账户关联或确认当前权限。持有摘要等于 `schema_hash` 的 schema 的验证方，可以离线渲染和检查命令。

桌面审批窗口读取认证登记，核对 `schema_hash`，只从 schema 渲染标题与标签；请求不携带展示文案。窗口说明文案由应用定义、经治理登记。只有新审批需要当前登记；暂停或登记版本更新后，已批准动作仍可打开对账，若登记不再匹配则不显示按 schema 渲染的详情。

仅文档入口 `dmsg_user.attest` 对 `AppAction` 返回 `UnsupportedProtocol`。`attest_app_action` 接收 `AppActionAttestRequest { account_id, issuer, action, signature, approval }`，依登记准入，调用固定产品权威的 `verify_dmsg_action` 核对精确动作和账户，并在 await 后重读账户、设备与配置。它复用文档认证的月额度、原请求重放 `get_attestation` 和 schema 2 认证回执机制。

## 应用接入

1. 编写 schema 和输入到 `ActionCommand` 的投影；准备、权威 `verify_dmsg_action` 与证据核验使用同一投影。
2. 经治理调用 `register_integration_app` 登记。验证预览列出 schema 摘要与命令名；更改 schema 必须递增 `config_version`，旧版本准备的在途动作将无法准入。
3. 构造带 schema 摘要的 `AppAction`，从登记 origin 调用 `signAction`。本机应用也可把规范 CBOR 直接交给设备（`dmsg attest action <file>` 或桌面签名页）。设备按当前登记准入，在签名前通过 `inspect_app_action` 查询产品权威，并展示动作自己的 origin；该路径没有来源转述。

## 回归证据

`dmsg_protocol` 使用真实、确定性的 Ed25519 fixture 签署示例审查 schema 的全部命令，拒绝被修改的 origin、receiver、actor、schema digest、命令、参数、展示名、文件摘要/版本/表示和意图期限。测试还覆盖 schema 边界、歧义标签、未知命令、参数交换/改名、越界数值、未声明选项、错误 Null、深度、未知值类型、缺 schema 登记、暂停与过期准入。`integration_vectors.json` 包含登记、每个动作及签名，由独立 TypeScript 编码器和校验器核对。既有文档向量的字节不变。
