# 桌面表单统一：整改与验证记录

更新日期：2026-10-26。本次整改基于 `main@777a7c3f`；原始排查的界面基线为 `643693a5`。
原始 9 项整改、11 处特殊控件/交互，以及设置的 17 处直接文本输入均已完成代码整改。
实际交互验证使用构建后的 Electron 与隔离的本地 mock daemon，不操作真实账户。

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
