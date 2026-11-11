# iOS 审核与服务验收

核对日期：2026-11-11。这里区分当前发布配置、App Store Connect 实际状态和仍待执行的验收；历史构建的验证范围见 [VALIDATION](../VALIDATION.md)。应用获批不代表其订阅商品已经获批。

本轮的英文审核说明、截图采集与五项送审清单见 [1.1.2 审核材料草稿](1.1.2_zh.md)。

## 应用标识

- SKU：`dmsg-ios`。
- Apple ID：`6827050680`。
- Bundle ID：`net.dmsg.ios`；开发团队：`S9Z2WAMAV8`。
- 当前版本与构建号以 `project.yml` 为准，内购开关为 `DMSG_IN_APP_PURCHASES_ENABLED=YES`。TestFlight 和 App Store 直接连接固定线上服务；商品查询、购买、恢复和交易投递已启用。
- 内购实现使用 StoreKit；当前线上 commerce 商品映射只包含 Plus、Pro、Max 三个年度订阅，不包含存储包。应用 SKU 与 Apple ID **不是**商品 ID。
- iOS 免费来信可发送；链上付费来信发送保持关闭。多签钱包是独立的资产管理功能，不用于解锁 iOS 数字服务。

## 首次订阅送审与正式可用性

2026-11-11 核对：App Store Connect 显示 iOS `1.1.1` 可分发，但 `dMsg Membership` 群组（`22537118`）和下面三个商品仍为“准备提交”。线上 commerce 的 `get_app_store_config` 与下表一致，配置为 Production，并显式允许 Sandbox。TestFlight 可显示并购买是用户反馈，本次未独立执行真机支付。

| 套餐 | 商品 ID | App Store Connect ID |
| --- | --- | --- |
| Plus | `net.dmsg.ios.plus.annual` | `6829725663` |
| Pro | `net.dmsg.ios.pro.annual` | `6829725216` |
| Max | `net.dmsg.ios.max.annual` | `6829724021` |

正式商店只返回已批准且在当前 Apple 账号地区可售的商品；TestFlight 沙盒成功不能替代该检查。当前商品尚未送审足以解释正式版空列表，不应通过更换商品 ID、放宽验签或给生产账号授予测试权益来处理。

首次上线必须完成以下步骤：

1. 补齐每个订阅的“审核信息”截图。使用可显示对应商品的实际 App 界面；空商品提示、推广图标和模拟付费成功均不能证明购买入口。2026-11-11 点击 Plus 的“添加以供审核”时，Apple 明确拒绝：必须添加“审核信息”截屏。
2. 创建并上传一个新的 iOS 版本，保留内购开关。已可分发的 `1.1.1` 不能充当首次订阅送审所需的新版本。
3. 把**新 App 版本、订阅群组、三个年度订阅**加入同一份送审草稿。检查项目列表后再提交审核；仅提交 App 二进制不会自动提交订阅。
4. 分别确认群组与三个商品已批准、销售范围覆盖目标地区，并核对付费 App 协议、税务和收款配置有效。批准后仍可能需要商店传播时间，不能只查看 App 的“可分发”状态。
5. 在 App Store 正式安装版本中验证商品、系统购买确认页、权益交付和恢复购买。未完成前不得记录为正式支付验收通过。

