# macOS 签名团队过渡

**1.5.8** 客户端发布版是由 LDC 签名的过渡到 Yiwen 的桥接版本。这是针对 Electron 应用与 `dmsg` 的 Developer ID 分发路径，与 iOS App Store 转移及其共享钥匙串组（shared Keychain group）相互独立。

## 策略与更新路径

[`scripts/dmsg/macos-signing.json`](../../../scripts/dmsg/macos-signing.json) 是经过审查的身份来源配置：

| 设置                                             | 值                             |
| ------------------------------------------------ | ------------------------------ |
| 源团队（Source team）                            | LDC Labs Limited, `S9Z2WAMAV8` |
| 接收团队（Recipient team）                       | Yiwen AI Limited, `63G8VNQPXB` |
| 当前发布签名团队（Current release signing team） | `S9Z2WAMAV8`                   |
| 桥接版本（Bridge version）                       | `1.5.8`                        |
| 桌面端标识符（Desktop identifier）               | `net.dmsg.desktop`             |
| CLI/守护进程标识符（CLI/daemon identifier）      | `net.dmsg.dmsg`                |

Squirrel.Mac 会根据已安装应用的指定要求（designated requirement）来验证更新。因此，旧版 LDC 应用会接受由 LDC 签名的桥接版本。桥接版本的外层应用要求同时接受这两个固定团队，并附加精确的桌面端标识符以及 Apple Developer ID Application 证书约束。自定义签名钩子会在公证（notarization）之前运行，并保留嵌套代码的独立要求及随附守护进程的签名；它还会在签名后对照 `releaseTeam` 检查实际签名方。

CLI 在安装或运行已下载/共享的二进制文件前，会独立检查精确的守护进程标识符和 Developer ID 签名。只有经过验证的 LDC 版 `dmsg` 才会同时接受接收团队及自身团队；Yiwen 版仅接受 Yiwen 团队。无论是环境变量还是发布元数据，都不能添加受信任的团队。未签名的开发二进制文件保留其本地行为。

旧版稳定版更新器仅识别 GitHub Latest。请将该指针保持在 1.5.8：旧稳定版 → LDC 桥接版 → 最新受支持版本。桥接版及后续客户端会枚举所有发布页面，为对应平台和渠道选择版本号最高的完整发布版本。后续稳定版发布时使用 `make_latest: false`；如果 Latest 不再指向 1.5.8，CI 将拒绝发布。每次稳定版发布仍会更新 Homebrew。现有的 `/releases/latest` 下载链接和默认安装脚本会先提供桥接版本，因此全新安装后可能需要再执行一次更新。显式配置了 `DMSG_RELEASE_URL` 的 CLI 镜像会保留其原有的 `/latest` JSON 约定，必须自行管理过渡路由。

**旧版 Beta 安装是一个例外：** 它们已经会枚举发布列表，因此 Latest 指针无法强制其经过桥接版本。在切换团队之前，请提醒 Beta 用户先安装 1.5.8。错过该步骤的用户如果当前版本低于 1.5.8，可以关闭 Beta 渠道并通过 1.5.8 更新，或手动安装桥接安装包。不要让用户删除其工作区。Homebrew 用户可使用 `brew upgrade dmsg dmsg-desktop` 同时更新 formula 和 cask。

## 钥匙串、硬件密钥与 FSKit

守护进程通过 `keyring` 使用基于文件的 macOS 钥匙串（file-based macOS Keychain），服务名为 `dmsg`。此版本保留了服务/账户名称、工作区路径以及已存储的密钥句柄。它不会将桌面端凭据迁移到 iOS App Group、导出硬件密钥、重写所有钥匙串 ACL，或向所有应用授予访问权限。更改应用的指定要求不会重写现有钥匙串项的权限。

当守护进程后续更换团队时，macOS 可能会请求允许读取现有钥匙串项。当系统弹出提示时，请允许经过验证的 Yiwen 签名 `dmsg` 访问，并在需要时重试操作。存储适配器会将访问错误报告为不可用；只有实际的 `NoEntry` 才会被视为不存在。不要为了绕过权限错误而重置 `~/.dmsg`、删除钥匙串项或轮换身份。在发布 Yiwen 版本之前，请使用单独测试账户上的临时凭据测试授权、拒绝和重试流程。

安全隔区（Secure Enclave）密钥保留其不透明的 CryptoKit 句柄和公钥。此实现**并不**保证每一个现有硬件句柄都能在团队变更后继续可用。发布的准入门槛是一次真实的“旧版 → 桥接版 → Yiwen 版”全流程测试，覆盖硬件锚点（hardware anchor）、Touch ID 解锁、审批签名与硬件协同签名（hardware cosign）。如果任何句柄变得不可访问，请停止团队切换并进行排查；不要将该错误视为替换用户密钥的许可。

