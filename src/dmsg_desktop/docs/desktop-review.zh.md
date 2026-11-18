# 桌面端代码复核

2026-11-18，基于桌面版 1.5.8（`2d0cd255`）及其隐式状态机修复。核对了主进程、preload、共享合同、业务视图和控件，以及测试、打包入口和原生 FSKit 模块。当前窗口白名单开放 390 个方法，另有 3 个钱包创建相关方法受开关控制，均能在 CLI 源码中找到对应方法名。preload 使用的 IPC 调用均有主进程处理器。具体行为的验证范围见下列回归。

## 本轮修复

| 用户操作 | 修复及回归入口 |
| --- | --- |
| 搜索频道消息、打开上下文、隐藏消息和切换频道 | 旧回复不能改写新频道的消息或搜索结果。离开频道前提交尚在防抖期的草稿。已读写入失败后，再次看到末尾可以重试。[频道](../src/renderer/views/Channels.svelte) · [Electron 回归](../scripts/navigation-regressions.mjs) |
| 在命令面板搜索文件、切换语言或主题 | 换词即撤下旧文件，当前查询失败可重试。写入语言或主题设置失败时显示对应提示。匹配排名只保留每档所需结果，并保持同分项原序。[命令面板](../src/renderer/shell/CommandPalette.svelte) · [排序](../src/renderer/shell/palette.ts) |
| 浏览或移动文件夹 | 树和移动选择器按父目录建索引，保留本地化数字排序和子目录排除规则。详情读取失败时显示重试，避免把读取失败解释为空文件夹。目录 ID 可完整复制。[文件夹详情](../src/renderer/views/files/FolderDetail.svelte) · [树与筛选回归](../tests/files.test.ts) |
| 用键盘编辑表单 | 输入法选词回车保留在输入框。文件改名与按钮共用提交条件。标签长度按每个标签校验，可一次粘贴多个。授权参数编辑器中，键盘激活取消会关闭编辑器。窄窗聚焦长表单底部时，仅正文滚动，标题、关闭与提交仍可见。[共享对话框](../src/renderer/components/Dialog.svelte) · [文件表单](../src/renderer/views/Files.svelte) · [标签](../src/renderer/components/TagInput.svelte) · [授权表单](../src/renderer/views/agents/Capabilities.svelte) |
| 修改本机设置、立即退出、点击系统通知 | 文件与闲置锁设置按顺序读取、写入和重新加载，使用原子替换，读取失败保留原文件。退出时保存尚在等待的窗口状态，提醒中心可在下次启动恢复。通知撤下后会释放去重记录，并忽略旧通知的点击。[设置回归](../tests/preferences.test.ts) · [窗口恢复](../src/main/desktop-state.ts) · [通知回归](../tests/notifications.test.ts) |
| 网络故障期间刷新 | 一次刷新失败后，等待中的最新读取仍会执行。反馈读取、挂载配置列表和钱包 RPC 设置读取已与各自的写操作区分，可按原有只读恢复策略处理。[合并刷新回归](../tests/single.test.ts) · [RPC 恢复回归](../tests/recovery.test.ts) |

删除了桌面端无调用方的 MIME 缩写、掩码常量、钱包余额分支和多余导入。钱包创建表单复用已有角色列表，桌面的 `pnpm check` 已包含未使用局部符号检查。

在相同输入上比较基线和现实现，遍历 1,000 个全部展开的合成目录时，读取父目录字段的次数从 1,100,000 次降至 1,100 次，显示顺序一致。对 5,000 个同前缀候选取前 20 项时，命令面板的评分次数从 5,000 次降至 20 次，结果和顺序一致。这里统计的是算法操作次数。

## 六项评审决定

