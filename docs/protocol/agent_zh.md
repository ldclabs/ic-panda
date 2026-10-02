# dMsg 与 Agent Delegation：托管 controller 与 principal 发布

简体中文 | 本文只描述公开 canister 接口与字节规则。协议正文以 [agent-protocols](https://github.com/ldclabs/agent-protocols) 固定版本为准：规范提交 `6a71313`，Rust 与 TypeScript SDK `agent-protocols 0.10.0`。

## 角色

| 角色 | 实现 | 说明 |
| --- | --- | --- |
| Principal host | `dmsg_directory` | 在 `principal_origin/<account_id>` 以 ICP 认证 HTTP 返回 principal 文档 |
| 权威状态 | `dmsg_user`（账户所在 home） | controller 登记、退役、泄露标记、改名与托管签名授权 |
| 托管 controller key | `dmsg_cose` | `KeyPurpose::AgentController` 的阈值 Ed25519 key，每次签名需设备批准 |
| Delegation 服务 | `delegation_query_url` 所在服务 | 按 Agent Delegation 1.0 接收事件、保存 accepted record、提供凭证与查询 |

user home 是 controller 变更与签名的线性化点：退役一旦提交，该 key 不能再签出新事件，与目录何时发布无关。directory 只发布 home 推送的状态，不参与授权。

## Principal 与 controller

- Principal ID 为 `principal_origin + "/" + 规范 Xid 文本`，永久不变。`principal_origin` 在 `UserInit` 与 `DirectoryInit` 中配置且不可修改。
- 账户命令（`dmsg/account/v2` 批准，需 RootManage 管理员设备）：
  - `EnablePrincipal { principal_type }`：需恢复材料已确认；创建空 principal 并发布。
  - `RegisterController { generation, public_key, name, delegation, supersedes }`：只能经 `register_controller` 提交；home 在提交前调用 COSE 派生该 generation 的公钥，必须等于批准的 `public_key`。generation 从 1 起连续分配、永不复用；需恢复材料已确认。
  - `RetireController { generation }`、`MarkControllerCompromised { generation, invalid_from }`、`RenameController { generation, name }`。
- principal 变更推进 `account_version`，不推进 `security_epoch`。每次变更 `version` 加一，`updated_at = max(now, previous + 1)`；新记录的 `valid_from`、退役的 `retired_at` 都取该值，因此 `supersedes` 所需的“更早”关系天然成立。
- 上限：当前 controller 不超过 8 个，终身记录不超过 32 条（退役记录不删除）。受限权限最多 8 个 scope（每个不超过 64 字节）与 4 个依赖方（HTTPS origin 或 Agent ID，每个不超过 256 字节），名称 1..64 字节。
- home 与 directory 共用 64 KiB 文档的保守字节预算，计入配置 URL 的最大长度、所有 controller 后续退役/泄露字段及最大名称。超预算注册在 home 提交前返回 `QuotaExceeded`；字节预算可能先于 32 条记录上限耗尽，已接受状态的退役、泄露标记和改名不再增加预算。
- `publish_principal(account_id)` 任何人可调用，幂等地把当前状态推送到 directory；变更命令在本地提交后会立即尝试一次。`get_principal` 返回 `PrincipalInfo`：当前状态、已发布版本与各 generation 已签的最大 nonce。`published_version < state.version` 时，变更尚未对外生效。

## 托管签名

`sign_agent_event(AgentEventSignRequest)` 转换为 `ExecutionKind::AgentEvent { key, event, principal_id, origin }`，按 `dmsg/execute/v3` 批准（与正式签名相同的设备批准、FormalApprove 能力、账户策略、商业额度与每日预算）。在任何跨 canister 调用之前，home 检查：

1. `event` 是 Agent Delegation 1.0 事件的精确 JCS 文本（不超过 16 KiB），只含六个 Agent Identity 字段；重复键、孤立代理项、非安全整数、非规范文本、未知或显式 `null` 的负载字段一律拒绝。
2. generation 为当前 controller，`actor` 等于其 Agent ID，`payload.principal_id` 等于本账户 principal ID。
3. delegation ID 为 `<account_id>.<非空后缀>`，服务按此前缀路由到账户。
4. `now - 60s ≤ created_at ≤ now + 5s`，且不早于 controller 的 `valid_from`。
5. 受限 controller 的 grant 必须在其 scope 与依赖方范围内；grant 必须带 `expires_at` 且不超过 `created_at` 后 366 天；`constraints` 的 JCS 不超过 4 KiB。
6. nonce 大于该 generation 已签的最大值（不超过 2^53-1），并在同一消息中记录。

revoke 与替换的所有权（lineage）由 delegation 服务在接收时检查。COSE 再次严格解析事件、核对 `actor` 与派生公钥、`principal_id`，然后对 32 字节 `SHA3-256(event)` 做阈值 Ed25519 签名，并在返回前验签。输出为 `ExecutionOutput::AgentSignature { event_hash, signature, key }`；信封的 `hash` 与 `signature` 分别是二者的 base64url（无填充）。`AgentEvent` 不产生 `ExecutionReceipt` 认证叶。

## Directory

- `publish(account_id, PrincipalState)`：调用者必须是已登记的 user home；首次发布要求账户 ID 字节 4..9 等于该 home 的 Xid 分配器指纹（`SHA-256` 承诺 `("dmsg", environment, issuer_namespace, home)` 的前 5 字节），此后只接受已记录的 home。版本更旧返回当前发布，同版本同内容幂等，同版本异内容为 `IdempotencyConflict`，新版本的 `updated_at` 必须更大。
- 渲染：principal 文档为 JCS JSON，`type` 为小写类型名，`controllers` 与 `retired_controllers` 使用配置的 `controller_source`，`delegation_query_url` 为配置值，另含一条 `rel: "profile"` 链接；发布前用 SDK 的 `validate_principal_document` 校验。
- 配置：`principal_origin` 与 `controller_source` 最多 512 字节，`delegation_query_url` 与 `profile_url_prefix` 最多 2 KiB；URL 不允许原始控制字节、双引号或反斜线，需要时使用 URL 百分号编码。配置边界与共享文档预算保持一致。
- HTTP：`GET /<account_id>` 返回 200 文档（`content-type: application/json`，`access-control-allow-origin: *`，`cache-control: public, max-age=30`）；其余路径返回认证 404 JSON；`/.well-known/ic-domains` 返回自定义域列表。所有响应按 response-only 方式认证全部响应头，HTTP 网关可验证。不做重定向，因为最终 URL 必须等于文档 `id`。
- HTTP 路由与认证树采用相同路径规则：百分号解码、忽略连续斜线产生的空段，保留尾斜线区别；等价路径返回相同认证响应。主体权威解析仍要求最终 URL 精确等于文档 `id`，其他 URL 下的副本不授予 controller 权限。
- directory 稳定布局 schema 2 保存确切正文及 SHA-256 摘要，查询复用摘要并接管正文缓冲区；升级逐条读取稳定记录重建认证树，最后统一发布根哈希，不收集全部正文。开发期拒绝 schema 1，不迁移。home 迁移（`handoff`）尚未实现。

## 安全快照

`SecuritySnapshot` schema 3 增加 `principal_updated_at`：账户当前 principal 状态的 `updated_at`，未启用时为 `null`。依赖方或服务若发现认证快照中的值大于 directory 文档的 `updated_at`，说明发布滞后，不应按旧文档接收新事件。

## 示例文档

```json
{"controllers":[{"delegation":{"audiences":["https://dmsg.net"],"scopes":["message.draft"]},"id":"did:agent:<key>","name":"dMsg hosted signer #1","source":"https://dmsg.net","valid_from":1790000000001}],"delegation_query_url":"https://agents.dmsg.net/v1/delegations/query","id":"https://id.dmsg.net/<account_id>","links":[{"name":"dMsg","rel":"profile","url":"https://dmsg.net/u/<account_id>"}],"protocol":"agent-delegation/1.0","type":"person","updated_at":1790000000001}
```

## 验证

- `dmsg_protocol::agent` 单测：严格解析与拒绝用例、托管签名政策、文档渲染经 SDK 校验、状态不变量、目录配置，以及与扩展共享的两条审批摘要向量。
- `dmsg_user` 单测：principal 生命周期的单调性、上限、重放与不推进 `security_epoch`；托管签名授权点。
- PocketIC `agent::hosted_principal_publishes_certified_documents_and_signs_acceptable_grants`：启用、错误公钥注册被拒、注册、发布、以 `ic-response-verification` 验证认证 200/404/域名响应、快照 schema 3、托管签名后以 Rust SDK `validate_delegation_acceptance` 接受、nonce/范围/他人 key 被拒、退役、目录发布规则与升级重建。
- 独立 PocketIC `directory` 测试：home 权限与跨 home 隔离、版本幂等及拒绝原子性、文档预算与后续安全变更、路径规范化与正文篡改、追加 home/替换域名、禁止修改永久配置和升级恢复。`directory_cost_and_rebuild_profile` 为显式运行的成本与内存样本，不代表最大容量。

以上为本地测试，不代表生产部署、容量实测或安全审计已经完成。
