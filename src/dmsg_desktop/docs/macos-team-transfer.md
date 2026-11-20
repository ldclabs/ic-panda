# macOS 签名团队过渡

从 `1.6.0-beta.1` 开始，桌面应用、随包 `dmsg` 和 SecretsFS 扩展统一由 Yiwen AI Limited（`63G8VNQPXB`）签名。LDC Labs Limited（`S9Z2WAMAV8`）已发布的 **`1.5.8` 和 `1.5.10` 都是桥接版**。两个版本的应用更新要求和 CLI 校验器都接受 Yiwen。桌面端走 Developer ID 分发，iOS App Store 转移独立进行。

## 签名策略与更新入口

| 对象 | 标识或规则 |
| --- | --- |
| 桌面应用 | `net.dmsg.desktop` |
| CLI / daemon | `net.dmsg.dmsg` |
| FSKit 扩展 | `net.dmsg.desktop.SecretFiles` |
| 新版签名与后续更新 | Yiwen Developer ID Application，精确校验代码标识和签发团队 |
| 旧版稳定客户端的 GitHub Latest 入口 | 已发布的 LDC 桥接版 `1.5.8` 或 `1.5.10` |

Squirrel.Mac 用已安装应用的指定要求验证候选包。用户已运行任一桥接版，即可在「设置 → 关于 dMsg」开启 Beta 更新，接收 Yiwen 的 beta。早于桥接版的 Beta 客户端可能直接发现新包，应先安装任一官方桥接版并保留工作区。Homebrew 的 formula 和 cask 共同升级，Beta 试用请使用官方桌面安装包。[签名策略](../../../scripts/dmsg/macos-signing.json)

截至 2026-11-20，`1.5.10` 已在线，GitHub `/releases/latest` 指向 `1.5.8`。旧稳定版和默认安装脚本从该入口取得可验证的 LDC 包，桥接版再枚举后续完整发布。新发布使用 `make_latest: false`，稳定版 CI 会核对入口仍为上述两版之一。保留两份已签名的桥接资产，避免吊销其签名证书。新构建只使用 `dmsg-release` 环境中的 Yiwen 凭据。[Apple 的 Developer ID 证书说明](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)

## 钥匙串、硬件密钥与 FSKit

守护进程通过 `keyring` 使用基于文件的 macOS 钥匙串（file-based macOS Keychain），服务名为 `dmsg`。此版本保留了服务/账户名称、工作区路径以及已存储的密钥句柄。它不会将桌面端凭据迁移到 iOS App Group、导出硬件密钥、重写所有钥匙串 ACL，或向所有应用授予访问权限。更改应用的指定要求不会重写现有钥匙串项的权限。

当守护进程更换团队时，macOS 可能会逐项请求允许访问旧钥匙串项，例如 `owner:<home>`、账户设备绑定密钥、快速解锁或 Passkey 会话。确认是 Yiwen 签名的 `dmsg` 后，可选择**「始终允许（Always Allow）」**并输入登录密码，以便保存授权。通过 SSH 或无 GUI 会话运行的 LaunchAgent 无法显示钥匙串弹窗，首次授权需在本地桌面登录会话中完成。

从 1.5.9 起，`dmsg start` 在同一次启动中复用 `owner` 凭据。如果已有 daemon，而钥匙串拒绝读取，当前命令立即返回错误。准备好授权时可重新运行。CLI 的启动等待时间为 120 秒，桌面端的 `start` 和 `app-token` 分别为 135 秒和 120 秒。只有钥匙串返回 `NoEntry` 才表示凭据不存在。遇到权限错误时请保留原工作区和钥匙串项。发布 Yiwen 版本前，用单独测试账户的临时凭据验证允许、拒绝及重试。

安全隔区（Secure Enclave）密钥保留其不透明的 CryptoKit 句柄和公钥。此实现**并不**保证每一个现有硬件句柄都能在团队变更后继续可用。发布的准入门槛是一次真实的“旧版 → 桥接版 → Yiwen 版”全流程测试，覆盖硬件锚点（hardware anchor）、Touch ID 解锁、审批签名与硬件协同签名（hardware cosign）。如果任何句柄变得不可访问，请停止团队切换并进行排查；不要将该错误视为替换用户密钥的许可。

