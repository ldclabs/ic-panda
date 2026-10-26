# iOS 审核与服务验收

记录日期：2026-10-27。这里描述提交审核前实际要执行的步骤，不表示 Apple 已批准该应用或服务已经部署。

## 应用标识

- SKU：`dmsg-ios`。
- Apple ID：`6827050680`。
- Bundle ID：`net.dmsg.ios`；开发团队：`S9Z2WAMAV8`。
- 内购使用 StoreKit；Plus、Pro、Max 沿用现有年度套餐；存储包对应商业目录中的存储产品，权益期限随当前订阅周期。
- App Store Connect 商品尚未创建。应用 SKU 与 Apple ID **不是**商品 ID。不得用示例 ID 冒充已配置商品。
- iOS 免费来信可发送；链上付费来信发送保持关闭。团队钱包是独立的资产管理功能，不用于解锁 iOS 数字服务。

## 审核登录与演示

登录页的“体验本地演示”对所有人可见，在 Release 中保留。演示可以编辑虚构登录项、尝试消息编辑与屏蔽、查看文件示例和演练审批；它不接触真实工作区，不访问网络，不创建签名、订单或推送订阅，关闭后丢弃示例修改。它是一份产品导览，**不替代真实账号的完整审核访问**，也不能被描述为已经满足 Apple 的完整演示模式要求。

真实功能始终走正式的 Internet Identity 和设备授权流程，无审核口令、隐藏超级用户或跳过签名验证的入口。完整验收按以下顺序进行：

1. 在干净设备上选择“使用 Internet Identity 继续”，使用专门用于测试的身份登录并创建新 dMsg 账号。新账号首台设备不依赖人工审批；已有账号的新设备必须按正常流程由授权设备批准。
2. 保存一项虚构登录记录、导入一个测试文件、创建测试频道，并在第二个测试账号中发送一封免费来信。不要把真实用户数据放入审核资料。
3. 使用第二台设备完成正常设备授权，验证同步、撤销与原账号恢复。需要共享审核账号时，在提交前安排测试身份及设备绑定，在 App Store Connect 审核备注中写清可复现步骤，并保证审核期间可用；不要提交私钥或把审核身份加入代码。
4. 购买测试使用 App Store 审核／沙盒环境支持的流程；部署一个与 Apple 环境相符的测试服务和商业配置。生产 canister 拒绝沙盒交易，不能为方便审核关闭此校验。
5. 如审核团队要求固定演示登录而无法使用测试身份完成上述自助流程，必须在提交前补齐可实际访问的专用账号安排，或先与 Apple 确认合适的完整演示方式。未准备好时不提交审核。

审核备注可使用以下公开说明，并补上实际可用的测试账号安排和联系人：

> dMsg is an encrypted private workspace. The public “Try the local demo” entry provides an offline tour with fictitious data. Full account features use Internet Identity and the standard device authorization flow. The attached review instructions explain test account access, sandbox purchases, and the second test account used for messaging. Account deletion is under Account → Delete account; reporting is in a message’s context menu; blocking and report status are under Account → Safety and support. The privacy policy is available before login and offline in the app. Paid on-chain letter sending is unavailable on iOS.

不要把未填写的说明或占位联系方式提交给 Apple。

## 配置门禁

- [ ] 在 App Store Connect 创建三个年度订阅及需要的存储包，填写本地化描述、价格、订阅组、等级和审核截图；存储包使用 consumable，套餐使用年度 auto-renewable subscription，保持 Family Sharing 关闭以匹配单一 dMsg 账号绑定；将**实际** ID 配入验证服务及商业 canister 的产品映射。
- [ ] 部署 Apple 验证与投递服务，配置 Apple 根证书、App Attest 环境与允许版本、App Store Server API 凭据、通知 V2 地址及 canister 验证者 principal。凭据只放受保护的部署配置，不进入本仓库。
- [ ] 配置 APNs topic、环境和密钥；验证撤销设备、关闭通知、无权限和 token 失效行为。通知只能显示通用提醒。
- [ ] 注册 Share、AutoFill、File Provider、File Provider UI、Widget 标识与各自 entitlement；核对主应用私有 Keychain group 位于首位，扩展不能访问它。
- [ ] 在 `dmsg.net` 和 `connect.dmsg.net` 部署正确 AASA；登录回调与 connect 导航规则分别验证。不要通过重定向返回 AASA。
- [ ] 配置举报处理权限及支持渠道，安排人工查看与处理。只有用户明确附上的正文可被处理人员读取；不能承诺服务可以扫描端到端加密内容。
- [ ] 配置公开头像 CDN 清理权限；删除来源对象及缓存失败时，删除任务必须保留可重试状态，不能报告完成。
- [ ] 发布六种语言的隐私政策，并在 App Store Connect 填入有效隐私政策 URL。核对隐私标签、数据用途、加密出口声明和第三方 SDK 声明。

