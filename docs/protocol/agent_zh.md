# dMsg 与 Agent Delegation：自持 controller 与 principal 发布

简体中文 | 本文只描述公开 canister 接口与字节规则。协议正文以 [agent-protocols](https://github.com/ldclabs/agent-protocols) 固定版本为准：本次采用 `v0.12.3`（本地 SDK 提交 `08e90ee`），Rust 与 TypeScript SDK `agent-protocols 0.12.3`。

## 角色

| 角色 | 实现 | 说明 |
| --- | --- | --- |
| Principal host | `dmsg_directory` | 在 `principal_origin/<account_id>` 以 ICP 认证 HTTP 返回 principal 文档 |
| 权威状态 | `dmsg_user`（账户所在 home） | 公开资料更新，以及 controller 登记、退役、泄露标记与改名 |
| controller key | 客户端 vault 中的普通 Ed25519 密钥条目 | 持根设备都能读取；注册时向 home 证明持有，事件在本机签名 |
| Delegation 服务 | 文档 `delegation_service` 指向的源，即 dmsg-cloud 中继的 API origin（`https://api.dmsg.net`） | 按 Agent Delegation 1.0 接收事件、保存 accepted record、提供凭证与查询；查询与读取端点由该源的发现文档给出 |

user home 是 controller 变更的线性化点：退役一旦提交，该 key 不能再签出被接受的新事件，与目录何时发布无关。directory 只发布 home 推送的状态，不参与授权。

## Principal 与 controller

- Principal ID 为 `principal_origin + "/" + 规范 Xid 文本`，永久不变。`principal_origin` 在 `UserInit` 与 `DirectoryInit` 中配置且不可修改。
- 账户命令（`dmsg/account/v2` 批准，需 RootManage 管理员设备）：
  - `EnablePrincipal { principal_type }`：创建空 principal 并发布；普通用户首次提交 `UpdatePrincipal` 时也可初始化，无需先登记 controller。
  - `UpdatePrincipal { document }`：UTF-8 官方 `PrincipalDocument` JSON，最多 64 KiB。仅替换 `type/name/description/avatar_url/links/extra`；`id` 必须匹配本账户，`updated_at` 必须等于当前值（首次为 0）。`controllers`、`retired_controllers`、`aliases`、服务地址与新时间戳由服务生成，表单不能修改它们。与其他账户命令一样要求 RootManage 管理员设备批准。
  - `RegisterController { generation, public_key, name, delegation, proof }`：`proof` 是 controller 私钥对 `controller_pop_message(home, account, generation, delegation, approval.request_id)`（域 `dmsg/controller-pop/v1`）的 Ed25519 签名，home 验签后才提交。generation 从 1 起连续分配、永不复用。不再有 `register_controller` 入口，也不调用 COSE。
  - `RetireController { generation }`、`MarkControllerCompromised { generation, invalid_from }`、`RenameController { generation, name }`。
- principal 变更推进 `account_version`，不推进 `security_epoch`。每次变更 `version` 加一，`updated_at = max(now, previous + 1)`；新记录的 `valid_from`、退役的 `retired_at` 都取该值。
- 上限：当前 controller 不超过 8 个，终身记录不超过 32 条（退役记录不删除）。受限权限最多 8 个 scope（每个不超过 64 字节）与 4 个依赖方（HTTPS origin 或 Agent ID，每个不超过 256 字节），名称 1..64 字节。
- home 与 directory 共用 64 KiB 文档的保守字节预算，计入配置 URL 的最大长度、所有 controller 后续退役/泄露字段及最大名称。超预算注册在 home 提交前返回 `QuotaExceeded`。
- `publish_principal(account_id)` 任何人可调用，幂等地把当前状态推送到 directory；变更命令在本地提交后会立即尝试一次。`get_principal` 返回 `PrincipalInfo`：当前状态与已发布版本。`published_version < state.version` 时，变更尚未对外生效。

### Controller 登记输入

桌面端提供 dMsg 默认签名密钥入口，私钥仍由客户端生成并保存在秘密库；生产目录发布的 `source` 为 `https://dmsg.net`。用户须明确选择不受限权限，再经审批登记；默认入口不会自动授予 Agent 能力。自定义受限密钥在高级选项中登记，范围由接入应用定义，格式校验不证明应用支持该范围。

`agent/register` 必须显式传 `unrestricted: true`，或同时传非空 `scopes` 与 `audiences`，不得混用；遗漏字段不再等价于 `*`。CLI 对应 `--unrestricted` 或 `--scope` 与 `--audience`。桌面表单和 Rust 客户端在审批前拒绝重复/空条目、范围中的空白或通配符、无效依赖方及超限输入：范围最多 8 项、每项 64 UTF-8 字节，依赖方最多 4 项、每项 256 UTF-8 字节，名称最多 64 UTF-8 字节且无控制字符。换钥沿用相同校验和显式权限选择。

## 本地签名

事件由客户端（桌面 daemon，`src/dmsg_client` 的 `agent` 模块）在本机构造、签名并提交，不经过 user home 或 COSE：

1. 客户端把 controller 私钥作为 vault 条目保存（对象 ID `SHA256(CBOR(["dmsg/agent-controller/1", account, generation]))`），与内容根同级别受保护、随根同步；泄露处置与内容根相同：撤销设备、换根、退役 controller。
2. 签名前核对 generation 为当前 controller、本机 vault 公钥等于已登记公钥、设备具备 `FormalApprove`（客户端规则）。`created_at = max(now, valid_from)`，nonce 为本机记录的 `max(last + 1, created_at)`。
3. 对 `SHA3-256(JCS(event))` 做 Ed25519 签名，信封的 `hash` 与 `signature` 为二者的 base64url（无填充）。信封先写入加密日志再提交；服务 5xx/408/429 时保留 `signed` 状态原样重提，4xx 为终态，需要新签名。
4. grant 的 scope、依赖方、`expires_at`（不超过 366 天）由 SDK 的 `validate_delegation_acceptance` 在服务端按认证文档检查。revoke 与替换同样由 delegation 服务在接收时检查，规则是 Delegation 第 5 节的上限：受限 controller 只能管理最新 grant 的 scope 与依赖方都在其上限内的凭证，`"*"` 可管理全部凭证，不记录凭证由哪把 key 签发。换钥时新 key 的上限覆盖旧凭证，即可替换或撤销旧 key 签出的凭证。客户端在创建审批前就按同一规则选 key：默认取上限覆盖该 grant（撤销与替换时取服务所存原凭证的 scope 与依赖方，替换的新范围不超出它）的最新当前 controller，指定的 generation 也必须覆盖，否则返回 `Forbidden`，不会让服务在批准、签名之后才以 `delegation_ceiling_exceeded` 拒绝。
5. delegation ID 为 `<account_id>.<后缀>`，前缀是该 principal 的规范账户 ID 文本，客户端使用 128 位随机后缀。替换（续期）沿用原 ID：客户端只为同一 subject 签发，scope 与依赖方不超出服务所存的原凭证，期限重新计算。服务按这个前缀把凭证读取路由到账户，不以本账户 ID 加 `.` 开头的提交被拒绝。

## Directory

- `publish(account_id, PrincipalPublicationInput)`：接收 principal 状态，`metadata` 为 optional bytes 以兼容逐个升级的旧 home；新 home 可直接发送完整 `PrincipalState`。调用者必须是已登记的 user home；首次发布要求账户 ID 字节 4..9 等于该 home 的 Xid 分配器指纹（`SHA-256` 承诺 `("dmsg", environment, issuer_namespace, home)` 的前 5 字节），此后只接受已记录的 home。版本更旧返回当前发布，同版本同内容幂等，同版本异内容为 `IdempotencyConflict`，新版本的 `updated_at` 必须更大。
- 渲染：principal 文档为 JCS JSON，`type` 为小写类型名，`controllers` 与 `retired_controllers` 使用配置的 `controller_source`，`delegation_service` 为配置的服务源，`aliases` 含稳定的 HTML 地址，`links` 原样保留用户具名链接；发布前用 SDK 的 `validate_principal_document` 校验。
- 配置：`principal_origin`、`controller_source` 与 `delegation_service` 是不含路径的 HTTPS origin，最多 512 字节，`profile_url_prefix` 最多 2 KiB；URL 不允许原始控制字节、双引号或反斜线，需要时使用 URL 百分号编码。这四项写入每个 principal ID 或文档，安装后不可修改。`custom_domains` 最多 8 个小写域名，必须包含 `principal_origin` 的主机名，由 `admin_set_custom_domains` 整体替换。
- HTTP：`GET /<account_id>` 返回 200 文档（`content-type: application/json`，`access-control-allow-origin: *`，`cache-control: public, max-age=30`）；`/.well-known/ic-domains` 返回自定义域列表；`/favicon.ico` 返回黑白 dMsg 图标（`image/x-icon`，缓存 24 小时）；其余路径返回认证 404 JSON。所有响应按 response-only 方式认证全部响应头，HTTP 网关可验证。不做重定向，因为最终 URL 必须等于文档 `id`。
- HTTP 路由与认证树采用相同路径规则：百分号解码、忽略连续斜线产生的空段，保留尾斜线区别；等价路径返回相同认证响应。
- 只接受两类 ingress：已登记 home 的 `publish`，以及 controller 或 governance 的调用；其余 ingress 在执行前被拒绝，查询请走 query。`directory_stats` 返回已发布账户数、stable 页数与 cycles 余额。
- directory 稳定布局 schema 7 保存确切正文及其 SHA-256 摘要；认证树在 stable memory 中，升级只重新认证 404、域名列表与 favicon，不访问文档。home 迁移（`handoff`）尚未实现。

## 安全快照

`SecuritySnapshot` schema 4 含 `principal_updated_at`：账户当前 principal 状态的 `updated_at`，未启用时为 `null`。依赖方或服务若发现认证快照中的值大于 directory 文档的 `updated_at`，说明发布滞后，不应按旧文档接收新事件。

## 示例文档

```json
{"aliases":["https://dmsg.net/<account_id>"],"controllers":[{"delegation":{"audiences":["https://dmsg.net"],"scopes":["message.draft"]},"id":"did:agent:<key>","name":"dMsg signer #1","source":"https://dmsg.net","valid_from":1790000000001}],"delegation_service":"https://api.dmsg.net","id":"https://id.dmsg.net/<account_id>","links":[{"name":"Website","rel":"related","url":"https://example.com/"}],"name":"Panda","protocol":"agent-delegation/1.0","type":"person","updated_at":1790000000001}
```

## 验证

- `dmsg_protocol::agent` 单测：文档渲染经 SDK 校验、状态不变量、目录配置，以及注册批准和 controller PoP 摘要向量（`protocol_vectors.json` 的 `controller_pop_v1`）。
- `dmsg_user` 单测：principal 生命周期的单调性、上限、重放与不推进 `security_epoch`；错误 PoP 被拒。
- PocketIC `agent::self_held_principal_publishes_certified_documents_and_its_grants_are_accepted`：启用、错误 PoP 注册被拒、注册、发布、以 `ic-response-verification` 验证认证 200/404/域名响应、快照 schema 4、本机 Rust SDK 签名的 grant 被 `validate_delegation_acceptance` 接受、范围外与他人 key 被拒、轮换后退役 key 被拒而上限相同的新 key 可撤销旧 key 签出的凭证、目录发布规则与升级后文档不变。
- 客户端 `dmsg_client` 单测：本机签名的 grant 与 revoke 通过 SDK 的 `validate_delegation_acceptance`（按 directory 渲染的文档），超出上限被拒，nonce 为 `max(last + 1, created_at)`，不早于 `valid_from`，有效期超过 366 天被拒；controller key 为固定 ID 的 vault 条目。
- 桌面端到端 `tests/dmsg_integration/tests/desktop.rs` 的 `desktop_agents`：启用、登记、经真实 `dmsg_directory` 发布，授权提交回复丢失后原样重提被接收，依赖方核验 `usable`，撤销后不可用。

以上为本地测试，不代表生产部署、容量实测或安全审计已经完成。

## 统一公开身份文档（2026-10-22）

用户公开资料直接使用 `agent_protocols::delegation::PrincipalDocument`。`id.dmsg.net/<account_id>` 返回其 JCS JSON，`dmsg.net/<account_id>` 与经认证 handle 解析得到的 `dmsg.net/<handle>` 渲染同一份文档。规范 `id` 永远使用 principal origin，已有签名 issuer 和密钥派生域不变。`aliases` 自动包含稳定 HTML 地址，不复制可转移 handle 的权属。

`PrincipalState.metadata` 只是在 Candid/稳定存储边界承载 SDK 展示成员的规范 JSON（最多 32 KiB），不是另一份公开 Profile。名称最多 80 字符、简介最多 2000 字符、链接最多 100 条；链接含 `name/url/rel`，URL 为 HTTPS、最多 2048 字节。`extra["dmsg.link_images"]` 以链接 URL 为键保存 HTTPS 图片 URL。元数据计入 64 KiB 总文档预算，同时保留未来 controller 退役所需空间。

资料发布现在是一次账户变更加一次 directory 推送；普通消息、内容授权和文件读写仍不因此写链。`updated_at` 与内部版本单调递增，不推进设备 `security_epoch`。失败或回复未知时保留已提交版本，使用 `publish_principal` 重试；客户端不能把提交成功等同目录已发布。

`principal_document(account_id)` 返回 `CertifiedBatch`，值是与 HTTP 完全相同的 JSON bytes，认证路径为 `[b"documents", account_id[12]]`。认证根为 `fork(labeled("documents", documents_root), labeled("http_expr", http_root))`；两个读取面分别保留另一个分支的 pruned hash。SDK 与原生客户端独立核对 ICP 根、canister、60 秒证书时效、账户路径及规范文档 ID。HTTP 响应头与认证表达式不变。user 稳定布局为 schema 15、directory 为 schema 7。

已有线上数据使用原地 `upgrade`：directory 支持 6 → 7，逐条保留原 JSON 并建立新增认证树；user 支持 14 → 15，保留账户、解锁主密钥与全部原稳定表，仅为旧 principal 缺失的展示元数据提供 `{}` 默认值。先升级 directory，再升级各 home；目录发布输入的 `metadata` 为 optional bytes，缺省时兼容旧 home。旧状态摘要的同版本重试只接受原字段完全一致且元数据为空，不把已有文档改写成新渲染结果。后续新版本发布使用当前规则。其他旧 schema 不在本次迁移范围内。

`scripts/test-dmsg-principal-upgrade.sh` 从固定提交 `70d67c86` 构建未修改的旧 Wasm，检查 Candid 升级兼容，并在 PocketIC 验证账户/设备/绑定、内容根、解锁密钥、执行证据、controller、原正文和认证查询在升级后保留，及后续编辑、重试和再次升级。

原 `dmsg/profile/v1` 用户资料命令、COSE Profile 对象与云端 CRUD 已移除。智能体自己的 `agent-profile/1.0` 保持独立。头像仍是明确公开的上传对象：提交后 URL 不变且持续可读，文档取消引用不会删除图片；普通对象回收拒绝已提交头像。桌面上传只返回 URL，发布文档才更换资料，避免覆盖编辑草稿。

本次本地验证覆盖：

- PocketIC 资料更新与 controller 生命周期测试：HTTP 与 query 返回相同正文，修改资料保留 controller，旧编辑版本被拒绝；真实 Wasm 生成的认证证明由 TypeScript SDK 独立验证。
- `desktop_principal_documents`、`desktop_content_and_sync` 和 `desktop_channels`：真实 daemon、canister 与本地中继贯通资料、头像、锁定权限、内容同步和频道名称读取。
- Electron smoke：旧资料名称、具名链接和图片导入，头像操作保留草稿，目录发布待定与重试。最终 smoke 关闭了 macOS 原生全屏检查，其余检查通过。
- 网页：真实 Wasm 证明在浏览器展示文档；篡改证明后拒绝显示。live share 与独立认证 fixture 检查不属于本次资料验证，未运行。
