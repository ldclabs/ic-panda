# 本设备凭据受控使用（P0 / P1）

日期：2026-10-24。本文描述公开客户端实现；不是部署或真实 GitHub 验收声明。

## 范围与权限

owner 在本设备创建策略，Agent 以 `secret.use:<policy_id>` 申请一次操作，人批准完整计划后由 daemon 执行。P0 只支持 GitHub.com 指定仓库的 `issue.create`；配置策略不批准执行。没有通用取密、HTTP 代理、命令执行或自动重试接口。

`secret.use:*` 不合法。Agent Delegation、`vault.read/write`、`files.read` 不隐含此权限。策略和 grants 用本设备 LDK 封存，不跨设备同步，不复制到 resident Agent 的锁定权限中。每设备最多 64 个策略；沿用 64 个 capability grants 的上限。

MCP 的 `dmsg_vault_read` 始终遮罩 concealed/otp；移除 `secret` 参数（即使为 false 也拒绝）及 `vault.secret`。旧 grant 不再匹配，不自动转为新权限。Agent 对既有条目的写入只针对当前版本，`base` 是并发版本校验，过期返回 `VERSION_CONFLICT`，不开放历史内容；不得改标签、不得把隐藏字段改为明文类型；新建仍须带授权的 write 标签。`dmsg-` 保留条目不进入上述任何通道。owner 的 reveal/copy/OTP 不变。

## 引用与策略

引用文本为 `dmsg-secret://<account_id>/<item_id>/<field_id>`，对象为：

```json
{"schema_version":1,"account_id":"040g2081040g2081040g","item_id":"1111111111111111111111111111111111111111111111111111111111111111","field_id":"credential"}
```

拒绝非规范账户 ID、非 64 位小写 hex 条目 ID、无效字段 ID、query/fragment/userinfo/转义/额外路径。执行引用只能取当前未删除、无冲突的安全条目；API 凭据字段必须 concealed，不接受 OTP、wallet 或保留条目。字段同 ID 重建仍是同一引用；审批固定条目 revision，任何后续改动使其失效。

Vault 现有加密粒度为整个条目。元数据校验会在可信核心内解开条目并清理字段缓冲；不会在审批前向扩展、MCP 或网络交付凭据。执行用的凭据缓冲仅在提交点取得，使用 Zeroizing，不实现返回值的 resolve RPC。

`UsePolicy` 返回 `{id, account_id, device_id, version, policy, created_at, updated_at}`，其中 `policy` 为：

```json
{
  "schema_version":1,
  "name":"Development issues",
  "enabled":true,
  "adapter":"github.issues/1",
  "credential":{"schema_version":1,"account_id":"040g2081040g2081040g","item_id":"1111111111111111111111111111111111111111111111111111111111111111","field_id":"credential"},
  "target":{"api_origin":"https://api.github.com","owner":"example","repo":"test"},
  "operations":["issue.create"]
}
```

创建生成随机 32 位小写 hex ID；修改/删除要求 `base_version`。禁用立即生效且增加版本；启用和扩大权限重新审批。可选 `grant_to` 授予持续到撤销的申请权限（每次使用仍另批），在同一配置审批中展示，并与策略、审批完成状态在同一 SQLite 事务中保存。不得通过 GitHub 重定向自动改变仓库，路径不代表永久仓库身份。

## Owner RPC 与 CLI

| RPC | 输入及结果 |
| --- | --- |
| `secrets/ref` | `item_id, field_id` → `{uri, reference}`，无值 |
| `secrets/policies/list` | `after?, limit?`（≤64）→ `{policies, next, device_local:true}` |
| `secrets/policies/save_request` | `source, id?, base_version?, policy, grant_to?` → 现有审批视图 |
| `secrets/policies/delete_request` | `source, id, base_version` → 删除审批 |
| `secrets/policies/disable` | `id` → 禁用后的策略 |
| `secrets/uses/get` | `request_id` → 状态/受限结果和 owner 审批视图；锁定时只有 ID、状态 |
| `secrets/uses/cancel` | `request_id` → 只取消 awaiting_user |
| `secrets/uses/list` | `before?`（上一页末请求 ID）, `actor?, kind?, limit?`（≤100）；P0 kind 仅 secret_use → `{uses,next,device_local:true}` |
| `secrets/uses/mark_reviewed` | `request_id` → 仅给 lost 添加 reviewed_at，不修改执行结果 |

