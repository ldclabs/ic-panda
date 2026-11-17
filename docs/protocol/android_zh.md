# dmsg-android/1

Android 原生服务的公开合同。新增能力需服务显式声明；源码和本地验证不表示已经部署。Play Integrity 仅证明应用安装与本次请求，账户权限仍由设备 COSE、HTTP PoP 和最新认证安全状态决定；它不代表用户批准、购买或会员权益。

## 安装与证明

`GET /v1/accounts/:account/mobile/android-configuration` 返回 `{available, protocol, package_name, project_id, cloud_project_number, environment}`。未配置或关闭能力时 `available:false`。项目编号是十进制字符串，包名、Firebase project ID 和业务环境必须与构建配置逐字匹配，不能由 Intent 切换。

固定写入口使用原有 `dmsg/mobile/<action>/v1` COSE 和 HTTP PoP，要求当前账户已批准的 `content_sign` 设备。Android action 为 `android-challenge`、`android-push-register`、`android-push-unregister`、`android-purchase-prepare`、`android-purchase`。

- challenge 请求 `{installation,purpose,digest}`，purpose 仅为 `push`、`purchase-prepare` 或 `purchase`；安装为独立通知 Ed25519 公钥的 SHA-256（64 小写 hex），`digest = hex(digest("dmsg/android/action/v1", details))`。
- push details 严格为 `{installation,token,project_id}`，token 为长度 1–4096 的不含空白文本。不得写入日志或诊断。
- 服务生成 32 字节 nonce，按 [AndroidAssertion](../../src/dmsg_protocol/src/android.rs) 的账户、设备、安装、用途、完整 action 摘要、nonce、环境、包名计算 `base64url(SHA256(canonical-CBOR([1,"dmsg/android/assert/v1",assertion])))`。返回 `{challenge_id,request_hash,expires_at}`，有效期 5 分钟。客户端仅把摘要交给 Google。
- 客户端使用 Standard Play Integrity API 对该 request_hash 获取 token。注册请求 `{details,proof:{challenge_id,token}}`，不是客户端填写的 verdict 或 `attested` 布尔值。
- 服务在线解密验证后，精确核对 requestHash、requestPackageName、时间窗、PLAY_RECOGNIZED、包名、签名证书集合、最低 versionCode、LICENSED 和 MEETS_DEVICE_INTEGRITY。任一缺失或不符即拒绝。测试判定只在隔离的测试绑定产生。
- await 后重新核对设备、challenge、原 action 摘要、配置及期限。成功证明的持久收据只允许原 request_hash；过期不可重用。Google 解密发生未知结果且没有收据时，使用新 token 证明原 challenge，不把重解密返回的空 verdict 当成功。
- 注册返回 `{installation,revision,expires_at}`，租期 7 天。令牌替换或续租需要新证明，递增 revision；仅原设备能更新。撤销 `{installation}` 保留修订墓碑，防止旧 revision 复活。

共享向量：[android-v1.json](../../tests/vectors/client/android-v1.json)，由 `cargo run -p dmsg_protocol --example android_vector` 生成。它验证 canonical 编码与绑定字段，不是 Google 实际验收。

## 通知登记与接收

能力端点 `platforms` 只有在 Android 服务接线和准入开关同时开启时包含 `android`。`POST /v1/notifications/register` 的 Android 外层额外含 `android:{installation,revision}`，`platform:"android"`、`apple:null`；引用已验证 Android 登记，且 installation 必须等于外层通知安装摘要。其他平台不发送 android 字段，原签名字节不变。完整外层仍受独立安装持有证明保护。服务只接受匹配设备、当前 revision、有效租期的引用。

受限接收凭据、偏好 CAS、撤销、opaque 路由和后台预算沿用 [通知合同](notifications_zh.md)。推送前再核对账户与安装登记、provider revision 和偏好。Google 凭据和 FCM token 不向其他账户返回。失效 token 停止投递；停用本地同意立即停止本机展示，网络恢复后撤销远程登记。

