# 桌面表单统一：整改与验证记录

更新日期：2026-11-13。原控件整改基于 `main@777a7c3f`；原始排查的界面基线为 `643693a5`。
原始 9 项整改、11 处特殊控件/交互，以及设置的 17 处直接文本输入均已完成代码整改。
实际交互验证使用构建后的 Electron 与隔离的本地 mock daemon，不操作真实账户。

## 2026-11-13 全局快捷键

基于本地 `main@f8624292` 及本轮会员提醒修复工作树。新增「设置 → 键盘快捷键」，
默认 Command/Ctrl + Shift + Space 打开工作台，Command/Ctrl + Shift + L 锁定。
快捷键在 dMsg 运行时生效；关闭窗口到后台仍可使用，退出应用后释放。
配置只保存在主进程应用目录的 `shortcuts.json`，不进入账户或 daemon 偏好。

录入复用共享 `ShortcutInput` / `TextField`、按钮与 `Notice`，23 种语言齐全。
录入时暂停本应用的全局快捷键，Escape 取消、Tab 离开、Backspace 停用；页面离开、
窗口失焦/隐藏/销毁或 renderer 失效会恢复。两个操作禁止同一组合；注册或原子保存失败
保留已生效组合与用户草稿。打开恢复原路由，不直接解锁；锁定只调用既有 `session/lock`。

本地验证：

- `pnpm check` 无错误/警告，`pnpm test` 200 项通过，`pnpm build` 通过；
  `pnpm audit:controls` 扫描 171 个 Svelte / 78 个 TypeScript 文件，0 未批准用法。
- 5 项快捷键单元测试覆盖平台默认值、组合校验、占用与本地写入失败回滚、保存竞争、
  交换/停用组合、从保存配置重建以及退出释放。
- `DMSG_DESKTOP_SHOW=1 DMSG_DESKTOP_SMOKE=shortcuts node scripts/smoke.mjs`：
  隔离 Electron 使用真实 `globalShortcut` 注册并核对 `isRegistered`，捕获其回调验证
  隐藏窗口唤起和后台锁定。覆盖无效/重复组合、占用回滚、磁盘保存等待/失败与重试、
  JSON 持久化、恢复默认、页面离开/窗口隐藏后结束录入、锁定失败提示和快捷键帮助。
- 检查中文明暗、德文长文案和阿拉伯语 RTL，1180×760 与最窄 760×540；
  截图为 `test-results/controls-shortcuts-{zh,de,ar}-*.png`，无横向溢出。
  键盘录入、Enter 保存、Escape 取消均在真实窗口焦点下执行。

尚未验证真实系统级按键分发、不同系统的保留组合/授权、Windows/Linux、签名安装包和
整应用重启；重启恢复由保存文件检查与注册器重建测试覆盖。未替换已安装应用。

## 2026-11-02 频道消息与菜单增量

基于 `main@4a51b3c8` 的工作区增量。自己发送的消息（含发送中）保持右对齐，
头像从认证资料投影与加密 URL 缓存读取，可见行才触发补读，成员抽屉复用同一缓存。
频道消息菜单对齐回复、复制、编辑、撤回与本机隐藏；按用户要求移除举报和屏蔽入口。

- `node scripts/channel-messages-smoke.mjs` 在构建后的 Electron 通过：发送前后对齐、
  PNG 头像渲染、重复进入复用图片、60 位发送者只补读可见资料、泛化提示抑制、
  键盘打开菜单、本机隐藏与恢复、举报和屏蔽入口缺席。
- 截图检查覆盖中文、德语长内容、阿拉伯语 RTL 的明暗主题和 760×540 窗口，
  包括消息列表与消息菜单；窄频道标题与操作分行排列。
  本地截图位于 `test-results/channel-messages/`，均为合成内容。
- 桌面 `pnpm check`、`pnpm test`、`pnpm build` 通过；控件审计为
  0 处未经批准的原生控件。共享核心缓存 8 项专项、daemon 展示 10 项专项通过；
  daemon 原有独立 Electron 集成用例保持默认忽略，测试编译使用包级 `opt-level=0`。
- iPhone 17e / iOS 27.0 Simulator 上，发送状态单测 5 项、频道 UI 测试 7 项通过；
  包含发送中/确认后的右侧位置与长按菜单核对。iOS 23 种本地化检查通过。
  菜单入口移除后的 SwiftUI 回归复用此前构建的 Rust 桥接产物，未重新构建桥接。