```text
dmsg secrets ref <item-id> <field-id>
dmsg secrets policies list
dmsg secrets policies save policy-request.json
dmsg secrets policies disable <policy-id>
dmsg secrets policies delete <policy-id> <base-version>
dmsg secrets uses list [--before <request-id>] [--actor <agent-id>]
dmsg secrets uses show <request-id>
dmsg secrets uses cancel <request-id>
dmsg secrets uses reviewed <request-id>
```

配置 JSON 文件最多 64 KiB，只包含 `policy` 及可选 `id/base_version/grant_to`；不写 token。保存和删除命令只创建审批。使用批准只能由桌面完成，或由具有在场验证能力的设备在交互终端经在场确认完成；MCP、脚本和普通 renderer 无批准接口。

## MCP 合同

| 工具 | 行为 |
| --- | --- |
| `dmsg_secret_policies` | 无参数；列有有效权限的启用策略的 ID、名称、版本、目标和操作，不列凭据引用 |
| `dmsg_secret_use_request` | `policy_id, operation, arguments, idempotency_key, reason?` → 待审批 ID/状态/期限 |
| `dmsg_secret_use_status` | `request_id` → 自己请求的状态及受限结果；仍需有效 capability |
| `dmsg_secret_use_cancel` | `request_id` → 取消自己未决定的申请 |

`operation` 只能为 `issue.create`，`arguments` 只含 `title`（1–256 Unicode 字符）、可选 `body`（≤32 KiB UTF-8），reason ≤4096 UTF-8 bytes。幂等键为 1–128 ASCII 字母/数字/`-_.`；拒绝所有额外参数。Agent 身份来自验证后的凭证；输入不能设置 actor、设备、confirmed 或执行凭证。

请求绑定账户、设备、Agent 凭证摘要、接收申请时的具体 grant ID 和 Agent 暂停代次、daemon 启动随机标识、session_generation、策略版本、字段 revision、固定目标和完整参数。现有 `dmsg/approval/v1` 摘要覆盖完整计划，UI 安全展示全文。幂等键在账户/设备/Agent 内隔离：同键同输入返回原请求，同键异参 `IDEMPOTENCY_CONFLICT`；终态保留 30 天，之后不保证去重。暂停后恢复、撤销或到期后重新授予同名 capability，都不会恢复旧请求；须使用新幂等键形成新计划并重新审批。

锁定时无法读取封存的请求归属，所以 Agent 状态查询返回 `LOCKED`，不开放额外的明文归属索引。owner 可看已知请求的最小状态。新 credential 的同一 Agent 可查看自己的旧结果，不能执行已因凭证变化失效的旧申请。

## 审批、执行与失败

使用和策略变更都是高风险批准。等待期限 5 分钟，批准后立即调度；不向 Agent 返回租约或可转交执行 token。全局待审批 20、每 Agent 来源 5；外部执行每 Agent 最多 1、本设备最多 4，超额明确失败。

同步提交点复核当前账户/会话、凭证、pause、grant、策略及 revision；保存原审批 `started=true` 后取得执行凭据。网络在 `Hold::with` 外执行；锁定/禁用/撤销等尽力中断在途请求，不能收回已经发送的 POST。锁定不保留根用于结算；原账户再次解锁时将遗留请求终结，不写入新账户。

| 状态 | 含义 |
| --- | --- |
| awaiting_user | 尚无外部执行 |
| rejected/cancelled/expired | 不执行 |
| executing | 已批准；started 区分是否过提交点 |
| done | 已确认创建，结果只有 number/html_url |
| failed | 本地拒绝或明确的上游拒绝 |
| lost | 可能已创建，结果不明；不重放 |

重启/任务丢失后，started 的请求为 lost/EXECUTION_UNKNOWN，未 started 的为 failed/SECRET_SESSION_ENDED。不能交给通用对账器重新发送。人工核查只添加标记，新尝试必须使用新幂等键和新审批。

HTTP 固定 origin/path、正常 TLS、无环境代理、无跳转、关闭 reqwest 自动重试；连接 5 秒、总计 30 秒、响应≤1 MiB。201 校验 issue number 和匹配目标的 html_url 后仅返回这两个字段；明确鉴权/参数等拒绝为 failed，503、网络中断、异常成功格式或跳转为 lost。发送后除了适配器明确判定的 HTTP 拒绝，其他未识别错误也统一结算为 lost/EXECUTION_UNKNOWN；未发送的本地失败走独立结算路径。原始错误/正文不向客户端输出；已知凭据回显被阻止，`SECRET_OUTPUT_BLOCKED` 的外部结果保持 lost，不重试。

