# Android 状态机索引

2026-11-19，基于 `ab2147a1` 及当时的工作区改动核对。扫描了主应用的 160 个 Kotlin 文件、GMS 的 3 个、无 Google 服务变体的 2 个，以及 QA 宿主的 1 个。沿入口核对了 Manifest、Activity/Service/Receiver/Provider、Compose 事件、协程回调、持久化读写和测试调用方。

这里列出 **64 组行为**。27 组用标记、可空对象、作业或持久字段表示阶段。33 组同时依赖显式快照与 Android 隐式状态。2 组在本次修复中显式化。其余 2 组是公共读模型和构建／QA 参照。每一组可以有并行子流程，这些数字按表中行计数。

例如，点击「重试」可能重放已提交的原请求，也可能创建新操作。先按用户行为找到状态所有者，再核对表中的触发事件、身份和回执，可以判断哪些按钮应当可用，以及迟到的结果能否改变画面。

## 会话、外部入口与共享机制

| 行为 | 表示 | 触发事件和转换 | 所有者、约束与源码 |
| --- | --- | --- | --- |
| 进程、主窗口与启动 | 隐式 | Provider 或推送可以先唤起进程。主窗口按需创建客户端 → 打开工作区并读取偏好 → 采用快照。主窗口数归零时延后转入后台，重建窗口可以撤销这次离开。 | Application 管窗口，Controller 管启动与工作区，系统导出可独立运行。[DMsgApplication]、[MainActivity]、[ClientController]、[NativeBootstrap] |
| 隐私遮挡与会话锁 | 隐式 | 前台活动 → 短暂遮挡或后台宽限 → 恢复当前代，或因息屏、超时、手动操作立即锁定。外部选择器和认证提示有单独的返回路径。 | `background/foreground/lock`、单调时钟和原生会话共同决定是否可读。遮挡、已锁、正在锁定是三个不同条件。[ClientController]、[SessionPolicy]、[DMsgApp] |
| 锁定时最后一次输入 | 隐式 | 四类编辑器采集待写输入 → 原生会话立即失效 → 清空界面 → 等待加密锁定回执 → 原账户、原设备解锁后恢复。重复锁定合并，成功后才能继续切换账户等操作。 | `afterLock/controlBusy` 管界面后续动作，`readSession/restoreEditors` 交回仍需编辑的输入。加密持久化由移动原生层负责。[ClientController]、[VaultDrafts]、[DraftLedger]、[LetterCoordinator]、[FeedbackCoordinator] |
| 采用快照与交付异步结果 | 混合 | 申请带账户和代号的句柄 → 操作占用资源键 → 成功、失败、替换或取消 → 仍拥有该键且会话有效才交付。外部 Activity 还须等待前台可见，否则释放临时资源。 | `SessionFence` 另核对 epoch 和快照 sequence。操作结束只释放自己的 Job。读、导航、签名等流程引用这条公共规则。[NativeSession]、[OperationOwners]、[ClientController] |
| 资源读取 | 参照 | `Idle → Loading(previous) → Empty/Ready`。读取失败且有旧值时进入 `Stale`，无旧值时进入 `Failed`。重置和下一次读取都会使旧请求失效。 | `ReadState` 已显式。请求号决定返回值能否被接受。可空资源上次成功返回的 `null` 在下次失败时得到 `Failed`，只读 `value()` 会丢掉这段历史。[ReadStore]、[Wire]、[ReadFeedback] |
| 自动解锁 | 隐式 | 进入前台重新布防 → 检查系统锁屏、欢迎页、已有弹窗和 fast unlock 能力 → 一次性消费或由手动入口抑制。 | 自动解锁的机会与工作区是否已解锁分别由策略和原生会话决定。[SessionPolicy]、[ClientController]、[AccessScreen] |
| 欢迎页与浏览器身份回调 | 隐式 | 选择新建或已有身份 → 发起 II 登录或绑定 → 浏览器返回 → 登录完成，或绑定进入有期限的原回调重试 → 审批页、取消或过期。 | `pendingLogin/bindingLogin/bindingReturn` 配合 `UiState.bindingRetry`。Activity 消费一次 Intent，绑定重试核对 generation。[AccessScreen]、[MainActivity]、[Models]、[ClientController]、[SecurityPages] |
| Passkey 与硬件在场验证 | 隐式 | 获取挑战 → 系统凭据选择或创建 → 拿到证明 → 提交。可重试故障保留有期限的证明，会话容量不足另走重置。取消会使待办失效。签名准备、授权、一次性消费和释放还有独立生命周期。 | `AuthenticationCoordinator` 管操作与恢复目的，系统提示和 Keystore 句柄分别管理 Activity、取消和硬件能力。[AuthenticationCoordinator]、[NativePasskeys]、[PasskeyWire]、[PresencePrompts]、[AndroidSecurity] |
| 账户、设备加入与恢复 | 混合 | 选择已存工作区 → 锁定后切换或登出。新设备生成绑定或配对请求 → 扫码、粘贴或传递邀请 → 轮询、手动检查、要求重新登录或获准。恢复按原生状态开放请求和完成动作。 | 设备授权页区分「已送交」与「已授权」。账户安全策略和硬件策略各自保留未提交输入。[AccountWorkspaceCoordinator]、[AccountCoordinator]、[DeviceAuthorizationScreen]、[SecurityPages]、[AccountScreens] |
| 设备保护策略与延迟恢复 | 混合 | 读取设备、硬件及恢复策略 → 编辑本机策略草稿或核对登记请求 → 提交审批 → 重新读取。账户恢复另有发起、等待、核对指定请求与完成路径。 | 两个 dirty 标记分别保护未提交输入。恢复入口会核对请求摘要和实际权限。[AccountCoordinator]、[SecurityPages] |
| 删除、移开或恢复工作区 | 混合 | 本机移除先读仅本地数据，再确认当前解锁账户。服务端注销需新鲜依赖预览、账户文本及钱包审查，再提交或继续既有任务。移开的旧设备副本有单独的处置入口。 | 三种操作作用范围不同。核对预览摘要、请求身份和实际任务结果。[AccountWorkspaceScreens]、[AccountWorkspaceCoordinator]、[AccountDeletionCoordinator]、[AccountDeletionScreen]、[HandoverCoordinator] |
| 工作区导航与重建 | 混合 | 账户激活 → 切换工作区或压入详情 → 返回、恢复选中项和滚动位置。切换账户后使用其自己的栈和位置。宽屏的列表与详情共享路由。 | 导航栈和可保存字段的边界在 `save/restore`。预测返回在手势完成时生效。[WorkspaceNavigation]、[DMsgApp]、[WorkspaceScroll]、[AdaptiveWorkspace] |
| 工作区搜索 | 显式 | 点按或用键盘激活搜索框后接替操作栏。清空文字继续输入，提交后收起键盘查看结果，取消后恢复浏览并保留筛选和排序。系统返回先收键盘，再退出搜索。 | 页面持有搜索展开状态，查询沿用各列表的数据来源。详情返回时恢复查询和列表位置。[WorkspaceBrowser]、[WorkspaceSearchField]、[WorkspaceScroll] |
| 外部导航仲裁 | 隐式 | 收到快捷入口、频道邀请、浏览器连接或通知 → 新意图取代旧意图 → 等启动、解锁及匹配账户 → 展示一次、关闭或完成。系统分享另走收件流程。 | `navigationIntent`、待办记录及已展示账户共同约束迟到结果。通知需先解析目标。[MainActivity]、[ClientController]、[WorkspaceLink]、[NoticeNavigation]、[WorkspaceEntries]、[WorkspaceEntriesScreen] |
| 浏览器连接 | 混合 | 定位邀请 → 核对发起源和目标账户 → 选择候选并请求或查看审批 → 从原生快照读取已配对、关闭、撤销或到期。 | 深链、进入详情、用户批准是不同事件。[BrowserConnectionsCoordinator]、[BrowserConnectionScreens]、[ApprovalCoordinator] |
| 网络、同步与提示流 | 混合 | 前台合格会话 → 启动内容同步和 hints 两个 Job → 网络变化重连、约束变化重启或手动同步 → 停止并取消旧订阅。频道收到提示还会合并后续刷新。 | `owner/sync/hints/connected` 是 Android 驱动状态。内容进度与重试调度取自 Rust。[SyncCoordinator]、[ForegroundEnvironment]、[SyncScreen]、[MessageCoordinator] |
| 二维码输入 | 本次显式化 | `Idle → CAMERA`，可进入 `CAMERA_PROMPTED`。相机识别直接产出 `QrResult`，选图则经 `PICKING_IMAGE → READING_IMAGE → QrResult`。取消、锁定或消费结果回到空闲。 | 本次显式化。选择器绑定请求 ID，来源、会话及接收目标均匹配才能交付一次。涵盖配对、频道邀请、秘密包、交接和 OTP。扫码本身只填写输入。[QrCoordinator]、[DMsgApp]、[QrCodeViews]、[QrImages] |

