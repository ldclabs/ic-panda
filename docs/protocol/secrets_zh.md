# 本设备凭据受控使用（P0）

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
