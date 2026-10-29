# 系统通知实现与验证记录

核对日期：2026-10-29。本页记录公开客户端的本地实现与检查边界，不代表生产推送或全部平台已验收。终态设计和批次方案在本地 `misc/`，公开合同见 [dmsg-notifications/1](../../../docs/protocol/notifications_zh.md)。

## 本地链路

客户端核心保存账户通知投影、设置、阶段与展示记录，daemon 通过 owner 接口提供列表、计数、实际查看、稍后提醒和原生展示计划。原生消费者取得计划前先登记尝试；系统提交与用户查看、业务已读、审批完成相互独立。仅运行 daemon 不消费展示资格。

桌面只通过 [主进程适配器](../src/main/notifications.ts) 提交通知。文案是固定本地化文本，不显示账户、频道、发信人、正文或审批载荷。消息、审批、恢复与服务的频率和声音预算来自共享 Rust 核心，不由 renderer 再计算。

全局工具区提供“提醒”，设置提供通知子页。锁定时仍能读取最小元数据；通知点击绑定原账户，切换和解锁由用户明确完成。浏览提醒列表不提交查看回执，业务详情的回报还须通过可见区域、页面焦点、主进程窗口焦点和会话代次检查。

旧 `RecoveryNotifications`、`notifyChannel`、MCP/Agent Mail/hint 的独立通知分支、`remindPlan` 及其定时器已经移除。旧 `reminders.json` 只在迁移入口读取一次，无法确认账户归属的合同阶段保留在安装运行状态中，直到认证业务读取确认合同。

## 已运行的检查

| 检查 | 本轮结果与边界 |
| --- | --- |
| Rust 核心与 daemon 通知测试 | 覆盖持久化、重放、静音、阶段、期限、预算、实际查看、账户边界、收件箱分页及旧阶段导入 |
| `pnpm check` | Svelte 和 TypeScript 检查通过；仍须在后续改动后按需复测 |
| `pnpm test` | 136 项通过；一次既有断线时序测试失败后，单独复测和全套复测通过，未修改其断线语义 |
| Electron 构建 | 通过 |
| 隔离 Electron 通知流程 | 通过，原生焦点显式模拟；覆盖等待、冲突草稿、禁用、空账户、前台提示、实际查看与业务已读分离、原账户路由及键盘操作 |
| Electron 恢复告警短流程 | 通过，系统通知 API 为模拟；未执行该脚本的其余业务流程 |
| 760 × 540 界面截图 | 已生成中文明暗、德文、法文、阿拉伯语及乌尔都语截图；检查代表性截图，没有页面横向溢出；控件沿用共享组件 |
| 实际原生焦点 | 本机测试窗口可见且可获得 DOM 焦点，但 `BrowserWindow.isFocused()` 仍为 false；未通过真实原生焦点验收 |
| OS 通知和提供者 | Electron 测试替换了系统通知 API。未据此认定真实 macOS、Windows、Linux、APNs 或 FCM 投递成功 |

原生焦点模拟只存在于测试脚本的显式开关中，产品校验没有被放宽。测试同时确认：仅有 DOM 焦点时主进程不接受实际查看回报。原生焦点模拟不能代替安装包、OS 权限、Focus、锁屏或真实通知中心的测试。

## 复现入口

在桌面目录运行：

```sh
pnpm check
pnpm test
pnpm build
DMSG_DESKTOP_SHOW=1 DMSG_DESKTOP_SMOKE=notifications node scripts/smoke.mjs
```

测试宿主无法提供原生焦点时，只为界面与协议集成验证显式使用：

```sh
DMSG_DESKTOP_SHOW=1 DMSG_DESKTOP_NOTIFICATION_FOCUS=mock DMSG_DESKTOP_SMOKE=notifications node scripts/smoke.mjs
```

该流程使用临时用户目录、模拟 daemon 与模拟系统通知，检查草稿保留、乐观版本冲突、权限边界、查看与业务已读分离、前台提示和原账户导航。生成的截图位于已忽略的 `test-results/`。

## 尚需完成

- 服务端最小投递、登记与偏好确认、多账户接收、多设备回执和移动端对接按后续批次交付；本地界面当前明确标出远端能力未就绪及设置待同步。
- 完成真实签名安装包下的原生焦点、权限拒绝、通知展示、历史点击和冷启动验证。Electron 的 macOS 通知需要代码签名，API 支持不等于权限或投递成功。参见 [Electron Notification](https://www.electronjs.org/docs/latest/api/notification)。
- Windows、Linux 与移动真机验证独立记录，不能由 macOS 的模拟通知结果替代。
