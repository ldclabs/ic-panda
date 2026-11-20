# 第一个 dMsg Android 应用的发布准备

日期：2026-11-17。这里记录第一次发布所需的外部配置，不把尚未开通的服务视为已验证。当前先用开发包和模拟器继续开发，不需要现在购买硬件或创建付费账户才能编译。

## 1 先使用开发包

- 开发包名为 net.dmsg.android.dev；正式包名建议固定为 net.dmsg.android。
- Gradle 可生成本机开发签名；开发钥匙不能作为正式发布身份，也不要把其指纹加入生产 Passkey 准入。
- productionNoneDebug 用于不依赖 Google 推送的本地验证。界面 fixture 只存在于独立 qa 变体，用于 instrumentation 验收，不代表账号、付款或服务端调用成功。
- 模拟器可以验证导航、存储、崩溃、生命周期与部分系统接口；不能证明真实 TEE/StrongBox 或真人认证能力。

本轮另提供使用现有 Android Debug 证书签署的 **Release/R8** 安装包：包名 `net.dmsg.android`，版本 0.1.0，最低 Android 12 / API31，包含 arm64-v8a 与 x86_64。它不同于带 `.dev` 后缀的 Debug 变体，也不是正式发行身份。none 与 gms 使用相同包名和测试证书，选择一个安装；没有 Google 服务配置时优先 none。交付文件与校验和保存在仓库本地 `misc/dmsg-android-delivery/`，该目录不进入公开提交。

## 2 固定正式签名身份

选择 Play 或独立 APK 渠道后，再配置正式签名。Play 的 app signing key 与 upload key 用途不同；App Links 和 Passkey 使用用户安装包的实际 app signing 证书指纹。

启用 Play 量子就绪签名后，从「应用签名密钥」分别复制当前传统证书和后量子证书的 SHA-256，再核对较低版本 Android 使用的旧证书。将三枚指纹同步到两个域名的关联文件与 `dmsg_auth.android_signers`；控制台生成的 Digital Asset Links 示例可能只列出旧证书。三个生产配置分别走 SNS 提案，验证后在 Android 17 和较低版本的 Play 安装包上测试。见 [Play 应用签名](https://support.google.com/googleplay/android-developer/answer/9842756) 和 [Android v3.2 签名身份](https://source.android.com/docs/security/features/apksigning/v3-2)。

保存私钥及密码到受保护的本地/CI secret 配置，不进仓库。记录正式 package、证书 SHA-256、环境和轮换策略。不同包名代表不同安装/设备，不能通过复制数据库迁移身份。

## 3 开通登录与网页关联

- 在 dmsg.net 的 assetlinks.json 发布准确包名和安装签名证书，包含链接处理和凭据共享关系；connect.dmsg.net 也须配置对应包名/证书的链接处理关系，供浏览器配对和请求链接验证。
- 对 dmsg_auth 配置经批准的 Android 证书 origin；不接受任意 android:apk-key-hash 前缀，不改写 authenticator 签署的 clientDataJSON。
- 部署支持精确 /_/android/auth/callback 的登录页和无持久化回退页。
- 正式和开发证书分别验收；开发 APK 的成功不代表商店重签产物成功。

这些内容会先准备成可审阅配置；本地构建不会自动修改线上站点或 canister。

## 4 推送

创建自有 Firebase/Google Cloud 项目并登记实际应用包名；Play Console 关联相同项目，登记实际 Play App Signing 证书 SHA-256。私有服务配置 Play Integrity 与 FCM 发送凭据，客户端不包含服务账户钥匙。当前 gms 使用手动 FirebaseOptions 初始化，不要求在公开源码放 google-services.json。

本地可将 Firebase 下载的文件放在 `app/google-services.json`，直接构建 `:app:bundleProductionGmsRelease`。CI 手动运行 dMsg Android 工作流并选择 `firebase_bundle` 时，从 `dmsg-release` 环境变量 `DMSG_ANDROID_CLIENT_JSON` 读取规范化的客户端 JSON。

规范化 JSON 的字段为 package_name、environment（production/staging）、firebase_app_id、api_key、project_id、cloud_project_number（十进制字符串）。这些值来自对应 Firebase 应用；firebase_app_id 的项目编号必须匹配 cloud_project_number。通过 `-Pdmsg.googleConfig=/path/to/android-google.json` 构建相应 gms 变体。Debug/qa 包有 .dev/.qa 后缀，必须使用与实际包名相符的配置；不能复用另一包的配置骗过检查。缺少或不匹配时入口明确不可用，Firebase 不自动初始化。构建期只接受上述公共客户端字段；传入服务账户 JSON 会在生成 APK 配置之前拒绝，错误不回显配置内容。

应用内先允许系统通知并开启全局/账号提醒，再选择“登记／续期后台投递”。它在线获取标准安装证明并通过当前设备权限登记，7 天租约在前台续期；不在后台解锁根。none 不包含 Google 适配。项目、业务环境和 token 生命周期分开；真实 Play 安装必须通过已配置的服务准入，模拟器不伪造成功。

收到 FCM token 不表示已登记账号通知；还需完成安装证明、设备授权、续期/撤销、通知权限和真实投递测试。none 版保留前台同步和提示，不假装具有后台推送。

## 5 商店购买

确定 Play 开发者账号、商品/base plan/offer、测试账号及发布地区后，配置受控权益映射、服务端 Play Developer API 与 RTDN。服务账号钥匙仅属于服务端。

测试 pending、交付未知、恢复、跨账号、升级、续订、退款和撤销乱序。客户端付款回调和模拟支付不直接授予权益。正式开通前，保留明确的能力状态和既有权益读取。

## 6 发布前证据

需要一台支持最低目标系统的真实 Android 设备，以及新系统/不同认证组合的补充设备覆盖。检查 app signing 产物、真实登录与复制检测、备份排除、系统扩展、Doze、通知和硬件签名。所有 native 库检查 4/16 KiB 与 Release/R8/TLS。

最后完成渠道当期要求、隐私/Data Safety、账号删除、支持入口、原生符号、崩溃去敏和发布说明。未完成项继续留在任务清单，不能因为本机可安装就标记全部对齐。

本地 Release/R8 APK 与 AAB 已可生成。正式签名使用 README 所列的四个 DMSG_ANDROID_* 环境变量；缺少时保持未签名，不生成或假冒正式证书。用于设备检查的临时 Debug 签名不应加入生产准入。

## 7 可重跑的硬件探针

本轮 AOSP API31 模拟器在生产 `AndroidSecurity` 的 Keystore 等级检查处明确返回 `HARDWARE_UNAVAILABLE`；这不表示硬件正向路径通过。临时模拟器 PIN 已清除，不为测试放宽 TEE/StrongBox 要求。

以后有专用、可清理的测试设备时，先人工设置该设备的测试 PIN，再显式运行 `AndroidSecurityDeviceTest`：使用 `hardwareProbe=true`、`disposableDevice=true` 和 `probePin` 三个 instrumentation 参数。测试经过真实系统凭据窗口，检查 RSA 解封、P-256 原消息验证、一次性使用、取消后禁止后续签名及缺失锚点拒绝；测试自身不会设置或更改锁屏。记录具体设备、系统、KeyInfo 和安装包签名，不能只记录“模拟器通过”。测试 PIN 不用于个人日常设备。