Apple 依据：[正式商店商品不可用排查 TN3188](https://developer.apple.com/documentation/technotes/tn3188-troubleshooting-in-app-purchases-availability-in-the-app-store)、[首次订阅送审要求](https://developer.apple.com/help/app-store-connect/manage-submissions-to-app-review/submit-an-in-app-purchase)。

## 审核登录与完整访问

应用已移除公开离线导览及其登录、账户入口。审核访问仍按 Apple 2.1 单独准备：在 App Store Connect 提供正常登录、初始化、第二个账号及第二台设备的完整复现步骤与所需资源，确保审核期间服务可用。注册入口可见、离线样例或截图均不能单独证明已满足完整访问要求；需要固定审核访问安排时按审核反馈提供，不在客户端添加认证绕过。

真实功能始终走正式的 Internet Identity 和设备授权流程，无审核口令、隐藏超级用户或跳过签名验证的入口。完整验收按以下顺序进行：

1. 测试人员在干净设备上通过通行密钥或 Internet Identity 登录，新账号选择“开始使用”（Get started）创建加密工作空间。没有预设或共享测试账号，也不要求 staging。新账号首台设备不依赖人工审批；需要把新的登录身份绑定到已有账号时，使用次级入口“加入现有账号”，仍须管理员批准。已有账号的新设备按正常流程授权。
2. 保存一项虚构登录记录、导入一个测试文件、创建测试频道，并在第二个测试账号中发送一封免费来信。不要把真实用户数据放入审核资料。
3. 使用第二台设备完成正常设备授权，验证同步、撤销与原账号恢复。上述操作使用真实线上账号与存储；账号删除会真实关闭该账号，不是模拟操作。不要提交私钥或把审核身份加入代码。
4. 在“我的 → 套餐与用量”验证订阅商品、恢复购买与管理订阅。沙盒购买必须保持 Sandbox 标记，不得变成正式付费权益；生产 App Attest/APNs 与 StoreKit 购买环境独立。
5. 在审核备注中说明 passkey 自助注册、首台设备初始化及多人功能的复现步骤，并保证线上服务在审核期间可用。如果 Apple 另行要求固定审核访问方式，再按其反馈安排；提交前核对审核人员能实际完成上述功能。

审核备注可使用以下公开说明，并填写实际联系人：

> dMsg connects to its production services, including when installed through TestFlight. Sign-in supports passkeys and Internet Identity. Create or sign in to an account and complete setup. A new account's first device needs no other device's approval; adding a device to an existing account follows normal device authorization. Use a second self-registered account to test messaging. Plus, Pro and Max annual auto-renewable subscriptions are under Account → Plan and usage, together with Restore purchases and Manage subscriptions. Purchases belong to one dMsg account; Family Sharing is not supported. Sandbox purchases remain test entitlements. Account deletion is under Account → Account security → Delete account and does not cancel an Apple subscription. Letters in the free inbox can be reported and their senders blocked from the letter's menu; blocked accounts and report status are under Messages → Message settings. Group conversations are invitation-only. The privacy policy is available before login and offline. Contact details and terms are also under Account → About dMsg. Paid on-chain letter sending is unavailable on iOS.

不要把未填写的说明或占位联系方式提交给 Apple。

## 配置门禁

- [ ] 核对三个年度订阅的本地化描述、价格、订阅组、等级、审核截图及实际批准状态；保持 Family Sharing 和多名额购买关闭以匹配单一 dMsg 账号绑定。商品 ID 必须与验证服务及 commerce 一致。新增存储包需单独配置 consumable、映射和验收，不能从客户端支持该类型推断已经可售。
- [ ] 部署 Apple 验证与投递服务，配置 Apple 根证书、App Attest 环境与允许版本、App Store Server API 凭据、通知 V2 地址及 canister 验证者 principal。凭据只放受保护的部署配置，不进入本仓库。
- [ ] 将 App Store Server Notifications V2 的 Production 和 Sandbox 均配置为 `https://api.dmsg.net/storekit/notifications`，由验证服务验签后按交易环境分流。按[公开回调合同](../../../docs/protocol/mobile_zh.md#31-app-store-服务端通知)验收签名拒绝、重复通知及服务不可用重试。
- [ ] 配置 APNs topic、环境和密钥；验证撤销设备、关闭通知、无权限和 token 失效行为。通知只能显示通用提醒。
- [ ] 注册 Share、AutoFill、File Provider、File Provider UI、Widget 标识与各自 entitlement；核对主应用私有 Keychain group 位于首位，扩展不能访问它。
- [ ] 在 `dmsg.net` 和 `connect.dmsg.net` 部署正确 AASA；登录回调与 connect 导航规则分别验证。不要通过重定向返回 AASA。
- [ ] 配置举报处理权限及支持渠道，安排人工查看与处理。只有用户明确附上的正文可被处理人员读取；不能承诺服务可以扫描端到端加密内容。
- [ ] 配置公开头像 CDN 清理权限；删除来源对象及缓存失败时，删除任务必须保留可重试状态，不能报告完成。
- [ ] 发布 23 种语言的隐私政策，并在 App Store Connect 填入有效隐私政策 URL。核对隐私标签、数据用途、加密出口声明和第三方 SDK 声明；通知元数据按 [README](../README.md) 的“Notifications”一节与 `PrivacyInfo.xcprivacy` 填写（用户 ID、设备 ID、产品交互，均关联用户、仅用于应用功能、不用于追踪）。

## 真机验收

- [ ] 新建账号、已有账号新设备授权、系统认证取消、严格生物识别策略、锁屏及退出。
- [ ] StoreKit 购买、待批准、取消、恢复、交易更新、退款／撤销和网络中断后原交易重试；未经服务端应用权益不能 finish 交易。
- [ ] App Attest 初次登记、丢失回包后的查询恢复、重放、错误环境和撤销设备。
- [ ] APNs 冷启动路由、原账号定位、隐私通知、禁用通知后正常前台使用。
- [ ] 文件 + 号菜单拍照和录像：相机与麦克风授权及拒绝、照片和视频进入当前文件夹并可预览、不出现在系统照片图库、录制中锁屏或切到后台。
- [ ] Share 暂存与恢复，AutoFill 独立认证，Files 访问到期／主应用锁定，24 小时快照到期；确认系统已经取得的副本无法远程收回。
- [ ] 后台密文上传／下载、系统挂起、重启、用户强退后再打开、任务到期、暂停／重试、切换账号、完成前锁定；正式提交和解密必须在原账号前台解锁后执行。
- [ ] 删除账号前处理未决业务、拥有的频道和钱包访问；确认服务访问永久关闭、云端清理进度可查、本机数据可单独移除，并提示 Apple 订阅需另行取消。
- [ ] 中文与长文本、阿拉伯语 RTL、深浅主题、大字体、窄屏和 iPad 分屏；VoiceOver 标签与键盘焦点。

自动化检查和实际运行结果维护在[验证记录](../VALIDATION.md)。模拟器测试不等于上述真机验收，不保证后台调度或 APNs 准时送达。

参考：[Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)，重点为 1.2、2.1、3.1.1、5.1.1。

钱包新建仍遵守产品现有的暂停决定；现有钱包可以加入、查看、提出地址／成员／协签／阈值变更、转账、批准、冻结、执行和对账。本轮没有将独立 ICP 激活费伪装成已有套餐内购，也没有解除产品级的新建暂停。钱包控制提案需经过原有成员与时间锁规则，查看或创建提案不表示资金已经转出。

## TestFlight 与业务环境

TestFlight 和 App Store Release 都连接线上服务，使用相同的生产工作区和 Keychain 命名空间。Apple 的 Sandbox 收据只描述购买环境，不决定业务端点；内购已启用，不需要独立测试服务或 `Resources/StagingDeployment.json`。

如开发者以后显式启用 Debug staging，才提供仅含公开端点和 canister ID 的 Resources/StagingDeployment.json，结构见 StagingDeployment.example.json。该本机配置不进入 Git，不含私钥；Release 不读取此 Debug 选项。

| 构建 / 分发 | App 连接 | APNs / App Attest | 内购（本次构建） |
| --- | --- | --- | --- |
| 普通 Debug | 默认生产 | development | 按签名与 StoreKit 测试环境验收；UI fixtures 不执行真实购买 |
| Debug + DMSG_USE_STAGING=YES | 显式配置的 Staging | development / development | 需匹配的服务配置，不用于正式验收 |
| TestFlight / 沙盒分发 Release | 固定生产 | production / production | Sandbox 年度订阅，须服务与 commerce 双方准入 |
| App Store 正式 Release | 固定生产 | production / production | Production；须商品与群组获批且地区可售 |

Core 的 APPLE_APNS_ENVIRONMENT 和网关的 apns.environment 独立设置；不能从 StoreKit 的交易环境推断它们。为 Debug 与 TestFlight 使用匹配的网关配置。生产服务不会因为连接来自 TestFlight 而授予测试付费权益。

Apple JWS 验证、交易环境分流及对应 commerce 权益授权始终保留；不能将 Sandbox 交易计为正式付费权益。可选 Debug staging 仍使用独立工作区、Keychain 和偏好设置；切换该 Debug 选项会清除旧系统扩展快照，普通网页链接不能切换环境。

登录 HTTPS 回调仍固定在 dmsg.net，线上 connect 桥接页为 connect.dmsg.net。审核备注说明 passkey 自助注册方法；不使用隐藏审核账号、预置共享 passkey 或授权绕过。
