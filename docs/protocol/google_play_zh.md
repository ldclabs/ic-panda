# dmsg-google-play/1

Google Play 独立购买合同，配合 [Android 安装证明](android_zh.md)。当前处于接入开发；合同、类型与本地测试不表示商品已配置、服务已部署或真实购买已经验收。Apple 路径保持独立，不能把 Google 结果包装成 Apple 交易。

## 渠道和信任

gms 发行版通过 Play Billing 查询商品、展示价格及打开付款界面；none 版保留已认证的权益和用量读取，不提供 Play 购买。安装来源、包名、环境及服务能力必须匹配。Firebase 客户端配置、Google 服务账户凭据、commerce 验证者身份分别属于不同信任边界；后两者不进入 APK。

commerce 治理配置 `GooglePlayConfig`：验证者 Principal、精确 package_name、主要购买环境、显式测试购买开关及商品映射。映射绑定 product_id、base_plan_id 或 purchase_option_id、可选 offer_id 及既有 dMsg PlanId/存储商品 Hash。SKU 字符串不决定套餐，现金目录价格不代替 Play 价格。`enabled:false` 关闭该映射的新购买，历史核验仍保留。

生产和 Google 测试购买分别为 Production/Test，来自 Developer API 的 test-purchase 字段，与业务 production/staging 无关。正式合同优先；测试合同不取代正式权益。生产接纳测试基础订阅需显式配置，不隐式接纳测试消耗品。服务未就绪时不显示虚构价格或可付款状态。

## 归属与标识

SDK 的 obfuscatedAccountId 为 `hex(SHA256("dmsg/google-play/account/1\n" + canonical_home + "\n" + canonical_account))`，固定 64 字符，不授予访问权限。验证服务必须匹配 Google 返回的绑定及当前认证受益人；付款回调、客户端 JSON、PENDING、acknowledged 或 consumed 标记都不能单独证明 dMsg 已交付。

原始 purchaseToken 只在客户端受保护的待对账记录与私有验证服务保存，不能写到公开 canister、诊断或日志。公开权威只保存其 SHA-256。token 身份包含环境及包名，已有 token 的受益人不可改变。linkedPurchaseToken 必须来自 Google 当前状态；同一未结束链不能被另一个 dMsg 账号接管。

存储购买另用 obfuscatedProfileId 绑定购买前确认的基础合同 Hash（64 小写 hex），验证后的 `base_contract_id` 必须指向相同受益人和环境的已交付 Google 基础合同。基础回执保留已核验的原账期终点，Google 后续缩短旧 token 的当前期限不会抹除独立加购的原期限。首次入账固定该合同期限，延迟恢复不会附着到下一期；已购存储资格独立刷新，基础订阅后来结束不直接抹除存储合同。首次到达时已过原期限的回执仍记录已处理，不能借恢复延长。

订阅合同按 token 和真实成功 order ID 区分账期；存储消耗品仅按 token 确定幂等身份，Google 后续补充 order ID 不能再发一次权益。算法见 [google_play.rs](../../src/dmsg_protocol/src/google_play.rs)，合成向量见 [google-play-v1.json](../../tests/vectors/client/google-play-v1.json)。这些向量不含真实购买或凭据。

## 权威事实与当前状态

受治理准入的服务调用 `apply_google_play_transaction` 提交 [PlayTransaction](../../src/dmsg_types/src/google_play.rs)。普通客户端不能调用该权威路径。事实包括受益人、token hash、商品/选项/优惠、包名/环境、当前状态、原账期锚点、当前到期、真实 linked token hash、替换状态、持久观察修订和核验时间。核验时间须在一分钟内；旧修订和同修订不同事实拒绝，不能靠新的网络请求 ID 绕过去重。

订阅当前权利来自 subscriptionsv2。当前成功订单的 servicePeriodStartTime 记录为 term_starts_at_ms 账期锚点；本 token 的权益起点 purchased_at_ms 取该锚点与 subscriptionsv2.startTime 的较晚值。到期采用当前 lineItem.expiryTime，不使用订单历史 endTime，也不按一年倒推。

