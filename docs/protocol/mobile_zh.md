# dMsg 原生 Apple 客户端接口

版本：1；日期：2026-10-30。此次补充服务端通知接入，客户端签名字节与用途域不变。描述公开客户端实际使用的接口与边界，不表示服务已部署或 Apple 验收通过。账号、设备和内容权限继续遵守现有[云端合同](cloud_zh.md)、[账号合同](account_root_zh.md)及[钱包合同](wallet_zh.md)。

## 1. 基础授权

原生客户端统一调用账户所在的 Core API，不持有 Apple 服务端凭据，也不直接访问 Apple 验证服务。普通移动端请求仍使用设备 HTTP PoP。修改操作还需对应用途的签名命令，绑定当前账号、设备、安全版本、请求编号及短期期限。App Attest 证明应用安装，不替代用户批准，不授予解密或付费权益。iOS 不提供任意 JSON RPC、任意路径读文件或通用签名入口。

原生界面持有按会话代次固定的句柄。锁定、暂停或切换账号使旧句柄失效；网络等待释放共享客户端。未决消息、提案、导入与备份沿用原编号恢复，不把超时当作未发生。

## 2. App Attest 与 APNs

设备接口前缀为 /v1/accounts/:account/mobile：

| 操作 | 用途 |
| --- | --- |
| GET configuration | 可用状态、Apple 应用及商品配置 |
| GET attestation/:key | 查询原登记结果；key 为 key ID 文本的 base64url 编码 |
| POST challenge | 为安装、key ID、purpose 和操作摘要申请一次性挑战 |
| POST attest | 提交 key ID、挑战编号及 Apple attestation 对象 |
| POST push-register | 提交安装标识、APNs token、环境及绑定该内容的 assertion |
| POST push-unregister | 注销本设备的安装订阅 |

命令域为 dmsg/mobile/<action>/v1。断言证明带 challenge_id、key_id 和 object，服务核对挑战的账号、设备、用途、安装、期限及摘要，并推进单调计数器。失去回包后先查登记状态，不重复使用一次性 attestation。

通知只有通用本地化提醒 DMSG_NOTIFICATION_GENERIC 和不透明路由／事件标识，不携带消息、密码或文件名。订阅最长七天，需续期；撤销、关闭或 token 失效后停止后续投递，已送达通知无法保证撤回。没有推送权限仍可前台使用。

APNs、App Attest、StoreKit 的环境分别配置。TestFlight 可使用 production APNs/App Attest 与 Sandbox StoreKit；不能从交易环境推导推送环境。

## 3. StoreKit 权益

2026-11-07：业务环境与 Apple 交易环境分开。TestFlight 与 App Store 均可连接生产业务服务；
production commerce 的主环境仍为 Production，治理字段 `accept_sandbox: opt bool` 显式为 true 时，
允许新的 Sandbox 年度订阅（缺失/false 默认关闭）。关闭只阻止新原始订阅；已登记原始订阅的恢复、
状态更新、续订及退款仍处理。无测试账号白名单、邀请或额外会员类型。

会员合同的 `AppStore.environment` 是可选字段；旧记录从已持久化的原交易补全。正式合同优先于
Sandbox，只有无正式合同生效时采用 Sandbox；两者不累加。升级只替换同环境合同，交易与原订阅
身份均包含环境。期限采用 Apple 实际 expiresDate，月账沿用同一 UTC 自然月计量，不因测试续订清零。
客户端 `current.store_environment` 用于标明测试订阅，Sandbox 合同不产生会员到期提醒；价格继续来自 StoreKit，账号 token 的算法不变。
配置与本地测试通过不代表已经部署或完成 Apple 真机验收。

应用 SKU dmsg-ios、Apple ID 6827050680、bundle net.dmsg.ios 与商品 ID 是不同标识。实际年度套餐及存储包 ID 必须由 App Store Connect 创建，并与商业配置对应。

appAccountToken 的输入为 UTF-8：
dmsg/apple/account/1、换行、user home Principal 文本、换行、AccountId 文本。
取 SHA-256 前 16 字节，设置 UUID 版本／variant 位后用 UUID 文本表示。这是固定协议的 UUID 形状账号绑定，不是访问凭据。

