# iOS App 转移：Keychain 准备与发布验收

初稿写于 2026-11-17，2026-11-19 补充安装标识、域名关联和 App Attest 的交接步骤。上线状态以发布记录为准。
实施前源码基线：`6b44959bd489f09b086fccb3c2f8448d81b59a48`。
旧团队为 `S9Z2WAMAV8`；本次修改仍使用旧团队签名，未发起 App Transfer。

## 1. 当前实现

| 材料 | 旧 Keychain access group | 新 access group |
| --- | --- | --- |
| 主 App 的设备绑定记录、快速解锁记录、Passkey 会话与通知凭据 | `S9Z2WAMAV8.net.dmsg.ios` | `group.net.dmsg.ios.keychain` |
| 自动填充的显式导出副本密钥 | `S9Z2WAMAV8.net.dmsg.ios.autofill` | `group.net.dmsg.ios.autofill` |
| Files 的显式导出副本密钥及短会话 | `S9Z2WAMAV8.net.dmsg.ios.files` | `group.net.dmsg.ios.files` |

`group.net.dmsg.ios.keychain` 只授予主 App。主账户秘密不放入分享、自动填充或 Files 扩展的组。
保留旧的 `keychain-access-groups`，让过渡版仍可访问旧项；迁移后的操作明确指定目标组。
目标组直接使用 `group.…`，不加 Team ID 前缀。App Group 仍需在 Apple 开发者后台注册、授权并交接，改 entitlements 文件本身不会完成后台配置。

主 App 在 `MobileEngine.open` 之前按当前安装的精确 service 迁移。该 service 覆盖本次安装的全部账户，production/staging 和其他安装保持隔离。迁移使用 `SecItemUpdate`，保留原字节、设备限制及访问保护；不改变工作区、设备身份、installation UUID，也不导出明文文件或上传密钥。

成功后在目标组写入安装专属收据 `dmsg:keychain:app-group:v1`。它只证明主 App 的 Keychain 准备步骤成功，不证明 Secure Enclave、扩展或跨团队升级已验收。每次旧团队启动仍检查并迁移旧组，失败可重试。冲突、权限不足、设备锁定均不能当作“没有旧数据”；冲突不会自动覆盖或删除任意一份凭据。

1.2.3 的修复在打开凭据库前检查原安装标识。已有工作区、暂存导入或其他本地状态时，缺少标识会进入保护界面。已存在但损坏或不可读取的标识会保留原状。仅首次启动的空目录可创建新 UUID。若旧版本已覆盖原标识，新版本无法从错误的迁移收据反推出原 UUID。

扩展导出密钥保留原在场认证：访问已有副本时迁移所选快照，后台文件访问只能迁移不需要弹窗的短会话。新副本直接写目标组。删除/锁定同时处理旧组，防止旧凭据再次生效；不延长快照和会话有效期。导出副本可在主 App 解锁后重新生成。

新团队启动已有非空工作区时，若目标组没有对应收据，则停止打开核心并保留文件。此屏幕不提供自动清空、重建身份或绕过硬件校验的入口。它是保护措施，不是跳过过渡版用户的完整恢复工具。

## 2. 旧团队发布前

1. 与 Apple Support 确认公司主体变更后的旧账号处置及过渡发布安排。
2. 在旧团队创建 `group.net.dmsg.ios.keychain`，只关联主 App `net.dmsg.ios`。
3. 保留原 incoming/autofill/files App Groups 及其扩展关联，刷新主 App 和扩展的 profiles。
4. 检查最终签名产物的 entitlements 与 embedded profiles；主 App 包含新组，扩展不包含主 App 的新组。
5. 按 README 的一键构建流程生成旧团队 TestFlight 包并上传内部测试。在真机保留当前商店版及测试账户、设备绑定、快速解锁、通知和扩展副本，再通过 TestFlight 覆盖安装过渡版。不要卸载；不要求先用本地开发包覆盖手机。
6. 验证下节的测试矩阵后发布过渡版，告知用户更新并打开 App；只下载更新、不运行，并未执行迁移。

没有任何固定等待天数可以证明全部公众用户完成迁移。本实现不新增密钥遥测或上报用户内容；App 版本安装率也不等于迁移成功率。

## 3. 团队交接与首个新团队版本