DEFERRED 变更立即换发 token，并把本期旧套餐的剩余权益转到新 token；此时应交付并确认旧档位，不提前开通尚未拥有的新条目。账期锚点可来自已验证 linked 链的原订单，新 token 不等于新付费年度。首次续订时旧条目已过期、新条目生效；选择唯一当前有效的 owned 条目，拒绝并行多基础套餐。SDK 付款仍未完成的 pending replacement 不替代原权益。

| Google 当前状态 | 权益处理 |
| --- | --- |
| PENDING / pending purchase canceled | 不交付、不确认；待支付的替换不结束旧订阅 |
| ACTIVE / IN_GRACE_PERIOD | 按当前真实到期与受限核验租约交付 |
| CANCELED | 取消续订，不等于立即失去当前权益 |
| PAUSED / ON_HOLD | 暂停资源资格，不补发暂停期间额度 |
| EXPIRED / 已替换 | 终止相应权益，旧回调不能复活已替换合同 |
| 一次性商品 CANCELLED / 全额退款 | 关闭对应存储附加权益，保留去重依据 |

恢复和续订重新查询当前状态；RTDN 仅触发核对，不能按回调顺序直接授予或回滚权益。退款不追回已消费的月额度，也不复活被后继替换的合同。Google 的订单退款与撤销访问不同：订阅是否仍有访问权继续以当前订阅资源为准。

## 交付、确认与恢复

购买流程分为准备、Google 付款确认、dMsg 权威交付。SDK 对象、offer token 和价格来自当前查询结果；存在未交付购买时不能再次发起同一购买。账号切换只丢弃旧界面结果，仍保存原绑定及 receipt，回到原账号继续对账。

准备阶段先检查认证权益、治理映射、云端验证者与 commerce home，再完成 purchase-prepare 安装证明。配置或证明失败时尚未打开 Google 付款界面；状态未知的购买保留原回执继续对账。暂停的基础订阅先通过 Google 管理入口处理，不借新购买掩盖原支付问题。

服务用持久队列恢复已验证事实的提交；外部 I/O 不置于本地事务中，返回后重读任务修订。Google 状态查询、canister 提交或回复丢失时保留原 token/order 和观察，不猜测为失败后重新购买。只有权威交付成功后，服务才 acknowledge 订阅或 consume 存储消耗品。确认响应丢失先回读状态；不把消费标记当作新的交付依据。

RTDN 公网入口验证 Google 签名的 OIDC、固定 audience、已验证的指定服务账户邮箱、期限与订阅来源；持久接收后才确认。重复、乱序通知均重新核对 Google 当前状态。重装通过 SDK queryPurchases 与服务端记录恢复；管理、下期变更和取消在 Google Play 界面处理，不通过删除 dMsg 账号代替取消订阅。

## 会员合同查询与升级

`get_membership_terms_v2` 返回包含 `GooglePlay` 的完整会员合同。原 `get_membership_terms` 冻结升级前的 Candid 来源 variant，原有现金、PANDA、Apple 合同及退款窗口保持不变；账户含有 Google Play 合同时返回已有的 `UnsupportedProtocol` 错误，不省略合同或冒充其他支付来源。旧客户端需更新后才能显示 Google Play 合同。认证 `EntitlementView` 与月执行额度格式不变。

先升级 commerce，再发布使用 v2 查询的客户端和服务调用方，最后开放 Play 购买。旧状态可原地升级；写入 `GooglePlay` 合同后，旧 Wasm 无法读取该新增稳定记录 variant，不能把回退旧模块当作兼容回滚。可运行 `bash scripts/test-dmsg-android-upgrade.sh`，同时检查旧 Candid 的兼容性和真实旧 Wasm 的状态升级。

实现依据：[Billing 接入](https://developer.android.com/google/play/billing/integrate)、[订阅资源](https://developers.google.com/android-publisher/api-ref/rest/v3/purchases.subscriptionsv2)、[一次性商品资源](https://developers.google.com/android-publisher/api-ref/rest/v3/purchases.productsv2)、[订单资源](https://developers.google.com/android-publisher/api-ref/rest/v3/orders)、[Pub/Sub 认证](https://cloud.google.com/pubsub/docs/authenticate-push-subscriptions)。实际 Google 购买、RTDN、配置身份和发布门禁需另行验收。