错误包括 `SECRET_REFERENCE_INVALID`、`SECRET_VERSION_STALE`、`SECRET_POLICY_STALE`、`SECRET_SESSION_ENDED`、`SECRET_OUTPUT_BLOCKED` 以及现有 FORBIDDEN/LOCKED/EXPIRED/QUOTA_EXCEEDED。测试 transport 只用于本地模拟，生产 RPC 无 endpoint override。

## 边界与验证

没有 canister 或稳定布局变更，没有新的同步种类。政策限制 dMsg 此入口，不证明 GitHub token 的实际权限，也不隔离同用户恶意代码。建议用户在上游限制仓库及 Issues 写权限。

P1 的 CLI 注入、SSH、文件挂载、快照分享和团队库不在本实现中。单元/本地 HTTP mock 与 Electron fixture 不代替真实 GitHub、真实系统在场验证或平台文件系统实测。具体验收命令与未运行项见 [P0 验证记录](../dmsg_secrets_P0_validation_zh.md)。

## P1：本机开发工作流

以下接口为 owner 控制面，未加入 MCP 工具目录。审批动作新增 `secret_process`、`secret_ssh_binding`、`secret_ssh`、`secret_mount_config`、`secret_mount_read`、`secret_share`、`secret_package_import`、`secret_discovery_import`，全部使用高风险在场审批。

### 进程交付

`secrets/run/request` 接收 `executable {path, identity, sha256}`、`argv`、`cwd`、`refs_file`、完整引用文档 `refs` 和 `files [{name, source}]`；返回 `request_id`、仅发给发起 CLI 的随机 `claim` 和不含值的审批。`secrets/run/claim {request_id, claim}` 只允许一次领取。`secrets/run/report` 记录 CLI 报告的启动/退出状态，不能改变交付事实。

`.env.refs` 只接受 `NAME=dmsg-secret://account/item/field`、空行和整行注释，不解释 shell。源文件上限 64 KiB、64 个变量；argv 上限 256 项/64 KiB；环境值总量上限 64 KiB。`--file` 最多 8 个、每个 8 MiB，来源为字段 URI 或 `dmsg-file://file_id/version_id`，交付为继承的顺序管道 `/dev/fd/N`。输出不会进入 MCP；读取结束为 EOF，没有重新解密或再次领取。

### SSH / Git

`secrets/ssh/keys` 只返回公钥、指纹、支持原因和本机 bindings；`binding_request {reference, purposes}` 中用途仅为 `ssh_auth`、`git_sign`；`disable {id}` 立即停止新签名。`status` 返回实际 socket 路径。socket 为当前 OS 用户所有，父目录 0700，socket 0600；最多 32 个连接，帧上限 256 KiB。

SSH 身份认证需要签名有效、非 forwarding 的 `session-bind@openssh.com`，并匹配 session、publickey、`ssh-connection` 和原连接。Git 仅允许 SSHSIG 的 `git` namespace、空 reserved 与 SHA-256/SHA-512 摘要。审批清楚区分核对协议摘要与核对提交原文；不把 host key 指纹解释成已验证 DNS 主机名。拒绝口令加密私钥和非 Ed25519 身份，不退化为返回私钥。

### 虚拟文件

`secrets/mounts/prepare` 返回路径、真实大小和来源版本预览，不创建卷。`save_request` 生成审批；`start/status/revoke_reads/stop` 负责生命周期。`input` 包含：

```json
{
  "project_label": "Local development",
  "project_directory": "/absolute/project",
  "mount_point": "/absolute/project/.dmsg-secrets",
  "entries": [{
    "relative_path": "service.json",
    "source": {"type": "library_file", "file_id": "<64 hex>", "version_id": "<64 hex>"}
  }],
  "aliases": [{"name": "service.json", "entry": "service.json"}]
}
```

另外两种来源为 `field_utf8 {reference: URI}` 和 `refs_dotenv {variables: [{name, reference: SecretRef}]}`。路径最多 8 层、64 个文件；原始 JSON/PEM/二进制文件不做内容改写。dotenv 渲染拒绝引号、反斜线、美元符、换行和 NUL，不进行猜测式转义。

平台模式由 daemon 选择，不能由读取者声称身份。Linux 使用 OS uid 与实际打开句柄，PID 只作来源提示，不声称进程隔离。macOS 使用 uid 和 `window_ms`（1–60,000 毫秒），显示“窗口内同一用户的其他程序也可能读取”。未决 IO 最长 30 秒，回调取消后迟到批准无效；每次范围读取最多 1 MiB。统计的 `released_bytes` 是交给文件系统后端的字节，不证明应用完整消费。mmap 和写回不属于支持合同。

