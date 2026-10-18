# 频道与共享旧历史客户端合同

版本：2026-10-18。本文件描述公开客户端和对接所需合同，不表示生产部署已经完成。

## 正式频道

云端命令和 HTTP PoP 使用 `dmsg-cloud/1`。客户端把 genesis hash 作为信任起点，逐项验证控制序号、前一 head、角色规则、设备签名与保留的 IC 账户证明。邀请绑定目标账户和已知 head。owner 转移需要双方对同一请求上下文批准；过期后不能自动替换目标签名。

加入不换代。频道可写时，邀请控制带 `envelope_upload`：邀请人把当前 epoch key 按历史授权的格式（`dmsg-channel-history/1`，范围只含当前 epoch）封装给被邀请账户当时有效且有内容权限的设备，上传绑定当前 epoch 与 head；中继保存被邀请账户的认证证据作为该控制的 `acceptance_evidence`。邀请只能在发出时的 epoch 被接受；带密钥的邀请还要求接受时被邀请账户的安全版本未变，否则须重新邀请。接受后新成员的 `joined_epoch` 为当前 epoch，频道保持可写，新成员可读当前 epoch 在其加入前已发的消息，更早的 epoch 仍须历史授权。频道等待换代时，邀请人可以协调换代就先完成换代再邀请，邀请附带新 epoch 的密钥；不能协调换代时邀请不带密钥，接受后从下一 epoch 起可读并需要换代，但这次换代先完成时邀请随之失效。

需要换代的情形：成员被移除或退出、`rekey` 控制（所有者、管理员或获准成员随时可发，例如不想让新成员读到当前 epoch 已有内容时先 rekey 再邀请），以及某成员原持有当前 epoch 的设备被撤销或失去内容权限。成员只新增设备时中继采纳其新安全版本而不换代，新设备经账户同步的频道记录取得已有 epoch key。所有者、管理员与 `can_rotate` 成员（已持有当前 epoch）可以协调换代；客户端 daemon 解锁时发现频道等待换代且本账户有权协调，就自动申请租约完成。中继在控制提交、epoch 激活和要求换代后经成员账户的提示通道发 `channel` 提示（`event: "control"`，带频道 ID，见 connect-v1 7.5），消息提交后发 `event: "message"`；没有频道各自的 WebSocket。所有者或管理员可以撤销尚未接受的邀请（`revoke_invite` 控制）。邀请可以 `dmsg://channel/join#<base64url(邀请 JSON)>` 链接交付，打开后只预填加入对话框。

换代租约绑定 epoch、head、fencing 和完整接收集合摘要。接收项为 `device:<account>:<device>`，只封装给当前有效且有内容权限的设备；最多 500 个设备。账户恢复后的设备经换根重新加入，不再有恢复接收项。HPKE 原文为随机 epoch key，info/AAD 使用 `dmsg/channel-epoch-envelope/1` 并绑定上述范围。每次换代保留公开接收集合证明；接收者验证自身信封与 key confirmation 后保存密钥。普通消息、文件、控制和 grant 不增加逐条 ICP update。

消息使用独立随机 ID，按 `dmsg/channel-message-key/1` 从 epoch key 派生消息 key；重试保留原密文、ID 和批准。中继未存下、而频道已换代或控制 head 已变的消息，以同一 ID 按当前纪元重新封装（C06）。密文正文为 `{text, file?, reply_to?, edit_of?, retract?, agent?, packet?}`：`reply_to`、`edit_of`、`retract` 以 `{seq, digest}` 指名一条消息（`digest` 为其签名命令的 SHA-256），编辑与撤回只对同一作者的消息生效，撤回让各成员设备删去原文与附件引用，但收不回已被取走的副本；`agent` 为经本设备写这条消息的 Agent ID 与名称（账户的声明，随正文由设备签名）；`packet` 为在频道内交付的转移包；计费转移的提议即暂停频道写入，桌面端只把所有权转移包发进频道，计费转移包另行交付。无法解开或解码的正文、缺少纪元 key 的消息记为“无法读取”而游标照常前进，取得 key 后重读该纪元时再打开。成员可报告投递与已读游标（`POST /v1/channels/:id/cursor`，`dmsg/channel/cursor/v1 {delivered, read|null}`，只增不减），已读只在用户选择发送时上报；`GET /v1/channels/:id/cursors` 返回当前成员的游标。消息游标使用服务返回的 `next_seq`，无历史权限的序号由 `skipped` 单列；不能按返回条数猜结束。历史授权独立选择代范围，并只封装给目标当时有效的设备/恢复项；接收方下一次读取时自动打开授予的纪元，只重读此前跳过的那些消息。计费转移进行期间中继暂停普通写入，所有者可经 `billing/cancel` 取消，频道视图返回进行中的转移（`billing_transition`）。文件保留固定版本、原分块认证和完整 SHA-256。