对于自动续订订阅，LDC 在发起转移前向 Yiwen 交接 App 专用共享密钥，Yiwen 在转移完成后重新生成。切换 APNs 时，推送签名配置须与实际发送推送所用密钥的签发团队一致。[Apple 的 App 转移说明](https://developer.apple.com/help/app-store-connect/transfer-an-app/overview-of-app-transfer/)

由 Account Holder 完成 App Transfer 后，协调交接主 App 专用组和原来的 incoming/autofill/files 组。Apple 当前 App Groups 流程涉及旧方释放、接收方重新注册同一标识，应协调完成，避免释放后无人接收。不要换组名来绕过冲突。

接收方准备新签名与 profiles，保留原 Bundle ID、组名和安装/工作区路径。
切换前部署两个网站的关联文件。`dmsg.net` 负责登录回调与 Passkey，`connect.dmsg.net` 负责配对与请求链接。在 Apple CDN 核对两个域名均包含 `S9Z2WAMAV8.net.dmsg.ios` 和 `63G8VNQPXB.net.dmsg.ios`。[Apple 关联域名说明](https://developer.apple.com/documentation/xcode/supporting-associated-domains)

迁移期间服务端应接受两个 App ID。新签名的 App 会独立登记用于推送登记与购买的 App Attest 密钥。每个密钥的后续请求均须匹配它登记时的 App ID。[Apple App Attest 验证说明](https://developer.apple.com/documentation/devicecheck/validating-apps-that-connect-to-your-server)

先上传新团队 TestFlight 内部测试构建，在同一台真机覆盖旧团队过渡版；验收后才公开发布。检查真实签名中的 `application-identifier` 和各 access groups，而非仅检查源码 plist。

| 路径/状态 | 必须验证 |
| --- | --- |
| 商店旧版 → 旧团队过渡版 → 新团队 TestFlight | 原账户、设备 ID、内容和登录关联不变 |
| 多账户、非当前选中账户、production/staging | 每个账户均能继续使用，不跨环境读取 |
| 锁屏、取消认证、重复启动、中断后重试 | 保留原数据，不生成替代设备绑定，不误报完成 |
| 迁移目标已存在同名不同值 | 明确失败，源和目标值都保留 |
| 重启后快速解锁、离线读取、过期登录 | 保持原解锁与登录边界 |
| Secure Enclave 硬件锚点、解锁、审批和钱包签名 | 使用原 handle 成功完成操作；不能重建锚点或关闭硬件绑定来通过验收 |
| 自动填充、Files、过期快照和锁定 | 副本访问仍要求原认证；过期/撤回的访问不恢复 |
| 登录回调、Passkey、桌面配对与请求链接 | 两个签名版本均能唤起对应入口，原有 Passkey 可继续认证 |
| 新团队 TestFlight 的推送与购买 | 原账号能登记并实际收到推送，沙盒账号可完成首购并恢复已有测试交易 |
| 跳过过渡版 → 新团队版 | 保留工作区，显示保护提示，不自动进入新账户初始化 |
| 新团队全新安装 | 正常初始化，与遗留工作区的失败处理区分 |

Secure Enclave 的 `dataRepresentation` 是封装密钥而非可导出的明文私钥。
现有硬件锚点和审批校验保持不变。模拟器和“传入另一前缀”的单元测试不能证明真实跨团队硬件密钥可用。

## 4. 未运行过渡版的用户

新团队没有旧团队普通 Keychain 组的访问权，补一段读取旧组的代码不能补救。
保留原 App 和工作区；先从另一台已授权设备确认云端内容、设备权限与恢复条件，再制定重新授权步骤。已启用的账户恢复也不保证找回只保存在原手机、尚未备份的内容。

本次未新增跳过迁移后的工作区导入/恢复功能。需要恢复但没有另一台已授权设备的用户，必须按实际已启用的恢复政策处理，不能许诺重新登录就能恢复。

## 5. 参考

- [Apple：App ID Prefix Change and Keychain Access](https://developer.apple.com/forums/thread/706128)
- [Apple：SecItem 迁移与错误处理](https://developer.apple.com/forums/thread/724013)
- [Apple：共享 Keychain access groups](https://developer.apple.com/documentation/security/sharing-access-to-keychain-items-among-a-collection-of-apps)

实际执行结果见 `../VALIDATION.md`。后台注册、发布、转移及跨团队真机测试应分别记录，不以本地测试替代。