## 保险库、秘密与审批

| 行为 | 表示 | 触发事件和转换 | 所有者、约束与源码 |
| --- | --- | --- | --- |
| 浏览、选择与编辑器退出 | 隐式 | 搜索和筛选 → 选择条目 → 创建或编辑 → 返回、保留、丢弃或保存。脏表单关闭时先确认。窄屏与双栏使用相同的条目和草稿。 | 列表请求、选中条目、编辑器展示和退出确认各有所有者。[VaultCoordinator]、[VaultBrowseScreen]、[VaultScreens]、[DMsgApp] |
| 保险库字段与模板 | 隐式 | 新条目选择模板，保留已改字段。已有秘密默认遮挡 → 保持原值、显式替换或清空。校验、密码生成器、OTP 扫码及文件选择写回当前条目和字段。 | `DraftField.edit` 以 `concealed && !changed` 产生保留原值的 `null`，与用户实际提交空字符串有不同含义。[VaultDrafts]、[VaultScreens] |
| 草稿恢复与写入 | 隐式 | 读取现有草稿 → 按原始版本打开 → 修订并延迟写入 → 确认特定修订，或保持待写 → 提交或丢弃。后台可暂停加载，重建后继续。 | `pending/recovered/selected/afterLoad` 不等同于当前可见表单。成功消费的草稿由原生日志防止复活。[VaultDrafts]、[ClientController] |
| 秘密显示、OTP 与剪贴板 | 隐式 | 隐藏 → 显式查看、放大或复制。普通秘密可手动隐藏并在字段离开时清理。OTP 按原生剩余时长刷新，剪贴板到期只清理仍由本应用拥有的内容。 | 字段、版本、查看期限和剪贴板 nonce 分别核对。页面的计时器不能延长系统授权。[VaultDetailScreen]、[SecretLifetime]、[PrivateClipboard] |
| 历史版本与冲突 | 混合 | 读历史、分页 → 选择版本、比较 → 确认还原。冲突按当前 head 的修订逐项选边 → 合并或保留一方 → 刷新。 | 原始编辑版本、历史详情和当前详情分开。head 改变时旧的冲突选择过期。[VaultHistoryCoordinator]、[VaultHistoryScreen] |
| 密码库批量导入 | 隐式 | 选源 → 暂存并预览 → 选择记录 → 冻结所选索引并导入 → 获得条目 ID 或重试原选择。取消预览可重新预览，明确失败可继续改选。 | `VaultImportForm.handle/selected/pending/completed` 与预览摘要配对。原始文件不进入 Compose 保存状态。[VaultImportCoordinator]、[VaultImportScreen]、[PrivateFiles] |
| 字段引用、附件、健康与轮换 | 混合 | 打开受限引用或附件 → 查看健康发现 → 忽略或创建轮换 → 记录步骤、重新核对依赖和修订 → 完成。活动页按游标继续读取。 | Android 选目标、展示证据和动作，实际轮换阶段由原生记录决定。[SecretToolsCoordinator]、[SecretToolsScreens] |
| 秘密快照发送 | 混合 | 选择源版本、字段、附件及目的地 → 准备快照 → 按原作业申请审批 → 核对结果。查看链接需另行请求，隐藏、撤销和刷新有独立动作。 | 源表单身份、作业和当前可见链接分别保留。审批复用原请求。[SecretSharingCoordinator]、[SecretSharingScreens]、[SecretReview] |
| 秘密包接收 | 混合 | 输入链接、频道消息或密码 → 检查预览 → 固定导入选择 → 请求审批 → 完成后读取实际导入 ID，或按终态解冻。 | 修改输入使预览失效。执行结果未知时保留原选择。[SecretSharingCoordinator]、[SecretSharingScreens] |
| 审批生命周期 | 混合 | 发起操作 → 保存请求回执 → 打开详情并核对摘要 → 批准或拒绝 → 重新读取、对账，终态才移除对应回执。 | 详情摘要改变会重新计算可操作时间，确认时还要检查到期时间、当前请求及新鲜详情。`decide/reconcile/settle` 核对原生请求 ID 和 digest。[ApprovalCoordinator]、[AccountScreens]、[SecretReview] |

## 会话消息、来信与提醒

