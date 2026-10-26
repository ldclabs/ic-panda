# 桌面原生控件排查与整改清单

核对日期：2026-10-26；界面源码基线：`643693a5`，`main`。
本文记录源码现状，不表示以下待办已修复，也不是运行中安装包的视觉验收。

## 范围与口径

- 遍历全部 **134 个 Svelte 文件与 48 个 renderer TypeScript 文件**，覆盖主窗口、独立审批窗口、设置子页面、弹窗与共享组件。另检查 HTML 入口、全局 CSS、主进程系统对话框调用及动态 DOM 创建。
- Svelte AST 统计 **124 个 `input`、26 个 `textarea`、1 个 `progress`** 源码位置；另有 **1 个 `window.confirm`**。循环或可复用组件每个源码位置计一次，不能当作屏幕上的控件个数。
- 实际模板中 **0 个 `select`、`datalist`、原生 radio、file input、color input、meter**；`Select.svelte`、`Suggest.svelte` 注释中的 HTML 示例不计。bits-ui 可能生成隐藏的表单节点，不等于用户看见了原生控件。
- `app.css:157–200` 统一设置文本框/多行框的尺寸、颜色、边框、焦点、禁用态，并移除数字输入框默认 spinner。因此 **直接使用 HTML 元素、未封装共享输入组件、保留浏览器默认外观是三个不同问题**。
- 明确需要统一的控件/交互共有 **11 个源码位置，涉及 9 个文件**。另列设置表单的标签和布局问题，不能用“未发现原生下拉框”概括设置体验。

## 优先整改项

