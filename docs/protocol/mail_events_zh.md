# dMsg Mail 推送与本地事件

日期：2026-10-23。实现合同，不表示生产部署或宿主即时唤醒已经验收。

## 1. 两种事件

- `dmsg.mail.available`：Mail relay 已接受密文，通知收件端同步。它可以合并同一邮箱的多次变化；不是收件或已读回执，不携带主题、正文、附件或发件人。
- `dmsg.mail.received`：本机已验证、解密、去重并保存邮件。由 `dmsg mcp` 提供 `poll` 和 `push`，数据仅含 `mailbox_id`、`from`、`message_id`、`thread_id`。

现有 `dmsg.relay.delivery` 继续转发 webhook 事件。Mail 的 Message、Packet、Mailbox Card 编码和签名域不变；连接器的 `sync_mode: explicit` 表示由宿主调用同步，不排除宿主通过事件驱动它。

## 2. Mail relay 的可选扩展

`GET /.well-known/dmsg-mail-events` 返回 `protocol: dmsg-mail-events/1`、规范 `service`、同源 `/mcp` 地址及事件名。该发现文档独立于 `agent-mail/1.0`。

`POST /mcp` 提供 `server/discover`、`initialize`、`events/list`、`events/subscribe` 和 `events/unsubscribe`。订阅需要该邮箱所有者的短期 Agent Identity JWT；`aud` 是 relay origin。参数为：

```json
{
  "name": "dmsg.mail.available",
  "arguments": {"mailbox_id": "<owned-mailbox>"},
  "delivery": {"mode": "webhook", "url": "<callback-url>", "secret": "<whsec-secret>"},
  "ttlMs": 86400000,
  "cursor": null
}
```

回调目的地受服务运营方的精确 origin 白名单限制。先验证回调挑战，再激活订阅；验证和投递均使用 Standard Webhooks，拒绝重定向。订阅 ID 对同一 owner、邮箱和回调 URL 稳定；续期返回 `id`、`refreshBefore`、安全的 `cursor` 和 `truncated`。服务可缩短期限，客户端必须按返回的截止续期。

`available` 的数据是 `{mailbox_id, through_seq}`。游标为不透明水位；通知可能合并、重试或乱序，只能作为同步提示。首次订阅、重连及缺口后都应检查权威邮箱。重投同一 Packet 不产生新的邮箱变化。退订不删除已接受的邮件。

合并的上游提示不支持 `maxAgeMs`；下述本地收信事件日志支持。

## 3. 设备订阅管理

所有者使用 `dmsg agent mail-watch <agent> on|off`，省略状态可查询。桌面邮件页有同一开关。启用需要该 Agent 已有 `mail.read:<relay>` 与 `events.endpoint` 授权，不自动扩大权限。

设备为每个 Agent 保存一个专用回调端点并复用它，维护各邮箱的订阅和续期。配置、进度与端点凭据保留在当前账户的本机加密工作区，不跨设备同步。关闭、暂停或撤权后停止本地处理并清理订阅；清理暂不可用时回调租约最终过期。网络等待不占用共享 Client；端点创建/删除沿用现有持久化生命周期。

登录和会话变化会立即唤醒提示通道连接检查；重连后补拉 Mail 与 MCP Events。现有账户提示通道的 `mcp-event` 唤醒收信。提示只触发检查，不能指定新的网络目的地或授权执行。锁定时常驻且有 Mail 权限的 Agent 可以收信；订阅管理和续期在设备解锁时进行，锁定期间现有订阅过期后仍有周期同步兜底。非驻留 Agent 在解锁后补收。

## 4. 本地 MCP 事件

`events/list` 包含 `dmsg.mail.received`，参数可选择 `mailbox_id`。`events/poll`、`events/stream` 按当前 Agent 的 Mail 权限返回事件。游标绑定 Agent 与邮箱筛选，独立于中继事件游标；`null` 从当前头开始。支持 `maxAgeMs`，越过保留边界返回 `truncated: true`。

本地事件与邮件快照原子保存，事件 ID 对 `(recipient, from, message_id)` 稳定。每 Agent 保留最多 10,000 条、7 天；断线后用保存的游标重放并按事件 ID 去重。缺口后检查本地未读邮件与待处理队列。仅收到事件不标已读，也不确认处理完成。

Agent 处理流程：先领取 `mail.received` 注意力条目，按 `from + message_id` 用 `mail_list(mark_read=true)` 读取，完成主人授权的处理后 `inbox_ack(handled)`。重试任务可能已经已读，仍应按消息 ID 读取；未完成任务使用 defer。桌面预览和历史查看保持 `mark_read=false`。已读、队列完成和信中请求的执行授权是三个不同状态。

## 5. 宿主边界

MCP 客户端需要订阅事件并将其接到任务调度器。Anda 原生事件触发器可直接消费本地 `push`；不支持事件唤醒的宿主仍须保留显式检查。`notifications/resources/updated` 继续提供界面/资源变更提示，但不能单独证明 LLM 已启动。

协议扩展、断线恢复、权限撤销和完整本地链路由仓库测试验证；真实生产延迟和具体宿主唤醒必须另行实测。
