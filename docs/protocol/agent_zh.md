# dMsg 与 Agent Delegation：自持 controller 与 principal 发布

简体中文 | 本文只描述公开 canister 接口与字节规则。协议正文以 [agent-protocols](https://github.com/ldclabs/agent-protocols) 固定版本为准：规范提交 `eb1160e`，Rust 与 TypeScript SDK `agent-protocols 0.11.2`。

## 角色

| 角色 | 实现 | 说明 |
| --- | --- | --- |
| Principal host | `dmsg_directory` | 在 `principal_origin/<account_id>` 以 ICP 认证 HTTP 返回 principal 文档 |
| 权威状态 | `dmsg_user`（账户所在 home） | controller 登记、退役、泄露标记与改名 |
| controller key | 客户端 vault 中的普通 Ed25519 密钥条目 | 持根设备都能读取；注册时向 home 证明持有，事件在本机签名 |
| Delegation 服务 | `delegation_query_url` 所在服务 | 按 Agent Delegation 1.0 接收事件、保存 accepted record、提供凭证与查询 |

user home 是 controller 变更的线性化点：退役一旦提交，该 key 不能再签出被接受的新事件，与目录何时发布无关。directory 只发布 home 推送的状态，不参与授权。

## Principal 与 controller

- Principal ID 为 `principal_origin + "/" + 规范 Xid 文本`，永久不变。`principal_origin` 在 `UserInit` 与 `DirectoryInit` 中配置且不可修改。
- 账户命令（`dmsg/account/v2` 批准，需 RootManage 管理员设备）：
  - `EnablePrincipal { principal_type }`：创建空 principal 并发布。
  - `RegisterController { generation, public_key, name, delegation, supersedes, proof }`：`proof` 是 controller 私钥对 `controller_pop_message(home, account, generation, delegation, supersedes, approval.request_id)`（域 `dmsg/controller-pop/v1`）的 Ed25519 签名，home 验签后才提交。generation 从 1 起连续分配、永不复用。不再有 `register_controller` 入口，也不调用 COSE。
  - `RetireController { generation }`、`MarkControllerCompromised { generation, invalid_from }`、`RenameController { generation, name }`。
- principal 变更推进 `account_version`，不推进 `security_epoch`。每次变更 `version` 加一，`updated_at = max(now, previous + 1)`；新记录的 `valid_from`、退役的 `retired_at` 都取该值，因此 `supersedes` 所需的“更早”关系天然成立。
- 上限：当前 controller 不超过 8 个，终身记录不超过 32 条（退役记录不删除）。受限权限最多 8 个 scope（每个不超过 64 字节）与 4 个依赖方（HTTPS origin 或 Agent ID，每个不超过 256 字节），名称 1..64 字节。
- home 与 directory 共用 64 KiB 文档的保守字节预算，计入配置 URL 的最大长度、所有 controller 后续退役/泄露字段及最大名称。超预算注册在 home 提交前返回 `QuotaExceeded`。
- `publish_principal(account_id)` 任何人可调用，幂等地把当前状态推送到 directory；变更命令在本地提交后会立即尝试一次。`get_principal` 返回 `PrincipalInfo`：当前状态与已发布版本。`published_version < state.version` 时，变更尚未对外生效。

## 本地签名

事件由扩展在本机构造、签名并提交，不经过 user home 或 COSE：

1. 客户端把 controller 私钥作为 vault 条目保存（对象 ID `SHA256(CBOR(["dmsg/agent-controller/1", account, generation]))`），与内容根同级别受保护、随根同步；泄露处置与内容根相同：撤销设备、换根、退役 controller。
2. 签名前核对 generation 为当前 controller、本机 vault 公钥等于已登记公钥、设备具备 `FormalApprove`（客户端规则）。`created_at = max(now, valid_from)`，nonce 为本机记录的 `max(last + 1, created_at)`。
3. 对 `SHA3-256(JCS(event))` 做 Ed25519 签名，信封的 `hash` 与 `signature` 为二者的 base64url（无填充）。信封先写入加密日志再提交；服务 5xx/408/429 时保留 `signed` 状态原样重提，4xx 为终态，需要新签名。
4. grant 的 scope、依赖方、`expires_at`（不超过 366 天）由 SDK 的 `validate_delegation_acceptance` 在服务端按认证文档检查；revoke 与替换的所有权（lineage）同样由 delegation 服务在接收时检查。
5. delegation ID 为 `<account_id>.<后缀>`，前缀是该 principal 的规范账户 ID 文本，扩展使用 128 位随机后缀。服务按这个前缀把凭证读取路由到账户，不以本账户 ID 加 `.` 开头的提交被拒绝。

## Directory

- `publish(account_id, PrincipalState)`：调用者必须是已登记的 user home；首次发布要求账户 ID 字节 4..9 等于该 home 的 Xid 分配器指纹（`SHA-256` 承诺 `("dmsg", environment, issuer_namespace, home)` 的前 5 字节），此后只接受已记录的 home。版本更旧返回当前发布，同版本同内容幂等，同版本异内容为 `IdempotencyConflict`，新版本的 `updated_at` 必须更大。
- 渲染：principal 文档为 JCS JSON，`type` 为小写类型名，`controllers` 与 `retired_controllers` 使用配置的 `controller_source`，`delegation_query_url` 为配置值，另含一条 `rel: "profile"` 链接；发布前用 SDK 的 `validate_principal_document` 校验。
- 配置：`principal_origin` 与 `controller_source` 最多 512 字节，`delegation_query_url` 与 `profile_url_prefix` 最多 2 KiB；URL 不允许原始控制字节、双引号或反斜线，需要时使用 URL 百分号编码。这四项写入每个 principal ID 或文档，安装后不可修改。`custom_domains` 最多 8 个小写域名，必须包含 `principal_origin` 的主机名，由 `admin_set_custom_domains` 整体替换。
- HTTP：`GET /<account_id>` 返回 200 文档（`content-type: application/json`，`access-control-allow-origin: *`，`cache-control: public, max-age=30`）；其余路径返回认证 404 JSON；`/.well-known/ic-domains` 返回自定义域列表。所有响应按 response-only 方式认证全部响应头，HTTP 网关可验证。不做重定向，因为最终 URL 必须等于文档 `id`。
- HTTP 路由与认证树采用相同路径规则：百分号解码、忽略连续斜线产生的空段，保留尾斜线区别；等价路径返回相同认证响应。
- 只接受两类 ingress：已登记 home 的 `publish`，以及 controller 或 governance 的调用；其余 ingress 在执行前被拒绝，查询请走 query。`directory_stats` 返回已发布账户数、stable 页数与 cycles 余额。
- directory 稳定布局 schema 5 保存确切正文及其 SHA-256 摘要；认证树在 stable memory 中，升级只重新认证 404 与域名列表，不访问文档。home 迁移（`handoff`）尚未实现。

## 安全快照

`SecuritySnapshot` schema 4 含 `principal_updated_at`：账户当前 principal 状态的 `updated_at`，未启用时为 `null`。依赖方或服务若发现认证快照中的值大于 directory 文档的 `updated_at`，说明发布滞后，不应按旧文档接收新事件。

## 示例文档

```json
{"controllers":[{"delegation":{"audiences":["https://dmsg.net"],"scopes":["message.draft"]},"id":"did:agent:<key>","name":"dMsg signer #1","source":"https://dmsg.net","valid_from":1790000000001}],"delegation_query_url":"https://agents.dmsg.net/v1/delegations/query","id":"https://id.dmsg.net/<account_id>","links":[{"name":"dMsg","rel":"profile","url":"https://dmsg.net/u/<account_id>"}],"protocol":"agent-delegation/1.0","type":"person","updated_at":1790000000001}
```

## 验证

- `dmsg_protocol::agent` 单测：文档渲染经 SDK 校验、状态不变量、目录配置，以及与扩展共享的注册批准和 controller PoP 摘要向量（`protocol_vectors.json` 的 `controller_pop_v1`）。
- `dmsg_user` 单测：principal 生命周期的单调性、上限、重放与不推进 `security_epoch`；错误 PoP 被拒。
- PocketIC `agent::self_held_principal_publishes_certified_documents_and_its_grants_are_accepted`：启用、错误 PoP 注册被拒、注册、发布、以 `ic-response-verification` 验证认证 200/404/域名响应、快照 schema 4、本机 Rust SDK 签名的 grant 被 `validate_delegation_acceptance` 接受、范围外与他人 key 被拒、轮换、目录发布规则与升级后文档不变。
- 客户端 `agent-client.test.ts`：vault key 签名与 nonce 推进、暂时失败后原样重提、终态拒绝、公钥不符拒签、注册 PoP。

以上为本地测试，不代表生产部署、容量实测或安全审计已经完成。
