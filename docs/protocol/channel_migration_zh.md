# 频道与共享旧历史客户端合同

版本：2026-09-22。本文件描述公开客户端和对接所需合同，不表示生产部署已经完成。

## 正式频道

云端命令和 HTTP PoP 使用 `dmsg-cloud/1`。客户端把 genesis hash 作为信任起点，逐项验证控制序号、前一 head、角色规则、设备签名与保留的 IC 账户证明。邀请绑定目标账户和已知 head。owner 转移需要双方对同一请求上下文批准；过期后不能自动替换目标签名。

换代租约绑定 epoch、head、fencing 和完整接收集合摘要。接收项为 `device:<account>:<device>` 与 `recovery:<account>:<generation>`；最多 500 个设备和 100 个恢复项。HPKE 原文为随机 epoch key，info/AAD 使用 `dmsg/channel-epoch-envelope/1` 并绑定上述范围。每次换代保留公开接收集合证明；接收者验证自身信封与 key confirmation 后保存密钥。普通消息、文件、控制和 grant 不增加逐条 ICP update。

消息使用独立随机 ID，按 `dmsg/channel-message-key/1` 从 epoch key 派生消息 key；重试保留原密文、ID 和批准。消息游标使用服务返回的 `next_seq`，无历史权限的序号由 `skipped` 单列；不能按返回条数猜结束。历史授权独立选择代范围，并只封装给目标当时有效的设备/恢复项。文件保留固定版本、原分块认证和完整 SHA-256。

认证记录保留历史设备证据，不把“当前仍有权限”当成当时的证明。中继记录的接收时间不构成独立 TSA 时间戳。已取得的历史 key 无法远程收回；撤销隔离依赖新的 root/epoch，并受认证新鲜度窗口约束。

## 唯一旧频道继承

来源键使用 `dmsg/legacy-channel-key/v1` 对完整 `(legacy canister, channel id)` 做规范摘要。来源包括冻结 ChannelAuthority、完整名称角色快照、cutover、原范围和加密 DEK 摘要。普通名称 delegator 不能代表冻结管理员。

`dmsg-legacy-shared-proposal/1` 固定来源摘要、版本、目标账户、新频道、签名 genesis 摘要及 nonce。旧 manager 的批准摘要域为 `dmsg/legacy-manager-consent/v1`，同时绑定被代表的 manager；修改提案版本或任一目标字段会使旧批准失效。旧站 `legacy_snapshot(Attestation {digest, expires_at})` 在 ReadOnly 状态下为实际 caller 提供 ingress 证明，期限最多七天；它本身不授予管理员资格。

目录按来源键执行 CAS，所有冻结 managers 同意后才固定 canonical 映射。新频道 ID 先预留，实际激活另外要求当前目标账户明确批准。原签名 genesis 和新的激活批准分别保留，跨步骤失败只能继续同一映射。

`dmsg-legacy-member-claim/1` 绑定旧成员、新账户、设备、HPKE 公钥和安全版本；旧身份与新设备分别批准。未接受成员不会加入新 epoch。旧历史通过 `dmsg-legacy-history-grant/1` 另行交付，只包含该来源频道的消息、附件、KEK/DEK；不携带 MK、静态 ECDH 私钥、个人资料或其他频道。档案分片使用独立 FileKey，另由 GrantKey 封装；GrantKey 只 HPKE 给指定已认领设备及恢复公钥。

个人档案不会被全体管理员共识阻塞。无法建立官方继承时仍可保存个人档案或另建普通新频道；不会降低经理同意门槛。

## 恢复和冻结对照

本机恢复包保留正式频道证据、历史 key、迁移对象、财务任务及已缓存附件。未缓存的附件列为缺口。恢复内容不会授予新设备链上权限。

最终来源比较使用消息/权限 ingress 证明和冻结 OSS 文件 manifest。OSS 证明仅保留原 Candid 参数的 SHA-256，不保留 bearer token；请求标识仍按 IC representation-independent hash 核对。消息、频道 DEK 或附件变化会逐项列出，不能把预迁移快照标成生产切换完成。