| 行为 | 表示 | 触发事件和转换 | 所有者、约束与源码 |
| --- | --- | --- | --- |
| 频道目录、时间线和搜索 | 混合 | 选择频道或新查询 → 读本地页 → 刷新或翻旧页。正在刷新时收到提示，只追加一轮。切换查询使旧目录结果失效。定位引用另有可取消的读取。 | 频道、查询、分页游标、目录修订和请求号共同确定结果归属。[MessageCoordinator]、[MessageScreens] |
| 消息输入与发件箱 | 混合 | 恢复编辑器 → 修改正文、回复或附件 → 按作者修订保存 → 冻结一次发送输入 → 本地排队 → 重试既有发件或继续新输入。 | `DraftLedger` 只确认对应修订，`pendingQueues` 保留结果未知的原发送意图。[DraftLedger]、[MessageCoordinator]、[MessageScreens] |
| 成员显示、已读与消息附件 | 隐式 | 可见行订阅资料或附件 → 合并读取、限流下载、进度、已验证本地副本或错误 → 取消、释放行。频道已读进度只推进到可见位置。 | 资料按可见账户，附件按频道、消息序号和操作所有者，通知观察另有版本。[ParticipantCoordinator]、[ParticipantViews]、[MessageAttachmentsCoordinator]、[MessageAttachmentRow]、[MessageScreens] |
| 频道创建、加入与管理 | 混合 | 填写新频道或检查特定接收人的邀请 → 保留原创建请求或加入 → 刷新。成员变更、邀请发布、名称修改、静音和隐藏各自有进度或确认。 | 创建或控制请求可从原生未完成记录恢复。角色与到期条件决定可执行的动作。[ChannelCoordinator]、[ChannelScreens] |
| 来信、信件草稿与反馈 | 混合 | 收件箱、草稿、待发和历史切换。按信件 ID 编辑 → 保存或排队 → 重试、检查投递、编辑未发送内容。反馈另按报告 ID 和修订保存截图、发送或丢弃。 | 正文保存、消息送达、标已读和归档各有回执。编辑期间迟到的读取不应覆盖本人输入。[LetterCoordinator]、[InboxScreen]、[FeedbackCoordinator]、[FeedbackScreen] |
| 封锁、收信规则与举报 | 混合 | 本地封锁后同步。收信设置从已观察值生成草稿，冻结待提交输入。举报选择对象、理由与披露内容 → 预览 → 提交同一报告或重试。 | 本地过滤立即可用。缓存缺失、已观察到关闭、同步失败有不同含义。[SafetyCoordinator]、[SafetyScreens] |
| 通知偏好 | 隐式 | 加载全局及账户设置 → 编辑、乐观呈现 → 串行保存，继续处理期间的新编辑 → 确认、冲突或重试。静默时段、暂停和系统权限还参与实际投递。 | `saved/dirty/saving`、各自的版本和错误决定下一次保存。[NotificationCoordinator]、[NotificationModels]、[NotificationScreens] |
| 提醒的导航与观察 | 混合 | 解析不透明路由或打开本地提醒 → 检查账户、是否已结束 → 必要时选账户 → 跳转一次。可见业务视口达到目标后才报告观察。标已读、稍后提醒和清理最近记录为独立动作。 | `intention/owner/routed` 管导航，`viewports/observing` 管可见性，观察按提醒版本核对。[NoticeNavigation]、[NoticeDestinationDialog]、[NotificationCoordinator]、[ReminderVisibility]、[NotificationScreens] |
| 前台横幅与系统通知 | 混合 | 取得可呈现批次 → 按窗口、类别、静默策略和现有横幅取舍 → 呈现或抑制并报告 → 到期、打开或关闭。远端通知按安装与租约去重，前台可转为刷新提示。 | 入口及回执分属应用、推送接收器、系统通知和 Rust。展示横幅不会替用户读消息或批准请求。[NoticePresenter]、[NoticeDelivery]、[NoticePresentation]、[RemoteNoticeReceiver]、[RemoteNotice]、[SystemNotices] |
| 推送注册与续租 | 混合 | 独立同意、系统许可和配置就绪 → 获取 token/Integrity 证明 → 注册租约 → 刷新、延期重试、停用或忘记账户。无 Google 服务变体返回能力不可用。 | 租约和偏好序号防止旧注册覆盖新选择。SDK 回调与前台协调器可能分别唤醒应用。[PushCoordinator]、[PushDeliveryStore]、[gms/GoogleMessagingService]、[gms/GooglePush]、[none/GooglePush]、[GoogleConfiguration] |

## 文件、传输与系统集成