FSKit 保留其同团队对等身份验证（same-team peer authentication）。切换完成后，应用、守护进程以及 `net.dmsg.desktop.SecretsFS` 模块必须使用相同的团队；打包器在嵌入模块时会检查这一点。Yiwen 需要拥有自己有效的 FSKit 描述文件（provisioning profile）以及 `<team>.net.dmsg.secrets` App Group 授权（entitlement）。该组仅包含临时 IPC 套接字，不包含用户工作区。LDC 桥接版本仍保留原有组配置。在启动新的完整安装包之前，请退出应用并停止旧的守护进程/挂载点；如果 macOS 有要求，请启用新模块。仅更新 CLI 而保留旧团队的文件系统模块无法提供跨团队的 FSKit 访问。

## 发布 LDC 桥接版本

1. 保留现有的 `dmsg-release` CI 密钥：LDC Developer ID 证书、证书密码、Apple 公证凭据以及 `APPLE_TEAM_ID=S9Z2WAMAV8`。保持 `releaseTeam` 设为 LDC。二进制签名脚本和桌面端发布打包器会拒绝团队不匹配的构建。
2. 构建并验证 1.5.8 版本。使用 `codesign -d -r- /path/to/dMsg.app` 检查打包后的应用：其指定要求必须同时包含两个固定团队以及 `net.dmsg.desktop`，而实际的 `TeamIdentifier` 仍必须为 LDC。验证随附 `dmsg` 的标识符/团队、签名完整性、公证状态以及完整的发布资产。
3. 在测试 Mac 上从旧版 LDC 安装执行就地更新，并保留其测试工作区。确认已安装的应用/守护进程版本、凭据读取、解锁、审批签名以及所有已启用的 FSKit 挂载。
4. 通过现有工作流发布裸标签（bare tag）`1.5.8`。确认发布资产完整且 GitHub Latest 指向该版本。在切换团队后保持其资产可用且不做更改。

本次源码变更不会创建标签、发布版本或更改密钥。

## 后续 Yiwen 切换

在桥接版本可用且下述跨团队检查全部通过后，仅在已审查的策略中将 `releaseTeam` 修改为 `63G8VNQPXB`；保持 `sourceTeam`、`recipientTeam`、标识符和 `bridgeVersion` 不变。使用更高的客户端版本号。同时将签名/公证 CI 密钥以及任何嵌入的 FSKit 描述文件切换为 Yiwen。此后同一签名钩子会生成仅限 Yiwen 的外层要求。不要在 Yiwen 下重新发布 1.5.8，也不要将后续版本标记为 Latest。

请在单独的测试 Mac/账户上验证完整的 Yiwen 安装包：从桥接版本更新、读取旧测试凭据、拒绝并重试钥匙串授权、使用 Touch ID 解锁、验证硬件公钥未发生变化，以及在旧进程停止后重新挂载 FSKit。还需测试长期离线的旧稳定版应用执行两步更新的过程。被下载但被拒绝的更新必须使旧安装和工作区保持可用。新团队测试需要使用真实的 Yiwen Developer ID 凭据；要求解析器或仅限 LDC 的测试夹具无法替代。

## 本地验证，2026-11-17

基于 `b413ed1d` 的实现（本地 `main` 分支上的工作补丁）。

- 桌面端 `pnpm check`：无 Svelte 或 TypeScript 错误/警告。
- 桌面端 `pnpm test`：203 项测试通过；精确的要求语法也已由 macOS `csreq` 编译验证。
- 桌面端 `pnpm build`：通过。锁定文件（frozen-lockfile）离线依赖安装通过。
- `cargo test --locked -p dmsg_cli`：126 项通过，6 项显式忽略；7 项发布测试包含真实的回环分页、稳定版/完整资产选择、单向签名方策略及替换行为。
- `cargo clippy --locked -p dmsg_cli --all-targets -- -D warnings`：通过。
- `DMSG_DESKTOP_SMOKE=updates node scripts/smoke.mjs`：使用隔离配置/模拟守护进程通过。验证了渠道持久化、保存失败重试、延迟回复、键盘操作、更新状态以及 Homebrew 限制。已检查全部七张生成的截图：浅色/深色主题下的中文标准/窄窗口、中文/法语/阿拉伯语窄窗口、阿拉伯语 RTL（含长版本文本）。
- `node scripts/dmsg/test-macos-transition.cjs`：真实 LDC Developer ID 测试夹具通过了“旧版 → 桥接版”要求兼容性、守护进程身份检查，以及对错误标识符、ad-hoc 签名、篡改和反向团队信任的拒绝测试。未安装任何生产应用或读取任何凭据项。

未运行项目：实际 Yiwen 签名（本地无 Yiwen 身份）、已公证分发包/更新安装、跨团队钥匙串授权、Touch ID 或 FSKit 交接、远程 CI、Windows/Linux 打包或发布上线。Electron 冒烟测试使用的是更新 IPC 夹具，并未执行 Squirrel 安装。

## 参考资料

- [Squirrel.Mac 安装器验证](https://github.com/Squirrel/Squirrel.Mac/blob/main/Squirrel/SQRLInstaller.m)
- [Apple DTS 关于基于文件的钥匙串 ACL 与分区列表的说明](https://developer.apple.com/forums/thread/845980)
- [GitHub 发布 API 与 `make_latest`](https://docs.github.com/en/rest/releases/releases?apiVersion=latest)