| 项目 | 已实施的行为与回归入口 |
| --- | --- |
| 大账户的名称刷新 | 已解锁且可见的窗口每 10 分钟最多重读一次当前资料和显示中的名称。短时间内多次切回窗口共用这次读取，滚动或离页会释放旧行。名称发布和待完成查询的事件仍能及时更新界面，搜索可按需补齐初次索引。[触发位置](../src/renderer/shell/AccountSwitcher.svelte) · [名称批读](../src/renderer/views/channels/names.svelte.ts) · [60 位发件人回归](../scripts/channel-messages-smoke.mjs) |
| 敏感操作失败后的确认框 | 提交中锁定表单，明确被拒绝时原地显示原因，保留对象、选择和输入。结果未知、仍在处理或回执已过期时，主操作改为“查看状态”，重读相应列表或打开审批、付款记录。回归核对实际写入次数和提交的对象及撤销理由。[确认框](../src/renderer/components/Confirm.svelte) · [Electron 回归](../scripts/confirmation-flow.mjs) |
| macOS 机密文件挂载 | 生产卷和合成探针均迁移到 `FSVolume.Handler` 与 `FSVolume.ReadWriteHandler`。读取回执返回实际字节数，文件查询和读取一并返回属性。真实挂载验证使用开发签名扩展。公开发行包嵌入扩展仍需分发签名和对应描述文件。[原生模块与验收](../native/fskit/README.md) |
| 桌面新建多签钱包 | 继续暂停创建。策略测试固定此产品决定，并检查创建、报价、报价预览三个 RPC 的窗口访问限制。已有钱包管理、加入及旧激活单的继续处理和退款各有入口。[产品开关](../src/shared/wallet-availability.ts) · [策略测试](../tests/policy.test.ts) |
| Electron 本机回归 | 在有图形会话的本机运行 `pnpm verify:local`。完成类型检查、单测和一次构建后，依次运行十组 Electron 回归，避免多个测试争用窗口焦点。可单独重跑敏感操作或原生全屏。[本机运行器](../scripts/local-regressions.mjs) · [运行说明](../README.md) |
| Node 测试的模块类型 | 桌面包显式声明 ESM。Electron 主进程与沙箱 preload 编译为 `.cjs`，由包的 `main` 和窗口设置指定。Playwright 从包入口启动，覆盖与打包一致的入口解析。[包入口](../package.json) · [构建格式](../electron.vite.config.ts) |

## 验证记录

- `pnpm check` 的 Svelte 和 TypeScript 检查通过。207 项单测全部通过，无跳过，Node 未报告模块类型推断警告。`pnpm audit:controls` 未发现未经批准的原生控件。
- 旧构建中，新查询等待期间仍可选中上一次找到的文件。同一回归在修复后通过，导航专项另验证了立即退出后的 `desktop.json`。[导航回归](../scripts/navigation-regressions.mjs)
- 本机十组 Electron 功能回归通过，涵盖 23 种语言和两个窗口。解锁 macOS 后，原生全屏专项也通过了普通窗口隐藏与恢复、全屏关闭后重开、退出全屏途中重开三项检查。最后一种情况下，重开的窗口保持可见。[本机运行器](../scripts/local-regressions.mjs) · [原生全屏专项](../scripts/window-flow.mjs)
- 解锁后使用真实窗口和页面焦点重跑通知专项。聚焦收件箱详情会回报当前版本已显示，邮件的未读状态保留。窗口切到后台后走系统通知路径。该测试使用模拟系统横幅。[通知回归](../scripts/notifications-flow.mjs)
- 60 位合成发件人的频道在窄窗滚动前后，每次刷新读取的作者均少于 20 位。一分钟后切回窗口没有重复读取，超过 10 分钟才重读当前视口，离开频道后回到其他界面不会重读历史作者。真实 Rust daemon 与 Electron 的缓存资料及同会话重连测试通过，旧钱包合成审批记录也通过键盘打开和只读详情回归。[频道回归](../scripts/channel-messages-smoke.mjs) · [缓存与重连](../scripts/cache-real.mjs) · [旧审批记录](../scripts/approval-history-smoke.mjs)
- 对命令面板、频道搜索、文件夹统计、文件夹命名与保险库标签检查了 20 张截图，覆盖 1180 × 760 中文浅色，以及 760 × 540 中文深色、法语浅色、阿拉伯语 RTL 深色。两类确认结果另检查了同一矩阵下的 8 张截图，并通过键盘完成重试与查看状态。输入法用例模拟组合输入中的键盘事件。
- 生产 FSKit 模块和探针均通过 macOS 27.2 SDK 将警告视为错误的类型检查。开发签名扩展完成真实挂载、授权读取、到期、撤销和停止。跨用户读取子项因本机 sudo 需要密码而跳过。[原生验收记录](../native/fskit/README.md)
- 在 macOS arm64 上从本地 ad hoc 签名 DMG 取出应用，用临时配置启动。包内的主进程、沙箱 preload 和中文登录页均已加载，应用安装并连通随包的真实 `dmsg 1.5.8`，读取到未登录状态。此项覆盖本机开发包启动。分发公证、生产服务以及 Windows、Linux 未实测。