| 行为 | 表示 | 触发事件和转换 | 所有者、约束与源码 |
| --- | --- | --- | --- |
| 浏览、元数据、版本与回收站 | 隐式 | 切目录、检索、选择文件或版本 → 查看或编辑 → 保存、移动、恢复、删除或确认永久清理。元数据基准改变产生冲突，保留草稿供重新核对。 | 列表按目录和回收站保存选择与位置。编辑、历史和文件预览各自持有版本。[FileCoordinator]、[FileBrowserScreen]、[FilesScreen]、[FileMetadataEditor]、[Models] |
| 批量文件动作 | 隐式 | 多选 → 核对目标和目的目录 → 逐项执行 → 收集成功与失败，取消时保留已完成项。 | 文件或文件夹 ID 决定每行结果。失败项可继续选择。[FileCoordinator]、[FileBrowserScreen] |
| 文档、照片和相机入口 | 隐式 | 先记录目标目录、可选频道和会话 → 权限或系统选择器 → 收到 URI、取消或延迟交付 → 导入、追加附件或清理相机临时文件。 | 选择器注册在隐私遮挡层之外。锁定撤销临时授权，回调不得改投新账户。[FileTransferCoordinator]、[FilePickerHost]、[FileIntakeButton]、[PrivateFiles] |
| 本机预览、系统分享和导出 | 隐式 | 选择已验证本地版本 → 按 nonce 打开副本 → 限定大小地解码、分享或复制到用户授权目的地 → 关闭、取消或删除未完成导出。 | 进度任务可中断。预览和缩略图按会话与文件版本失效。[FilePreviewCoordinator]、[FilePreviewScreen]、[FileThumbnail]、[FileTransferCoordinator]、[PrivateFiles]、[BoundedImages] |
| 接收入站分享与拖放 | 隐式 | 接收文本或 URI、取得临时读权 → 加密暂存、去重 → 待审查 → 指定账户和目录 → 显式导入或丢弃。导航意图可以推迟自动打开待审页。 | `needsReview/claim/receipts` 存于接收区。拖放授权和打开的流均有释放路径。[IncomingShareCoordinator]、[IncomingShareStore]、[IncomingSharesScreen]、[FileDropTarget]、[IncomingKeys] |
| 备份清单与批量准入 | 隐式 | 选择具体文件版本 → 核对容量和版本清单 → 逐项申请排队 → 保留未准入项及原因。云端库存、已排队、已完成验证分开显示。 | `BackupBatch.review/outcomes` 绑定已核对的版本。删除云备份从原生快照刷新。[CloudFilesCoordinator]、[CloudFilesScreen] |
| 前台文件队列 | 混合 | 合格前台会话启动 watcher → 查看原生上传或下载阶段 → 执行一轮、记录退避 → 手动暂停、网络变化唤醒或继续。 | 原生日志可跨进程，`watch/owner/paused/attempts` 只调度本机工作。交给系统的任务不应被前台重复推进。[FileWorkCoordinator]、[BackgroundFileCoordinator]、[TransferScreen] |
| 后台密文传输 | 本次显式化 | 准备分块以 `paused` 和 `PREPARING_CIPHERTEXT` 落盘。调度进入 `transferring`，暂停保留进度，非暂时故障或重试耗尽进入 `failed`。全部分块完成或下载计划本来就为空时进入 `verifying`，原账户解锁核验后清除。 | 本次显式化 `CipherPhase`，磁盘仍使用原来的四个小写值，未知值按完整性错误处理。停止通知、系统 Job 和存储写入核对计划 ID 与 attempt。暂停旧尝试不能取消重试后的 Job。[CipherTransferStore]、[CipherTransferService]、[BackgroundFileCoordinator]、[BackgroundTransferPanel] |
| 文件外链 | 混合 | 固定本机版本或备份来源 → 填选项 → 得到可持久恢复的发布意图 → 执行、暂停、续传 → 已发布、丢弃或撤销。 | 表单身份决定是否跳页，进度按意图 ID 和本轮 owner。离开表单不抹掉已准备的原生意图。[FileLinksCoordinator]、[FileLinkScreens] |
| 文件请求与收件 | 混合 | 固定收件人或批次 → 准备并逐份发布 → 暂停或重试原批次。收到的文件先展示状态，再显式保存到选定目录，也可取消保存、删除或关闭请求。 | 发布用 batch，保存用收到的文件 ID。`received` 和 `saved` 有不同动作。[FileRequestsCoordinator]、[FileRequestScreens] |
| 设备间的收发会话 | 混合 | 选择收或发 → 准备计划，发现对端或使用手动地址 → 等待、运行、结束、失败或取消。后台停止 socket 和广播。接收可重开原会话，发送须再次核对授权。 | `plan/owner/sequence/suspended/pending` 配合原生传输状态。NSD 注册、发现、解析和释放有独立回调。[DeviceTransferCoordinator]、[DeviceDiscovery]、[DeviceTransferScreen] |
| 系统导出快照 | 隐式 | 工作区解锁或登录项变化时更新本机登录项目录；用户选择文件后另行发布限时文件副本。两者在发布前核对会话和当前版本。 | 自动填充通过独立身份验证后显示可搜索的登录项。数据、密钥和当前指针的清理由 store 管，设置页只显示已发布的状态。[SystemExportsCoordinator]、[SystemExportStore]、[ExportKeyVault]、[SystemExportsScreen] |
| 自动填充与系统文件访问 | 隐式 | 系统建议绑定应用、字段和来源。选择器在两分钟无操作后关闭，解密密钥独立限时。连续浏览期间认证到期或目录刷新，重新认证后可继续填入已确认的同一账号。文件入口另授权短时文件会话，每次读都核对快照、设备锁和期限。 | 已变更的账号需重新选择，记住账号仍需核对目标。系统可在同一表单复用建议，返回的密码只供本次使用。关闭文件描述符后不可继续读取。[DMsgAutofillService]、[AutofillTrust]、[ExportAccessActivity]、[ExportFileAccess]、[ExportDocumentsProvider] |

## 账户资料、交接、Agent、钱包与购买

| 行为 | 表示 | 触发事件和转换 | 所有者、约束与源码 |
| --- | --- | --- | --- |
| 公开资料、头像与发布 | 隐式 | 缓存回显 → 获取新快照 → 编辑、选择或移除照片 → 暂存上传并提交 → 已发布、冲突或结果未知。冲突时可保留输入并重新核对，未知时重查发布记录。 | `expected/edited/uploaded/uncertain` 分别对应资料基准、草稿和发布操作。迟到头像须匹配当前表单。[ProfileCoordinator]、[ProfileImages]、[AccountWorkspaceScreens] |
| 设备交接与迁移副本 | 混合 | 新设备生成请求 → 旧设备核对完整设备及能力 → 申请审批 → 逐步处理授权、审批密钥、联签者、内容、钱包、原生地址与撤销。可跳过的步骤记录遗留项，取消后不能继续执行。 | 新旧设备视角、审核过的 token、步骤证据和移动副本各有状态。仅有 `settled` 还可能表示跳过。[HandoverCoordinator]、[HandoverScreens] |
| 托管 Agent 与设备权限 | 混合 | 创建、暂停、恢复或移除 Agent。设备权限草稿 → 申请审批 → 查看有效、过期或撤销的 grant。 | 草稿按 Agent ID 区分，本地界面不能自行使授权生效。[AgentCoordinator]、[AgentScreens] |
| Principal、控制器与 delegation | 混合 | 读取 principal → 在具备权限时启用或发布。登记、退役控制器 → 填写凭据对象和权限 → 申请授予、查看状态或撤销。 | 缺失、读取失败和已注册是不同条件。底层协议决定凭据有效性。[AgentCoordinator]、[AgentScreens]、[ApprovalCoordinator] |
| 钱包读取与转账预览 | 混合 | 选钱包、账户和资产 → 读缓存与新余额 → 修改金额、接收方或手续费 → 预览 → 在有效期内提交。 | 角色、精度、阻断项、费用上限和预览期限共同控制动作。改输入使旧预览失效。[WalletCoordinator]、[WalletModels]、[WalletScreens] |
| 钱包提案、控制变更与联签 | 混合 | 新建提交身份 → 请求提案 → 成功拿到提案号，或查询本地 journal 再决定能否新建 → 审批、等待、执行。联签包另有导入、签名收集与最终提交路径。 | `WalletSubmission` 保留未知结果对应的 request。权限、门限、延迟与链上状态来自原生记录。[WalletCoordinator]、[WalletControlsScreens]、[WalletScreens] |
| 钱包设备密钥与地址迁移 | 混合 | 创建和测试本地联签密钥 → 注册绑定。原生密钥可增换保护器或解绑。地址搬迁按来源、目的、冷却、待办提案和清扫步骤继续。 | 硬件成功、本机绑定和提案成功分别核对。迁移表单只推进当前可执行步骤。[WalletCoordinator]、[WalletModels]、[WalletKeyScreens] |
| 会员权益、目录与用量 | 混合 | 先读权益缓存，再刷新。权益是否新鲜、套餐目录是否可用、云空间和执行用量是否分别读到，独立决定展示。 | 缺少权益证据时保留未知。购买结果通过权益刷新体现。[MembershipCoordinator]、[MembershipScreens] |
| Play 报价与结账 | 混合 | 加载目录和商店报价 → 选择特定 offer → 恢复未结交易、预检和证明 → 系统确认或取消 → 待验证、已交付或失败。选择、账户或前台状态改变会使准备结果失效。 | `PurchasePhase` 已显式。`reviewRevision`、selected、受益账户、SDK 的 launchOwner 和未结账项决定能否实际启动。[PlayPurchaseCoordinator]、[PlayPurchasePanel]、[PlayModels]、[PlayBillingGateway]、[gms/GooglePlayBilling]、[none/GooglePlayBilling] |
| 购买回执、恢复与权益交付 | 混合 | 收到或恢复购买 → 加密入账 → 绑定已知账户或保留待认领 → 送验、记录提交时间 → 对账、等待确认或收尾。退款、到期、账户不匹配有各自结果。 | 收到 SDK 回调和会员权益交付相互独立。进程重开从本地账簿继续。[PlayPurchaseStore]、[PlayPurchaseKeys]、[PlayPurchaseCoordinator] |