| 编号 | 优先级 | 功能与源码 | 现状及影响 | 修复要求 |
| --- | --- | --- | --- | --- |
| NC-01 | P1 | [保险库导入附件](../src/renderer/views/vault/Import.svelte#L130) | 1 个原生 checkbox；同一弹窗的条目选择已用 `Checkbox`，附件选择外观不一致 | 复用 `Checkbox`，保留附件选择、不可用原因、父条目选择约束与 busy 禁用状态 |
| NC-02 | P1 | [钱包撤销激活](../src/renderer/views/Wallet.svelte#L1002) | 1 个 `window.confirm`；阻塞式浏览器确认，不能遵循应用的确认布局、按钮命名与状态反馈 | 使用 `Dialog`，说明作用于哪个激活请求，取消默认聚焦，提交等待/失败可见，保留实际审批与执行边界 |
| NC-03 | P2 | [密码生成器](../src/renderer/views/vault/Generator.svelte#L93) | 两个原生 range：词数、密码/PIN 长度；仅加 `accent-forest`，还被通用文本 input CSS 命中 | 实现共享 Slider，统一轨道、滑块、焦点和禁用态，保留 min/max/step、键盘操作及读出的当前值；核验中文与 RTL |
| NC-04 | P2 | [Agent 泄露起点](../src/renderer/views/agents/Overview.svelte#L449) | 1 个 `datetime-local`，外框受主题控制，日期时间弹层仍由浏览器绘制 | 共享日期时间组件；明确本地时区、校验无效/未来输入，保持现有时间含义 |
| NC-05 | P2 | [文件清理日期](../src/renderer/views/files/Storage.svelte#L96)；[钱包历史起止日期](../src/renderer/views/wallet/History.svelte#L101) | 共 3 个原生 date；弹层、内部日期分段和图标未统一 | 与 NC-04 共用日期基础组件，保留清空、范围边界、查询语义；日期值不得隐式当作 UTC 时间戳 |
| NC-06 | P2 | [保险库字段编辑](../src/renderer/views/vault/ItemForm.svelte#L198) | 1 个动态 input，映射同时包含 date/month 与 text/url/email/tel，简单搜索固定 type 会漏报 | 按字段类型显式选择日期/月组件；普通文本保留语义与主题，秘密字段继续使用 `SecretInput` |
| NC-07 | P2 | [文件分享密码](../src/renderer/views/Files.svelte#L1109) | 1 个 `input type=password`，已统一文本样式且默认掩码，但与凭据快照分享的秘密输入交互不一致 | 复用 `SecretInput`，统一显示/隐藏、关闭清理和错误重试；不要改变密码值或把密码写进日志 |
| NC-08 | P2 | [秘密快照上传](../src/renderer/views/secrets/Sharing.svelte#L319) | 1 个原生 progress，仅加 accent 色；这是本次凭据流程优化后仍遗漏的一处 | 实现共享 Progress，统一 track/fill、高度、动画与状态文本；保留无障碍名称、完成量和总量，区分确定与未知进度 |
| NC-09 | P2 | 设置各子页，见下表 | 17 个文本类原生源码位置仍由页面直接编排；部分只靠 placeholder，尺寸与说明组织分散 | 先统一标签/说明/错误和布局；提取有实际复用需求的 TextField/TextArea 等共享组件，再迁移同类表单；保留原生输入语义 |

P1 指现成组件即可替换的明显一致性问题及确认流程问题；P2 需要共享组件或更完整的表单整理，不表示安全漏洞严重性。

## 设置页面专项核查

设置必须沿 `Settings.svelte` 的组件引用继续检查，不能只扫描入口文件。

| 设置区域 | 直接 HTML 表单位置 | 当前实现与需要处理的点 |
| --- | --- | --- |
| 会话与账户 | [Onboarding.svelte:75](../src/renderer/views/Onboarding.svelte#L75)：1 个 input | 加入账户输入框仅有 placeholder，没有关联的持久标签；补 `Field` / 明确可访问名称与输入说明 |
| 安全 | [Security.svelte:171](../src/renderer/views/Security.svelte#L171)：4 个 input、3 个 textarea | 请求、设备标识、执行限额、硬件请求令牌；已有 `Field`，但控件仍直接编排。将只读值/复制动作与编辑字段区分，统一宽度、说明及失败状态；复选框和恢复天数已用 `Checkbox` / `Segmented` |
| 公开资料 | [Profile.svelte:241](../src/renderer/views/Profile.svelte#L241)：7 个 input、1 个 textarea | 昵称、简介、地址与链接；编辑区大部分用 `Field`。[查询账户框](../src/renderer/views/Profile.svelte#L273)仅有 placeholder，需补持久标签。链接编辑组应统一列宽、换行及删除操作布局 |
| 旧数据迁移 | [Legacy.svelte:168](../src/renderer/views/Legacy.svelte#L168)：1 个 input | Principal 核对字段已用 `Field`；单选和勾选用共享组件，仍需检查长标识的布局与错误说明 |
| 语言与外观 | [Settings.svelte:257](../src/renderer/views/Settings.svelte#L257) | `Select` 与 `Radio` / bits-ui `RadioGroup`，没有直接原生 select/radio；组件间对齐、宽度与选中反馈仍属于视觉验收范围 |
| 文件设置 | [FilesSettings.svelte:69](../src/renderer/views/files/FilesSettings.svelte#L69) | 挂载用 `Checkbox`，回收站期限用 `Select`；缓存区是共享按钮。文件库清理页的 date 另列 NC-05，它不是设置入口里的控件 |
| 设备、浏览器、关于 | `Devices.svelte` / `Settings.svelte` / `About.svelte` | 设备权限用 `Checkbox`，其余为按钮、状态与列表；未发现直接原生表单选择控件 |

所以“设置使用了原生表单元素”是成立的；“设置全部是浏览器默认样式”不成立。
源码表明大部分已有主题和共享选择组件，但不能因此否定视觉上的粗糙感。
本次没有核对正在运行的客户端版本或逐页截图，不能把源码结论直接套到当前安装包。

## 可保留的原生机制

- 普通文本、搜索、数字、URL、邮件、电话输入与 textarea：保留语义和全局样式，并持续统一 Field、标签、状态和布局，不需要仅为消除 HTML 标签就套无意义的包装。
- `SecretInput` 的 textarea 与 `TagInput` 的内部 input：共享组件的实现细节；仍需检查复合组件的名称、焦点与关联方式。
- 主进程的文件/保存/目录选择：`src/main/index.ts` 中 7 处 `showOpenDialog` / `showSaveDialog` 调用，是 OS 文件访问入口，应保留；系统身份确认同样不能被 renderer 自制确认替代。
- 本次源码未发现无 class 的直接 button，但有 class 不保证正确复用了按钮体系；手写导航、列表行和组件内部按钮需结合用途人工检查。

## 防回退与复查

```sh
pnpm --dir src/dmsg_desktop audit:controls
pnpm --dir src/dmsg_desktop audit:controls --json
pnpm --dir src/dmsg_desktop test
```

`audit:controls` 输出待审查的特殊类型与浏览器对话框；JSON 同时输出所有文本输入位置。
`tests/ui-controls.test.ts` 在现有 11 处基线上阻止新增同类用法。整改后同步删去基线；不为通过测试增加豁免。
检查会扫描 Svelte 模板、脚本和表达式中的直接调用，以及 renderer TypeScript 中的直接调用。
它不执行页面，不解析动态 DOM/属性展开的最终类型，也不能证明第三方组件内部、无障碍名称或视觉状态正确；这些仍按 `DESIGN.md` 做实际界面验证。

## 全量源码位置索引

以下是所有直接 input/textarea/progress 及浏览器确认的源码位置；数字是行号，不是渲染数量。共享组件调用与 bits-ui 内部生成的元素不重复计数。

| 源码（相对 `src/renderer`） | 元素与行号 |
| --- | --- |
| [components/SecretInput.svelte](../src/renderer/components/SecretInput.svelte) | `textarea`: 33 |
| [components/TagInput.svelte](../src/renderer/components/TagInput.svelte) | `input:text`: 71 |
| [views/Approval.svelte](../src/renderer/views/Approval.svelte) | `input:text`: 895 |
| [views/Channels.svelte](../src/renderer/views/Channels.svelte) | `input:search`: 554, 772；`input:text`: 922；`input:number`: 961, 966 |
| [views/Files.svelte](../src/renderer/views/Files.svelte) | `input:search`: 819；`input:text`: 1019, 1031；`input:number`: 1105；`input:password`: 1109；`textarea`: 1146 |
| [views/Legacy.svelte](../src/renderer/views/Legacy.svelte) | `input:text`: 168 |
| [views/Onboarding.svelte](../src/renderer/views/Onboarding.svelte) | `input:text`: 75 |
| [views/Profile.svelte](../src/renderer/views/Profile.svelte) | `input:text`: 241, 246, 251, 254, 273；`textarea`: 247；`input:url`: 252, 253 |
| [views/Security.svelte](../src/renderer/views/Security.svelte) | `textarea`: 171, 269, 274；`input:text`: 174, 189, 268, 276 |
| [views/Vault.svelte](../src/renderer/views/Vault.svelte) | `input:search`: 427 |
| [views/Wallet.svelte](../src/renderer/views/Wallet.svelte) | `input:text`: 751, 972, 1024；`dialog:confirm`: 1002 |
| [views/agents/AgentIds.svelte](../src/renderer/views/agents/AgentIds.svelte) | `input:text`: 173 |
| [views/agents/Capabilities.svelte](../src/renderer/views/agents/Capabilities.svelte) | `input:text`: 177 |
| [views/agents/Events.svelte](../src/renderer/views/agents/Events.svelte) | `input:text`: 173, 177 |
| [views/agents/Knowledge.svelte](../src/renderer/views/agents/Knowledge.svelte) | `input:text`: 208, 365 |
| [views/agents/Mail.svelte](../src/renderer/views/agents/Mail.svelte) | `input:text`: 378 |
| [views/agents/Overview.svelte](../src/renderer/views/agents/Overview.svelte) | `input:text`: 307, 310, 312, 379, 380, 384, 424, 470, 474, 475, 508, 526；`input:number`: 381, 509；`input:datetime-local`: 449 |
| [views/approvals/Sign.svelte](../src/renderer/views/approvals/Sign.svelte) | `textarea`: 168；`input:text`: 173, 175, 176, 179 |
| [views/channels/LibraryPicker.svelte](../src/renderer/views/channels/LibraryPicker.svelte) | `input:search`: 39 |
| [views/channels/Members.svelte](../src/renderer/views/channels/Members.svelte) | `input:text`: 84, 221；`textarea`: 85 |
| [views/channels/NewChannel.svelte](../src/renderer/views/channels/NewChannel.svelte) | `input:text`: 107, 119, 136；`textarea`: 141 |
| [views/channels/Packet.svelte](../src/renderer/views/channels/Packet.svelte) | `textarea`: 65, 68 |
| [views/channels/Thread.svelte](../src/renderer/views/channels/Thread.svelte) | `textarea`: 529 |
| [views/channels/Transfer.svelte](../src/renderer/views/channels/Transfer.svelte) | `textarea`: 101 |
| [views/files/Detail.svelte](../src/renderer/views/files/Detail.svelte) | `textarea`: 355, 543 |
| [views/files/Storage.svelte](../src/renderer/views/files/Storage.svelte) | `input:text`: 91；`input:date`: 96 |
| [views/inbox/Compose.svelte](../src/renderer/views/inbox/Compose.svelte) | `input:text`: 77；`textarea`: 81 |
| [views/inbox/Rules.svelte](../src/renderer/views/inbox/Rules.svelte) | `input:text`: 85 |
| [views/plan/Names.svelte](../src/renderer/views/plan/Names.svelte) | `input:text`: 252, 387, 408；`textarea`: 432 |
| [views/secrets/Activity.svelte](../src/renderer/views/secrets/Activity.svelte) | `input:search`: 66 |
| [views/secrets/Health.svelte](../src/renderer/views/secrets/Health.svelte) | `input:search`: 171 |
| [views/secrets/Mounts.svelte](../src/renderer/views/secrets/Mounts.svelte) | `input:text`: 248, 252, 315, 344, 353；`textarea`: 335；`input:number`: 390 |
| [views/secrets/Policies.svelte](../src/renderer/views/secrets/Policies.svelte) | `input:text`: 163, 167, 175；`input:search`: 250 |
| [views/secrets/Sharing.svelte](../src/renderer/views/secrets/Sharing.svelte) | `input:number`: 248；`progress`: 319 |
| [views/vault/Conflict.svelte](../src/renderer/views/vault/Conflict.svelte) | `textarea`: 167 |
| [views/vault/FilePicker.svelte](../src/renderer/views/vault/FilePicker.svelte) | `input:search`: 43 |
| [views/vault/Generator.svelte](../src/renderer/views/vault/Generator.svelte) | `input:range`: 93, 102；`input:text`: 97 |
| [views/vault/Import.svelte](../src/renderer/views/vault/Import.svelte) | `input:checkbox`: 130 |
| [views/vault/ItemForm.svelte](../src/renderer/views/vault/ItemForm.svelte) | `textarea`: 189, 280；`input:dynamic`: 198；`input:text`: 215, 230, 258 |
| [views/wallet/AmountField.svelte](../src/renderer/views/wallet/AmountField.svelte) | `input:text`: 43 |
| [views/wallet/Create.svelte](../src/renderer/views/wallet/Create.svelte) | `input:text`: 223, 278 |
| [views/wallet/DurationField.svelte](../src/renderer/views/wallet/DurationField.svelte) | `input:number`: 14 |
| [views/wallet/History.svelte](../src/renderer/views/wallet/History.svelte) | `input:text`: 100；`input:date`: 101, 102 |
| [views/wallet/Manage.svelte](../src/renderer/views/wallet/Manage.svelte) | `input:text`: 201, 236, 237, 245, 277, 281；`textarea`: 205；`input:number`: 264 |
| [views/wallet/MemberLookup.svelte](../src/renderer/views/wallet/MemberLookup.svelte) | `input:text`: 42 |
| [views/wallet/Operations.svelte](../src/renderer/views/wallet/Operations.svelte) | `input:text`: 185；`input:url`: 314, 315, 318；`input:number`: 317 |
| [views/wallet/Policy.svelte](../src/renderer/views/wallet/Policy.svelte) | `input:number`: 44, 71, 108, 155, 158；`input:text`: 73, 84；`textarea`: 95 |
| [views/wallet/PolicyUsage.svelte](../src/renderer/views/wallet/PolicyUsage.svelte) | `input:text`: 78 |
| [views/wallet/Proposal.svelte](../src/renderer/views/wallet/Proposal.svelte) | `input:text`: 266, 288；`textarea`: 362, 394 |
| [views/wallet/Transfer.svelte](../src/renderer/views/wallet/Transfer.svelte) | `input:text`: 189, 206, 236, 239；`textarea`: 231 |