客户端在付款前检查经认证的套餐和商品，以及验证服务的环境与配置。正式购买中，外部渠道有效套餐、缺少同环境基础订阅的存储购买或不可验证状态不会进入付款。反过来，正式 Apple 基础合同结束前，commerce 不为该账号报价现金或 PANDA 基础套餐，桌面套餐页只说明由 App Store 续订。年度套餐使用 auto-renewable subscription，当前周期存储使用 consumable。Sandbox 首发仅开放年度套餐，不开放存储包。

POST purchase 提交安装标识、Apple 交易 JWS 和对应断言。服务验证 Apple 签名、证书、应用、环境、商品、购买所有权及账号 token，并查询当前 Apple 状态。治理配置的验证者才可调用 commerce 的 apply_app_store_transaction。事实包含交易及原始交易 ID、受益主体、商品、购买／到期／撤销时间、Apple signedDate 和状态复核时间 checked_at_ms。

同一原始交易永久绑定同一受益主体；旧观察不能覆盖新观察，重复交易不重复授予权益。权益由 commerce 的认证结果提供，客户端没有本地 isPro 开关。可撤销的 Apple 权益使用短期资格租约，验证服务失联不能无限延长。退款更新原合同；旧订阅晚到不能在替代订阅退款后重新生效。

原生端只在权威服务应用成功后 finish StoreKit 交易，未完成交易留待恢复购买和更新监听重试。删除 dMsg 账号不会取消 Apple 自动续订。生产 commerce 拒绝 Sandbox 事实。

### 3.1 App Store 服务端通知

App Store Server Notifications V2 的接收入口是 Core 的 `POST /storekit/notifications`，供 Apple
服务器回调；原生客户端继续使用上面的设备接口，不调用这个回调路径。

| 环境 | App Store Connect 中的通知 URL |
| --- | --- |
| Production | `https://api.dmsg.net/storekit/notifications` |
| Sandbox | `https://api.dmsg.net/storekit/notifications` |

请求为 `Content-Type: application/json` 和 `{ "signedPayload": "<Apple JWS>" }`，正文最多 96 KiB，
不使用查询参数或设备 HTTP PoP。接收方必须验证 Apple 签名、证书状态、应用标识和环境；知道 URL 或
传入自报的购买 JSON 不构成授权。只有通知已经被验证并可靠接收，才能返回 `200 {"accepted":true}`；
服务或验证不可用时返回 503，不能提前确认成功。重复通知沿用原交易身份，不重复授予权益。

两种通知使用同一生产入口，由验签后的 Apple 环境分流。URL 不表示已经完成后台配置或真实回调验收。
网关与 commerce 必须分别显式开启 Sandbox 新购买；客户端发行开关仍独立控制是否显示购买入口。

## 4. 举报、屏蔽与账号删除

POST report 仅发送举报原因、说明、目标账号／资源／消息定位，以及用户明确选择披露的正文。内容来自已验证的本地记录；不会发送账号内容密钥。原报告编号及内容在本机加密保存，重试不改变原举报。GET reports 返回本人举报状态和处理结果。

POST block 更新目标账号的屏蔽状态；本机立即隐藏，网络失败时保留待同步操作。在线屏蔽阻止新的直接联系与邀请，不保证收回他人已有副本。免费收信规则可设置关闭、指定账号或所有人；修改会保留独立屏蔽、邀请和容量设置。iOS 不配置付费发送。

删除前刷新账号目录及频道控制记录，展示待处理操作、拥有的会话和钱包访问；不会借此上传未提交的本机修改。需要归档／转移拥有的会话，并核对钱包与外部凭证访问。可编辑公开资料先清除并完成发布。POST delete 经系统确认及 root-manage 管理员授权提交，GET status 查询不可逆的服务清理进度。清理失败保留任务，不能把退出登录或一项失败的清理标为删除完成。

范围是账号拥有的云端密文、私有服务记录、推送登记和公开头像来源／缓存。共享频道中其他成员的副本、公开链上记录以及必要财务和防重放记录有独立边界；本机账号数据另行移除。举报内容最多保留九十天用于处理。

## 5. 后台密文传输

前台已解锁设备通过 /v1/accounts/:account/mobile/transfers，使用 dmsg/mobile/transfer/grant/v1 签署 id、upload_id、method。它只能引用已存在的不可变 backup 计划：PUT 用于原上传者的暂存备份，GET 用于已提交备份。根、保险库对象和任意业务提交不在授权范围。

