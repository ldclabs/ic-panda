# 问题反馈验证记录

日期：2026-11-14。范围：桌面设置与 iOS 账户页的“反馈问题”、共用 Rust 草稿与发送流程。
收件账号：`dbrt466u97gecq800000`。本记录说明本地验证，不代表生产部署或真实投递验收。

## 已验证

| 检查 | 结果 |
| --- | --- |
| `cargo test -p dmsg_client feedback --locked` | 6 项通过；加密草稿重启恢复、保存幂等、原 ID 重试、固定截图版本、上传计划恢复、丢失分享回执的对账、不可变来信与成功状态判断 |
| `cargo test -p dmsg_client library::link::tests --locked` | 10 项通过；1 个既有打印测试向量的工具函数按标记忽略 |
| `cargo test -p dmsg_client letters::tests --locked` | 3 项通过 |
| `cargo test -p dmsg_mobile --lib --locked` | 34 项通过 |
| `cargo clippy -p dmsg_client -p dmsg_cli -p dmsg_mobile --all-targets --locked` | 完成；保留 `file_library.rs::create_file_link` 既有参数数量警告；本次代码无新增 Clippy 警告 |
| 桌面 `pnpm check` / `pnpm test` / `pnpm build` | 类型检查与构建通过，200 项测试通过 |
| 桌面 `pnpm audit:controls` | 0 个未批准控件用法 |
| `DMSG_DESKTOP_SMOKE=feedback node scripts/smoke.mjs` | 隔离 Electron 与 mock daemon；空表单、导航和重载恢复、剪贴板图片导入、失败与原请求重试、新反馈、迟到保存、丢失保存回执后继续编辑、超长 UTF-8 草稿与键盘提交通过 |
| `bash scripts/ios/build-core.sh` / `python3 scripts/ios/artifacts.py verify` | host、ARM64 iOS 与模拟器 Rust 库及 UniFFI 绑定构建完成，源码指纹一致 |
| iOS Debug / Release Simulator 构建 | 通过；Rust bridge 使用本次 debug profile，未制作签名发行包 |
| iOS `FeedbackTests`、`InboxTests`、`MessagingTests`、`FeedbackUITests` | 14 项通过，0 失败；末轮 3 项定向复核也通过；iPhone SE（第 3 代）/ iOS 18.4 Simulator |
| `scripts/ios/localizations.py --derived-data …` | 23 种语言、格式与 Swift 提取结果通过 |
| `git diff --check` | 通过 |

桌面已检查 760×540 最窄窗口的中文、德文长文案、阿拉伯语 RTL，均覆盖明暗主题、输入区、截图区和失败重试状态。计数器与账号方向独立于 RTL；图片不会自动上传。

iOS 已检查同三种语言的明暗主题、空表单、文字与截图、失败保留、成功及新反馈状态。键盘可通过“完成”收起，截图使用完整比例预览。原生测试使用内存界面 fixture；“Add test screenshot”仅存在于 DEBUG 且 `--ui-testing` 启用时，不进入 Release 界面。持久化与网络恢复由上表 Rust 测试另行覆盖。

桌面截图在本地 `test-results/feedback-*.png`；iOS 截图作为 XCTest 附件保存在结果包。图像与构建产物不提交到仓库。

## 验证边界

- iOS 18.4 测试在进入反馈页时记录了 SwiftUI `Invalid frame dimension (negative or non-finite)` 警告。相关测试通过，已检查的截图未见遮挡或溢出；警告来源尚未定位，不能称为零运行时警告。
- 未运行真机、iPad、VoiceOver 或真实 OS 图片选择器验收；图片选择/导入的系统部分继续复用现有实现。
- 未向生产收件账号发送测试反馈。公开策略读取在本环境返回 HTTP 403 / Cloudflare 1010，不能据此判断该账号的联系模式。
- 上线前仍需确认支持账号已发布有效免费收件策略，并实际验证一封文字及带截图来信。后续反馈仍受普通 pending-pair 和存储配额约束；支持人员处理后需重新允许来信。