- 真实生产消息、系统推送、iOS 真机、Windows/Linux 未在本次验证中运行。

## 2026-11-01 通知设置增量

基于 `main@c7fb38bd`。通知页移除独立账户选择器，读写跟随当前会话；
账户 ID 使用共享 `Id`，仅查看与复制。Agent Mail 使用带 legend 的 fieldset
和逐行列表，复用 `Checkbox`、`Id`，长名称换行，无 Agent 时显示明确空态。

- `pnpm check`、151 项 `pnpm test`、`pnpm build` 通过；`pnpm audit:controls`
  报告 0 处未经批准的原生控件。
- `DMSG_DESKTOP_SMOKE=notifications DMSG_DESKTOP_NOTIFICATION_FOCUS=mock node scripts/smoke.mjs`
  通过：当前账户读写、会话切换与旧读取回包隔离、空格键选择、等待时禁用、
  保存失败保留草稿与重试、空 Agent/无账户状态，以及现有通知联动与关联确认。
- 检查英文常规窗口、中文明暗主题、德语长文案、阿拉伯语 RTL 的截图，
  并检查保存等待和空列表状态；窄窗口为 760×540，长名称含无空格文本。
  截图在本地 `test-results/notifications-*.png`，不提交测试账户图片。
- 原生焦点模式未通过环境前置检查（窗口可见但 macOS 不授予焦点），
  通过的回归使用现有焦点模拟模式；真实系统通知、远端推送、签名发布包和
  Windows/Linux 未在本次验证中运行。

## 2026-11-01 Beta 更新通道增量

基于 `main@83f6548b` 的工作区增量。「关于 dMsg」复用共享 `Checkbox`、
`Notice`、`Pill` 和 `buttonClass`；复选框通过 `aria-describedby` 关联说明，
版本号使用 LTR 隔离。通道偏好仅由主进程写入，不经 daemon 或账户同步。

- 桌面 `pnpm check` 通过；`pnpm test` 151 项通过；`pnpm audit:controls`
  报告 0 处未经批准的原生控件。
- `DMSG_DESKTOP_SMOKE=updates pnpm test:e2e` 在隔离 Electron 中通过：
  默认稳定通道、键盘切换、实际偏好保存、保存失败后的草稿与重试、等待时禁用、
  页面切换后的迟到回包、检查/下载/就绪/失败/等待正式版、Homebrew 限制。
- 检查 `test-results/updates-*.png` 七张截图：中文、法语长文案、阿拉伯语 RTL
  各覆盖明暗主题与 760×540，另有中文明色 1180×760；检查换行、边界和 Tab 焦点。
- 更新选择和取消竞态由单元测试覆盖；真实 GitHub 发布、macOS 签名包覆盖安装、
  Windows 安装和 PowerShell 执行未在本次本地验证中运行。

## 整改结果

| 编号 | 结果 | 实现与保留的行为 |
| --- | --- | --- |
| NC-01 保险库附件选择 | 已替换 | `Checkbox` 统一条目与附件；条目有可访问名称，busy 时禁用；不支持的附件与未选父条目的附件不可选，提交仍按所选父条目过滤 |
| NC-02 钱包终止激活 | 已替换 | `Dialog` 展示目标请求和后果，取消默认聚焦；取消不调用接口，失败留在弹窗内可重试，等待期间禁止重复提交和关闭，账户改变清理状态 |
| NC-03 密码生成器 | 已替换 | `Slider` 统一轨道、滑块、数值、禁用态与键盘；词数、随机密码和 PIN 各自保留范围；弹层按可用高度滚动 |
| NC-04 泄露起点 | 已替换 | `DateInput` 显示本地时区，支持手动输入和日历；保留本地时间转换，阻止无效/未来时间提交 |
| NC-05 文件/钱包日期筛选 | 已替换 | 共享日期控件保留日历字符串；可清空，无效日期不给出筛选结果，钱包倒置范围和无效日期禁止导出 |
| NC-06 保险库日期/年月 | 已替换 | 按字段类型使用日期/月份选择器，月份仍序列化为 `YYYY-MM`；无效日期阻止保存（含快捷键），普通字段用受限类型的 `TextField`，秘密字段保留 `SecretInput` |
| NC-07 文件分享密码 | 已替换 | `SecretInput` 统一掩码与显隐；失败保留草稿，成功清除密码，关闭后重开为空；迟到结果不覆盖新弹窗 |
| NC-08 快照上传进度 | 已替换 | `Progress` 统一主题、可访问值和百分比；未知总量显示不确定进度，不伪报 0% 或完成 |
| NC-09 设置表单 | 已统一 | `TextField` / `TextArea` 覆盖 13 个单行、4 个多行源码位置，标签与输入关联、说明/错误通过 ID 关联；只读值有独立外观，请求令牌提供独立复制动作 |