认证记录保留历史设备证据，不把“当前仍有权限”当成当时的证明。中继记录的接收时间不构成独立 TSA 时间戳。已取得的历史 key 无法远程收回；撤销隔离依赖新的 root/epoch，并受认证新鲜度窗口约束。

## 唯一旧频道继承

来源键使用 `dmsg/legacy-channel-key/v1` 对完整 `(legacy canister, channel id)` 做规范摘要。来源包括冻结 ChannelAuthority、完整名称角色快照、cutover、原范围和加密 DEK 摘要。普通名称 delegator 不能代表冻结管理员。

`dmsg-legacy-shared-proposal/1` 固定来源摘要、版本、目标账户、新频道、签名 genesis 摘要及 nonce。旧 manager 的批准摘要域为 `dmsg/legacy-manager-consent/v1`，同时绑定被代表的 manager；修改提案版本或任一目标字段会使旧批准失效。旧站 `legacy_snapshot(Attestation {digest, expires_at})` 在 ReadOnly 状态下为实际 caller 提供 ingress 证明，期限最多七天；它本身不授予管理员资格。

目录按来源键执行 CAS，所有冻结 managers 同意后才固定 canonical 映射。新频道 ID 先预留，实际激活另外要求当前目标账户明确批准。原签名 genesis 和新的激活批准分别保留，跨步骤失败只能继续同一映射。

`dmsg-legacy-member-claim/1` 绑定旧成员、新账户、设备、HPKE 公钥和安全版本；旧身份与新设备分别批准。未接受成员不会加入新 epoch。旧历史通过 `dmsg-legacy-history-grant/1` 另行交付，只包含该来源频道的消息、附件、KEK/DEK；不携带 MK、静态 ECDH 私钥、个人资料或其他频道。范围档案至多 8 段、每段 32 MiB，作为新频道当前 epoch 的文件上传；各段的文件引用（含 FileKey）以 GrantKey 封装为 `manifest`，GrantKey 只 HPKE 给指定已认领设备（`device:<account>:<device>`）。接收设备核对上传者、manifest 摘要、整份档案摘要，并按同一冻结来源重新裁剪核对后，才作为个人档案保存。

批准请求 `dmsg-legacy-approval-request/1`（管理员同意或成员认领）由桌面端生成，经 `https://dmsg.net/legacy#approve=<base64url(JSON)>` 或 panda.fans 同路径在系统浏览器中打开；旧网页重新计算意图摘要后由旧身份调用 `legacy_snapshot(Attestation)`，把 `{proof, expires_at}` 交回桌面端。不是目录 reader 的成员使用其他参与者交来的完整目录记录，在本机逐项核验后认领。

个人档案不会被全体管理员共识阻塞。无法建立官方继承时仍可保存个人档案或另建普通新频道；不会降低经理同意门槛。

## 恢复和冻结对照

本机加密存储保留正式频道证据、历史 key、迁移对象、财务任务及已缓存附件；没有恢复包，新设备经链上批准与换根后通过云端同步取得内容（个人档案见 [legacy_archive_zh.md](legacy_archive_zh.md)）。

最终来源比较使用消息/权限 ingress 证明和冻结 OSS 文件 manifest。OSS 证明仅保留原 Candid 参数的 SHA-256，不保留 bearer token；请求标识仍按 IC representation-independent hash 核对。消息、频道 DEK 或附件变化会逐项列出，不能把预迁移快照标成生产切换完成。