## 屏幕局部与平台生命周期

| 行为 | 表示 | 触发事件和转换 | 所有者、约束与源码 |
| --- | --- | --- | --- |
| 表单、选择器与确认框 | 隐式 | 打开菜单或选择器 → 草拟动作或目标 → 确认、返回或丢弃。具体实例包括文件批操作和收件目录、频道成员、设备/Agent 撤销、报告、系统导出、钱包冻结和保险库字段编辑。 | 一个独立布尔值足以表达单个弹窗开关。涉及对象或版本的确认需保留当时选中的对象。入口见各业务行。通用选择、开关和搜索框由调用方持有值。[SelectionControls]、[SettingToggle]、[WorkspaceSearchField]、[WalletControlsScreens] |
| 语言、公开信息与布局 | 隐式 | 选择跟随系统或固定语言 → 应用配置，必要时重建 Activity 并刷新系统标签。公开信息可在关于和隐私间返回。隐私正文按语言经历加载、成功或失败。窗口变化会在单、双栏间切换。输入聚焦和键盘变化触发滚动。 | 语言偏好跨重建。页面查询、焦点和弹窗的寿命由各自的 Compose 调用点决定。[AppLanguage]、[LanguagePicker]、[AboutScreens]、[AdaptiveWorkspace]、[InputVisibility] |
| 全局提示与局部内容加载 | 隐式 | 错误或通知触发 Snackbar → 新消息替换等待中的展示 → 展示结束清除。缩略图、头像和二维码随来源变化重新加载，失败保留各自的替代操作。 | 展示时长和读取任务属于 UI。业务状态取自原协调器，后台提示不能充当操作回执。[DMsgApp]、[FileThumbnail]、[ParticipantViews]、[QrCodeViews] |
| 系统入口与资源持有 | 隐式 | 固定快捷方式、请求小组件、由系统启动指定工作区。剪贴板、拖放 URI、临时副本、网络监听、相机分析器和硬件弹窗各有申请、使用、失效和释放路径。 | 系统回调需核对自己持有的标识。适配层负责清理。[WorkspaceEntries]、[WorkspaceEntriesScreen]、[PrivateFiles]、[ForegroundEnvironment]、[QrCodeViews]、[PresencePrompts] |
| 变体、生成物和测试宿主 | 参照 | GMS 与无 GMS 使用各自的能力实现。Gradle 选择环境、服务类型及构建类型，生成并校验绑定及原生库后打包。本地化工具在生成与核对模式间选择。QA Activity 的 ViewModel 创建独立样例工作区，重建复用实例，清除时关闭。 | 这些是构建或测试生命周期。QA 的预置解锁与回调不会使发行版获得对应能力。[QaWorkspaceActivity] [变体与原生任务](../app/build.gradle.kts) [本地化生成](../tools/sync-localizations.py) [通知资源生成](../tools/sync-notifications.py) |

此外，[Deployment] 选择部署，[BoundedInput]、[P256Signatures]、[LocalizedActions]、[Theme] 和 [VaultFieldNames] 处理输入或展示。循环游标、资源映射和一次纯函数调用没有独立的业务生命周期。测试源码另有 66 个 Kotlin 文件及 4 个 Java 夹具，覆盖入口还包括系统服务、GMS 初始化、布局和语言。测试文件数与通过的用例数分别统计。


## 可独立评审的下一步

优先复验浏览器启动失败后立即切后台的路径。它同时影响用户看到的错误与后台宽限判断，可以用一个会抛错的启动器验证。其余三项的测试条件列在表中。

| 场景 | 源码所示行为 | 选择方案前要复验的路径 |
| --- | --- | --- |
| 浏览器登录或绑定启动失败 | `login` 回调先置 `pendingLogin` 再调用宿主的 `open`。若打开浏览器抛错，通用操作报告错误，但等待浏览器标记仍在。下次后台判定会读到它。[ClientController]、[MainActivity] | 用可失败的浏览器启动器覆盖打开失败、用户取消和随即另启登录。若调整状态，保留绑定回调只能在原 generation、有效期内重试的约束。 |
| Passkey 容量恢复 | `limitedCredential`、`limitedOperation`、`attempt` 和 UI 的三个布尔值分别更新。重置成功后，控制器还会读取先前的凭据与原操作继续流程。[AuthenticationCoordinator]、[ClientController] | 分别复验登录和绑定遇到容量限制，以及重置取消后换凭据。用一个带原操作的数据对象表达恢复上下文前，先验证真实系统提示的取消顺序。 |
| 内容同步和 hints 订阅 | 两个 Job 独立退出，`start` 的提前返回只查看 `sync.isActive`。若 hints 已退出而同步仍活跃，再次 `start` 会直接返回。网络恢复路径调用原生重连。[SyncCoordinator] | 单独结束 hints 并恢复网络，观察原生监听能否自行恢复。再在同一会话切换同步约束，检查旧 observer 的迟到回调是否会覆盖新连接状态。 |
| 多种入口争用导航 | 浏览器连接、频道邀请、工作区链接、通知路由及本地提醒分别保存。`beginNavigationIntent` 和账户记录决定哪一个仍有效。[ClientController]、[NoticeNavigation] | 覆盖旧偏好恢复、锁屏中连续两个入口、选账户时被新入口取代。以后合并意图类型时，应兼容既有偏好字段并保留读取提醒与实际导航的区别。 |

## 回归入口