## 设置页落实情况

- **会话与账户**：加入账户使用持久标签和共享输入，窄窗口可换行，不再只靠 placeholder。
- **安全**：请求、设备 ID、执行限额和硬件令牌统一；限额错误在字段下显示，等待时禁用编辑，原来的审批窗口边界不变；只读请求令牌标为“请求令牌”，提供复制按钮。
- **公开资料**：昵称、简介、地址与链接全部使用共享输入；身份类型补可见标签；查询账户补持久标签；链接组用统一行列、间距和分隔线，移除动作有明确名称。
- **旧数据迁移**：Principal 核对字段使用共享输入与持久标签，保留确认勾选和忙碌状态。
- **语言、外观、设备和文件设置**：继续使用已有 `Select`、`Radio`、`Checkbox`、`Segmented`；不重复建设一套选择控件。

## 共享组件与安全边界

组件位于 `src/renderer/components`：`TextField`、`TextArea`、`DateInput`、`Slider`、`Progress`，以及扩展的 `SecretInput`、`Checkbox`、`page/Field`。
日期和生成器弹层有可用高度限制；Escape 先退出弹层，模态表单交由 `Dialog` 管理关闭层级，避免一次按键把整个表单关闭。

`TextField` 的动态 input 只允许 text/search/url/email/tel，检查器单独识别这一个共享实现；业务页面的历史例外全部移除。
原生文本语义和 bits-ui 内部节点仍保留，统一的是外观、标签、状态与操作，而不是以 div 模拟输入。
OS 文件/目录选择、系统身份确认、主进程路径校验和审批权限均保持原边界。

## 检查与复查方法

```sh
pnpm --dir src/dmsg_desktop audit:controls
pnpm --dir src/dmsg_desktop audit:controls --json
pnpm --dir src/dmsg_desktop check
pnpm --dir src/dmsg_desktop test
pnpm --dir src/dmsg_desktop build
# 在桌面目录内，对最新构建执行：
node scripts/smoke.mjs
node scripts/secrets-smoke.mjs
DMSG_DESKTOP_SMOKE=forms node scripts/smoke.mjs
```

检查器扫描当前 **139 个 Svelte 文件与 49 个 renderer TypeScript 文件**，结果为 **0 个未批准用法**。
设置的四个有文本输入的子页面不得再直接定义 input/textarea；这一要求也由 `ui-controls.test.ts` 检查。
当前剩余的直接 HTML 元素是主题化文本输入、文本域和数字输入；没有直接原生 checkbox/radio/range、date/month/datetime-local、password、select/datalist、progress/meter 或浏览器确认框。

交互脚本覆盖：钱包取消/失败/重试/等待关闭；无效日期、范围倒置、日历与月份选择；Slider 的 Home/End/方向键与 RTL；附件禁用约束；分享密码显隐、失败保留、成功清理与迟到结果隔离；设置字段名称、错误和编辑草稿；已知/未知总量的进度语义。

视觉矩阵在实际 Electron renderer 中覆盖中文明暗、法语长标签、阿拉伯语 RTL，含主窗口 760×540 的最小内容尺寸；截图在忽略的 `test-results/controls-*.png` 与 `test-results/secrets/controls-upload-progress.png` 中。
脚本检查横向尺寸与弹层完整边界；已人工复核设置、确认弹窗、日历、滑块和进度条的代表性截图，修正了窄窗口裁切和错误文字导致的按钮错位。

本次验证结果：

