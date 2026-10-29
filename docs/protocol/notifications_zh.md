# dmsg-notifications/1

此合同定义无内容通知投影、偏好、实际查看回执和受限导航，不授予消息读取、签名、付款或审批权限。协议能力须由服务声明；文档存在不代表已经部署。客户端策略在 `dmsg_client::notifications`，共享向量为 [notifications-v1.json](../../tests/vectors/client/notifications-v1.json)。

## 身份与状态

类别固定为 `security`、`action`、`message`、`service`，由验证后的业务来源决定。Agent、消息作者不能任意声明优先级。事项 ID 为 `SHA256(canonical-CBOR([1, "dmsg/notification/item/v1", [account, target]]))`；`target` 使用核心枚举的序列化结构。系统分组同样绑定账户。安装 ID 不是认证凭据。

投影仅携带账户范围、opaque 资源、业务版本、水位、可信截止时间、是否待处理、已知未读数和类型化目标。正文、名称、来源 URL、文件名、金额、密钥和审批载荷不进入投影。锁定可读的元数据不具有内容根加密的保密性。

JSON u64 业务版本、水位和游标使用无前导零的十进制字符串，时间使用 UTC 毫秒。目标允许 `channel`、`inbox`、`approval`、`recovery`、`agent_mail`、`task`、`plan`、`wallet`（钱包 Principal 与十进制提案 ID）、`service`（设备服务事项）、`summary`，不允许任意 URL、路径或 RPC。设备本地审批、任务、设备服务及连接器 Agent Mail 不自动成为跨设备事项。

业务终态、实际查看和展示尝试相互独立。系统受理、点击、划掉横幅、清理近期历史均不能推进业务已读、商业通知签收或 Agent handled 状态。点击须验证原账户登记并重新读取原事项，不能打开当前账户的另一条待审批。

## 能力和频道兼容

`GET /public/notifications/capabilities` 返回 `{ "protocol": "dmsg-notifications/1", "publish_metadata": true, "feed": true, "preferences": true }`，仅声明已实现能力。端点不存在或没有对应能力时，客户端继续使用原有消息合同，并明确远程过滤尚未启用。

服务声明 `publish_metadata` 后，只有 `POST /v1/channels/:id/messages` 的 canonical CBOR 外层允许：

```text
{ cose_sign1: <原命令>, notification?: { version: 1, kind: "message" | "update" | "silent" } }
```

普通文字、附件、回复为 `message`；编辑与撤回为 `update`；后台协作控制为 `silent`。这会暴露“新消息或更新”这一元数据。标记不进入 `dmsg/channel/message/v1` 载荷，不改变原 COSE、AAD、密文、摘要或读端结构。省略字段时原请求字节保持不变，接收者不需要同时升级。

HTTP PoP 的 body digest 覆盖整个外层。服务同时校验原命令作者、HTTP 身份与作者绑定、当前权限和原幂等键。原操作第一次提交时固定标记；重投返回原结果或拒绝冲突，不能新建候选。客户端 journal 和预签请求固定原标记；结果未知时先查原操作，不能为降级重建消息 ID。其他命令拒绝该字段。无标记的旧请求只驱动补读或有界静默待核对提示，不能宣称能够识别密文编辑。

## 登记、偏好与补读

设备接口使用既有 HTTPS、canonical CBOR 与 HTTP PoP 认证，请求最多 64 KiB；账户与有效设备绑定。专用接收凭据限于 `notifications:read`、`notifications:observe`、`notifications:revoke`，原 hint bearer 不自动获得新权限。偏好写入仍需设备认证，接收凭据不能延长业务授权。

| 接口 | 合同 |
| --- | --- |
| `POST /v1/notifications/register` | 安装、应用、环境、平台、账户设备、provider token、语言；验证安装持有证明和原平台证明，返回登记版本、有效期、opaque 路由和限定接收凭据 |
| `POST /v1/notifications/revoke` | 登记 ID、期望版本；幂等停用本安装对应账户 |
| `GET /v1/notifications/settings` | 认证范围内的安装与账户偏好版本，不返回其他安装 token |
| `POST /v1/notifications/settings` | 作用域、期望版本、允许字段 patch；冲突为 `VERSION_CONFLICT`，不按客户端时间覆盖 |
| `GET /v1/notifications/feed?after=…&limit=…` | 当前账户的 `entries`、`next_cursor`、`complete`、`gap`，最多 100 项 |
| `POST /v1/notifications/observed` | 本账户设备实际查看的 item/version 或 watermark；只用于通知去重 |
| `GET /v1/notifications/routes/:opaque` | 验证安装登记后返回原账户允许的目标；未知、注销路由拒绝 |

游标绑定账户和协议版本；缺口返回 `gap: true` 并要求当前状态重建。只有完整成功快照才能结束不再存在的旧待办；未完分页和失败不能清空状态。源版本排序由领域决定，不能比较摘要大小。投递过期不结束业务事项。