FCM 使用 data-only HTTP v1 消息，字段仅为字符串：`version:"1"`、`route`（128 小写 hex）、`system_id`（64 小写 hex）、`event_id`（64 小写 hex 的原投递 ID）、`registration`（64 小写 hex 的不透明接收登记 ID）、`category`（四固定类别）、`generic`（固定通用文案键）、`sound:"true"|"false"`、`expires`（UTC 毫秒十进制）。不发送 notification 对象、账户、内容、任意 URL。固定四个 collapse keys 按类别，系统 tag 使用独立 system_id。有效期取服务计划剩余时间，最长 24 小时；高优先级只用于应立即展示的用户可见提醒。

接收 Service 不创建第二个 Client、不解锁内容根、不触发业务动作。前台仅通知现有协调器刷新；后台按匹配 registration 的本机有效租约、同意、有效期及严格载荷展示通用文本；账号撤销后即使旧消息仍在 provider 队列，本机也拒绝该登记。点击使用不可变 PendingIntent 进入 opaque 路由，必须重新验证原账户与事项。到达和点击不等于已读或实际查看。同一 event_id 重投保持 system_id 且静音；新投递的声音仍受共享预算和本机类别设置约束。权限、Doze 和平台丢弃不能被解释为送达。

## 配置与验证边界

none 变体不包含 Google 接入，保留前台同步和应用内提醒。gms 缺 Firebase/Integrity 配置时明确不可用，不初始化 Firebase、不伪造证明。首次启用需要用户同意和系统通知权限；不得默认采集推送 token。正式包名、Play App Signing SHA-256、Cloud/Firebase 项目与真实 Play 安装验收是发布配置，不在本仓库存储服务账户秘密。

API 依据：[Standard Integrity](https://developer.android.com/google/play/integrity/standard)、[verdict 字段](https://developer.android.com/google/play/integrity/verdicts)、[FCM Android 接收](https://firebase.google.com/docs/cloud-messaging/android/receive-messages)。

## Play 购买接入

独立商业权威合同见 [dmsg-google-play/1](google_play_zh.md)。`android-configuration.play` 单独声明 `enabled`、`environment`、`accept_test` 与允许的 `{product_id,option_id,offer_id,kind,enabled}` 列表。未声明或未就绪不进入付款；推送登记成功不表示购买服务可用。购买不要求打开推送同意，也不要求取得 FCM token。

play 配置还声明公开 `verifier` Principal 和 `commerce_canisters`（home 到 commerce 的映射），不能包含签名私钥。付款预检必须确认验证者属于 commerce 治理白名单、当前 home 指向实际读到的 commerce，并且包名、环境、测试准入和精确商品选择一致。

在打开 Google 付款界面前，以 purpose `purchase-prepare` 证明 `{installation,product_id,option_id,offer_id,kind,base_contract_id}`；可空字段保留为 null，基础订阅 base_contract_id=null，存储加购为已核验原基础合同的 64 小写 hex。`android-purchase-prepare` 接收该 details 与 proof，重新核对当前启用的选择、设备、完整 action 摘要及配置，返回 `{prepared:true,digest,expires_at}`。该准备只确认本次安装和选择可受理，不授予权益，也不能代替付款后的原 token 证明；证明失败时不进入付款。

purchase details 严格为 `{installation,token,kind,product_id}`，kind 为 `subscription` 或 `storage`，原 purchaseToken 长度 1–4096 且不含空白；商品标识长度 1–200，字符为 ASCII 字母、数字、点、下划线和连字符。先使用 purpose `purchase` 为完整 details 取得证明，再以 `{details,proof:{challenge_id,token}}` 提交 `android-purchase`；未知响应重试同一 request ID、details 和 proof。Token 不出现在诊断、错误文本或通知中。

`GET .../mobile/android-purchases` 返回当前账号的对账状态与有限的下一页游标，不返回原 token。`accepted` 只表示任务已持久接收，`delivered` 才表示 commerce 已确认交付；`pending`、`unavailable`、`account_mismatch` 与 `unsupported_product` 必须明确区分。客户端凭 SDK 回读及受保护的原始回执重试，不因页面销毁或切换账号丢弃购买。Google 确认和消耗在服务确认权威交付后执行。