- Svelte / TypeScript 检查通过，Svelte 0 errors / 0 warnings；构建成功。
- 124 项桌面单元测试通过，原生控件检查 0 个未批准用法。
- 完整 `smoke.mjs` 通过；最终设置布局另由 `DMSG_DESKTOP_SMOKE=forms` 实际界面回归通过。
- `secrets-smoke.mjs` 通过，额外验证确定/未知上传进度的可访问值；审批、挂载、快照、导入、迟到响应隔离和键盘行为保持通过。
- 检查直接调用已安装的 Node 工具执行，避免本机 Corepack 在受限目录创建临时目录失败；命令内容与 package scripts 一致。截图与日志为本地验证产物，不提交测试账户图片到源码仓库。

本次不代表真实云端交易、硬件身份确认、原生磁盘挂载、跨操作系统或读屏软件验收。运行中的用户客户端和 daemon 未被替换。

## 2026-11-02 文件传输状态复查

- 文件任务统一使用共享 `Progress`：上传字节发完后显示云端确认，下载字节收齐后显示校验，均不提前显示完成。重试、取消、锁定提示及云端备份页面的任务卡片已接入；失败的状态读取保留上次结果，账号切换清理旧任务展示。
- `pnpm check` 通过（0 errors / 0 warnings），`pnpm test` 169 项通过，`pnpm audit:controls` 为 0 个未批准用法，最终构建通过。
- 最终构建运行 `DMSG_DESKTOP_SMOKE=transfers node scripts/smoke.mjs`，验证排队、确认/校验阶段、失败重试、键盘 Enter、取消、锁定说明与稳定任务标识。中文明暗、法语长标签、阿拉伯语 RTL 覆盖 760×540 最窄窗口；补充任务卡片边界断言并检查截图，修正长文件名与字节进度的挤压裁切。
- 截图位于忽略的 `test-results/controls-transfer-*.png`。本次通过真实 Electron renderer/main IPC 和可控 relay fixture 验证，不代表生产云端 100/1000 MiB 吞吐、运行中的用户 daemon 或跨平台验收。

## 当前全量源码索引

下列行号来自整改后的源码；每个源码位置计一次，循环和共享组件调用不展开。重新扫描可更新 JSON 位置。