扩展通过专用 0700 目录中的 Unix socket 连接 daemon，目录不存密钥或明文 backing file。daemon 验证同 UID、固定扩展 ID、FSKit entitlement 和同一签名团队。普通 owner RPC 没有挂载明文读取方法。失联、来源变更、锁定和重启关闭旧授权，不自动恢复窗口；原生缓存撤销或卸载失败保留失败状态。

### 快照发送与接收

`secrets/shares/prepare {input, password?}` 的 `input` 固定 `item_id`、`revision`、`field_ids`、`include_notes`、附件的 `file_id/version_id`、`expires_in_ms` 与 `max_download_sessions`。默认有效期一天、一个下载会话；有效期接受 6 分钟至 7 天，会话数 1–100。准备产生独立密钥及固定身份，不发布。密码派生在核心锁外运行，密码不持久化。

`request {id}` 产生审批。`list/get` 默认只有元数据；仅显式 `get {id, include_link:true}` 可在确认发布且未取消时取得链接。`reconcile` 查询原 share ID；`revoke` 撤销原身份。发布提交点之后不能再次创建，通用恢复器不能重做。未发布的上传中断，需要新的批准，保留原 upload/share 身份和密钥。

包的外层仍为现有加密分享文件；内层 MIME 为 `application/vnd.dmsg.secret-package`，格式为 4 字节大端 JSON 长度、严格 JSON manifest、按序附件字节。manifest 上限 256 KiB，整个包最多 100 MiB、64 字段、20 附件；拒绝未知字段、重复字段、坏长度/摘要和尾随字节。包只包含所选字段/备注/附件，不带源账户、源 item/field ID、标签、权限或历史。`snapshot_id` 独立随机；字段使用包内编号，标准模板语义可由经验证的角色后缀保留。

`secrets/packages/import_preview {link, password?}` 只接受当前配置的登录站点链接。先持久化随机下载会话 ID，再发起领取；重试沿用原会话。完整验证后返回元数据 `package_id/digest/preview`。`import_request {selection}` 绑定字段、备注、附件序号及目标 folder；审批后在同一事务提交新 Vault ID、Files、outbox 和收据。附件默认仅本地。包下载缓存只含密文，key 由本机 LDK 封存。

浏览器安全按纯文本展示；用户主动 reveal/copy 字段或单独保存附件，不提供整包明文下载。复制是独立副本；源条目后续修改和链接撤销均不会删除副本。浏览器领取后再导入客户端会使用另一个下载会话，默认仅一个会话时必须在领取前选择接收方式。

### 健康、扫描和导入

`secrets/health` 返回规则、subject/revision、观测时间和 evidence；云端状态保留 `last_checked_at`，未刷新不当作安全。`dismiss` 只忽略当前 finding ID。`rotation/create/update/list` 的步骤依次是 `created`、`tested`、`references_checked`、`upstream_revoked`；新版本必须实际更改所选凭据值。测试和上游撤销是用户报告，不是 dMsg 独立核实的事实。

`secrets/discovery/start {path}` 只扫描用户选定目录；`status/cancel/import_request` 跟踪原扫描与明确候选。扫描不跟随符号链接或跨设备、不读依赖/构建目录，最多 10,000 个目录项、100 MiB 总读取、单文件 1 MiB、256 个候选；达限或不可读显示 incomplete。报告只有路径、文件身份和整文件摘要、变量名及公钥算法，不含值。再次读取必须通过原摘要及身份验证，锁定立即取消 worker；导入生成纯引用内容，原文件不修改。

外部导入沿用 `vault/import_preview` / `vault/import`，增加明确的 attachment ID 选择和逐源条目遗漏报告。1pux 成员路径、重复名、符号链接和大小受限；仅选定附件解包为加密暂存，所有可见写入和重试收据同事务提交。相同完整源摘要、字段/附件选择和目标文件夹重复提交返回原收据。

平台与实测边界见 [P1 验证记录](../dmsg_secrets_P1_validation_zh.md)。生产/私有云端完整联调、正式 macOS 扩展内核联调及分发公证不以 mock 或开发签名替代。

附件进入 Files 后遵循目标文件夹现有的 `files.read:<folder>` 授权（含祖先文件夹授权）。本地/云端存储状态不是 Agent 访问控制；导入 UI 提供文件夹选择并明确告知继承权限，不能把“未备份”理解为 Agent 不可读。Vault 隐藏字段仍遵循单独的 Vault/secret.use 边界。
