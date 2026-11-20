# dMsg iOS 隐式状态机清单

以用户操作或资源生命周期为单位，在 **12 个领域识别出 104 组含隐式转换的流程**。基线为 2026-11-18 本地 `925f582d` 加上本工作区既有的 iOS 修复。主应用与五个扩展的编译目录原有 146 个 Swift 文件，加上本次共享选择器共 147 个，其中 4 个仅在 DEBUG 下编译。

同一个事件在不同历史状态下允许不同的下一步，而且需要合读多个字段、任务、计时器、系统回调或持久记录才能判定，即计入清单。每行归并操作同一资源的入口。已有枚举的流程仅计其仍依赖外部条件的部分。本次修正的三处转换保留在基线计数中，字段、符号和源码链接指向修复后的工作区。

## 查找入口

| 领域 | 组数 | 领域 | 组数 |
| --- | ---: | --- | ---: |
| [启动、会话与导航](#session) | 11 | [保险库、秘密包与可见值](#vault) | 12 |
| [同步、本地读取与资源调度](#sync) | 7 | [审批、浏览器与 Agent](#approvals) | 6 |
| [身份、设备、恢复与账户安全](#identity) | 14 | [会员与 StoreKit](#storekit) | 4 |
| [推送、提醒与已读](#notifications) | 8 | [团队钱包](#wallet) | 7 |
| [频道、信件与反馈](#messaging) | 14 | [系统扩展、认证与跨进程副本](#extensions) | 7 |
| [文件库、导入与传输](#files) | 12 | [共享交互](#controls) | 2 |

## 跨流程的状态归属

- **核心会话快照**的 `sequence` 判定先后，`generation` 将 UniFFI 句柄绑定到核心会话。Swift 的 `epoch` 使在途读取失效，`viewGeneration` 决定草稿、导航和审批回执能存活到何时。单个页面还可能用独立请求 ID 区分同一会话的两次读取。[SessionModel · adopt](../App/SessionModel.swift#L614)
- **提交进度**需要区分按下按钮、写入本地队列、对方确认和刷新到最新快照四个事件。消息以消息 ID、审批以请求和摘要、钱包以提案请求 ID、StoreKit 以交易和账户对账。核对是否可以重试，需同时看本地操作与核心返回。[MessageSending · start](../App/MessageSending.swift#L43) · [ApprovalReceipts](../Features/ApprovalsView.swift#L350) · [WalletProposalSubmission](../App/WalletProposalSubmission.swift) · [PurchaseModel · deliver](../App/PurchaseModel.swift#L589)
- **跨进程访问**涉及工作区解锁、系统认证、已发布快照与扩展短期会话，各自有时限。后台 URLSession、系统钥匙串和 App Group 暂存可以跨主应用进程存活。[SystemExports · publish](../Shared/SystemExports.swift#L168) · [BackgroundTransfers · reconnect](../Platform/BackgroundTransfers.swift#L187) · [IncomingShares](../Shared/IncomingShare.swift#L12)

<a id="session"></a>

## 启动、会话与导航

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **启动与迁移**<br>`opening` · `keychainPreparationFailure` | 读取部署和安装标记，迁移钥匙串后打开核心。已在打开时复用当前过程。 | 迁移受阻进入独立重试页面。成功后才按会话快照选择首页。 | [SessionModel · start](../App/SessionModel.swift#L287) · [MobileDeployment · recordSelection](../Platform/MobileDeployment.swift#L31) · [RootView](../Features/RootView.swift) |
| **会话快照与账户边界**<br>`lastSessionSequence` · `epoch` · `viewGeneration` | 采用新快照时重新取得 scoped 核心句柄。切换账户或锁定会重建账号视图，短暂切后台会使在途读取失效。 | 过期序号不能回滚界面。设置草稿、回复对象和已选附件随工作区清理。 | [SessionModel · adopt](../App/SessionModel.swift#L614) · [SessionModel · clearContent](../App/SessionModel.swift#L1192) |
| **欢迎、登录与创建工作区**<br>`signingIntoAnotherAccount` · `initializingAccount` · `initializationIsSlow` | 根据登录身份、账户、设备授权和内容根选择创建、加入、恢复或解锁。创建中的慢网提示由计时器驱动。 | 未确认的初始化继续原操作。已能使用本机工作区时仍可单独续登。 | [AccessView](../Features/RootView.swift#L197) · [SessionModel · initializeAccount](../App/SessionModel.swift#L1314) |
| **登录与系统回调**<br>`authenticationGeneration` · `continuation` · `controller` | 发起 Internet Identity 或通行密钥操作，保存当前系统会话。成功、用户取消或系统失败只能结束对应尝试。 | 旧浏览器回调按代数丢弃，通行密钥回调按控制器身份匹配。会话额度耗尽保留可重置的凭据。 | [SessionModel · login](../App/SessionModel.swift#L1213) · [WebLogin · authenticate](../Platform/WebLogin.swift#L9) · [PasskeyLogin · authenticate](../Platform/PasskeyLogin.swift#L11) |
| **快捷解锁与显式操作**<br>`automaticUnlock` · `busyOperation` · `unlocking` | 锁屏就绪时自动尝试一次。显式解锁、用户取消、锁定与新一轮访问分别消费、抑制或重新开启机会。 | 忙碌操作按 token 完成。只有已配置快捷解锁的当前工作区才提示系统认证。 | [AutomaticUnlockPolicy](../App/SessionModel.swift#L1666) · [SessionModel · fastUnlock](../App/SessionModel.swift#L1294) · [AccessView](../Features/RootView.swift#L197) |
| **前后台、隐私遮盖与后台收尾**<br>`lifecycle` · `lifecycleGeneration` · `backgroundFinish` | 暂时 inactive 加遮罩，真正后台保存草稿、暂停或限时收尾。回到前台决定复用已核验会话还是重读。 | 锁定策略、系统保护数据不可用、后台时间耗尽和主动锁定分别结束工作。完成回调只能结束自己的后台任务。 | [DMsgApp](../App/DMsgApp.swift) · [SessionModel · background](../App/SessionModel.swift#L1003) · [SessionModel · foreground](../App/SessionModel.swift#L1076) · [PrivacyShield](../Platform/PrivacyShield.swift) |
| **自动锁定与剪贴板**<br>`lastActivity` · `writtenChange` | 触摸和传输进展延长活动时间，空闲检查发起锁定。私密复制记录系统 change count 并设置过期。 | 清理时只擦除本应用仍拥有的那次复制。用户在其他应用后续复制的内容保留。 | [SessionModel · idleTick](../App/SessionModel.swift#L997) · [TouchActivityProbe](../Features/Theme.swift#L96) · [PrivateClipboard](../Platform/PrivateClipboard.swift) |
| **多账户选择、退出与本地移除**<br>`working` · `signingOut` · `accountIdentities` | 读取本地账户与公开身份，选择后切换工作区。退出和删除本机副本各自确认，锁屏后另行认证。 | 展示名读取失败仍可用账户 ID。移除当前账户会清理本机集成和对应工作区。 | [AccountsView · choose](../Features/AccountsView.swift#L74) · [AccountView · logout](../Features/AccountView.swift#L204) · [LocalAccountRemovalView](../Features/AccountSettingsViews.swift#L71) · [SessionModel · removeLocalAccount](../App/SessionModel.swift#L495) · [LocalPreferences](../Platform/LocalPreferences.swift) |
| **迁入的只读副本**<br>`moved` · `copies` · `request` | 检测迁移副本后可搁置旧副本、作为新设备加入、申请替换，或确认后丢弃指定副本。 | 本机副本、设备授权和后续导入分别记录。进入普通工作区仍取决于当前设备的会话。 | [MovedView](../Features/RootView.swift#L377) · [MovedCopiesSection](../Features/RootView.swift#L411) · [MovedImportSection](../Features/DeviceHandoverViews.swift#L605) |
| **标签页、分栏与外部入口**<br>`selectedTab` · `route` · `IntentNavigation` | 紧凑布局推入详情，宽屏更新右侧选择。快捷指令、频道邀请、浏览器 URL 或 widget 链接记录待处理目标。 | 工作区可用时消费目标，换账户重置路由。外部打开请求只进入相关页面，后续确认沿用业务入口。 | [RootView · routeIntent](../Features/RootView.swift#L63) · [WorkspaceNavigation](../Features/WorkspaceNavigation.swift) · [DMsgApp](../App/DMsgApp.swift) · [WorkspaceIntents](../Intents/WorkspaceIntents.swift) · [WorkspaceWidget](../Extensions/Widget/WorkspaceWidget.swift) · [ConnectLink](../Platform/ConnectLink.swift) · [ChannelInvitationLink](../Platform/ChannelInvitationLink.swift) |
| **通知条、面板与弹框归属**<br>`noticeHosts` · `noticeRevision` · `dismissed` | 最上层可见页面接收错误和复制/保存通知。状态变更可唤起提示，普通提示计时关闭，紧急项可进入详情。 | 页面退出注销宿主。同一段文字再次出现按修订号计时，隐私遮盖期间隐藏提示。 | [PrivateScreen](../Features/Theme.swift#L68) · [TransientNotice](../Features/WorkspaceFeedback.swift#L105) · [WorkspaceFeedback](../Features/WorkspaceFeedback.swift#L59) |

相关回归入口见 [SessionPolicyTests](../Tests/SessionPolicyTests.swift) · [AccountInitializationTests](../Tests/AccountInitializationTests.swift) · [AccountInitializationUITests](../UITests/AccountInitializationUITests.swift) · [KeychainMigrationUITests](../UITests/KeychainMigrationUITests.swift) · [NavigationTests](../UITests/NavigationTests.swift)。

<a id="sync"></a>

## 同步、本地读取与资源调度

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **自动同步与手工同步**<br>`automaticPassRunning` · `automaticOperation` · `manualSyncAccounts` | 解锁后接入同步流，由核心判断到期。手工触发和自动开关分别影响当前任务与下次运行。 | 后台、锁定或停用会取消对应操作。已验证游标或初始化状态改变才触发相关内容重读。 | [SessionModel · watchContentSync](../App/SessionModel.swift#L678) · [SessionModel · synchronize](../App/SessionModel.swift#L1434) · [ContentSyncBridge](../App/ContentSync.swift#L28) · [ContentSyncStatusView](../Features/ContentSyncStatusView.swift) |
| **网络、提示通道与补读**<br>`hintsConnected` · `channelHints` · `inboxHints` | 观测离线到在线、提示通道重连或指定资源有更新。按事件安排即时读取和延时补读。 | 断开时切回轮询，前台重连补查错过的提示。低电量、温度与连接状态共同决定间隔。 | [SessionModel · watchNetwork](../App/SessionModel.swift#L749) · [SessionModel · watchHints](../App/SessionModel.swift#L772) · [SessionModel · receiveHint](../App/SessionModel.swift#L839) · [HintBridge](../App/ContentSync.swift#L44) |
| **频道预读、维护与标签页刷新**<br>`channelChecked` · `channelReadAhead` · `tabRefreshedAt` | 定时检查最近频道控制数据，只给有新内容的非静音非当前频道预读尾页。页签切换按上次读取时间节流。 | 会话变更中断旧读取。后台维护还推进待发送消息、收据和待处理文件工作。 | [SessionModel · checkChannels](../App/SessionModel.swift#L800) · [SessionModel · readChannelsAhead](../App/SessionModel.swift#L815) · [SessionModel · maintenancePass](../App/SessionModel.swift#L956) · [SessionModel · refreshSelectedTab](../App/SessionModel.swift#L766) |
| **缓存优先的详情与清缓存**<br>`request` · `observed` · `stale` · `failed` | 先采用核心的本机快照，新鲜时停止，过期或强刷再读远端。各页面分别持有自己的数据或编辑草稿。 | 刷新失败显示已保存内容的观察时间并可重试。清缓存需要用户操作，设备撤权另走会话处理。 | [DisplayLoad · load](../App/DisplayLoad.swift#L23) · [DisplayCacheView · clear](../Features/DisplayCacheView.swift#L34) · [SessionModel · decode](../App/SessionModel.swift#L1570) |
| **参与者名称与头像**<br>`checked` · `waiting` · `worker` | 可见身份请求排队、去重、分批，先读本机再刷新网络。资料修订使缓存失效。 | 重置取消旧工作。头像远端读取失败保留已有图。信件与收件人搜索仍按既有范围预读。 | [PeopleDirectory · request](../App/PeopleDirectory.swift#L19) · [PeopleDirectory · read](../App/PeopleDirectory.swift#L57) · [MessagePeople · loadMessagePerson](../Features/MessagePeople.swift#L8) · [AvatarImage](../Platform/AvatarImage.swift) |
| **缩略图与临时打开的文件**<br>`cache` · `tail` · `nonce` | 按文件 ID 与版本命中缩略图或排队，轮到后打开核心副本、下采样、关闭 nonce。 | 旧会话结果跳过缓存写入。锁定清内存，单个视图的取消传给其读取。 | [ThumbnailStore · image](../App/ThumbnailStore.swift#L19) · [ThumbnailStore · read](../App/ThumbnailStore.swift#L34) |
| **前台可取消长操作**<br>`operation` · `completed` · `total` | 导入、复制等独占进度条。接收工作 token，轮询进度，用户取消或 Swift Task 取消传回核心。 | 同时再次开始返回 PENDING。完成或出错统一清理进度任务和当前操作。 | [TransferModel · run](../App/TransferModel.swift#L15) · [CoreOperation · coreOperation](../Platform/CoreOperation.swift#L7) · [TransferStatusView](../Features/PendingImportsView.swift#L56) |

相关回归入口见 [ContentSyncTests](../Tests/ContentSyncTests.swift) · [SyncUITests](../UITests/SyncUITests.swift) · [InboxTests](../Tests/InboxTests.swift) · [MessagingTests](../Tests/MessagingTests.swift) · [FileTransferTests](../Tests/FileTransferTests.swift)。

<a id="identity"></a>

## 身份、设备、恢复与账户安全

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **账户页资料、头像与套餐徽章**<br>`loadedProfileRevision` · `avatarTask` · `planAccount` | 先显示本地身份，再独立读资料、头像和权益。资料发布、前台返回或商店修订触发重读。 | 切换账户、版本改变或任务取消后忽略过时回调。未知权益保留更新入口。 | [AccountView · loadProfile](../Features/AccountView.swift#L173) · [AccountView · loadPlan](../Features/AccountView.swift#L160) · [SessionModel · rememberIdentity](../App/SessionModel.swift#L356) |
| **公开资料编辑与发布**<br>`draft` · `saved` · `pending` · `conflict` · `uploadedAvatar` | 缓存回填 → 修改名称、链接或头像 → 确认发布。已经保存但尚未发布时可单独重试发布。 | 原草稿和期望版本保留。收到不同版本时需显式重载，重新发布已保存资料不会提交当前草稿。 | [PublicProfileView · publish](../Features/PublicProfileView.swift#L194) · [PublicProfileView · apply](../Features/PublicProfileView.swift#L152) · [SessionModel · keepFormDraft](../App/SessionModel.swift#L399) |
| **绑定、移除登录身份**<br>`passkeyBinding` · `limitedCredential` · `receipts` | 选择绑定现有/新凭据或 II，获得系统响应后提交。暂时失败复用该响应，达到容量限制后可确认重置。 | 取消或工作区退出释放本次尝试。管理权限决定能否移除登录，绑定结果可跳转审批。 | [AccountSecurityView · bind](../Features/AccountSecurityView.swift#L227) · [PasskeyBinding · run](../Features/PasskeyBinding.swift#L22) · [LoginBindingRequestView](../Features/LoginBindingRequestView.swift) |
| **申请绑定到已有账户**<br>`account` · `issued` · `working` | 无账户的登录者输入目标账户并生成绑定凭据，分享给管理员后再接受已批准的绑定。 | 绑定和设备授权分步进行。更改账户清除旧凭据，分享和接受均绑定到已签发的账户。 | [LoginBindingRequestView](../Features/LoginBindingRequestView.swift) · [AccessView](../Features/RootView.swift#L197) |
| **设备加入和另一台设备审核**<br>`token` · `expires` · `created` · `needsLogin` | 待授权设备申请二维码，按到期时间更新、轮询批准。授权端扫描或粘贴，先预览指纹和角色，再创建审批。 | 收到批准后按核心状态继续解锁或续登。扫描、复制和展示本身不授予设备权限。 | [PairView](../Features/DeviceAuthorizationViews.swift#L17) · [PairApprovalView](../Features/DeviceAuthorizationViews.swift#L98) · [QRCodeScanner](../Platform/QRCodeScanner.swift) |
| **扫描桌面邀请**<br>`attempt` · `delivery` · `expired` | 输入或扫描桌面邀请 → 向桌面递交本机请求 → 保存回执并等授权。到期解除等待，也可手工检查。 | 离开、取消和新尝试隔离旧页面回调。最终凭核心会话是否已批准推进。 | [DesktopPairingView · refresh](../Features/DesktopPairingView.swift#L116) · [DesktopPairingView · check](../Features/DesktopPairingView.swift#L122) |
| **已授权设备与权限变更**<br>`devices` · `canManage` · `request` · `receipts` | 设备列表缓存与辅助数据各自刷新。详情可更新本地标签，管理者另可审阅撤销或能力变更。 | 角色、当前设备和未结束审批决定按钮。回到详情时重读原回执。 | [DeviceDetail](../Features/DevicesView.swift#L64) · [SessionModel · setDeviceLabel](../App/SessionModel.swift#L388) · [ApprovalReceipts](../Features/ApprovalsView.swift#L350) |
| **设备继任与本地数据交接**<br>`shown` · `leaving` · `stopping` · `revoking` | 选择前任/后继、准备请求、另一端审阅并批准。核心返回的任务决定可导入、跳过、停止或撤销哪一步。 | 任务与遗留数据逐项展示。传输完成、继任完成和旧设备撤权是不同结果。 | [SuccessionsView](../Features/DeviceHandoverViews.swift#L140) · [SuccessionReviewView](../Features/DeviceHandoverViews.swift#L329) · [SuccessionRequestView](../Features/DeviceHandoverViews.swift#L432) |
| **近场收发与发现**<br>`phase` · `watching` · `advert` · `discovery` | 接收或发送计划经启动、可选导出审批到传输。通过 Bonjour 广播、浏览、解析，失败或拒绝本地网络权限可输入地址。 | 后台保留暂停的计划，接收可自动重开。发送恢复仍需审核。取消、超时与回调只结束各自的发现请求。 | [Phase](../App/DeviceTransferModel.swift#L49) · [DeviceTransferModel · foreground](../App/DeviceTransferModel.swift#L128) · [Request](../Platform/Bonjour.swift#L74) · [DeviceTransferView](../Features/DeviceHandoverViews.swift#L498) |
| **恢复观察、申请与完成**<br>`recoveryWarning` · `seenRecoveryDigests` · `pending` | 读取当前账户恢复记录并按新鲜度提醒。申请、等待期结束、可完成窗口和取消各有前置条件。 | 核对请求属于本机、成员状态和核心截止时间。缓存失效或长时间未确认会显示提醒，锁屏只显示限定信息。 | [SessionModel · refreshRecoveryWarning](../App/SessionModel.swift#L1511) · [RecoveryNotice](../App/RecoveryNotice.swift) · [RecoveryState](../Features/RecoveryView.swift#L13) · [RecoveryView · complete](../Features/RecoveryView.swift#L135) |
| **硬件策略与密钥登记**<br>`policy` · `bindingChange` · `enrollmentToken` · `incomingToken` | 读当前设备与密钥，选择用途、认证策略、登记本机/其他设备或退役密钥，生成相应审批。 | 已登记、绑定中和权限不足各自限制下一步。草稿归当前工作区，已发请求由回执追踪。 | [HardwareSecurityView · approval](../Features/HardwareSecurityView.swift#L172) · [HardwareSecurityView · enrollment](../Features/HardwareSecurityView.swift#L191) · [AccountSecurityView · reviewRecovery](../Features/AccountSecurityView.swift#L200) |
| **系统认证与签名句柄**<br>`active` · `authorization` · `prepared` | 建立 LAContext，认证后分配受控签名或授权句柄。对应操作签署、释放或取消。 | 系统回调只完成当前等待。生物状态变化、无效密钥、取消与设备临时不可用有不同结果。 | [AppleSecurity · prepareSignature](../Platform/AppleSecurity.swift#L183) · [AppleSecurity · authorize](../Platform/AppleSecurity.swift#L286) · [AppleSecurity · cancelPresence](../Platform/AppleSecurity.swift#L341) · [Biometry](../Platform/Biometry.swift) |
| **钥匙串迁移与通知命名空间**<br>`signingPrefix` · `notificationWorkspace` · `notificationNamespace` | App/扩展按签名组择源迁移，遇到既有目标或授权暂不可用各走不同路径。容器位置变化时复用安装级命名空间。 | 迁移码决定重试或要求准备版本。钥匙串身份、通知注册和业务会话各自存储。 | [KeychainMigration · move](../Shared/KeychainMigration.swift#L37) · [AppleSecurity · prepareNotificationStorage](../Platform/AppleSecurity.swift#L124) · [SessionModel · start](../App/SessionModel.swift#L287) |
| **账户删除与本机清理**<br>`preview` · `reviewedWallets` · `confirmation` · `requestID` | 先查已有删除作业，再审阅约束、输入账户 ID 和确认钱包处置。提交后读作业状态。 | 回执未明时按原请求核对。取消 Apple 订阅和移除本机数据各有独立入口与确认。 | [AccountDeletionView](../Features/SafetyViews.swift#L147) · [SafetyViews · checkStatus](../Features/SafetyViews.swift#L210) · [SessionModel · removeLocalAccount](../App/SessionModel.swift#L495) |

相关回归入口见 [AccountSettingsTests](../Tests/AccountSettingsTests.swift) · [AccountSettingsUITests](../UITests/AccountSettingsUITests.swift) · [RecoveryPolicyTests](../Tests/RecoveryPolicyTests.swift) · [DeviceTransferTests](../Tests/DeviceTransferTests.swift) · [HardwareVectorTests](../Tests/HardwareVectorTests.swift) · [PasskeyBindingTests](../Tests/PasskeyBindingTests.swift)。

<a id="notifications"></a>

## 推送、提醒与已读

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **操作系统推送入口**<br>`token` · `pendingRoute` · `finishBackgroundEvents` | APNs 注册成功或失败通知应用。静默推送触发接收，用户点通知可在根视图就绪前留下路由。 | 旧版路由仍可解析。收到静默推送、点开提醒与把业务条目标为已读分别发生。 | [DMsgAppDelegate](../Platform/NativePush.swift#L16) · [NotificationRoute](../Platform/NotificationModels.swift#L139) · [DMsgApp](../App/DMsgApp.swift) |
| **每账户推送订阅**<br>`subscriptions` · `enabledAccounts` · `pendingRetries` · `idleWaiters` | 偏好允许、系统许可、有效 token 与注册凭据一起决定等待、续期、失败或启用。重试按账户排队。 | 注册后重新检查偏好。退出或忘记账户撤销对应登记。状态文字并不代表其他账户也已启用。 | [NativePush](../Platform/NativePush.swift#L63) · [NativePush · renew](../Platform/NativePush.swift#L77) |
| **通知偏好自动保存**<br>`saved` · `draft` · `linking` · `saveTask` | 修改设备开关、账户开关、免打扰、类别或暂停时间后合并保存。给第二个账户启用时先显示关联确认。 | 离开时保存待写草稿，原有保存继续。失败、冲突和系统注册失败各有对应重试。 | [NotificationSettingsView · scheduleSave](../Features/NotificationSettingsView.swift#L211) · [NotificationSettingsView · save](../Features/NotificationSettingsView.swift#L223) · [NotificationSetupPrompt](../Features/NotificationSettingsView.swift#L294) |
| **通知计划与投递**<br>`working` · `retry` · `receiveAgain` · `wake` | 前台、推送、内容修订或到期唤醒触发接收和重算，已有刷新时合并请求。根据计划更新角标、应用内提示或系统通知。 | 投递结果报回核心，已有系统通知可移除。只有当前会话继续应用异步结果。 | [NativeNotifications · refresh](../Platform/NativeNotifications.swift#L26) · [DMsgApp](../App/DMsgApp.swift) · [ReminderPlan](../Platform/NotificationModels.swift#L127) |
| **提醒列表与处理**<br>`recent` · `next` · `itemScope` · `reading` | 按账户查看待办或最近记录，翻页、稍后提醒、清除最近和逐条/全部已读各自改变列表和计数。 | 读请求绑定账户和范围。一条提醒按所见版本确认，刷新后重读计数。 | [RemindersView · load](../Features/RemindersView.swift#L137) · [SnoozeReminderView](../Features/RemindersView.swift#L216) · [NativeNotifications · markRead](../Platform/NativeNotifications.swift#L121) |
| **从提醒回到业务**<br>`reminderEntry` · `ready` · `loadedEpoch` · `unavailable` | 路由可能已结束、读取失败、属于另一账户、尚未解锁或可打开。选择账户与认证后再定位对应业务内容。 | 取消解锁仍保留未读。详情未找到、权限不足与暂时失败呈现不同出口。 | [SessionModel · openPushRoute](../App/SessionModel.swift#L574) · [ReminderDestinationView · load](../Features/ReminderDestinationView.swift#L142) · [RemindersView](../Features/RemindersView.swift) |
| **屏幕可见的已读回执**<br>`owner` · `viewport` · `watermark` · `openedReminder` | 已加载、窗口在前、详情确实可见且账户匹配后，才确认所见版本/水位并报告目标可见。 | 遮盖、退场或离开前台会注销。新版本提醒不能被旧屏幕的一次确认消费。 | [ReminderVisibility](../Features/ReminderVisibility.swift) · [NativeNotifications · observed](../Platform/NativeNotifications.swift#L104) · [ConversationView](../Features/ConversationView.swift) |
| **首次通知引导**<br>`notificationSetupDismissed` · `securityAlertsOn` | 已解锁的当前账户尚未启用安全提醒时出现引导。进入设置或选择稍后，分别进入设置页或保存该账户的选择。 | 重新绑定登录可再显示引导，另一账户有独立选择。是否实际注册仍由系统权限和推送流程决定。 | [NotificationSetupPrompt](../Features/NotificationSettingsView.swift#L294) · [SessionModel · setNotificationSetupDismissed](../App/SessionModel.swift#L659) |

相关回归入口见 [NotificationTests](../Tests/NotificationTests.swift) · [NotificationKeychainTests](../Tests/NotificationKeychainTests.swift) · [NotificationUITests](../UITests/NotificationUITests.swift)。

<a id="messaging"></a>

## 频道、信件与反馈

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **消息入口与文件夹**<br>`mode` · `folder` · `unreadOnly` · `limit` | 在频道、收件、已归档、草稿、待发送、已发送之间切换。搜索、分页和当前详情随范围调整。 | 无匹配与首次读取分别显示，可从列表静音、标记已读或继续已有草稿。 | [MessagesView](../Features/MessagesView.swift) · [SentLetterDetail](../Features/LetterComposerView.swift#L70) |
| **频道时间线读取与定位**<br>`messageRequest` · `refreshRequested` · `nextBefore` · `preserveMessage` | 先读本机消息，再按提示刷新。新提示合并补读。切词重建查询，向前翻页保住锚点，初次定位未读。 | 消息版本按序号合并。停在上方不被新消息拖动到底部，靠近末尾才更新已读水位。 | [ConversationView · refreshMessages](../Features/ConversationView.swift#L353) · [ConversationView · loadEarlier](../Features/ConversationView.swift#L470) · [ConversationTimeline](../App/ConversationTimeline.swift) · [SessionModel · channelRead](../App/SessionModel.swift#L543) |
| **频道编辑框、回复和附件**<br>`drafts` · `dirtyDrafts` · `attachmentDrafts` · `replyDrafts` | 编辑文字延后写入，选文件或回复只改当前频道草稿，点发送才采样文字、回复和固定版本附件。 | 离开再进可继续同一解锁工作区的选择。只清已入队的内容，期间新输入保留。 | [ConversationComposer · send](../Features/ConversationComposer.swift#L199) · [SessionModel · saveEditedDraft](../App/SessionModel.swift#L529) · [ComposerQueue](../App/MessageSending.swift#L129) |
| **消息队列、投递与重试**<br>`jobs` · `tails` · `pending` · `confirmed` | 请求入队后按频道串行投递，失败保留原消息 ID，服务端确认前在时间线显示待处理气泡。 | 确认后保留临时回显直至本地页读到相同序号。会话变更取消工作，恢复从核心队列继续。 | [MessageSending · start](../App/MessageSending.swift#L43) · [SessionModel · recoverMessages](../App/SessionModel.swift#L901) · [ConversationView · pendingBubble](../Features/ConversationView.swift#L321) |
| **消息操作与收到的附件**<br>`editingMessage` · `retracting` · `receivedFiles` · `checkedAttachments` | 查看引用/原文、隐藏、编辑、撤回分别依赖消息可读性和作者/频道权限。附件需先保存或恢复本地副本，随后可预览。 | 引用按原消息的隐藏/撤回状态显示。文件库修订会失效先前的附件判断。 | [ConversationView · messageActions](../Features/ConversationView.swift#L305) · [ConversationView · receiveAttachment](../Features/ConversationView.swift#L507) · [ConversationMessageRow](../Features/ConversationMessageRow.swift) · [MessageAttachments](../Features/MessageAttachments.swift) |
| **新建、加入与邀请成员**<br>`joining` · `creation` · `created` · `invitation` | 名称校验后创建，重试相同名称复用请求。加入则先核对邀请账户/期限。创建成功才进入为指定收件人生成邀请的步骤。 | 更换受邀者使旧链接失效。扫描或粘贴的邀请要经过用户确认与核心核验。 | [ConversationSetupView · complete](../Features/ConversationSetupView.swift#L101) · [ChannelInvitationView](../Features/ConversationSetupView.swift#L126) · [ChannelInvitationLink · parse](../Platform/ChannelInvitationLink.swift#L11) |
| **频道设置与成员操作**<br>`pendingChange` · `operationID` · `ledger` · `dirty` | 名称草稿和静音分别保存。角色、移除成员、轮换密钥、归档或离开等操作按 ledger 决定可用性，确认后提交。 | 状态未明或待处理的控制操作保留其身份，再读取频道详情和待办。 | [ChannelSettingsView · change](../Features/ChannelSettingsView.swift#L164) · [ChannelSettingsView · load](../Features/ChannelSettingsView.swift#L112) |
| **信箱快照与补读**<br>`revision` · `refreshPending` · `known` · `failure` | 缓存读取和显式刷新可重叠。刷新期间的新提示安排一次补读，失败后尝试读取已验证的本地页。 | 较早缓存回调不能覆盖后来的数据。首次未知、已知为空和刷新失败有不同界面。 | [InboxLoading · refresh](../App/InboxLoading.swift#L35) · [MessagesView · loadLetters](../Features/MessagesView.swift#L221) |
| **信件草稿、投递与核对**<br>`dirty` · `writes` · `sends` · `failures` | 草稿按 ID 排队写入，投递前刷盘再入队。未确认可查状态、重试原信，允许编辑时才回到草稿。 | 离开编辑页继续写，锁定或换账户结束内存任务，成功信件可从历史重新打开。 | [LetterWorkspace · send](../App/LetterWorkspace.swift#L176) · [LetterWorkspace · accept](../App/LetterWorkspace.swift#L155) · [LetterComposerView](../Features/LetterComposerView.swift) |
| **收件规则与来信处理**<br>`saved` · `draft` · `pending` · `working` | 修改收件模式和允许名单后审阅再提交。收到的信件可归档、恢复接收、回复、屏蔽或举报。 | 配置冲突/版本与本机草稿分别处理。屏蔽、举报和再次允许来信是独立操作。 | [InboxSettingsView · save](../Features/InboxSettingsView.swift#L80) · [InboxDetail](../Features/InboxView.swift#L40) · [SessionModel · blockAccount](../App/SessionModel.swift#L415) |
| **问题反馈**<br>`dirty` · `saving` · `edits` · `record` | 编辑描述与截图 → 加密草稿 → 提交原报告。未确认重试同一个报告，支持明确丢弃和新建。 | 写失败可重试原文。已发报告与待确认报告提供不同动作。选相片、文件或取消不触发发送。 | [FeedbackWorkspace · startWrite](../App/FeedbackWorkspace.swift#L119) · [FeedbackWorkspace · send](../App/FeedbackWorkspace.swift#L162) · [FeedbackView](../Features/FeedbackView.swift) |
| **举报与屏蔽记录**<br>`reportID` · `disclose` · `received` · `working` | 先选择举报原因、说明和披露范围，再提交并保留回执。列表支持查询与重试，屏蔽名单单独解除。 | 页面和提交任务各有忙碌/失败状态，已提交记录按自己的 ID 继续处理。 | [ReportContentView](../Features/SafetyViews.swift#L10) · [ReportsView](../Features/SafetyViews.swift#L100) · [BlockedAccountsView](../Features/SafetyViews.swift#L68) |
| **频道阅读游标**<br>`lastSeen` · `cursorReports` | 看到新序号或手工标为已读时，先更新本机游标，再按频道延后上报。持续阅读会替换待上报任务。 | 未读分隔、列表计数和其他设备最终收到的游标处于不同的时间点。 | [SessionModel · channelRead](../App/SessionModel.swift#L543) · [ConversationView](../Features/ConversationView.swift) · [MessagesView · markRead](../Features/MessagesView.swift#L146) |
| **已发消息的修改草稿**<br>`text` · `working` · `scope` | 消息菜单打开编辑页，原消息身份与摘要随提交。输入改变后离开可保存未提交编辑，再次打开继续。 | 编辑成功才清除表单并回到会话，撤回另走确认。原文选择页只展示可复制的内容。 | [MessageEditView](../Features/ConversationView.swift#L535) · [ConversationView · withdraw](../Features/ConversationView.swift#L497) |

相关回归入口见 [ConversationTimelineTests](../Tests/ConversationTimelineTests.swift) · [MessageSendingTests](../Tests/MessageSendingTests.swift) · [MessagingTests](../Tests/MessagingTests.swift) · [InboxTests](../Tests/InboxTests.swift) · [FeedbackTests](../Tests/FeedbackTests.swift) · [ConversationTests](../UITests/ConversationTests.swift) · [MessagingUITests](../UITests/MessagingUITests.swift) · [FeedbackUITests](../UITests/FeedbackUITests.swift)。

<a id="files"></a>

## 文件库、导入与传输

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **文件导航、搜索与批选**<br>`filesFolder` · `filesRequest` · `filesQuery` · `selection` | 从本地/云端范围进入目录、搜索、详情或批量操作。相同文件夹的旧列表可先展示。 | 列表应答绑定请求、目录与搜索范围。选择退出、批量操作结果及空目录分别反馈。 | [FilesView](../Features/FilesView.swift) · [FilesWorkspace · loadFiles](../App/FilesWorkspace.swift#L29) · [FileLibrarySheet](../Features/FileComponents.swift#L16) · [FileSort](../App/FilePresentation.swift#L6) |
| **外部文件、照片与相机导入**<br>`record.ready` · `started` · `results` · `capturing` | 获得权限后由系统交出 URL，暂存记录先标记未就绪，协调读取并复制成功后交核心导入。批量逐项记录结果。 | 取消、空间或大小不合要求时清理未完成的副本。就绪的暂存可从待导入列表恢复或丢弃。 | [PrivateFiles · importDocument](../Platform/PrivateFiles.swift#L75) · [FileImportView · run](../Features/FileImportView.swift#L44) · [PendingImportsView · resume](../Features/PendingImportsView.swift#L44) · [CameraImport](../Platform/CameraImport.swift) · [CameraCapture](../Platform/CameraCapture.swift) · [PhotoImport](../Platform/PhotoImport.swift) |
| **文件预览、系统分享与释放**<br>`opened` · `openedNonce` · `preview` · `sharing` | 选择本地可读版本后短时打开并取得 nonce。类型决定嵌入预览、系统分享或详情。 | 预览/分享结束或请求失效关闭临时文件。已备份、仅云端、需恢复与已驱逐的版本有不同操作。 | [FileScreen](../Features/FileDetailView.swift#L63) · [FileDetailView · cleanup](../Features/FileDetailView.swift#L295) · [VerifiedPreview](../Platform/VerifiedPreview.swift) |
| **改名、移动、版本与回收站**<br>`canSave` · `destinationChosen` · `completed` · `restoring` | 改名须有效且已变更，选择目标文件夹后逐项移动。历史版本另读，移入回收站与恢复分别确认。 | 失败保留输入或逐项结果。键盘与按钮共用保存条件，返回文件夹后读取对应列表。 | [FileNameEditor](../Features/FileManagementView.swift#L37) · [FileMoveView](../Features/FileManagementView.swift#L109) · [FileVersionsView](../Features/FileManagementView.swift#L223) · [FileTrashView](../Features/FileTrashView.swift) |
| **云备份目录与恢复**<br>`cloudBackupsComplete` · `cloudBackupRequest` · `pending` · `localDetails` | 先读本地目录，按需刷新远端。分页中断可保留已提交的目录。恢复、删除云副本、建立分享各有单独确认。 | 本机可用性、云副本状态和下载作业分别展示。切账户清目录元数据。 | [SessionModel · fetchCloudBackups](../App/SessionModel.swift#L1462) · [CloudBackupDetailView](../Features/CloudBackupsView.swift#L76) · [FilesWorkspace · restoreCloudFile](../App/FilesWorkspace.swift#L120) |
| **批量备份准入**<br>`capacity` · `outcomes` · `completed` | 取可用额度和单文件限制，再逐个提交。已备份、超过限制、进入队列或失败分别显示。 | 重复提交只作用于仍可处理的文件。入队后可转到传输列表跟踪最终结果。 | [FileBackupView · submit](../Features/FileBackupView.swift#L70) · [BackupAdmission](../App/FilePresentation.swift#L45) · [FilesWorkspace · queueFileBackup](../App/FilesWorkspace.swift#L105) |
| **前台备份/下载工作**<br>`backupWork` · `downloadWork` · `generation` · `handingOff` | 可见且解锁时读取核心作业，按重试期限分别推进上传和下载。显式交给 OS 前先等前台工作收尾。 | 有相同方向的后台移交时暂缓前台工作。换会话停止内存任务，已请求的作业由核心日志保留。 | [FileTransfers · watch](../App/FileTransfers.swift#L64) · [FileTransfers · beginHandoff](../App/FileTransfers.swift#L38) · [TransfersView](../Features/TransfersView.swift) |
| **后台密文传输与最终验证**<br>`scheduling` · `attempt` · `completed` · `retryAfter` | 准备并保存授权与分块，调度有限个 URLSession 任务。分块完成、暂时错误、暂停/重试和过期分别更新本机清单。 | 过时的 attempt 回调被过滤，分块收齐进入待验证，仍需原账户在前台完成或取消。系统重启应用处理传输时，后台完成回调保留到全部委托事件结束。 | [BackgroundTransfers · schedule](../Platform/BackgroundTransfers.swift#L206) · [CipherTransferState](../Platform/BackgroundTransfers.swift#L9) · [BackgroundTransferView · finish](../Features/BackgroundTransferView.swift#L42) |
| **文件分享链接**<br>`work` · `link` · `current` · `confirming` | 审阅指定版本与限制后生成链接，成功才显示复制/分享。管理页先显示缓存，再读取远端变化。 | 失败保留输入，撤销后更新详情与列表。快照链接走独立管理入口。 | [FileLinkComposer](../Features/FileLinkViews.swift#L4) · [FileLinksView](../Features/FileLinkViews.swift#L92) · [FileLinkDetailView](../Features/FileLinkViews.swift#L139) |
| **文件收集与接收件**<br>`capacity` · `created` · `progress` · `saving` | 确认额度、期限和接收人标签后逐条建收集链接。接收件可保存到库、删除，收集链接可单独关闭。 | 部分成功保留已建链接并重试剩余标签。已保存、被撤回和无法读取的接收件提供不同动作。 | [FileRequestComposer](../Features/FileRequestViews.swift#L82) · [FileRequestDetailView](../Features/FileRequestViews.swift#L225) |
| **系统分享暂存与认领**<br>`completedProviders` · `accountHash` · `sha256` | 扩展或快捷指令先暂存输入，主应用提示有新项目。手工选择账户导入、核对摘要并清除暂存。 | 扩展重试跳过已处理项。只有认领账号能继续导入，其他账号看见文件名及可丢弃的既有行为保留。 | [ShareViewController · save](../Extensions/Share/ShareViewController.swift#L49) · [IncomingShares](../Shared/IncomingShare.swift#L12) · [IncomingSharesView · receive](../Features/IncomingSharesView.swift#L34) · [FilesWorkspace · announceIncomingShares](../App/FilesWorkspace.swift#L85) · [ImportIntoDMsgIntent](../Intents/WorkspaceIntents.swift#L28) |
| **本地独有数据与备份提醒**<br>`filesLocalOnly` · `backupHintDismissed` · `atRisk` | 从未备份版本、未同步数量和最后成功时间计算风险。切账户或传输完毕后重读摘要。 | 日常提醒可按账户关闭。移除本机数据、替换设备和继任审阅仍展示与该操作相关的本地资料。 | [FilesWorkspace · loadFileSummary](../App/FilesWorkspace.swift#L66) · [SessionModel · dismissBackupHint](../App/SessionModel.swift#L490) · [LocalOnly](../Features/DeviceHandoverViews.swift#L8) · [LocalAccountRemovalView](../Features/AccountSettingsViews.swift#L71) |

相关回归入口见 [FileLibraryTests](../Tests/FileLibraryTests.swift) · [FileTransferTests](../Tests/FileTransferTests.swift) · [CipherTransferTests](../Tests/CipherTransferTests.swift) · [SessionPolicyTests](../Tests/SessionPolicyTests.swift) · [FileLibraryUITests](../UITests/FileLibraryUITests.swift) · [FileTransferUITests](../UITests/FileTransferUITests.swift)。

<a id="vault"></a>

## 保险库、秘密包与可见值

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **保险库浏览与筛选**<br>`vaultRequest` · `vaultCountsKey` · `vaultHasMore` | 类型、收藏、标签、排序或搜索改变时重新读取。项目和计数有各自对应的查询，向下翻页追加。 | 只采纳当前查询，加载、空集合、离线旧内容与失败分开。从列表可打开指定条目或新建。 | [SessionModel · loadVault](../App/SessionModel.swift#L1395) · [VaultView](../Features/VaultView.swift) · [VaultBrowsePage](../App/VaultModels.swift#L26) |
| **工作区编辑草稿**<br>`records` · `dirty` · `presented` · `presenter` · `writing` | 打开/恢复一份编辑会话，变更按草稿 ID 持久化。前后台切换先保存，用户另行选择保存条目或放弃草稿。 | 锁定后再恢复加密草稿。丢弃完成才移除对应记录，保存失败保留编辑内容和重新提交入口。 | [VaultEditing · open](../App/VaultEditing.swift#L77) · [VaultEditing · flush](../App/VaultEditing.swift#L200) · [VaultEditor · save](../Features/VaultEditor.swift#L253) · [RootView](../Features/RootView.swift) |
| **编辑交互与自定义字段**<br>`saveAttempts` · `focus` · `nameVisited` · `showingDraft` | 类型变化保留已填值。新增、排序或删除字段，提交时校验标题与字段。 | 交互式关闭脏草稿需选择。每次无效保存都重新定位首个错误，大字和键盘也可完成。 | [DraftDismissGuard](../Features/VaultEditor.swift#L301) · [VaultEditorField](../Features/VaultEditorFields.swift#L6) · [VaultPasswordGenerator](../Features/VaultEditorFields.swift#L292) |
| **字段显示、复制与大字**<br>`revealed` · `codes` · `revealRequest` · `copied` | 默认掩盖，显式操作读取对应版本的一个字段。OTP、复制提示和大字页各有自己的到期时间。 | 离开、遮盖、版本变化或到期后清理显示。迟到的读取不能替换当前展示。 | [VaultDetailView · read](../Features/VaultDetailView.swift#L226) · [VaultDetailField](../Features/VaultDetailField.swift) · [VaultLargeTypeView · load](../Features/VaultLargeTypeView.swift#L62) · [RevealedVaultValue](../App/VaultModels.swift#L49) |
| **历史、回收站与冲突**<br>`revision` · `restoring` · `choice` · `theirs` | 查看历史、与当前修订比较，可确认恢复。冲突可保留任一版本或勾选字段合并。 | 恢复/合并携带期望修订。冲突头改变清除已选方案，失败可重新审阅。回收站恢复成功才移出列表。 | [VaultHistoryView · restore](../Features/VaultHistoryView.swift#L66) · [VaultTrashView · restore](../Features/VaultTrashView.swift#L31) · [ConflictsView · merge](../Features/ConflictsView.swift#L124) |
| **密码库文件导入**<br>`phase` · `selected` · `digest` · `working` | 选择并暂存导出文件或继续待导入项，读摘要预览、筛选和勾选后才导入。 | `needsCopy` 只能重选文件，`staged` 可重试这份暂存，只有 `reviewed` 同时持有来源和已审阅摘要。成功后显示已导入条目。 | [VaultImportReview](../Features/VaultImportView.swift#L6) · [PrivateFiles · pending](../Platform/PrivateFiles.swift#L124) · [PendingImportsView](../Features/PendingImportsView.swift) |
| **秘密查找与选择**<br>`query` · `phase` · `opening` | 频道快照先选秘密再选字段。输入变化回到 `waiting`，`reading` 持有请求 ID。`ready` 标明是否还有下一页，`failed` 可重试。 | 下一页失败保留上页并重试原 offset。过期读取不能清除新请求的状态。快照的详情读取仅为当前点选项导航。 | [VaultPickerSearch](../Features/VaultItemPicker.swift#L5) · [SecretSnapshotItemPicker](../Features/SecretShareViews.swift#L256) |
| **秘密快照准备、发布与撤销**<br>`fields` · `attachmentIDs` · `job` · `request` · `link` | 固定条目修订、所选字段和附件版本，选择频道或限时链接，先得到分享作业，再进入审批或执行结果。 | 作业状态决定可复制、核对或请求撤销。页面离开/遮盖会隐藏显示的链接，收回链接无法收回已取走的副本。 | [SecretSnapshotComposer](../Features/SecretShareViews.swift#L4) · [SecretShareDetailView](../Features/SecretShareViews.swift#L151) · [SecretShareJob](../Features/SecretModels.swift#L44) |
| **秘密包预览与导入**<br>`source` · `preview` · `fields` · `folder` · `completed` | 从链接/密码或频道中的指定消息读取预览，勾选字段与附件后选择目标目录，再提交导入请求。 | 更新输入废弃旧预览。已完成可打开结果，需要审核则进入原审批。目录选择还要匹配当前目标。 | [SecretImportView · inspect](../Features/SecretImportView.swift#L104) · [SecretImportView · importRequest](../Features/SecretImportView.swift#L121) · [SecretFolderPicker](../Features/SecretImportView.swift#L138) |
| **引用、附件、健康与轮换**<br>`reference` · `dependencies` · `includeDismissed` · `confirmStep` | 从字段取得引用，读本机依赖与附件版本。健康结果可查看、忽略或恢复显示，轮换按清单逐步确认。 | 重新校验引用需依赖摘要。每一步记录用户声明，最后一个条件满足后才显示完成。 | [SecretReferenceViews](../Features/SecretReferenceViews.swift) · [SecretHealthView](../Features/SecretHealthViews.swift#L4) · [SecretHealthViews · record](../Features/SecretHealthViews.swift#L169) |
| **秘密活动日志**<br>`next` · `loaded` · `cancelling` · `reviewing` | 分页读操作记录，根据原状态决定能否取消待审请求或给结果丢失的记录补人工复核。 | 确认后重读日志。展示的协议阶段来自核心，本地只维护加载与当前动作。 | [SecretActivityView · act](../Features/SecretActivityView.swift#L56) · [SecretModels · secretStateTitle](../Features/SecretModels.swift#L122) |
| **编辑字段辅助输入**<br>`tool` · `showingDraft` · `entry` · `selected` | 聚焦/离开字段分别控制输入与校验。标签遇分隔符或提交才成为标签，日期、密码生成和扫描各在子界面产生候选值。 | 日期或生成器须确认才写回，取消保留原值。生成器设置改变重算候选，扫码权限被拒可转系统设置。 | [VaultEditorField](../Features/VaultEditorFields.swift#L6) · [VaultTagEditor](../Features/VaultEditorFields.swift#L155) · [VaultDatePicker](../Features/VaultEditorFields.swift#L221) · [VaultPasswordGenerator](../Features/VaultEditorFields.swift#L292) |

相关回归入口见 [VaultTests](../Tests/VaultTests.swift) · [VaultDraftLifecycleTests](../Tests/VaultDraftLifecycleTests.swift) · [VaultUITests](../UITests/VaultUITests.swift) · [MessagingUITests](../UITests/MessagingUITests.swift) · [AccountSettingsUITests](../UITests/AccountSettingsUITests.swift)。

<a id="approvals"></a>

## 审批、浏览器与 Agent

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **审批列表、详情与决定**<br>`payload` · `supported` · `readyAt` · `decision` · `result` | 读取待办并展示可验证详情，确认决定绑定请求 ID 与摘要。是否可在移动端批准、是否到期和审阅等待分别限制入口。 | 拒绝、已处理、待核对和结果就绪各有展示。未知执行结果走原请求核对。 | [SessionModel · loadApprovals](../App/SessionModel.swift#L1499) · [ApprovalDetailView](../Features/ApprovalsView.swift#L66) · [ApprovalsView · reconcile](../Features/ApprovalsView.swift#L204) |
| **跨页面的审批回执**<br>`requests` · `generation` · `finalStates` | 按动作键保留已生成的请求，重新打开相同操作显示原审批。返回时逐项读取其状态。 | 读不到的回执保留，到终态才释放。工作区代数变化清表。 | [ApprovalReceipts](../Features/ApprovalsView.swift#L350) · [DeviceDetail](../Features/DevicesView.swift#L64) · [RecoveryView](../Features/RecoveryView.swift) |
| **浏览器配对与连接管理**<br>`invitations` · `candidates` · `confirming` · `revoking` | 创建邀请并打开浏览器，回前台/手动刷新后列出候选。选择、确认授权、关闭邀请、撤销连接分别提交。 | 本地记录和候选读取先后变化。候选随重新获取的列表变化。此屏没有独立的候选到期计时器。后续签名另经审批。 | [BrowserConnectionsView · load](../Features/BrowserConnectionsView.swift#L70) · [BrowserConnectionsView · grant](../Features/BrowserConnectionsView.swift#L105) · [BrowserConnectionsView · revoke](../Features/BrowserConnectionsView.swift#L111) |
| **浏览器请求深链**<br>`pending` · `approval` · `missing` | 解析链接后按目标账户、解锁状态和目的选择配对或请求页。同步连接后以原请求查审批。 | 其他账户显示归属说明，找不到目标可重试或进入审批列表。 | [ConnectLink](../Platform/ConnectLink.swift) · [ConnectLandingView](../Features/BrowserConnectionsView.swift#L119) |
| **托管 Agent 与能力授权**<br>`paused` · `grants` · `capabilities` · `request` | 列举/创建 Agent，进入详情后可暂停或恢复、审阅能力授权、撤销或移除。各动作由相应核心入口处理。 | 详情和草稿各按工作区存活，提交或读失败保留错误反馈，权限与批准结果由核心记录。 | [AgentsView](../Features/AgentViews.swift#L4) · [AgentDetailView](../Features/AgentViews.swift#L40) |
| **Principal、控制器与委托**<br>`principal` · `credentials` · `draft` · `request` | 未启用时可申请启用。已启用后可申请登记或退役控制器，按范围、受众、期限准备凭据并审阅撤销。 | 带离开确认的草稿可恢复。控制器代数、凭据状态与本机是否正在提交分别判断。 | [AgentDelegationsView](../Features/AgentViews.swift#L136) · [AgentAuthorityDraft](../Features/AgentViews.swift#L120) |

相关回归入口见 [AccountSettingsTests](../Tests/AccountSettingsTests.swift) · [NavigationTests](../UITests/NavigationTests.swift) · [NotificationUITests](../UITests/NotificationUITests.swift) · [ConnectLinkTests](../Tests/ConnectLinkTests.swift)。

<a id="storekit"></a>

## 会员与 StoreKit

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **商品、环境与订阅归属**<br>`loadRevision` · `productPlans` · `subscriptions` · `loadIssue` | 读取商店配置、商品和订阅组，核对交易与当前账户。环境补读和产品缺失有各自结果。 | 本机或其他账户拥有、验证失败和无订阅决定可购买、切账户或重试，旧读取不得改写新账户结果。 | [PurchaseModel · load](../App/PurchaseModel.swift#L284) · [PurchaseModel · canPurchase](../App/PurchaseModel.swift#L212) · [PurchaseModel · subscription](../App/PurchaseModel.swift#L229) |
| **选择价格与支付阶段**<br>`working` · `purchaseActivity` · `awaitingApproval` | 准备阶段并行完成准入检查和设备证明，再次核对后进入 Apple 确认、交易应用。状态显示在所选套餐。 | 取消、等待 Apple 批准、交付未确认及失败分支分别处理，已有活动抑制重复购买。 | [PurchaseModel · purchase](../App/PurchaseModel.swift#L371) · [PurchaseButton](../Features/PurchaseButton.swift) · [SubscriptionView · planAction](../Features/SubscriptionView.swift#L334) |
| **收据投递、重放与恢复购买**<br>`deliveries` · `pendingTransactions` · `pendingRefreshes` · `revision` | 购买结果、交易监听、主动恢复、回前台或网络恢复均可能交付同一收据。相同 JWS 合用在途任务。 | 同交易有更新的 JWS 时先等前次结束。核心确认应用后才允许 finish，失败保留原交易继续核对。 | [PurchaseModel · deliver](../App/PurchaseModel.swift#L589) · [PurchaseModel · reconcile](../App/PurchaseModel.swift#L532) · [PurchaseModel · listen](../App/PurchaseModel.swift#L500) |
| **权益、用量与订阅管理**<br>`loadRequest` · `usageRequest` · `catalogueRequest` · `managing` | 分别读有效套餐、用量、目录及续费安排。过期或不可核实的权益需刷新，管理订阅从 Apple 面板返回后重读。 | 用量未知单独显示。当前订阅、下期套餐和沙盒状态来自各自数据源，页面退出使旧刷新失效。 | [SubscriptionView · load](../Features/SubscriptionView.swift#L401) · [SubscriptionView · loadUsage](../Features/SubscriptionView.swift#L466) · [MembershipComparisonView](../Features/MembershipPlans.swift#L121) |

相关回归入口见 [DeploymentTests](../Tests/DeploymentTests.swift) · [PurchaseDeliveryTests](../Tests/PurchaseDeliveryTests.swift) · [MembershipPlansTests](../Tests/MembershipPlansTests.swift) · [NavigationTests](../UITests/NavigationTests.swift)。

<a id="wallet"></a>

## 团队钱包

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **钱包清单、加入与提案浏览**<br>`walletID` · `pending` · `history` · `next` | 读取钱包目录，可按现有钱包 ID 审阅加入。详情按待办/历史和游标读取提案。 | 已存在的加入请求继续审核，翻页或刷新更新所属钱包的列表。 | [WalletsView](../Features/WalletViews.swift#L4) · [WalletDetailView](../Features/WalletViews.swift#L67) |
| **转账草稿与预览**<br>`draft` · `preview` · `feeDecimals` · `scope` | 选择链、资产与来源地址，按精度校验金额、读取费用/准入预览。输入改变使旧预览失效。 | 只采用对应草稿的返回值。离开保留要求保留的草稿和已尝试的请求身份。 | [WalletTransferView · inspect](../Features/WalletTransferView.swift#L152) · [WalletTransferView · submit](../Features/WalletTransferView.swift#L163) |
| **提案提交与结果不明**<br>`requestID` · `attempted` · `phase` | 首次创建，失败后根据本地日志决定继续核对还是允许新请求。导航或后台中断保留不确定状态。 | 只有原始结果确认后显示创建成功。转账、权限变更和密钥注册/替换复用此规则。 | [WalletProposalSubmission · begin](../App/WalletProposalSubmission.swift#L21) · [WalletProposalSubmission · failed](../App/WalletProposalSubmission.swift#L34) · [WalletControlsView · submit](../Features/WalletControlsView.swift#L121) |
| **成员、阈值、地址与冻结**<br>`confirming` · `submission` · `balances` · `working` | 在详情加载成员、策略、地址与余额，选定变更并审阅。管理入口依据角色、状态及未决提案。 | 变更结果进入提案流程。余额显示和冻结动作独立，不用列表缓存代替执行检查。 | [WalletControlsView](../Features/WalletControlsView.swift) · [WalletDeviceKeysView](../Features/WalletDeviceKeysView.swift#L8) |
| **设备密钥、保护器与替换**<br>`keys` · `replacing` · `replacementSubmission` · `unbind` | 创建本机密钥/保护器，登记联签人，检查本机持有与已登记的绑定。替换与解除绑定各有确认。 | 遇到未知创建结果沿原请求核对，移除保护器受剩余数量约束。 | [WalletDeviceKeysView · replace](../Features/WalletDeviceKeysView.swift#L175) · [WalletDeviceKeysView · nativeRun](../Features/WalletDeviceKeysView.swift#L168) · [WalletCosignersView](../Features/WalletViews.swift#L224) |
| **原生地址迁移**<br>`plan` · `next` · `l2` · `pending` | 读取原地址与目标设备，依据高度、余额、已有提案和选定路径显示登记、建地址、地址簿、归集或归档。 | 每一步重新读迁移计划，提案待批或不足门槛的输出会改变可用动作。 | [WalletNativeMoveView](../Features/WalletDeviceKeysView.swift#L212) · [WalletDeviceKeysView · next](../Features/WalletDeviceKeysView.swift#L345) |
| **提案审阅、联签与执行**<br>`observedVersion` · `plan` · `packet` · `cosign` | 打开原提案后决定审批或拒绝，本机联签、导入外部联签包、提交已收集的执行计划分别推进。 | 显示结果依赖核心提案状态与当前观察版本，未满足的联签要求仍在详情中。 | [WalletProposalView](../Features/WalletViews.swift#L112) · [WalletViews · cosignHere](../Features/WalletViews.swift#L191) · [WalletViews · executeCollectedPlan](../Features/WalletViews.swift#L201) |

相关回归入口见 [WalletProposalSubmissionTests](../Tests/WalletProposalSubmissionTests.swift) · [AccountSettingsTests](../Tests/AccountSettingsTests.swift) · [NavigationTests](../UITests/NavigationTests.swift)。

<a id="extensions"></a>

## 系统扩展、认证与跨进程副本

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **系统文件副本** | 选择文件版本，准备独立加密副本，复核账户后发布。 | 已发布副本可在短期会话开启失败后重新验证。[文件设置](../Features/SystemExportsView.swift) |
| **密码自动填充** | 解锁、内容变化和回到前台时核对保险库版本。扩展经系统认证后显示可搜索目录。 | 已知目标按稳定条目 ID 记住多个账号。仅本次填充不保存关联。目录替换使已打开的旧版本失效。[同步](../App/SessionModel.swift) · [选择器](../Extensions/Credentials/CredentialViewController.swift) |
| **凭据扩展交互** | 系统认证后，用户在同一页面选择账号并确认。对当前目标已记住的账号可直接点选。 | 认证超时或目录刷新时重新认证。所属账户、登录项及显示信息相同便继续完成这次填充。变更或删除的登录项需要重新选择。关闭请求会丢弃待交付结果。 | [CredentialPickerSession](../Shared/CredentialPicker.swift) · [选择与确认界面](../Shared/CredentialPickerView.swift) · [扩展回填](../Extensions/Credentials/CredentialViewController.swift) |
| **系统「文件」可见性与认证**<br>`fileSession` · `expires` · `generation` | 发布时可建立短期访问。到期或锁定后，Files UI 交互认证并为当前快照签发短会话。 | 扩展用自己的认证与钥匙串有效期。旧快照不能覆盖新会话。显式清理或失效时撤销访问。 | [SystemExports · unlockFiles](../Shared/SystemExports.swift#L243) · [FileAccessViewController · unlock](../Extensions/FilesUI/FileAccessViewController.swift#L23) · [SystemExportsView · signalFiles](../Features/SystemExportsView.swift#L212) |
| **File Provider 枚举与读取**<br>`currentSyncAnchor` · `shutdown` · `progress` | 用快照 ID 作为变更锚点，枚举根/工作集。按请求版本定位文件，写到临时输出。 | 域失效或 Progress 取消中断打开。校验失败清除不完整输出，回调返回文件或错误，写入操作由只读合同拒绝。 | [ExportEnumerator](../Extensions/Files/ExportFileProvider.swift#L20) · [ExportFileProvider · fetchContents](../Extensions/Files/ExportFileProvider.swift#L74) · [SystemExports · materialize](../Shared/SystemExports.swift#L219) |
| **App Attest 与系统证明**<br>`challenge` · `nonce` · `object` · `idleWaiters` | 本机保存待登记密钥与挑战，生成证明再登记。未知登记结果先查状态。推送和购买共用排队获取密钥的入口。 | 无效密钥允许在本次操作内重新取得一次。暂时失败保留可继续的登记，不重复并行触发系统证明。 | [AppAttestation · key](../Platform/AppAttestation.swift#L40) · [NativeApple · withKey](../Platform/NativeApple.swift#L62) |
| **系统选择器与临时资源**<br>`received` · `denied` · `file` | 摄像头、扫码、相册、文件和 Quick Look 由系统代理回调。采集需要许可，照片/视频持有单独的临时副本。 | 扫码协调器只接收一次，结束释放扫描器，导入完或启动时清理弃置临时文件。分享去向与预览由原调用页决定。 | [QRCodeScanner](../Platform/QRCodeScanner.swift) · [CameraImport · open](../Platform/CameraImport.swift#L54) · [PhotoImport · finish](../Platform/PhotoImport.swift#L66) · [VerifiedPreview](../Platform/VerifiedPreview.swift) · [CameraCapture](../Platform/CameraCapture.swift) |

相关回归入口见 [SystemExportsTests](../Tests/SystemExportsTests.swift) · [FileLibraryTests](../Tests/FileLibraryTests.swift) · [SessionPolicyTests](../Tests/SessionPolicyTests.swift) · [AppAttestationTests](../Tests/AppAttestationTests.swift) · [AccountSettingsUITests](../UITests/AccountSettingsUITests.swift)。

<a id="controls"></a>

## 共享交互

| 流程与状态依据 | 事件和转换 | 结果、恢复或失效条件 | 源码入口 |
| --- | --- | --- | --- |
| **未保存表单离开与恢复**<br>`settingsDrafts` · `viewGeneration` · `confirming` | 可离开的编辑页按表单键暂存，关闭/返回时根据是否修改提示保留或丢弃。回到同一工作区再读取。 | 后台与锁定的生命周期不同。旧页面写草稿时须匹配工作区代数。各专用编辑器仍管理自身持久化。 | [SessionModel · keepFormDraft](../App/SessionModel.swift#L399) · [DraftExitConfirmation](../Features/SettingsComponents.swift#L141) · [PublicProfileView](../Features/PublicProfileView.swift) |
| **分页触发与列表呈现**<br>`trigger` · `working` · `visible` | 显示更多行或点按钮时按触发值请求一页，正在加载改为进度。组件提供事件，游标、范围、是否还能读和错误由业务持有。 | 选择器读取中保留分页行，防止 SwiftUI 撤销正在取页的任务。失败后用户可以针对同一页重试。 | [LoadMoreRow](../Features/SettingsComponents.swift#L107) · [VaultItemPicker](../Features/VaultItemPicker.swift) · [RemindersView · load](../Features/RemindersView.swift#L137) |

相关回归入口见 [VaultUITests](../UITests/VaultUITests.swift) · [AccountSettingsUITests](../UITests/AccountSettingsUITests.swift) · [MessagingUITests](../UITests/MessagingUITests.swift)。

## 已有权威状态与覆盖边界

| 权威状态 | 它决定的部分 | 仍须合读的条件 |
| --- | --- | --- |
| 核心 `SessionSnapshot`、Swift `SessionLifecycle` / `SessionOperation` | 核心身份和锁定状态、场景阶段、独占操作类别 | 认证回调、后台收尾和会话代数共同决定何时能解除遮罩。 |
| `DeviceTransferModel.Phase`、`CipherTransferState.Phase` | 设备传输计划、后台分块阶段 | Bonjour、当前账户、授权期限、任务 attempt 和最终验证各有独立状态。 |
| `PurchasePhase`、`Delivery`、`WalletProposalSubmission.Phase` | 本次购买进度、交付结论和本机提案提交阶段 | 跨进程结果和重试由交易、作业日志、当前账户和外部权威回执共同决定。 |
| 核心的频道 ledger、审批、信件、秘密作业、钱包提案和通知版本 | 成员资格、是否已批准/投递/完成、业务有效期 | Swift 中的确认框、按钮、缓存和页面加载分别记录人的操作与可见性。 |

### 无独立业务生命周期的入口

- 显示模型、名称、图标、系统色与触感将输入映射为界面。浏览范围通过 Binding 写回所属页面，语言设置链接交由 iOS 处理，隐私政策是本地静态内容。[显示模型](../App/DisplayModels.swift) · [名称](../Features/VaultNames.swift) · [图标](../Features/AppGlyph.swift) · [原生主题](../Shared/NativeTheme.swift) · [触感](../Platform/Haptics.swift) · [浏览控件](../Features/BrowseBar.swift) · [语言](../Features/LanguageSettingsLink.swift) · [隐私政策](../Features/PrivacyPolicyView.swift)
- 消息耗时诊断记录既有操作和固定错误码。Widget 的时间线返回静态入口，打开和解锁由主应用负责。[耗时记录](../Platform/MessageTiming.swift) · [Widget](../Extensions/Widget/WorkspaceWidget.swift)
- DEBUG 的工作区、文件、商店预览夹具分别模拟账户、消息、故障和展示数据。录制用触点观察器有按下、移动和结束的生命周期，仅在预览模式安装。[基础夹具](../App/PreviewFixtures.swift) · [文件夹具](../App/FileFixtures.swift) · [商店夹具](../App/StorePreviewFixtures.swift) · [录制触点](../App/PreviewTouches.swift)
- [工程目标](../project.yml) 和 [桥接包](../CoreBridge/Package.swift) 定义编译与链接边界。覆盖 App 26 个、Features 84 个、Platform 27 个、Shared 4 个、Extensions 5 个、Intents 1 个 Swift 文件。资源、权限清单、Review 材料，以及 30 个单元测试和 17 个界面测试 Swift 文件用于定位合同和回归。生成的 UniFFI 胶水及 Rust 内部状态由各自模块维护，本表跟踪到调用/回执边界。

### 本次收敛的三处转换

| 用户操作 | 原逻辑的问题 | 修复后的可观察结果与回归入口 |
| --- | --- | --- |
| 在频道选择分享的秘密，随后更换搜索词 | 旧行留在列表中，读失败后仅弹出错误。分页读和新搜索还共用一个忙碌标志。 | 输入改变即清旧行。已确认的零匹配、进行中和可重试失败可区分。频道快照选择器按页读取。[界面回归](../UITests/MessagingUITests.swift#L64) · [分页与迟到应答](../Tests/VaultTests.swift#L42) |
| 为账户 A 生成绑定请求后改填 B | 页面存有 A 的 token，分享沿用 A，而接受操作读取当前输入 B。 | 换目标后立即收起分享、复制和接受操作。为 B 新建请求后重新可用。[绑定入口](../Features/LoginBindingRequestView.swift) · [换账户回归](../UITests/AccountInitializationUITests.swift#L46) |
| 已预览文件 A，随后选择 B 或继续一次中断的密码导入 | B 复制失败时仍保留 A 的 handle。从待导入列表恢复后，预览失败又缺少重试按钮。 | 新来源暂存失败时重新选文件。已有暂存只重试该 handle。成功预览与提交共用同一份来源和摘要，已导入的条目可打开。[导入阶段](../Features/VaultImportView.swift) · [重试回归](../Tests/VaultTests.swift#L7) |

上轮待评审的五项维持现行行为，包括跨锁定恢复附件/回复、跨账号管理已认领的暂存、大收件箱预读、后台下载进度清单读取，以及编辑器在 iOS 27 上的布局警告。对应证据保存在[前次复核记录](../VALIDATION.md#L4009)。

### 本地验证

2026-11-18，在 iPhone 18 Pro（iOS 27.0）模拟器上使用测试账户和夹具，旧逻辑复现了换搜索词后旧条目仍可点、改填账户后旧绑定请求仍可用。密码导入的单元测试使用临时 CSV 和缺失文件覆盖复制失败，并验证从待导入记录恢复后只重试该文件。

修复后，全部 241 项 iOS 单元测试和 6 项相关界面测试通过，零跳过。本机结果包位于 `.build/ios-state-machines-20261118/final.xcresult`。Rust 核心的 15 项密码导入测试通过，23 种本地化校验通过。面向真机的未签名 arm64 Release 主应用和五个扩展均编译成功。本次未在真机运行，也未重跑完整界面套件。