## 真机验收

- [ ] 新建账号、已有账号新设备授权、系统认证取消、严格生物识别策略、锁屏及退出。
- [ ] StoreKit 购买、待批准、取消、恢复、交易更新、退款／撤销和网络中断后原交易重试；未经服务端应用权益不能 finish 交易。
- [ ] App Attest 初次登记、丢失回包后的查询恢复、重放、错误环境和撤销设备。
- [ ] APNs 冷启动路由、原账号定位、隐私通知、禁用通知后正常前台使用。
- [ ] Share 暂存与恢复，AutoFill 独立认证，Files 访问到期／主应用锁定，24 小时快照到期；确认系统已经取得的副本无法远程收回。
- [ ] 后台密文上传／下载、系统挂起、重启、用户强退后再打开、任务到期、暂停／重试、切换账号、完成前锁定；正式提交和解密必须在原账号前台解锁后执行。
- [ ] 删除账号前处理未决业务、拥有的频道和钱包访问；确认服务访问永久关闭、云端清理进度可查、本机数据可单独移除，并提示 Apple 订阅需另行取消。
- [ ] 中文与长文本、阿拉伯语 RTL、深浅主题、大字体、窄屏和 iPad 分屏；VoiceOver 标签与键盘焦点。

自动化检查和实际运行结果维护在[验证记录](../VALIDATION.md)。模拟器测试不等于上述真机验收，不保证后台调度或 APNs 准时送达。

参考：[Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)，重点为 1.2、2.1、3.1.1、5.1.1。

钱包新建仍遵守产品现有的暂停决定；现有钱包可以加入、查看、提出地址／成员／协签／阈值变更、转账、批准、冻结、执行和对账。本轮没有将独立 ICP 激活费伪装成已有套餐内购，也没有解除产品级的新建暂停。钱包控制提案需经过原有成员与时间锁规则，查看或创建提案不表示资金已经转出。

## 测试与生产环境隔离

在本机提供仅含公开端点和 canister ID 的 Resources/StagingDeployment.json，结构见 StagingDeployment.example.json。该本机配置不进入 Git；不要放入 APNs、App Store API、网关或签名私钥。测试 canister/服务必须自行部署并核对，示例没有可用地址。

| 构建 / 分发 | App 连接 | APNs / App Attest | StoreKit |
| --- | --- | --- | --- |
| 普通 Debug | 默认生产，内购关闭 | development | 不进行购买 |
| Debug + DMSG_USE_STAGING=YES | 配置的 Staging | sandbox / development | Sandbox |
| TestFlight / 沙盒分发 Release | 自动选择 Staging | production / production | Sandbox |
| App Store 正式 Release | 固定生产 | production / production | Production |

Core 的 APPLE_APNS_ENVIRONMENT 和网关的 apns.environment 独立设置；不能从 StoreKit 的交易环境推断它们。为 Debug 与 TestFlight 使用匹配的网关配置。未提供测试配置的沙盒分发会明确提示缺少配置，不自动改用生产服务购买。

App 使用系统提供的沙盒 receipt 路径作为选择隔离环境的提示，并显示测试环境标识。这不是付款凭证：交易仍须通过 Apple 验证和对应 commerce canister 的授权；生产仍拒绝 Sandbox 交易。测试工作区、Keychain 服务名和安装偏好与生产分开；切换环境会清除旧系统扩展快照，普通网页链接不能切换环境。

登录 HTTPS 回调仍固定在 dmsg.net；测试 connect 桥接页通过配置的 HTTPS 来源提供邀请。请在审核备注中说明测试环境和账号进入方法，保证测试服务在审核期间可用，不使用隐藏审核账号或授权绕过。
