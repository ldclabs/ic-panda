# dMsg 桌面端隐式状态机清单

桌面版 1.5.8 共识别出 **95 组包含隐式状态转换的运行时流程**，分布在 12 个领域。基线为本地 `ic-panda@2d0cd255`，核对日期 2026-11-18。检视范围包含 `src/dmsg_desktop/src/` 下全部 172 个 Svelte 文件和 133 个 TypeScript 文件，以及 `native/fskit/` 的 5 个 Swift 文件。测试、探针与打包入口见文末。

同一次输入、点击、回调或计时器，在不同的前置状态下会产生不同结果。如果要合读几个字段、一个尚未完成的 Promise、一张 Map、一个定时器或多个事件处理器，才能知道当前允许哪一步，这部分就是隐式状态机。每行按一项用户操作或一个资源的生命周期归并，可含多个子流程。共享实现只计一次，使用点独有的转换另行列出。已有显式阶段和仅映射远端状态的部分单独标明。反引号标出可搜索的源码标识。

## 已复现并修复的六项转换

以下行为均在本机 Electron 窗口中使用合成账户和模拟 daemon 复现。前 3 项在首次盘点时确认，后 3 项在修复前通过拒绝写入、延迟查询和邀请失败逐一确认。清单保留基线计数，源码入口和相关字段指向修复后的实现。