- 修改扫码入口时，先跑 `WorkspaceFlowTest.qrAcceptsOnlyTheChosenSourceAndDeliversItsResultOnce` 和 `deniedCameraRetainsPasteAndDoesNotCreateAnApproval`。前者交错投递旧选图、相机帧和新图片，检查只有选定来源能填回、只能消费一次，图片超限仍保留对应的错误提示。后者走系统权限窗口。[工作区设备用例](../app/src/androidTestQa/java/net/dmsg/android/WorkspaceFlowTest.kt)
- 修改传输阶段时，先跑 `CipherTransferStoreTest`、`WorkspaceFlowTest.anOldNotificationPauseCannotCancelTheRetriedTransfer` 和 `expiredCipherGrantStopsInTheRealSystemJobWithoutOpeningOrFinishingAFile`。覆盖磁盘已有阶段值、暂停后重试、迟到回调、过期授权，以及待核验与传输完成的区别。[传输单元测试](../app/src/test/java/net/dmsg/android/platform/CipherTransferStoreTest.kt) [系统 Job 用例](../app/src/androidTestQa/java/net/dmsg/android/WorkspaceFlowTest.kt)
- 涉及跨账户、提醒、UI 重建或购买时，可从 `SessionFenceTest`、`NoticeNavigationTest`、`VaultDraftRecreationTest`、`PlayPurchaseStoreTest` 和 `PlayPurchaseUiTest` 找到独立的驱动入口。真实 Play 交付、外部身份提供方和硬件成功路径需要对应环境。[JVM 测试](../app/src/test/java/net/dmsg/android/) [QA 设备测试](../app/src/androidTestQa/java/net/dmsg/android/) [GMS 专属测试](../app/src/androidTestGms/java/net/dmsg/android/)

本次在 API 36 ARM64 模拟器运行了标准版 86 项、GMS 版 8 项设备用例。两种变体各通过 114 项 JVM 单元测试，两个 Release 变体通过 Kotlin 编译。真实硬件认证、线上购买和远端推送送达仍需相应环境。