| 源码（相对 `src/renderer`） | 元素与行号 |
| --- | --- |
| [components/DateInput.svelte](../src/renderer/components/DateInput.svelte) | `input:text`: 91 |
| [components/SecretInput.svelte](../src/renderer/components/SecretInput.svelte) | `textarea`: 40 |
| [components/TagInput.svelte](../src/renderer/components/TagInput.svelte) | `input:text`: 71 |
| [components/TextArea.svelte](../src/renderer/components/TextArea.svelte) | `textarea`: 41 |
| [components/TextField.svelte](../src/renderer/components/TextField.svelte) | `input:dynamic`: 47 |
| [views/Approval.svelte](../src/renderer/views/Approval.svelte) | `input:text`: 895 |
| [views/Channels.svelte](../src/renderer/views/Channels.svelte) | `input:search`: 554, 772；`input:text`: 922；`input:number`: 961, 966 |
| [views/Files.svelte](../src/renderer/views/Files.svelte) | `input:search`: 826；`input:text`: 1026, 1038；`input:number`: 1113；`textarea`: 1152 |
| [views/Vault.svelte](../src/renderer/views/Vault.svelte) | `input:search`: 427 |
| [views/Wallet.svelte](../src/renderer/views/Wallet.svelte) | `input:text`: 772, 993, 1054 |
| [views/agents/AgentIds.svelte](../src/renderer/views/agents/AgentIds.svelte) | `input:text`: 173 |
| [views/agents/Capabilities.svelte](../src/renderer/views/agents/Capabilities.svelte) | `input:text`: 177 |
| [views/agents/Events.svelte](../src/renderer/views/agents/Events.svelte) | `input:text`: 173, 177 |
| [views/agents/Knowledge.svelte](../src/renderer/views/agents/Knowledge.svelte) | `input:text`: 208, 365 |
| [views/agents/Mail.svelte](../src/renderer/views/agents/Mail.svelte) | `input:text`: 378 |
| [views/agents/Overview.svelte](../src/renderer/views/agents/Overview.svelte) | `input:text`: 304, 307, 309, 376, 377, 381, 421, 465, 469, 470, 503, 521；`input:number`: 378, 504 |
| [views/approvals/Sign.svelte](../src/renderer/views/approvals/Sign.svelte) | `textarea`: 168；`input:text`: 173, 175, 176, 179 |
| [views/channels/LibraryPicker.svelte](../src/renderer/views/channels/LibraryPicker.svelte) | `input:search`: 39 |
| [views/channels/Members.svelte](../src/renderer/views/channels/Members.svelte) | `input:text`: 84, 221；`textarea`: 85 |
| [views/channels/NewChannel.svelte](../src/renderer/views/channels/NewChannel.svelte) | `input:text`: 107, 119, 136；`textarea`: 141 |
| [views/channels/Packet.svelte](../src/renderer/views/channels/Packet.svelte) | `textarea`: 65, 68 |
| [views/channels/Thread.svelte](../src/renderer/views/channels/Thread.svelte) | `textarea`: 529 |
| [views/channels/Transfer.svelte](../src/renderer/views/channels/Transfer.svelte) | `textarea`: 101 |
| [views/files/Detail.svelte](../src/renderer/views/files/Detail.svelte) | `textarea`: 355, 543 |
| `views/files/Storage.svelte`（2026-10-28 已移除） | 原 `input:text`: 93 |
| [views/inbox/Compose.svelte](../src/renderer/views/inbox/Compose.svelte) | `input:text`: 77；`textarea`: 81 |
| [views/inbox/Rules.svelte](../src/renderer/views/inbox/Rules.svelte) | `input:text`: 85 |
| [views/plan/Names.svelte](../src/renderer/views/plan/Names.svelte) | `input:text`: 252, 387, 408；`textarea`: 432 |
| [views/secrets/Activity.svelte](../src/renderer/views/secrets/Activity.svelte) | `input:search`: 66 |
| [views/secrets/Health.svelte](../src/renderer/views/secrets/Health.svelte) | `input:search`: 171 |
| [views/secrets/Mounts.svelte](../src/renderer/views/secrets/Mounts.svelte) | `input:text`: 248, 252, 315, 344, 353；`textarea`: 335；`input:number`: 390 |
| [views/secrets/Policies.svelte](../src/renderer/views/secrets/Policies.svelte) | `input:text`: 163, 167, 175；`input:search`: 250 |
| [views/secrets/Sharing.svelte](../src/renderer/views/secrets/Sharing.svelte) | `input:number`: 249 |
| [views/vault/Conflict.svelte](../src/renderer/views/vault/Conflict.svelte) | `textarea`: 167 |
| [views/vault/FilePicker.svelte](../src/renderer/views/vault/FilePicker.svelte) | `input:search`: 43 |
| [views/vault/Generator.svelte](../src/renderer/views/vault/Generator.svelte) | `input:text`: 96 |
| [views/vault/ItemForm.svelte](../src/renderer/views/vault/ItemForm.svelte) | `textarea`: 197, 284；`input:text`: 219, 234, 262 |
| [views/wallet/AmountField.svelte](../src/renderer/views/wallet/AmountField.svelte) | `input:text`: 43 |
| [views/wallet/Create.svelte](../src/renderer/views/wallet/Create.svelte) | `input:text`: 223, 278 |
| [views/wallet/DurationField.svelte](../src/renderer/views/wallet/DurationField.svelte) | `input:number`: 14 |
| [views/wallet/History.svelte](../src/renderer/views/wallet/History.svelte) | `input:text`: 104 |
| [views/wallet/Manage.svelte](../src/renderer/views/wallet/Manage.svelte) | `input:text`: 201, 236, 237, 245, 277, 281；`textarea`: 205；`input:number`: 264 |
| [views/wallet/MemberLookup.svelte](../src/renderer/views/wallet/MemberLookup.svelte) | `input:text`: 42 |
| [views/wallet/Operations.svelte](../src/renderer/views/wallet/Operations.svelte) | `input:text`: 185；`input:url`: 314, 315, 318；`input:number`: 317 |
| [views/wallet/Policy.svelte](../src/renderer/views/wallet/Policy.svelte) | `input:number`: 44, 71, 108, 155, 158；`input:text`: 73, 84；`textarea`: 95 |
| [views/wallet/PolicyUsage.svelte](../src/renderer/views/wallet/PolicyUsage.svelte) | `input:text`: 78 |
| [views/wallet/Proposal.svelte](../src/renderer/views/wallet/Proposal.svelte) | `input:text`: 266, 288；`textarea`: 362, 394 |
| [views/wallet/Transfer.svelte](../src/renderer/views/wallet/Transfer.svelte) | `input:text`: 189, 206, 236, 239；`textarea`: 231 |