返回结果绑定账号、设备、安全版本、方法、上传编号、完整不可变计划、传输块及到期时间；每块有 slot、size、digest、url。URL 只授权该备份既定密文，不持有签名或内容密钥。期限最多 24 小时，上传还受原计划和配额租约截止限制。每次传输检查设备、撤销状态和对象条件。dmsg/mobile/transfer/revoke/v1 可撤销后续传输授权；已经交付的字节不能收回。

iOS 一次处理一个选定文件，最多三个系统任务。操作及完成块保存在独立目录，系统任务描述保留操作／尝试／块的映射；只允许该目录的密文与窄权限在首次设备解锁后继续使用。普通工作区仍使用 Complete 保护。

下载完成仅表示收到字节。回到原账号并解锁后，Rust 按原加密日志校验块、AEAD 和整文件摘要，才更新目录；上传也由前台核对并 finalize 原计划。暂停、强退、到期和断网都保留原操作的恢复路径，不保证 iOS 准时调度。

## 6. 系统扩展与环境

Share 只暂存用户选择的文件或文本，账号在主应用导入时确定。中断记录可查看和丢弃，重试共享条目沿用原编号。

AutoFill 与 Files 使用不同 App Group 和 Keychain group，只有明确选中的、分别加密的快照；它们没有工作区数据库、根密钥或设备签名密钥。快照最多 24 小时。AutoFill 每次交互要求独立系统认证；Files 使用独立认证开启的五分钟会话，主应用锁定结束该会话。系统已取得的明文副本可能保留。当前 Files 是只读副本，不宣称实时目录镜像。

Widget 和 App Intents 仅提供通用导航及显式暂存，不返回秘密、不后台解锁或批准。浏览器唤起沿用[connect 合同](connect-v1_zh.md)的导航提示；页面结果仍由原操作恢复和验证。

当前 TestFlight 和 App Store Release 固定连接生产业务服务，内购入口、恢复与交易监听/提交由构建开关关闭；receipt 路径不选择业务环境，也不能证明付款。只有显式启用 `DMSG_USE_STAGING=YES` 的 Debug 构建读取 `StagingDeployment.json`，该文件仅含公开端点和 canister ID，使用独立工作区、Keychain 与偏好设置。配置没有自定义 IC 根密钥或任意登录回调选项，普通网页链接不能选择环境。

## 7. 验证边界

本地单元测试、Wasm 与模拟器检查不替代真实 App Attest、APNs、StoreKit、签名 entitlement 和系统扩展验收。配置与审核步骤见 [iOS 审核说明](../../src/dmsg_ios/Review/README_zh.md)，实际运行结果见 [验证记录](../../src/dmsg_ios/VALIDATION.md)。

## 8. 本地信件与会话入口（2026-11-03）

iOS 的信件目录包含收件、已发送、待发送、草稿和归档。未发送草稿与发信原文、编号、时间、结果保存在当前账户的加密工作区；不加入账户内容的自动复制，也不由当前记录传输导入器搬走，与已有发送意图一样计入留在源设备的记录。清除可重新下载的展示缓存不删除这些记录。旧发信日志没有正文或时间时不得补造。

免费来信在排队前统一校验账户、64 位十六进制联系邀请及最多 6000 UTF-8 字节的正文。排队与移除对应草稿在同一本地事务完成。发送重试复用原编号与原内容；已准备联系请求或结果未知时不能通过修改原单重新发送。尚未准备联系请求的本机意图可以显式恢复为草稿。查询发送结果不产生新投递，服务接受不表示对方已读。

iOS 整理信件调用既有 `dmsg/inbox/mark/v1` 时显式设置 `reopen_contact=false`；“允许再次来信”经单独确认后才设置为 true。它不添加联系人或频道成员，既有收信／屏蔽规则仍生效。桌面与 CLI 的原标记调用保持兼容。

`dmsg://channel/join#<base64url-json>` 仅携带现有频道邀请。iOS 解析后展示目标账号、名称与到期时间；未登录时保留原导航请求，登录后仍须用户明确选择加入，并由共享核心校验邀请与权限。不同账号不能静默切换或自动加入。创建邀请后修改接收账号会使旧分享结果失效。

会话编辑器按已验证的频道状态限制发布与邀请，附件选择不触发发送，正式入队时保留回复引用。搜索对正文与附件名使用一致范围并提供历史分页。提醒打开内容时先读取已有本地消息，再独立尝试网络刷新。