FSKit 模块与 daemon 通过各自签名中的 `63G8VNQPXB.net.dmsg.secrets` App Group 连接。该组只保存临时 IPC 套接字。macOS 会校验 Team ID 前缀，这种格式的组无需在开发者网站单独注册。FSKit Module 能力本身需要分发描述文件。[Apple 的 macOS App Group 说明](https://developer.apple.com/documentation/xcode/accessing-app-group-containers)

LDC 开发测试曾为 `net.dmsg.desktop.SecretsFS` 签发描述文件。已发布的 `1.5.8` 和 `1.5.10` 均未内嵌该扩展。Yiwen 首次随包发布时使用独立的 `net.dmsg.desktop.SecretFiles`，这样可以直接在新团队配置能力。扩展产物名为 `SecretsFS.appex`。

## Yiwen 发行配置与验收

1. 在 GitHub 的 `dmsg-release` 环境配置 `MAC_CSC_LINK`（包含 Yiwen Developer ID Application 证书及私钥的 `.p12` 的 Base64 内容）、`MAC_CSC_KEY_PASSWORD`、`APPLE_ID`、`APPLE_APP_SPECIFIC_PASSWORD` 和 `APPLE_TEAM_ID=63G8VNQPXB`。
2. 在 Yiwen 团队为 `net.dmsg.desktop.SecretFiles` 配置 FSKit Module 能力。用本次签名证书生成 **Developer ID 分发描述文件**，将原始 `.provisionprofile` 的 Base64 内容填入新增的 `MAC_FSKIT_PROFILE_BASE64`。CI 安装该 profile，通过 macOS 27 SDK 构建 `SecretsFS.appex`，将它嵌入 `dMsg.app/Contents/Extensions/`。最终更新 ZIP 将校验宿主、daemon、模块及其授权和公证票据。[FSKit 构建说明](../native/fskit/README.md)
3. 在分支上手动运行 **dMsg release**，取出构建产物。在测试 Mac 上用下面的命令核对桥接包接受候选包及反向签名限制。用独立测试账户从桥接版原地升级，确认旧钥匙串凭据可读、Touch ID 解锁和审批签名成功、硬件公钥一致。在 macOS 27 的系统设置中启用 **dMsg secret files**，用测试资料完成挂载、读取和卸载。
4. 验收后使用裸标签 `1.6.0-beta.1` 发布。Beta 使用 `beta-mac.yml`，标记为 prerelease 并跳过 Homebrew。分支运行只生成测试产物。

```sh
node scripts/dmsg/test-macos-transition.cjs /path/to/bridge/dMsg.app /path/to/yiwen/dMsg.app
```

## Yiwen 切换的本地验证，2026-11-20

- 公开的 `1.5.8` 更新 ZIP 通过发布的 SHA-256 校验。本机 `1.5.10` 随包 CLI 的 SHA-256 与发布值一致。两个实际应用及其 daemon 均通过 LDC Developer ID 签名校验。两个应用的公证票据均通过 `xcrun stapler validate`，签名中写入的更新要求均接受 Yiwen。
- 使用锁定的 Electron 在 arm64 上完成了本地 ad hoc 试打包。核对了应用标识、版本、随包 CLI，以及 Beta 更新 ZIP、清单与校验和。FSKit 扩展使用 macOS 27.2 SDK 编译通过。正式签名、公证、桥接版原地升级、钥匙串授权、Touch ID 和实际挂载仍需使用 Yiwen 分发候选包验收。

## LDC 桥接阶段的本地验证，2026-11-17

基于 `b413ed1d` 的实现（本地 `main` 分支上的工作补丁）。

- 桌面端 `pnpm check`：无 Svelte 或 TypeScript 错误/警告。
- 桌面端 `pnpm test`：203 项测试通过；精确的要求语法也已由 macOS `csreq` 编译验证。
- 桌面端 `pnpm build`：通过。锁定文件（frozen-lockfile）离线依赖安装通过。
- `cargo test --locked -p dmsg_cli`：126 项通过，6 项显式忽略；7 项发布测试包含真实的回环分页、稳定版/完整资产选择、单向签名方策略及替换行为。
- `cargo clippy --locked -p dmsg_cli --all-targets -- -D warnings`：通过。
- `DMSG_DESKTOP_SMOKE=updates node scripts/smoke.mjs`：使用隔离配置/模拟守护进程通过。验证了渠道持久化、保存失败重试、延迟回复、键盘操作、更新状态以及 Homebrew 限制。已检查全部七张生成的截图：浅色/深色主题下的中文标准/窄窗口、中文/法语/阿拉伯语窄窗口、阿拉伯语 RTL（含长版本文本）。
- `node scripts/dmsg/test-macos-transition.cjs`：真实 LDC Developer ID 测试夹具通过了“旧版 → 桥接版”要求兼容性、守护进程身份检查，以及对错误标识符、ad-hoc 签名、篡改和反向团队信任的拒绝测试。未安装任何生产应用或读取任何凭据项。

当日未运行 Yiwen Developer ID 分发签名、公证包更新安装、跨团队钥匙串授权、Touch ID、FSKit 交接、远程 CI 或 Windows/Linux 打包。Electron 冒烟测试只覆盖更新 IPC 夹具。

## 参考资料

- [Squirrel.Mac 安装器验证](https://github.com/Squirrel/Squirrel.Mac/blob/main/Squirrel/SQRLInstaller.m)
- [Apple DTS 关于基于文件的钥匙串 ACL 与分区列表的说明](https://developer.apple.com/forums/thread/845980)
- [GitHub 发布 API 与 `make_latest`](https://docs.github.com/en/rest/releases/releases?apiVersion=latest)