| 流程 | 1.5.8 基线行为 | 修复后的行为 |
| --- | --- | --- |
| 频道附件选择（C04） | 分别搜索并勾选 A 和 B，添加后草稿只剩 B。 | 以文件 ID 保存选中的附件。搜索和零匹配只改变候选列表，勾选、计数和提交使用同一份选择。关闭后开始新的选择。[LibraryPicker](../src/renderer/views/channels/LibraryPicker.svelte#L29) |
| 浏览器配对到期（D05） | 邀请过期后轮询停止，页面仍提示等待且旧候选可点。 | 候选读取、到期提示和提交检查使用同一截止时间。过期后可发起新邀请，只有新邀请的候选可以连接。[Settings](../src/renderer/views/Settings.svelte#L141) |
| 创建 Agent ID（G02） | daemon 拒绝创建后名称被清空，重试要重新输入。 | 动作返回明确结果。失败保留草稿，成功只清除实际提交的名称。提交途中写入的新名称保留。使用同一动作的能力授权也在失败后保留选择。[Agents.attempt](../src/renderer/views/Agents.svelte#L73) · [AgentIds.create](../src/renderer/views/agents/AgentIds.svelte#L60) |
| 反馈草稿（D10） | 保存被拒绝后，草稿仍在，但状态行仍显示“正在保存”。 | 仅实际写入时显示保存中。失败显示未保存并保留正文，重试沿用原 ID 和修订号。确认成功后才显示已保存。[保存队列](../src/renderer/feedback.svelte.ts#L57) · [反馈页](../src/renderer/views/Feedback.svelte#L106) |
| 文件搜索（F01、V02） | 在文件库和秘密库附件选择器中，新查询先成功、旧查询随后失败，新结果被清成空列表。 | 文件库及两个附件选择器共用搜索生命周期。换词、关闭和卸载会使上一轮失效。当前查询的加载、失败可重试和确实零匹配分别展示。[searchLibrary](../src/renderer/views/files/search.svelte.ts) · [SearchStatus](../src/renderer/views/files/SearchStatus.svelte) |
| 直接频道邀请（C05） | 创建成功但邀请失败，再点提交会第二次调用 `channel/create`。 | 创建回执固定频道 ID 和已解析的收件账户。可在当前对话框重试邀请，成功再展示可复制的链接。新建协作频道、分发频道与接受邀请各走原有入口。[NewChannel.submit](../src/renderer/views/channels/NewChannel.svelte#L83) |

这些操作可通过现有附件列表、状态文字、按钮可用性和 RPC 回执验证。测试同时核对用户看见的结果和提交时使用的 ID。[Electron 回归用例](../scripts/state-transitions-flow.mjs)

**按领域查找**

- [主进程与 FSKit](#主进程进程通信和-fskit) · [窗口与会话](#窗口会话与共享异步行为) · [通知与回执](#通知提醒与可见回执) · [账户、设备与设置](#账户设备与设置)
- [审批](#审批中心与独立审批窗口) · [频道与收件箱](#频道与收件箱) · [文件库](#文件库分享与收取文件) · [秘密库](#秘密库与凭证工作流)
- [会员、名称与支付](#会员名称与支付) · [团队钱包](#团队钱包) · [Agent 工作台](#agent-工作台) · [共享控件](#共享控件与短暂交互)

## 主进程、进程通信和 FSKit

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| M01 主窗口、审批窗口与退出<br>`hideAfterFullScreen` · `leavingFullScreen` · `quitting` | 窗口引用为空 → 创建并等待首帧 → 显示。关闭主窗口通常隐藏到托盘。全屏时先退出全屏，期间再次显示会取消待隐藏动作。审批窗口按请求 ID 创建或复用。显式退出时先结束剪贴板清理，再关闭连接。 | [showMain / showApproval](../src/main/index.ts#L251) · [before-quit](../src/main/index.ts#L1431) |
| M02 启动路由、桌面位置、托盘<br>`pendingLink` · `desktopTimer` · `trayState` | 启动时解析运行环境、检查 daemon 版本，初始化连接。ready 之前的 URL 暂存于 `pendingLink`，之后分发。运行中的深链接或第二实例复用现有窗口。路由、尺寸、最大化和首次关闭提示有“待写 / 已落盘”，有待写入的状态会在退出时落盘，下次启动校验屏幕范围。托盘图标由连接、锁定和待处理数投影。 | [主入口](../src/main/index.ts#L99) · [runtime](../src/main/runtime.ts) · [desktop-state](../src/main/desktop-state.ts) · [tray-state](../src/main/tray-state.ts) |
| M03 系统活动与离席<br>`reported` · `screenLocked` · `online` | 输入经 `reported` 节流报告活动。锁屏、睡眠按设置请求锁定。解锁屏幕恢复通知评估，唤醒或网络由离线变在线时通知 daemon 恢复工作。系统解锁和账户解锁是两个事件。 | [活动与电源事件](../src/main/index.ts#L1423) |
| M04 RPC 等待、合并与结算<br>`inFlight` · `waiting` · `timer` | 请求按方法和参数合并，按 RPC ID 保存等待者。回复、超时、连接丢失或会话变化决定结算。长文件操作有单独的超时规则。迟到回复查不到等待者即结束。写入结果未知时由业务日志对账。底层 socket 已有显式状态。 | [Daemon.call / requestOnce / receive](../src/main/daemon.ts#L188) · [请求分类](../src/shared/requests.ts) |
| M05 只读恢复与合并刷新<br>`failed` · `rounds` · `again` | 读失败入队 → 等待退避 → 逐个重读 → 清空或再等。可见的 `idle/recovering/offline` 背后还有 `running/timer/rounds/generation`。会话变化清旧队列。另一种机制 `single` 在读进行时记下最新参数，完成后补跑。 | [ReadRecovery](../src/main/recovery.ts) · [single](../src/shared/single.ts) |
| M06 原生选择、导入和系统临时副本<br>`systemCopies` · `folder` | 选文件、目录、图片、凭证或保存目标 → 取消或交 daemon 操作 → 报进度或结果。粘贴图片的临时文件在调用结束清理。系统预览或拖出的副本由 `systemCopies` 跟踪，到时、换会话或退出清理。长期打开的编辑副本另见 F04。 | [文件 IPC](../src/main/index.ts#L458) · [systemCopy](../src/main/index.ts#L511) · [preload 的文件桥](../src/preload/index.ts) |
| M07 剪贴板与 OTP 有效期<br>`copied` · `clearAfter` · `expiresAt` | 读取值 → 清前一次复制 → 写入并安排清除。到期或退出先核对剪贴板内容，尊重用户后来复制的其他内容。OTP 另受本次码的有效期约束。刷新发现账户未解锁时也会触发清理。 | [forgetCopied / copy-secret](../src/main/index.ts#L615) · [refreshNotices](../src/main/index.ts#L1078) |
| M08 全局快捷键注册与录制<br>`keys` · `saving` · `shortcutLocking` | 已注册集合 → 试注册新组合 → 持久化 → 释放旧组合。失败回滚本次新增项。录制期间暂停全局快捷键，失焦、隐藏、导航、进程消失等事件恢复。锁定动作另有单次在途标记。 | [Shortcuts.configure](../src/main/shortcuts.ts) · [窗口事件](../src/main/index.ts#L184) · [录制 IPC](../src/main/index.ts#L923) |
| M09 更新源、通道和下载<br>`candidate` · `generation` · `retryAt` | Feed 按 URL 缓存、ETag 重验、受限时等待。已有显式 update 状态。另由 `generation/abort/pending/work/candidate` 约束通道切换、串行下载与安装资格。设置中的通道草稿、保存错误和正在检查分别变化。 | [ReleaseFeed](../src/main/release-feed.ts) · [UpdateSession](../src/main/update-session.ts) · [关于页](../src/renderer/views/About.svelte) |
| M10 停 daemon 与安装<br>`ifIdle` · `prepared` · `resumeInstallation` | 手动安装 → 等待 daemon 空闲 → 超时后由人选择推迟或中断 → 确认已停止后安装。安装失败、后续 updater 错误和恢复运行各有出口。不支持自动安装的包走发布页。 | [stopForUpdate](../src/main/update.ts) · [Updates.apply](../src/main/updater.ts) · [applyUpdate](../src/main/index.ts#L1202) |
| M11 FSKit 卷、节点与读取窗口<br>`watching` · `closed` · `windows` | 文件系统加载有显式枚举。卷另用 `active/watching/jobs` 管理轮询和停止。节点的打开/关闭、版本失效、授权窗口到期、内核缓存撤销和回执是独立维度。关闭或停卷取消相应任务，旧节点不能因新窗口复活。 | [SecretVolume](../native/fskit/SecretsFS.swift#L107) · [open / close / read](../native/fskit/SecretsFS.swift#L286) |
| M12 FSKit 分帧 IPC<br>`expected` · `body` · `finished` | 建连、写请求 → 累积 4 字节长度头 → 按声明长度读 body → 完成。拒绝、断流、取消和超时均经 `finish` 一次结算、关闭句柄并释放缓冲区。 | [ContentRequest](../native/fskit/IPC.swift) |

## 窗口、会话与共享异步行为

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| S01 断线遮挡与会话核验<br>`connection` · `waited` · `stateRevision` | `verifying/disconnected/verified` 已显式。是否保留旧 view、400 ms 后是否显示等待、手动重连是否在途仍组合决定 UI。仅证实同一会话后恢复原表单。`session/changed` 立即拆旧视图，包括弹层。 | [App](../src/renderer/App.svelte#L30) · [connection-gate](../src/renderer/connection-gate.ts) |
| S02 快速解锁、切账户和系统提示<br>`offered` · `opening` · `unlocking` | 每轮锁定可自动提示一次。焦点、`offered/opening/unlocking`、登录弹窗共同决定是否开始或复用请求。取消在场确认仍锁定，凭证不可用可进入登录。资料读取与账户菜单的打开、聚焦和切换另有读序号。 | [session](../src/renderer/shell/session.ts) · [AccountSwitcher](../src/renderer/shell/AccountSwitcher.svelte) |
| S03 登录、绑定与首次进入工作台<br>`run` · `expiry` · `finishing` | 登录已有 phase。`run/expiry`、provider、action 和绑定目的共同决定浏览器返回、过期、重开、后续审批或再次登录。首开、无账户、待配对、待初始化和欢迎页收尾决定进哪个页面。 | [login](../src/renderer/shell/login.svelte.ts) · [LoginDialog](../src/renderer/shell/LoginDialog.svelte) · [Welcome](../src/renderer/shell/Welcome.svelte) · [MainShell](../src/renderer/shell/MainShell.svelte) |
| S04 导航、页面记忆与侧栏<br>`visits` · `visit` · `traveling` | hash 导航 → 加载页面 → 记下普通访问。前进/后退移动 `visit`，普通导航截断后续历史。账户变化、锁定对位置与明文草稿有不同清理规则。侧栏由用户选择、窄窗和临时展开共同决定。 | [MainShell.travel](../src/renderer/shell/MainShell.svelte#L197) · [view-memory](../src/renderer/view-memory.ts) · [WindowNav](../src/renderer/shell/WindowNav.svelte) |
| S05 命令面板与快捷帮助<br>`open` · `highlighted` · `detail` | 打开 → 读账号内容、搜索或选择 → 进入条目动作子页 → 返回、执行或关闭。文件搜索有防抖，锁态限制可用动作。键盘高亮和焦点跟随当前层级。 | [CommandPalette](../src/renderer/shell/CommandPalette.svelte) · [Shortcuts](../src/renderer/shell/Shortcuts.svelte) |
| S06 缓存、可见性与名称批读<br>`requests` · `generation` · `rerun` | display 的数据状态和陈旧/刷新/失败可并存。会话代数约束整次读取，每个 key 的请求序号约束元数据。视图自行校验业务结果。缓存事件读本地，可见窗口定时刷新。频道/收件箱提示另由可见性和连接决定是否读正文。名称按当前视口和显示位置合批读取。已解锁的可见窗口每 10 分钟最多刷新一次，搜索索引按需补齐。发布和待完成名称的事件可提前唤起读取。头像按 URL 复用。 | [display](../src/renderer/display.svelte.ts) · [watchContent](../src/renderer/content-refresh.ts) · [namesSession / ask](../src/renderer/views/channels/names.svelte.ts) · [DisplayNotice](../src/renderer/components/DisplayNotice.svelte) |
| S07 同步栏、连接提示和手动同步<br>`requested` · `syncingSince` · `coolingDown` | 首次、用户发起、持续较久的同步有不同可见性。待上传、最近成功、连续失败、冷却和锁定共同生成提示。点击产生本地等待，实际进度来自 daemon。网络恢复横幅反映 M05。 | [GateRail](../src/renderer/shell/GateRail.svelte) · [syncState](../src/renderer/sync-state.ts) · [RecoveryBanner](../src/renderer/shell/RecoveryBanner.svelte) |
| S08 语言与外观<br>`wanted` · `current` · `catalogs` | 首帧等待语言包，`wanted/current/catalogs` 决定异步加载后可应用的语言与 RTL。主窗口保存偏好并通知其他窗口。主题另有 system/light/dark 和系统变化。 | [i18n.show](../src/renderer/i18n/index.svelte.ts) · [appearance](../src/renderer/appearance.svelte.ts) · [主进程应用偏好](../src/main/index.ts#L219) |
| S09 页面读取与动作结果<br>`loaded` · `errors` · `busy` | 首次读、后台重读、确实为空、部分读失败是不同情形。写操作通常设置 busy，捕获错误后清 busy。需要在成功后清除草稿的 G02/G03 使用动作返回值，分项读取失败仍只更新对应资源。 | [Agents.attempt](../src/renderer/views/Agents.svelte#L73) · [Grants.read](../src/renderer/views/approvals/Grants.svelte) · [panel.read / act](../src/renderer/views/secrets/workflow-state.svelte.ts) |

## 通知、提醒与可见回执

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| N01 通知消费与原生投递<br>`noticeHistoryLoaded` · `noticeActivation` · `shown` | 连接后迁移旧配置、恢复历史和待处理点击，读取 context/next，依前后台路由至原生、应用内或抑制。平台 show/failed 回调再更新回执。同一投递去重，旧实例的回调受拥有者检查。会话、屏幕和窗口变化使旧轮询失效。 | [refreshNotices](../src/main/index.ts#L1078) · [Notifications](../src/main/notifications.ts) |
| N02 点击原通知并确认已见<br>`noticeReads` · `noticeViewports` · `noticeViewEpoch` | 点击 → 核对来源账户、读取业务对象 → 再核对会话 → 打开目标、提示已结束或等待解锁。窗口进入目标后还需可见、聚焦、未被弹层遮挡。主进程按窗口、提醒版本和会话登记可见性或确认已读。 | [openNotice](../src/main/index.ts#L1024) · [可见性 IPC](../src/main/index.ts#L1158) · [noticeVisibility](../src/renderer/notification-visibility.ts) |
| N03 提醒中心、铃铛与通知引导<br>`scope` · `cursor` · `opening` | 按当前账户和筛选加载、翻页、打开、标读、清除或稍后提醒。跨账户目标需人工切换。popover 的请求与页面有各自寿命。安全提醒引导可按账户暂缓，新增绑定可再提示。 | [Reminders](../src/renderer/views/Reminders.svelte) · [RemindersPopover](../src/renderer/shell/RemindersPopover.svelte) · [NoticeSetup](../src/renderer/views/NoticeSetup.svelte) · [dismissSetup](../src/renderer/notice-setup.svelte.ts) |
| N04 通知偏好与测试通知<br>`dirty` · `pending` · `conflict` | 加载本机及账户设置 → 修改草稿 → 按 revision 保存。远端待同步和冲突单独显示。暂停、安静时段、系统设置、测试通知的 submitted/shown/failed，以及本机已读回执开关各有独立状态。 | [NotificationSettings](../src/renderer/views/NotificationSettings.svelte) · [共享合同](../src/shared/notifications.ts) · [quietUntil](../src/shared/notification-time.ts) |

## 账户、设备与设置

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| D01 新机建户、绑定、加入和恢复<br>`token` · `pairing` · `recovery` | `no_account` 下创建账户或申请绑定，目标改动使绑定 token 失效。`pairing_required` 下选择角色、申请、等批准、尝试解锁，也可发起设备接替。待初始化和等待恢复另据 setup 标记、恢复策略、本机身份和执行时间开放下一步。 | [Onboarding](../src/renderer/views/Onboarding.svelte) |
| D02 授权另一台设备<br>`invitation` · `candidate` · `pending` | 邀请并轮询，或粘贴/选图片/摄像头识别 → 候选预览 → 创建审批 → 打开原审批。修改输入、关闭、过期和换模式分别失效。`revision/cameraTurn/pending` 保护迟到结果和已有审批 ID。摄像头另有请求权限、启动流、逐帧识别、重试和停轨的寿命。 | [DeviceAuthorization](../src/renderer/views/DeviceAuthorization.svelte) · [QrCamera](../src/renderer/components/QrCamera.svelte) |
| D03 设备权限、安全策略和恢复设置<br>`editor` · `needsLogin` · `pending` | 加载数据 → 选设备或编辑器 → 草稿有效且有更改 → 建立/重开审批 → 根据终态重载。设备按 ID，安全策略和恢复按动作组保留 pending。需要在线登录、本机解锁、缺审批 key、只改了部分字段，都会改变可做的事。 | [Devices](../src/renderer/views/Devices.svelte) · [Security](../src/renderer/views/Security.svelte) · [Recovery](../src/renderer/views/Recovery.svelte) |
| D04 设备间传输<br>`sending` · `view` · `pending` | 接收或填写发送目标 → 校验会话、设备或接替任务 → 必要时保存审批 ID 与原参数 → 获批后继续同一操作。进度由 daemon 的 waiting/running/done/failed/cancelled 驱动。停止后可在原会话恢复。 | [DeviceTransfer.approved / resume](../src/renderer/views/DeviceTransfer.svelte) |
| D05 浏览器配对<br>`invite` · `candidates` · `pairingRevision` | 请求/复制邀请 → 按邀请 ID 轮询 → 展示候选 → 用户确认 → 已配对或可重试。取消、换页和换会话会停轮询。到期收起候选，提示重新配对。撤销已有 pairing 是另一项操作。 | [Settings.stopPairing / poll / grant](../src/renderer/views/Settings.svelte#L135) |
| D06 设备接替和已移动副本<br>`reviewing` · `review` · `leaving` | 粘贴接替 token → 检查设备及本机影响 → 核对设备 ID → 生成一项或多项审批。任务的内容、钱包、原生地址和撤销等步骤按可执行方、证据和跳过记录逐项推进。已移动的副本另走批准后继、导入或二次确认丢弃。 | [Succession](../src/renderer/views/Succession.svelte) · [Moved](../src/renderer/views/Moved.svelte) |
| D07 旧系统迁移<br>`offer` · `selected` · `confirmed` | 生成 offer → 浏览器交接 → 选择 nonce、核对指纹与 principal → 原生选文件 → 本地归档 → 可分页查报告、核实冻结快照、导出文件或同步。切换 offer 重置核对信息。主壳可按 offer 暂缓提示。 | [Legacy](../src/renderer/views/Legacy.svelte) · [原生交接](../src/main/index.ts#L585) · [MainShell.loadOffers](../src/renderer/shell/MainShell.svelte) |
| D08 资料编辑、发布和查询<br>`editing` · `pending` · `lookupRevision` | 文档和发布状态独立加载。打开草稿、导入旧资料或上传头像 → 校验改动 → 发布 → 已发布或保留日志重试。头像成功仅更新草稿。查询别人的资料用独立查询目标和序号。 | [Profile](../src/renderer/views/Profile.svelte) · [profileChanged](../src/renderer/views/profile-form.ts) |
| D09 锁定、文件偏好和缓存维护<br>`bytes` · `prefs` · `busy` | 读设置 → 修改并保存 → 采用返回值或展示错误。空闲锁、随屏锁、开机启动由不同入口保存。挂载支持/配置、回收站天数、缓存清理后的重读和仅本机数据风险有各自条件。快捷键草稿和系统登记另见 M08/U04。 | [LockSettings](../src/renderer/views/LockSettings.svelte) · [FilesSettings](../src/renderer/views/files/FilesSettings.svelte) · [CacheSettings](../src/renderer/views/CacheSettings.svelte) · [LocalOnly](../src/renderer/views/LocalOnly.svelte) |
| D10 问题反馈草稿与发送<br>`dirty` · `writes` · `edits` | 按账户/会话加载 → 编辑变脏 → 串行保存并合并后续输入。版本冲突可先读回日志。发送等待 flush，未确认时读回原记录。可编辑的 pending、sent、丢弃/新建有不同动作。未保存、保存中和已保存有不同提示。截图选择、粘贴、预览和移除独立于正文保存。 | [FeedbackWorkspace](../src/renderer/feedback.svelte.ts) · [Feedback](../src/renderer/views/Feedback.svelte) |

## 审批中心与独立审批窗口

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| A01 审批队列、历史与授权清单<br>`requests` · `history` · `loaded` | 分别读取队列、历史、设备、浏览器、grants 和凭证。可部分失败。筛选、到期计时、路由指定请求、取消、结果未知后的对账决定可用动作。daemon 的审批结果是权威。 | [Approvals](../src/renderer/views/Approvals.svelte) · [Requests](../src/renderer/views/approvals/Requests.svelte) · [Grants](../src/renderer/views/approvals/Grants.svelte) |
| A02 审批窗口的多个时钟<br>`lapsed` · `completion` · `queue` | 阅读延迟、长按、报价、比对文件和批准/拒绝已有显式阶段。外围还有锁定后提示解锁、到期时只重查一次、摘要/焦点变化后重新计时、后续请求队列，以及决定后 2 秒自动关闭或因继续操作而停留。截止、审批提交、daemon 执行完成分别判断。 | [Approval.arm / decide / refocus](../src/renderer/views/Approval.svelte#L82) · [决定入口](../src/main/index.ts#L432) |
| A03 发起签名与核验凭据包<br>`profile` · `readiness` · `report` | 文本、文件、摘要和应用动作依各自来源、就绪状态和配额准备 → 创建请求 → 打开审批。核验另走选包、拖入、可选原文件或选历史记录 → 报告 → 导出。内容一致性、签名、主体绑定、签署时证据和当前状态分别展示。 | [Sign](../src/renderer/views/approvals/Sign.svelte) · [Bundles](../src/renderer/views/approvals/Bundles.svelte) · [Report](../src/renderer/views/approvals/Report.svelte) |

## 频道与收件箱

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| C01 频道列表与时间线<br>`openRequest` · `channelVisit` · `reading` | 本地列表 → 选择频道并装载消息页/outbox → 追赶或事件刷新。另可打开旧频道。最新、翻早期消息、搜索并跳至上下文、回到最新，有不同的游标与合并规则。`openRequest/channelVisit/reading`、滚动锚点和 seq 决定一页或一条 ACK 可否合入。 | [Channels.open / earlier / jump](../src/renderer/views/Channels.svelte) · [Thread](../src/renderer/views/channels/Thread.svelte) · [LegacyHistory](../src/renderer/views/channels/LegacyHistory.svelte) |
| C02 草稿与发送队列<br>`drafts` · `attempts` · `tails` | 按频道保存新消息、回复、编辑与附件草稿，输入防抖写回。发送后按消息 ID 排队和串行交付。投递状态已显式。内存任务、每频道队尾、可恢复尝试、接管过的 outbox ID、已确认但列表尚未包含的行另有寿命。恢复只重试符合条件的原消息。 | [Channels.send / retry](../src/renderer/views/Channels.svelte) · [MessageDeliveries](../src/renderer/message-delivery.ts) · [Thread](../src/renderer/views/channels/Thread.svelte) |
| C03 阅读位置、回执与消息动作<br>`atEnd` · `marked` · `receiptsAt` | 是否在末尾、焦点、可见性、弹窗和最后已标 seq 决定何时标读。定位历史时保留滚动位置。回执开关、置顶/静音、未读筛选、编辑、撤回和本机隐藏分别更新。可见作者触发名称和头像读取。 | [Thread.read / onScroll](../src/renderer/views/channels/Thread.svelte) · [Channels.markRead](../src/renderer/views/Channels.svelte) · [preferences](../src/renderer/views/channels/preferences.ts) · [ParticipantAvatar](../src/renderer/views/channels/ParticipantAvatar.svelte) |
| C04 附件草稿与库搜索<br>`query` · `found` · `chosen` | 菜单最近文件或浏览库 → 防抖搜索 → 勾选多项 → 合并到当前频道草稿。文件选择、拖入或粘贴另走本机导入后附加。附件预览、放大、保存到库和导出是独立动作。搜索结果按当前查询更新，已勾选的文件保留至本次选择结束。 | [LibraryPicker](../src/renderer/views/channels/LibraryPicker.svelte) · [Channels.addFiles](../src/renderer/views/Channels.svelte) · [Thread](../src/renderer/views/channels/Thread.svelte) |
| C05 建频道、邀请和成员管理<br>`tab` · `direct` · `invitation` | 创建、直接会话邀人、解析邀请加入各有下一步。已建频道和已取得邀请链接分别建模，邀请失败只重试第二步。成员面板处理邀请结果、角色变更、移除、历史授权和换代。归档/恢复、离开及 Agent 读写授权受当前成员角色与 ledger 状态约束。 | [NewChannel](../src/renderer/views/channels/NewChannel.svelte) · [Members](../src/renderer/views/channels/Members.svelte) · [Access](../src/renderer/views/channels/Access.svelte) · [权限投影](../src/renderer/views/channels/channels.ts) |
| C06 所有权、费用承担与频道快照<br>`packet` · `handed` · `snapshotGeneration` | 发起转移 → 交接 packet → 对方接受 → 原发起方提交，可经消息或手工交接。转移中的写入条件单独判断。收到秘密快照时按频道、序号和请求代数读预览，再接 V08 的导入流程。 | [Transfer](../src/renderer/views/channels/Transfer.svelte) · [Packet](../src/renderer/views/channels/Packet.svelte) · [Channels.settlePacket / receiveSnapshot](../src/renderer/views/Channels.svelte) |
| C07 来信列表、规则与资金<br>`box` · `openedNotice` · `orderFailed` | 当前箱、已选信、是否刷新、出站日志是否已读、提醒目标是否找到单独决定页面。已读、归档、放行、屏蔽及联系规则各自提交。订单和托管/退款腿可独立未知或失败。按原订单查询、对账或修订。 | [Inbox](../src/renderer/views/Inbox.svelte) · [Rules](../src/renderer/views/inbox/Rules.svelte) |
| C08 写信、免费投递与付费审批<br>`stage` · `pending` · `generation` | 草稿 → 解析收件账户 → 固定输入对应的账户和订单 ID → 发送。直接投递或收到报价后请求付款审批。结果未知时同一份草稿沿用相同账户和订单，关闭或销毁会使旧 UI 回调失效。 | [Compose.send](../src/renderer/views/inbox/Compose.svelte#L49) |

## 文件库、分享与收取文件

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| F01 目录、搜索与选择<br>`scope` · `anchor` · `found` | 路由/跨页请求 → 文件夹、标签、收藏、云端或回收站 → 列表 → 单选、范围、多选及详情。普通文件夹先筛本地行，再防抖搜全库。只有当前查询的回复能更新列表或错误。放大列表、键盘光标、锚点、目录展开和拖入目标各有状态。目录消失时回根目录。 | [Files.pick / select / range](../src/renderer/views/Files.svelte) · [Tree](../src/renderer/views/files/Tree.svelte) · [Row](../src/renderer/views/files/Row.svelte) · [跨页 focus](../src/renderer/views/files/focus.svelte.ts) |
| F02 文件、文件夹与版本详情<br>`previewFor` · `editingNote` · `confirming` | 点选 → 加载详情 → 当前/历史版本预览。按文件、版本、内容状态丢弃旧预览。编辑备注/标签、完整性校验、切换版本、释放本地数据、删除版本及针对某一版本签声明各有提交点。文件夹统计另读，移动目标要排除自身与子目录。 | [Detail](../src/renderer/views/files/Detail.svelte) · [FolderDetail](../src/renderer/views/files/FolderDetail.svelte) · [FolderPicker](../src/renderer/views/files/FolderPicker.svelte) |
| F03 导入、后台任务与任务卡<br>`jobs` · `stage` · `cancelling` | 选取/拖入 → 按文件或父目录接收进度 → 导入 RPC 返回批次结果。备份、抓取、校验还会排队、暂停、重试、提交或验证。取消中与取消完成不同。完成卡延迟移除，失败卡可重试或关闭。按账户清理、按任务身份合并事件和轮询结果。 | [adding](../src/renderer/views/files/adding.svelte.ts) · [Jobs](../src/renderer/views/files/Jobs.svelte) · [Files.cancelImport / retryTransfer](../src/renderer/views/Files.svelte) |
| F04 打开、编辑副本与文件库挂载<br>`program` · `changedCopy` · `mounting` | 本机打开可先要求确认可执行文件。打开的副本用 nonce 跟踪，变化后保存为新版本或丢弃，未改可清理。导出、批量导出、系统预览和拖出走主进程。文件库磁盘另有支持/配置、挂载中和实际挂载状态。 | [Files.open / settleCopy / mountDisk](../src/renderer/views/Files.svelte) · [OpenCopies](../src/renderer/views/files/OpenCopies.svelte) · [原生文件操作](../src/main/index.ts#L516) |
| F05 库管理、回收站与云端清单<br>`trashOpened` · `purging` · `cloudLoaded` | 创建/改名/移动按本次目标确认。移入回收站、恢复、永久删除或清空有独立选择。云端清单区分尚不完整与确实为空。选择远端上传 → 下载到目标目录、验证或确认删除。用量和在途任务各自刷新。 | [Files.reload / refreshInventory](../src/renderer/views/Files.svelte) · [Trash](../src/renderer/views/files/Trash.svelte) · [Cloud](../src/renderer/views/files/Cloud.svelte) |
| F06 备份、频道分享和外链<br>`sharing` · `quoteState` · `linking` | 备份计划、分享报价和外链提交已有显式 phase。外层还要结合版本/上传 ID、目的频道、口令、有效期、当前会话来判断确认是否可用。分享和备份是不同提交。已有外链支持本地读取、刷新、确认撤销和按项更新。 | [Files.askBackup / askShare / createLink](../src/renderer/views/Files.svelte#L580) · [Links](../src/renderer/views/files/Links.svelte) |
| F07 目录授权<br>`folder` · `agent` · `here` | 打开目录 → 读 Agent 与现有授权 → 选择读写范围和主体 → 授予或撤销 → 重读。关闭对话框与授权到期或撤销各自生效。 | [GrantDialog](../src/renderer/views/files/GrantDialog.svelte) |
| F08 收取链接与批量创建<br>`working` · `progress` · `created` | 本地清单先显示，刷新后取得远端清单和额度。新建进入编辑或升级提示。按名字逐项建链接。部分完成时同时保存已建链接和未完成名字，重试只处理剩余输入。撤销链接另需确认。 | [Requests](../src/renderer/views/files/Requests.svelte) · [RequestComposer.create](../src/renderer/views/files/RequestComposer.svelte#L72) · [requests](../src/renderer/views/files/requests.ts) |
| F09 收件上传的保存与删除<br>`choosing` · `saving` · `deleting` | 进入链接 → 读上传列表 → 选文件夹或用默认目录 → 按 upload ID 跟踪保存进度 → 重读结果。删除走另一确认。替代、不可读和已保存由服务数据投影。切会话或销毁使旧操作失效。 | [RequestDetail](../src/renderer/views/files/RequestDetail.svelte) |

## 秘密库与凭证工作流

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| V01 库选择、健康检查与批量动作<br>`selected` · `leaving` · `checkedKey` | 列表、回收站、冲突、健康结果和模板各自就绪。筛选/分页、单选、多选、批量标签和跨页定位改变目标。编辑有改动时切条目先询问。会话变化清理选择和明文。 | [Vault](../src/renderer/views/Vault.svelte) · [Filters](../src/renderer/views/vault/Filters.svelte) · [focus](../src/renderer/views/vault/focus.svelte.ts) |
| V02 新建、编辑与辅助输入<br>`base` · `dirty` · `hiddenState` | 选模板或载入原版本 → 编辑字段、顺序、标签和附件 → 校验改动 → 保存或确认放弃。编辑固定 base revision。已有秘密字段区分保留、替换和清除。密码生成器随模式/选项更新候选。附件搜索、OTP 扫描和 SSH key 生成后填入草稿。 | [Inspector](../src/renderer/views/vault/Inspector.svelte) · [NewItem](../src/renderer/views/vault/NewItem.svelte) · [ItemForm](../src/renderer/views/vault/ItemForm.svelte) · [字段草稿编码](../src/renderer/views/vault/vault.ts#L327) · [Generator](../src/renderer/views/vault/Generator.svelte) · [FilePicker](../src/renderer/views/vault/FilePicker.svelte) |
| V03 字段揭示、复制与大字查看<br>`armed` · `secret` · `shownUntil` | 隐藏 → 首击武装 3 秒 → 二次点击等待 → 显示最多 60 秒。超时、手动隐藏、改字段、锁定或关闭查看器失效。复制值、复制引用与大字视图有独立 busy、反馈和焦点归还。普通值与受限字段允许的动作不同。 | [SecretField](../src/renderer/views/vault/SecretField.svelte) · [FieldActions](../src/renderer/views/vault/FieldActions.svelte) · [FieldViewer](../src/renderer/views/vault/FieldViewer.svelte) · [ValueField](../src/renderer/views/vault/ValueField.svelte) |
| V04 OTP<br>`current` · `due` · `generation` | 读本周期 → 倒计时 → 到期遮住旧码 → 取下一周期。失败可重试。字段/解锁状态改变停止旧 timer。UI 显示和主进程复制各自考虑请求耗时，不共用明文。 | [OtpField](../src/renderer/views/vault/OtpField.svelte) · [主进程 OTP 复制](../src/main/index.ts#L645) |
| V05 历史与冲突合并<br>`open` · `shown` · `picks` | 历史列表 → 展开某 revision → 读该版 → 可恢复为新版本。冲突按 head/copy 展示差异 → 选择保留哪方或逐字段合并 → 提交，重新读取余下冲突。切冲突重置该次取舍。 | [History](../src/renderer/views/vault/History.svelte) · [Conflict](../src/renderer/views/vault/Conflict.svelte) |
| V06 密码库文件导入<br>`operation` · `ticked` · `attachments` | 选择、预览和一次性提交已有显式状态。待导入条目、重复项的默认勾选、附件及目标文件夹形成预览内的选择状态。主进程持有路径与 digest。取消、会话变化或已消费会使旧选择失效。 | [Import](../src/renderer/views/vault/Import.svelte) · [PreparedOperation](../src/main/prepared-operation.ts) · [原生准备与消费](../src/main/index.ts#L681) |
| V07 凭证面板的读取与活动<br>`loaded` · `errors` · `retries` | 切换页签/跳到目标，载入多个独立资源。`panel` 用 loading/loaded、分项错误和 retry map 管读，用 busy 管一次写。特定进行态触发轮询，卸载清理。活动列表按目标/类型/状态筛选分页，跟随或取消原请求。 | [Workflows](../src/renderer/views/secrets/Workflows.svelte) · [panel](../src/renderer/views/secrets/workflow-state.svelte.ts) · [Activity](../src/renderer/views/secrets/Activity.svelte) · [跨页导航](../src/renderer/views/secrets/navigation.svelte.ts) |
| V08 创建与接收秘密包<br>`draft` · `preview` · `request` | 选来源版本、字段、附件、目的地 → 准备预览 → `DraftVersion` 和输入快照仍匹配时发起审批 → 观察加密、上传、发布、撤销或对账。接收方输入链接/密码或从频道进入 → 预览 → 选择导入内容 → 审批 → 已导入 ID。已导入再次打开有单独路径。 | [Sharing](../src/renderer/views/secrets/Sharing.svelte) · [Receiving](../src/renderer/views/secrets/Receiving.svelte) · [PackageImport](../src/renderer/views/secrets/PackageImport.svelte) · [DraftVersion](../src/renderer/views/secrets/model.ts) |
| V09 秘密挂载配置与运行<br>`preparation` · `statuses` · `project` | 选字段/库文件/引用映射 → 编辑路径和窗口 → 原生准备 → 审阅并消费一次性配置。启动能力、打开系统扩展设置、运行状态轮询、撤销读窗口、停止和删除是另一组状态。底层资源见 M11/M12。 | [Mounts](../src/renderer/views/secrets/Mounts.svelte) · [原生预览](../src/main/index.ts#L695) |
| V10 健康、发现与凭证轮换<br>`scan` · `picks` · `rotations` | 读健康报告 → 忽略或定位问题。本机扫描 → 运行中/取消/完成 → 勾选发现项并申请导入。轮换按候选、测试、已查引用和上游撤销推进，依赖、版本和动作资格分别判断。 | [Health](../src/renderer/views/secrets/Health.svelte) · [Dependencies](../src/renderer/views/secrets/Dependencies.svelte) · [rotationAllowed](../src/renderer/views/secrets/model.ts) |
| V11 用途策略、使用记录和 SSH 绑定<br>`selected` · `uses` · `bindings` | 编辑策略及来源 → 解析凭证引用 → 带 base version 申请保存。停用、删除、用途授权及“结果丢失后已复核”各有动作。SSH 另按本机 socket、key 支持情况、绑定用途决定能否申请启用或禁用。审批页投影本次实际范围。 | [Policies](../src/renderer/views/secrets/Policies.svelte) · [Ssh](../src/renderer/views/secrets/Ssh.svelte) · [Review](../src/renderer/views/secrets/Review.svelte) · [ReviewDetails](../src/renderer/views/secrets/ReviewDetails.svelte) |

## 会员、名称与支付

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| P01 会员权益与退款<br>`stale` · `paidAt` · `refund` | 套餐、目录、存储和执行用量各自加载。本机可用、在线身份过期、权益陈旧、验证中和用量读取失败可并存。支付后重新核对权益。退款先读报价再按订单确认，实际转出由退款腿决定。超额通知另有确认动作。 | [Membership](../src/renderer/views/plan/Membership.svelte) · [membershipState / refundOutcome](../src/renderer/views/plan/plan.ts) |
| P02 结账<br>`payRun` · `quotedFor` · `selectionKey` | 选产品 → 选现金/神经元 → 查资产或资格 → 新建或复用未过期的精确报价 → 创建结账请求 → 打开审批。用 `offer/payRun/selectionKey/quotedFor` 识别某次报价。余额不足、资格变化、失效和网络失败有不同重试条件。 | [Checkout.pay](../src/renderer/views/plan/Checkout.svelte) |
| P03 订单与付款人<br>`checkouts` · `orders` · `seen` | 先读本地 checkout 日志，再有界并发查仍需更新的订单。资金、激活和退款结果分别决定可支付、推进、对账、取消或退款。切换付款身份、选择 PEM 与确认取消也有 UI 生命周期。 | [Payments](../src/renderer/views/plan/Payments.svelte) · [orderActions](../src/renderer/views/plan/plan.ts) |
| P04 名称查价、认领、转移<br>`lookupVersion` · `moving` · `incoming` | 路由可携带待处理输入 → 规范名查价 → 按归属/预留/身份开注册或认领审批。转移另有发起、交接 token、接受和完成。按两方批准、操作记录和期限计算可操作阶段，遗失结果可对账。 | [Plan](../src/renderer/views/Plan.svelte) · [Names](../src/renderer/views/plan/Names.svelte) · [nameState](../src/renderer/views/plan/plan.ts) |
| P05 运维查询与补处理<br>`orders` · `transfers` · `claims` | 手动打开后查订单、转账和 claims，按各自游标翻页。处理前据返回状态决定对账、重试、修订或核查资格。操作完成重读三个列表。 | [Operations](../src/renderer/views/Operations.svelte) |

## 团队钱包

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| W01 钱包选择与分项刷新<br>`scope` · `epoch` · `queueRevision` | 当前账号 → 钱包列表 → 选择/加入 → 按 tab 加载核心、资产、余额、地址簿、操作队列与审计。切钱包或会话使旧 epoch 失效。同一钱包内还用 audit head、策略版本和分项序号约束异步更新。收款窗口另选地址/网络，冻结与访问授权各有确认。 | [Wallet](../src/renderer/views/Wallet.svelte) · [ViewScope](../src/renderer/views/wallet/model.ts) · [Balances](../src/renderer/views/wallet/Balances.svelte) |
| W02 创建资格、激活和未用资金<br>`setup` · `abort` · `creation` | 成员草稿、角色、门槛、报价和就绪条件控制可提交性。创建、审批 → 激活、取消确认已有显式 phase。继续、取消、退款和进入就绪钱包仍需核对所属账户和 quote。当前创建入口受产品开关限制，已有激活/退款路径单独处理。 | [Create](../src/renderer/views/wallet/Create.svelte) · [Wallet.readPending / abortActivation](../src/renderer/views/Wallet.svelte#L493) · [产品开关](../src/shared/wallet-availability.ts) |
| W03 转账、批量转账和成员解析<br>`operation` · `fingerprint` · `attempted` | 资产、来源、收件人、精度、手续费或策略版本变化会影响草稿和预览。按已审阅快照提交已有显式阶段。金额有输入文本、原子值与无效输入三种语义。成员名称异步解析后需核对发起查询的输入。 | [Transfer](../src/renderer/views/wallet/Transfer.svelte) · [AmountField](../src/renderer/views/wallet/AmountField.svelte) · [MemberLookup](../src/renderer/views/wallet/MemberLookup.svelte) |
| W04 管理提案与策略编辑<br>`kind` · `confirmed` · `removeCosign` | 选择成员、地址簿、资金、地址、策略等操作 → 编辑对应草稿 → 校验 → 必要时再次确认 → 提案。移除成员对共签人的影响、补充资金的准备情况、策略用量示例与动作资格各自变化。 | [Manage](../src/renderer/views/wallet/Manage.svelte) · [Policy](../src/renderer/views/wallet/Policy.svelte) · [CosignPolicy](../src/renderer/views/wallet/CosignPolicy.svelte) · [PolicyUsage](../src/renderer/views/wallet/PolicyUsage.svelte) |
| W05 提案投票、执行与对账<br>`p` · `available` · `nonceAction` | 提案、当前成员、冻结、票数和时钟共同决定可批准、拒绝、否决、撤销、执行或对账。外部链另区分准备、签名、广播和重播。手动确认账本结果与重新签名有不同入口。备注读写独立于提案终态。 | [Proposal.run](../src/renderer/views/wallet/Proposal.svelte) · [stateAt / actions](../src/renderer/views/wallet/model.ts) |
| W06 共签计划与替代操作<br>`selectedPlan` · `collection` · `digest` | 选计划 → 校验仍有效、未签、策略允许 → 收集/导入 packet → 允许执行。切计划、签名、过期或更换 digest 影响可用性。替代计划、nonce 操作和共签状态的读结果各自校验。 | [Proposal.readCollection](../src/renderer/views/wallet/Proposal.svelte#L91) · [Operations](../src/renderer/views/wallet/Operations.svelte) |
| W07 本机 key、保护器与配置向导<br>`software` · `setupStep` · `tested` | 读硬件状态 → 硬件生成或在明确确认后软件生成 → 增删保护器。最后一个保护器的删除受限。运维页另据本地 key、钱包登记和共签地址推导当前向导步骤，支持测试、替换和解绑。 | [NativeKey](../src/renderer/views/wallet/NativeKey.svelte) · [Operations.setupStep](../src/renderer/views/wallet/Operations.svelte#L52) |
| W08 版本升级与运行维护<br>`versionRead` · `codePending` · `installedVersion` | 查询版本、健康、RPC 和频道绑定分别就绪。提出某版本 → 团队接受 → 尚待安装 → 安装中 → 记录新版本或失败后重读。`versionRead` 防止旧版本查询覆盖已完成安装。提出提案与执行升级的错误分开。 | [Operations.installUpgrade](../src/renderer/views/wallet/Operations.svelte#L132) |
| W09 原生地址迁移<br>`path` · `read` · `view` | 选 L1/L2 路径 → 读取该路径计划 → 按权威 next 显示登记、地址、地址簿、冷却/等待、清扫、归档或完成。只对可提案步骤发起动作。换路径用读序号废弃上次结果。 | [NcMigrate.load / next](../src/renderer/views/wallet/NcMigrate.svelte) |
| W10 历史与报告<br>`complete` · `busy` · `generation` | 载入历史和备注 → 本地筛选、日期范围校验 → 数据完整且有结果才允许导出。窗口离开或再次读取使旧结果失效。保存位置由主进程选择。 | [History](../src/renderer/views/wallet/History.svelte) · [报告格式](../src/shared/wallet-report.ts) |

## Agent 工作台

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| G01 工作台上下文<br>`tab` · `generation` · `loaded` | 当前账户、tab、解锁与选中的 Agent 决定读取哪些资源。已载入、部分失败、可重试和操作在途并存。查看某 Agent 的路由选择与设备当前 active Agent 独立。 | [Agents.loadTab / request / run](../src/renderer/views/Agents.svelte) |
| G02 托管 Agent 身份与一次性凭证<br>`issued` · `label` · `profiles` | 创建、选择、常驻、暂停、撤回凭证、删除、本次签发后显示 MCP 配置，各自有不同前置条件。展开公开 Profile 按 Agent 单独记 loading/已读/error。创建失败保留名称，见首表。 | [AgentIds](../src/renderer/views/agents/AgentIds.svelte) · [mcpSetup](../src/renderer/views/agents/agents.ts) |
| G03 能力授予<br>`ticked` · `asking` · `confirming` | 选主体、服务/origin、期限及带目标的 capability → 词表校验和去重 → 宽权限再确认 → 写入、重读或撤销。标签/钱包、频道、文件夹入口限制各自的范围。主能力表的 `grant()` 和 G02 共用动作结果，确认成功才清除选择。 | [Capabilities](../src/renderer/views/agents/Capabilities.svelte) · [AgentAccess](../src/renderer/views/agents/AgentAccess.svelte) · [频道授权](../src/renderer/views/channels/Access.svelte) · [目录授权](../src/renderer/views/files/GrantDialog.svelte) |
| G04 Principal、controller 与委托<br>`registrationPending` · `rotation` · `successor` | 默认/定制 controller 有不同同意与校验步骤。登记经审批。轮换保留旧 generation 和候选后继 generation，待新条目出现才有后续退役动作。改名、失陷标记、委托授权/续期/撤回各有草稿。凭证和发布结果来自协议。 | [Overview](../src/renderer/views/agents/Overview.svelte) · [controllerParams](../src/renderer/views/agents/controllers.ts) |
| G05 Mail 收取、浏览与邮箱发布<br>`receiving` · `refreshPending` · `cursor` | 本地列表、后台收取和推送设置可并行。按 Agent 合并接收，在忙时记一次待刷新。按文件夹分页、选消息和看正文不会替 Agent 确认处理。发布、更新、关闭、恢复邮箱与部分路由失败是另一组流程。恢复按 host/cursor 推进。 | [Mail](../src/renderer/views/agents/Mail.svelte) · [recoverMailboxes](../src/shared/mail-recovery.ts) |
| G06 Knowledge 与 Discourse 检视<br>`revision` · `opened` · `room` | Knowledge 按 Agent、服务、模式和读序号使搜索、翻页、详情/评估过期。Discourse 按 Agent 读本地房间、草稿和待处理项，选房间后另按序号读成员、时间线和快照。这些检视不会更改 Agent 的业务已读状态。 | [Knowledge](../src/renderer/views/agents/Knowledge.svelte) · [Discourse](../src/renderer/views/agents/Discourse.svelte) |
| G07 MCP Events 端点<br>`endpoints` · `secret` · `rotating` | 载入端点与 relay → 创建/绑定 → 一次性显示 secret。轮换或删除前确认，旧 secret 与已不存在的端点脱离。服务端订阅/投递状态、窗口内显示和远端读取相互独立。 | [Events](../src/renderer/views/agents/Events.svelte) |
| G08 审计翻页<br>`actor` · `revision` · `displayedActor` | 选 actor → 拉首页 → 按 seq 载入更早记录。切 actor、事件重读、较早请求返回或销毁时按 `revision/displayedActor` 判断覆盖与追加。 | [Audit.load](../src/renderer/views/agents/Audit.svelte) |

## 共享控件与短暂交互

| 流程与关键字段 | 状态、事件与边界 | 状态所在 |
| --- | --- | --- |
| U01 弹层、确认与焦点<br>`open` · `target` · `leaving` | 打开、当前目标、初始焦点、允许关闭、提交结果和焦点归还分别影响对话框。敏感操作在提交中禁止重复点击，明确拒绝后原地重试，结果未知时转到状态或审批记录。右键菜单按触发元素记住目标，外部链接暂存 URL/host，确认后才经主进程打开。应用内链接直接导航。普通折叠、菜单和选项复用 Bits UI 的状态。 | [Dialog](../src/renderer/components/Dialog.svelte) · [Confirm](../src/renderer/components/Confirm.svelte) · [TargetMenu](../src/renderer/components/TargetMenu.svelte) · [followLink](../src/renderer/components/link-confirm.svelte.ts) · [LinkConfirm](../src/renderer/components/LinkConfirm.svelte) |
| U02 选择、建议、标签与日期<br>`open` · `query` · `text` | 选择框打开后管理焦点。搜索词和已选值分开，关闭清查询。自由输入的建议可选也可直接保留原文。标签区分尚未提交的文本、已提交项和数量上限。日期区分可继续编辑的文本、合法日期、弹层浏览年月和超限状态。 | [Select](../src/renderer/components/Select.svelte) · [SearchSelect](../src/renderer/components/SearchSelect.svelte) · [Suggest](../src/renderer/components/Suggest.svelte) · [TagInput](../src/renderer/components/TagInput.svelte) · [DateInput](../src/renderer/components/DateInput.svelte) |
| U03 输入显隐、单位与资源回退<br>`shown` · `last` · `failed` | 输入秘密有独立的遮盖、显示、生成候选和采用动作。金额随资产精度重算，时长可更换显示单位。头像失败记录在该 URL 下并显示首字母，新 URL 可再次加载。 | [SecretInput](../src/renderer/components/SecretInput.svelte) · [AmountField](../src/renderer/views/wallet/AmountField.svelte) · [DurationField](../src/renderer/views/wallet/DurationField.svelte) · [Avatar](../src/renderer/components/Avatar.svelte) |
| U04 快捷键录制与保存<br>`recording` · `armed` · `generation` | 聚焦 → 等主进程暂停全局键 → armed → 捕获、拒绝、清除或取消 → 失焦恢复。保存前的 draft、系统已登记的组合和保存失败分开处理。 | [ShortcutInput](../src/renderer/components/ShortcutInput.svelte) · [ShortcutSettings](../src/renderer/views/ShortcutSettings.svelte) · [Shortcuts.configure](../src/main/shortcuts.ts) |
| U05 分隔栏与拖动<br>`from` · `width` · `key` | 捕获指针和起点 → 拖动期间预览宽度 → 松开持久化。键盘微调、到边界和重置走同一限制，RTL 改变方向。业务行上的文件拖入/拖出由 F01/F03/F04 各自处理。 | [PaneHandle](../src/renderer/components/PaneHandle.svelte) |
| U06 操作反馈与计时<br>`mark` · `watched` · `running` | 点击时记 RPC 序号，延迟 150 ms 后仍禁用且有后来发起的调用在途才标记按钮，用观察器/调用完成清理。Toast 另按 ID 排队，到时、关闭或选择动作后移除。网络恢复和重复临时错误影响是否展示。 | [pending](../src/renderer/components/pending.ts) · [rpc.bridge](../src/renderer/rpc.ts) · [toast](../src/renderer/components/toast.svelte.ts) · [Toaster](../src/renderer/components/Toaster.svelte) · [failure](../src/renderer/failure.ts) · [network](../src/renderer/network.svelte.ts) |

## 哪些状态已有权威表示

以下实现可直接用作审阅和测试的依据。前面清单登记的是围绕它们仍需组合判断的行为。

| 已有表示 | 由谁决定 | 应保留的区别 |
| --- | --- | --- |
| daemon `Connection` 与窗口 `Connection` | [daemon](../src/main/daemon.ts#L40) · [App](../src/renderer/App.svelte#L30) | socket 初始化完成才接受业务调用。重连后还要核对账户会话才能恢复可编辑界面。 |
| `Phase`、`Review`、`Work`、`PreparedOperation` | [登录](../src/renderer/shell/login.svelte.ts) · [审批窗口](../src/renderer/views/Approval.svelte#L45) · [原生预览](../src/main/prepared-operation.ts) | 登录窗口关闭、审批阅读完成、作出决定、真正执行完成，以及一次性预览被消费，分别有自己的证据。 |
| `Delivery.state`、文件 `Job.stage`、`Backup/Quote/LinkSubmission`、`UpdateState` | [消息交付](../src/renderer/message-delivery.ts) · [文件任务](../src/renderer/views/files/adding.svelte.ts) · [文件确认](../src/renderer/views/Files.svelte#L90) · [更新合同](../src/shared/contract.ts#L28) | 排队、进度、确认、待重试分别可见。一次超时不足以判定一次写入是否发生。 |
| 钱包 `Setup/Abort/Creation/Operation`、挂载 `Preparation`、FSKit `State` | [钱包](../src/renderer/views/Wallet.svelte#L110) · [创建](../src/renderer/views/wallet/Create.svelte#L30) · [转账](../src/renderer/views/wallet/Transfer.svelte#L55) · [挂载](../src/renderer/views/secrets/Mounts.svelte#L44) · [FSKit](../native/fskit/SecretsFS.swift#L14) | 特定阶段拥有原始请求、报价、审阅快照、资源或取消句柄。已有阶段所有权可直接复用。 |
| 远端状态的纯投影 | [同步](../src/renderer/sync-state.ts) · [会员及订单](../src/renderer/views/plan/plan.ts) · [钱包提案](../src/renderer/views/wallet/model.ts) · [提醒版本](../src/renderer/views/reminders/notice.ts) · [文件状态](../src/renderer/views/files/files.ts) | 给定快照、权限和时间即可算出显示和可用动作。写入结果、凭证授权、资金到账与业务已读均以原业务接口为准。 |

## 覆盖与验证边界

- **检索方法。** 以 `git ls-files` 固定文件范围，从 main、preload、renderer 入口跟踪导入和路由，同时反查 Svelte 状态及 effect、普通局部变量、Promise/Map/Set、定时器、IPC/窗口事件和 Swift 可变资源。复核时可从桌面目录运行 `git ls-files src native/fskit scripts tests`，再用 `rg -n '[$]state|[$]effect|onMount|onDestroy|setTimeout|setInterval|new (Map|Set)|private var' src native/fskit` 定位候选。单次扫描的匹配数不用于状态机计数。
- **共享与 preload。** preload 负责固定窗口角色、IPC 转发、监听/解除监听以及将拖放文件交给主进程。全屏状态在订阅时先登记事件并查询当前值，此后由窗口事件更新 M01 的标题栏。上表将订阅寿命计入持有它的窗口或组件。方法白名单、静态词表、图标、翻译目录、纯格式转换、CSS 动画及只渲染传入值的组件按消费方核对。[preload](../src/preload/index.ts) · [renderer 入口](../src/renderer/main.ts) · [IPC 合同](../src/shared/contract.ts) · [主进程策略](../src/main/policy.ts)
- **开发工具。** 测试用 FSKit probe 另有延迟读、短时开放、到期撤销、并发上限与停卷状态。两个 host 只启动开发窗口。打包按 runtime 版本、平台、签名、扩展验证与产物校验逐步通过或中止。Electron fixture 的 `hold` 可暂存匹配 RPC 的回复，释放时再将该回复交给客户端。模拟钱包、Agent 和通知各自持有测试状态，这些流程单列在运行时计数以外。[Probe](../native/fskit/Probe.swift) · [开发 host](../native/fskit/DevelopmentHost.swift) · [probe host](../native/fskit/ProbeHost.swift) · [准备 runtime](../scripts/prepare-runtime.mjs) · [FSKit 打包](../scripts/fskit-package.cjs) · [产物校验](../scripts/verify-release.cjs) · [hold](../scripts/smoke.mjs#L1515) · [钱包](../scripts/wallet-fixture.mjs) · [Agent](../scripts/agent-fixture.mjs) · [通知](../scripts/notifications-fixture.mjs)
- **回归入口。** 在桌面目录运行 `pnpm check`、`pnpm test` 和 `pnpm test:e2e`。完整 Electron smoke 包含上述六组用例。单独复核时可运行 `DMSG_DESKTOP_SMOKE=state-transitions pnpm test:e2e`。用例覆盖临时故障、过期、乱序回复和重试，在中文明暗主题、窄窗、法语和阿拉伯语 RTL 下检查布局，并通过键盘操作核对提交结果。
- **首轮六项修复验证（2026-11-18）。** macOS 上类型检查 0 错误、0 警告，203 项单元测试全部通过。完整 Electron smoke 和六组回归通过。检查了 7 个受影响界面的 28 张截图，包括 760 × 540 窄窗下的重试按钮换行和 RTL 布局。
- **验证边界。** 这些回归使用打包前的 Electron 构建和本机 WebSocket fixture。后续评审记录了本机完整回归和开发签名 FSKit 的真实挂载。[桌面端代码复核](desktop-review.zh.md) · [单次刷新](../tests/single.test.ts) · [恢复](../tests/recovery.test.ts) · [消息投递](../tests/message-delivery.test.ts) · [更新](../tests/update-session.test.ts) · [Electron smoke](../scripts/smoke.mjs) · [审批](../scripts/approval-smoke.mjs) · [设置](../scripts/settings-flow.mjs) · [原生验证范围](../native/fskit/README.md)