### v1 请求字段

安装持有证明使用独立 Ed25519 密钥，私钥保存在宿主安全存储，`installation = SHA256(public_key)`。登记的 canonical CBOR 外层为 `{installation, public_key, proof, platform, language, apple}`：公钥为小写 hex，proof 为无 padding base64url，platform 为 `desktop/ios/android`，apple 为 `null` 或 `{installation: UUID, revision}`。iOS 的 apple 引用已通过 App Attest 验证的 APNs 登记，不接受绕过原平台证明的 token。Android 尚未启用时明确拒绝登记，不声明该平台可用。

证明签名输入为 `SHA256(canonical-CBOR([1, "dmsg/notification/installation/v1", details]))`；details 为除 proof 外的登记字段，加上 `account`、`device`、HTTP PoP 的 `request_id`、服务的 `environment`（local/staging/production）及 `application: "dmsg"`。HTTP PoP 仍覆盖完整请求体。返回 `{protocol, installation, registration, revision, expires_at, receiver, scopes}`；租期 7 天，续期需要有效设备认证，不通过接收凭据续租。

限定接收凭据在 `Authorization: Bearer <receiver>` 发送，仅可访问 feed、observed、revoke 与 routes。请求 query 同时包含 `account`、`installation`、`registration`，服务逐次校验有效设备和登记；不与 hint bearer 混用。feed 附加 `after`（十进制游标，初始 0）及 `limit`（1–100）；失效租约不当作空快照。路由是 128 字符的小写 hex 文本（安装摘要 64 字符加随机路由 ID 64 字符），不含账户或业务目标。

设置写入 `{scope, installation, expected_revision, value}`；scope 是 `global`、`account` 或 `resource`。value 分别使用核心安装设置（不含 revision）、账户设置（不含 revision/remote_revision）或 `{resource, muted, until}`，返回已提交 `revision`。全局及账户开关首次默认关闭，resource 的版本按账户资源共享。观察请求是 `{installation, id, version}`，撤销是 `{installation, expected_revision}`。通知版本：频道用原日志水位；不可变来信事项用 `1`；恢复用认证账户版本。读、归档等终态不新增来信版本。

偏好包括本安装总开关、暂停、IANA 时区每日免打扰、安全例外、四类开关/声音、账户登记、跨设备合并和资源静音/截止。服务在 provider 调用前过滤。本机未确认修改显示待同步，不保证撤回已经入 provider 队列的通知。

## 统一策略与平台

决策顺序固定：验证业务 → 保存投影与计数 → 偏好/静音 → 免打扰 → 实际查看 → 合并/预算 → 展示。已关闭的项不能被后续优先级恢复。

消息与审批合并 3 秒，同会话/来源 60 秒至多一次新横幅；每安装跨账户分别最多 3 次消息、3 次审批/60 秒，超额使用一条有明确覆盖水位的汇总。安全首条立即，其后 3 秒合并。全安装声音每 60 秒至多一次，重试静音。前台用应用内提示；正在查看同一事项则只更新该页。移动共享事项的后台展示仅由远程路径负责。

恢复首次及经验证截止前 24 小时、1 小时各一次；没有可信截止只保留首次告警。审批首次及截止前 `min(5 分钟, 原有效期 * 20%)` 一次，查看后不催促，与首次不足 60 秒则合并。服务期限 30/7/1 天三个阶段，前两阶段仅应用内。阶段跨重启保留，稍后提醒不能延长业务有效期。

多设备查看协调与原 3 秒窗口同时开始，不串行累加。仅实际查看同版本可抑制普通通知，在线、收到提示和 OS 受理都不是查看。安全阶段不受其他设备查看抑制。协调不可用时独立按时展示，不选举主设备。

计划先保存稳定 system ID、delivery ID、覆盖项/版本/阶段，再提交平台。重复消费同次计划保持 ID 且静音。提交成功不等于送达。失败退避从 5 秒起、最大 15 分钟、不超过 12 次及有效期，可加最多 20% 抖动；token 失效则停用。消息 TTL 最长 1 小时，审批同时受业务截止约束，安全/服务最长 24 小时。

推送 payload 仅包含版本、opaque 安装路由、稳定通知 ID、类别、固定文案键、有效期与合并标识，不带原账户 ID 或任意 URL。APNs 按类别和组区分 collapse ID；Android 四个稳定渠道，FCM 传输使用有限固定 collapse keys，系统 tag/ID 单独分组。不得重建渠道绕过用户设置。Focus、权限、节能限制不通过高频重试绕过。

## 验证边界

策略向量只证明纯决策一致性；字节/PoP 测试证明附加字段绑定和旧格式保持。设备授权、provider、安装包、冷启动及真机展示须分别集成验收。无平台可靠回调时只报告“已提交”，不伪造“已送达”。