[AboutScreens]: ../app/src/main/java/net/dmsg/android/ui/AboutScreens.kt
[AccessScreen]: ../app/src/main/java/net/dmsg/android/ui/AccessScreen.kt
[AccountCoordinator]: ../app/src/main/java/net/dmsg/android/core/AccountCoordinator.kt
[AccountDeletionCoordinator]: ../app/src/main/java/net/dmsg/android/core/AccountDeletionCoordinator.kt
[AccountDeletionScreen]: ../app/src/main/java/net/dmsg/android/ui/AccountDeletionScreen.kt
[AccountScreens]: ../app/src/main/java/net/dmsg/android/ui/AccountScreens.kt
[AccountWorkspaceCoordinator]: ../app/src/main/java/net/dmsg/android/core/AccountWorkspaceCoordinator.kt
[AccountWorkspaceScreens]: ../app/src/main/java/net/dmsg/android/ui/AccountWorkspaceScreens.kt
[AdaptiveWorkspace]: ../app/src/main/java/net/dmsg/android/ui/AdaptiveWorkspace.kt
[AgentCoordinator]: ../app/src/main/java/net/dmsg/android/core/AgentCoordinator.kt
[AgentScreens]: ../app/src/main/java/net/dmsg/android/ui/AgentScreens.kt
[AndroidSecurity]: ../app/src/main/java/net/dmsg/android/platform/AndroidSecurity.kt
[AppLanguage]: ../app/src/main/java/net/dmsg/android/platform/AppLanguage.kt
[ApprovalCoordinator]: ../app/src/main/java/net/dmsg/android/core/ApprovalCoordinator.kt
[AuthenticationCoordinator]: ../app/src/main/java/net/dmsg/android/core/AuthenticationCoordinator.kt
[AutofillTrust]: ../app/src/main/java/net/dmsg/android/platform/AutofillTrust.kt
[BackgroundFileCoordinator]: ../app/src/main/java/net/dmsg/android/core/BackgroundFileCoordinator.kt
[BackgroundTransferPanel]: ../app/src/main/java/net/dmsg/android/ui/BackgroundTransferPanel.kt
[BoundedImages]: ../app/src/main/java/net/dmsg/android/platform/BoundedImages.kt
[BoundedInput]: ../app/src/main/java/net/dmsg/android/platform/BoundedInput.kt
[BrowserConnectionScreens]: ../app/src/main/java/net/dmsg/android/ui/BrowserConnectionScreens.kt
[BrowserConnectionsCoordinator]: ../app/src/main/java/net/dmsg/android/core/BrowserConnectionsCoordinator.kt
[ChannelCoordinator]: ../app/src/main/java/net/dmsg/android/core/ChannelCoordinator.kt
[ChannelScreens]: ../app/src/main/java/net/dmsg/android/ui/ChannelScreens.kt
[CipherTransferService]: ../app/src/main/java/net/dmsg/android/platform/CipherTransferService.kt
[CipherTransferStore]: ../app/src/main/java/net/dmsg/android/platform/CipherTransferStore.kt
[ClientController]: ../app/src/main/java/net/dmsg/android/core/ClientController.kt
[CloudFilesCoordinator]: ../app/src/main/java/net/dmsg/android/core/CloudFilesCoordinator.kt
[CloudFilesScreen]: ../app/src/main/java/net/dmsg/android/ui/CloudFilesScreen.kt
[DMsgApp]: ../app/src/main/java/net/dmsg/android/ui/DMsgApp.kt
[DMsgApplication]: ../app/src/main/java/net/dmsg/android/DMsgApplication.kt
[DMsgAutofillService]: ../app/src/main/java/net/dmsg/android/platform/DMsgAutofillService.kt
[Deployment]: ../app/src/main/java/net/dmsg/android/core/Deployment.kt
[DeviceAuthorizationScreen]: ../app/src/main/java/net/dmsg/android/ui/DeviceAuthorizationScreen.kt
[DeviceDiscovery]: ../app/src/main/java/net/dmsg/android/platform/DeviceDiscovery.kt
[DeviceTransferCoordinator]: ../app/src/main/java/net/dmsg/android/core/DeviceTransferCoordinator.kt
[DeviceTransferScreen]: ../app/src/main/java/net/dmsg/android/ui/DeviceTransferScreen.kt
[DraftLedger]: ../app/src/main/java/net/dmsg/android/core/DraftLedger.kt
[ExportAccessActivity]: ../app/src/main/java/net/dmsg/android/platform/ExportAccessActivity.kt
[ExportDocumentsProvider]: ../app/src/main/java/net/dmsg/android/platform/ExportDocumentsProvider.kt
[ExportFileAccess]: ../app/src/main/java/net/dmsg/android/platform/ExportFileAccess.kt
[ExportKeyVault]: ../app/src/main/java/net/dmsg/android/platform/ExportKeyVault.kt
[FeedbackCoordinator]: ../app/src/main/java/net/dmsg/android/core/FeedbackCoordinator.kt
[FeedbackScreen]: ../app/src/main/java/net/dmsg/android/ui/FeedbackScreen.kt
[FileBrowserScreen]: ../app/src/main/java/net/dmsg/android/ui/FileBrowserScreen.kt
[FileCoordinator]: ../app/src/main/java/net/dmsg/android/core/FileCoordinator.kt
[FileDropTarget]: ../app/src/main/java/net/dmsg/android/ui/FileDropTarget.kt
[FileIntakeButton]: ../app/src/main/java/net/dmsg/android/ui/FileIntakeButton.kt
[FileLinkScreens]: ../app/src/main/java/net/dmsg/android/ui/FileLinkScreens.kt
[FileLinksCoordinator]: ../app/src/main/java/net/dmsg/android/core/FileLinksCoordinator.kt
[FileMetadataEditor]: ../app/src/main/java/net/dmsg/android/ui/FileMetadataEditor.kt
[FilePickerHost]: ../app/src/main/java/net/dmsg/android/ui/FilePickerHost.kt
[FilePreviewCoordinator]: ../app/src/main/java/net/dmsg/android/core/FilePreviewCoordinator.kt
[FilePreviewScreen]: ../app/src/main/java/net/dmsg/android/ui/FilePreviewScreen.kt
[FileRequestScreens]: ../app/src/main/java/net/dmsg/android/ui/FileRequestScreens.kt
[FileRequestsCoordinator]: ../app/src/main/java/net/dmsg/android/core/FileRequestsCoordinator.kt
[FileThumbnail]: ../app/src/main/java/net/dmsg/android/ui/FileThumbnail.kt
[FileTransferCoordinator]: ../app/src/main/java/net/dmsg/android/core/FileTransferCoordinator.kt
[FileWorkCoordinator]: ../app/src/main/java/net/dmsg/android/core/FileWorkCoordinator.kt
[FilesScreen]: ../app/src/main/java/net/dmsg/android/ui/FilesScreen.kt
[ForegroundEnvironment]: ../app/src/main/java/net/dmsg/android/platform/ForegroundEnvironment.kt
[GoogleConfiguration]: ../app/src/main/java/net/dmsg/android/platform/GoogleConfiguration.kt
[HandoverCoordinator]: ../app/src/main/java/net/dmsg/android/core/HandoverCoordinator.kt
[HandoverScreens]: ../app/src/main/java/net/dmsg/android/ui/HandoverScreens.kt
[InboxScreen]: ../app/src/main/java/net/dmsg/android/ui/InboxScreen.kt
[IncomingKeys]: ../app/src/main/java/net/dmsg/android/platform/IncomingKeys.kt
[IncomingShareCoordinator]: ../app/src/main/java/net/dmsg/android/core/IncomingShareCoordinator.kt
[IncomingShareStore]: ../app/src/main/java/net/dmsg/android/platform/IncomingShareStore.kt
[IncomingSharesScreen]: ../app/src/main/java/net/dmsg/android/ui/IncomingSharesScreen.kt
[InputVisibility]: ../app/src/main/java/net/dmsg/android/ui/InputVisibility.kt
[LanguagePicker]: ../app/src/main/java/net/dmsg/android/ui/LanguagePicker.kt
[LetterCoordinator]: ../app/src/main/java/net/dmsg/android/core/LetterCoordinator.kt
[LocalizedActions]: ../app/src/main/java/net/dmsg/android/platform/LocalizedActions.kt
[MainActivity]: ../app/src/main/java/net/dmsg/android/MainActivity.kt
[MembershipCoordinator]: ../app/src/main/java/net/dmsg/android/core/MembershipCoordinator.kt
[MembershipScreens]: ../app/src/main/java/net/dmsg/android/ui/MembershipScreens.kt
[MessageAttachmentRow]: ../app/src/main/java/net/dmsg/android/ui/MessageAttachmentRow.kt
[MessageAttachmentsCoordinator]: ../app/src/main/java/net/dmsg/android/core/MessageAttachmentsCoordinator.kt
[MessageCoordinator]: ../app/src/main/java/net/dmsg/android/core/MessageCoordinator.kt
[MessageScreens]: ../app/src/main/java/net/dmsg/android/ui/MessageScreens.kt
[Models]: ../app/src/main/java/net/dmsg/android/core/Models.kt
[NativeBootstrap]: ../app/src/main/java/net/dmsg/android/core/NativeBootstrap.kt
[NativePasskeys]: ../app/src/main/java/net/dmsg/android/platform/NativePasskeys.kt
[NativeSession]: ../app/src/main/java/net/dmsg/android/core/NativeSession.kt
[NoticeDelivery]: ../app/src/main/java/net/dmsg/android/core/NoticeDelivery.kt
[NoticeDestinationDialog]: ../app/src/main/java/net/dmsg/android/ui/NoticeDestinationDialog.kt
[NoticeNavigation]: ../app/src/main/java/net/dmsg/android/core/NoticeNavigation.kt
[NoticePresentation]: ../app/src/main/java/net/dmsg/android/ui/NoticePresentation.kt
[NoticePresenter]: ../app/src/main/java/net/dmsg/android/core/NoticePresenter.kt
[NotificationCoordinator]: ../app/src/main/java/net/dmsg/android/core/NotificationCoordinator.kt
[NotificationModels]: ../app/src/main/java/net/dmsg/android/core/NotificationModels.kt
[NotificationScreens]: ../app/src/main/java/net/dmsg/android/ui/NotificationScreens.kt
[OperationOwners]: ../app/src/main/java/net/dmsg/android/core/OperationOwners.kt
[P256Signatures]: ../app/src/main/java/net/dmsg/android/platform/P256Signatures.kt
[ParticipantCoordinator]: ../app/src/main/java/net/dmsg/android/core/ParticipantCoordinator.kt
[ParticipantViews]: ../app/src/main/java/net/dmsg/android/ui/ParticipantViews.kt
[PasskeyWire]: ../app/src/main/java/net/dmsg/android/platform/PasskeyWire.kt
[PlayBillingGateway]: ../app/src/main/java/net/dmsg/android/platform/PlayBillingGateway.kt
[PlayModels]: ../app/src/main/java/net/dmsg/android/core/PlayModels.kt
[PlayPurchaseCoordinator]: ../app/src/main/java/net/dmsg/android/core/PlayPurchaseCoordinator.kt
[PlayPurchaseKeys]: ../app/src/main/java/net/dmsg/android/platform/PlayPurchaseKeys.kt
[PlayPurchasePanel]: ../app/src/main/java/net/dmsg/android/ui/PlayPurchasePanel.kt
[PlayPurchaseStore]: ../app/src/main/java/net/dmsg/android/platform/PlayPurchaseStore.kt
[PresencePrompts]: ../app/src/main/java/net/dmsg/android/platform/PresencePrompts.kt
[PrivateClipboard]: ../app/src/main/java/net/dmsg/android/platform/PrivateClipboard.kt
[PrivateFiles]: ../app/src/main/java/net/dmsg/android/platform/PrivateFiles.kt
[ProfileCoordinator]: ../app/src/main/java/net/dmsg/android/core/ProfileCoordinator.kt
[ProfileImages]: ../app/src/main/java/net/dmsg/android/platform/ProfileImages.kt
[PushCoordinator]: ../app/src/main/java/net/dmsg/android/core/PushCoordinator.kt
[PushDeliveryStore]: ../app/src/main/java/net/dmsg/android/platform/PushDeliveryStore.kt
[QaWorkspaceActivity]: ../app/src/qa/java/net/dmsg/android/QaWorkspaceActivity.kt
[QrCodeViews]: ../app/src/main/java/net/dmsg/android/ui/QrCodeViews.kt
[QrCoordinator]: ../app/src/main/java/net/dmsg/android/core/QrCoordinator.kt
[QrImages]: ../app/src/main/java/net/dmsg/android/platform/QrImages.kt
[ReadFeedback]: ../app/src/main/java/net/dmsg/android/ui/ReadFeedback.kt
[ReadStore]: ../app/src/main/java/net/dmsg/android/core/ReadStore.kt
[ReminderVisibility]: ../app/src/main/java/net/dmsg/android/ui/ReminderVisibility.kt
[RemoteNotice]: ../app/src/main/java/net/dmsg/android/core/RemoteNotice.kt
[RemoteNoticeReceiver]: ../app/src/main/java/net/dmsg/android/platform/RemoteNoticeReceiver.kt
[SafetyCoordinator]: ../app/src/main/java/net/dmsg/android/core/SafetyCoordinator.kt
[SafetyScreens]: ../app/src/main/java/net/dmsg/android/ui/SafetyScreens.kt
[SecretLifetime]: ../app/src/main/java/net/dmsg/android/core/SecretLifetime.kt
[SecretReview]: ../app/src/main/java/net/dmsg/android/core/SecretReview.kt
[SecretSharingCoordinator]: ../app/src/main/java/net/dmsg/android/core/SecretSharingCoordinator.kt
[SecretSharingScreens]: ../app/src/main/java/net/dmsg/android/ui/SecretSharingScreens.kt
[SecretToolsCoordinator]: ../app/src/main/java/net/dmsg/android/core/SecretToolsCoordinator.kt
[SecretToolsScreens]: ../app/src/main/java/net/dmsg/android/ui/SecretToolsScreens.kt
[SecurityPages]: ../app/src/main/java/net/dmsg/android/ui/SecurityPages.kt
[SelectionControls]: ../app/src/main/java/net/dmsg/android/ui/SelectionControls.kt
[SessionPolicy]: ../app/src/main/java/net/dmsg/android/platform/SessionPolicy.kt
[SettingToggle]: ../app/src/main/java/net/dmsg/android/ui/SettingToggle.kt
[SyncCoordinator]: ../app/src/main/java/net/dmsg/android/core/SyncCoordinator.kt
[SyncScreen]: ../app/src/main/java/net/dmsg/android/ui/SyncScreen.kt
[SystemExportStore]: ../app/src/main/java/net/dmsg/android/platform/SystemExportStore.kt
[SystemExportsCoordinator]: ../app/src/main/java/net/dmsg/android/core/SystemExportsCoordinator.kt
[SystemExportsScreen]: ../app/src/main/java/net/dmsg/android/ui/SystemExportsScreen.kt
[SystemNotices]: ../app/src/main/java/net/dmsg/android/platform/SystemNotices.kt
[Theme]: ../app/src/main/java/net/dmsg/android/ui/Theme.kt
[TransferScreen]: ../app/src/main/java/net/dmsg/android/ui/TransferScreen.kt
[VaultBrowseScreen]: ../app/src/main/java/net/dmsg/android/ui/VaultBrowseScreen.kt
[VaultCoordinator]: ../app/src/main/java/net/dmsg/android/core/VaultCoordinator.kt
[VaultDetailScreen]: ../app/src/main/java/net/dmsg/android/ui/VaultDetailScreen.kt
[VaultDrafts]: ../app/src/main/java/net/dmsg/android/core/VaultDrafts.kt
[VaultFieldNames]: ../app/src/main/java/net/dmsg/android/ui/VaultFieldNames.kt
[VaultHistoryCoordinator]: ../app/src/main/java/net/dmsg/android/core/VaultHistoryCoordinator.kt
[VaultHistoryScreen]: ../app/src/main/java/net/dmsg/android/ui/VaultHistoryScreen.kt
[VaultImportCoordinator]: ../app/src/main/java/net/dmsg/android/core/VaultImportCoordinator.kt
[VaultImportScreen]: ../app/src/main/java/net/dmsg/android/ui/VaultImportScreen.kt
[VaultScreens]: ../app/src/main/java/net/dmsg/android/ui/VaultScreens.kt
[WalletControlsScreens]: ../app/src/main/java/net/dmsg/android/ui/WalletControlsScreens.kt
[WalletCoordinator]: ../app/src/main/java/net/dmsg/android/core/WalletCoordinator.kt
[WalletKeyScreens]: ../app/src/main/java/net/dmsg/android/ui/WalletKeyScreens.kt
[WalletModels]: ../app/src/main/java/net/dmsg/android/core/WalletModels.kt
[WalletScreens]: ../app/src/main/java/net/dmsg/android/ui/WalletScreens.kt
[Wire]: ../app/src/main/java/net/dmsg/android/core/Wire.kt
[WorkspaceBrowser]: ../app/src/main/java/net/dmsg/android/ui/WorkspaceBrowser.kt
[WorkspaceEntries]: ../app/src/main/java/net/dmsg/android/platform/WorkspaceEntries.kt
[WorkspaceEntriesScreen]: ../app/src/main/java/net/dmsg/android/ui/WorkspaceEntriesScreen.kt
[WorkspaceLink]: ../app/src/main/java/net/dmsg/android/core/WorkspaceLink.kt
[WorkspaceNavigation]: ../app/src/main/java/net/dmsg/android/core/WorkspaceNavigation.kt
[WorkspaceScroll]: ../app/src/main/java/net/dmsg/android/ui/WorkspaceScroll.kt
[WorkspaceSearchField]: ../app/src/main/java/net/dmsg/android/ui/WorkspaceSearchField.kt
[gms/GoogleMessagingService]: ../app/src/gms/java/net/dmsg/android/platform/GoogleMessagingService.kt
[gms/GooglePlayBilling]: ../app/src/gms/java/net/dmsg/android/platform/GooglePlayBilling.kt
[gms/GooglePush]: ../app/src/gms/java/net/dmsg/android/platform/GooglePush.kt
[none/GooglePlayBilling]: ../app/src/none/java/net/dmsg/android/platform/GooglePlayBilling.kt
[none/GooglePush]: ../app/src/none/java/net/dmsg/android/platform/GooglePush.kt
