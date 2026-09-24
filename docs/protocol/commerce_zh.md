# dMsg commerce 2

状态：开发接口已经实现，生产配置、真实身份与资金验收需单独留证。完整字段和方法见 [英文规范](commerce.md)、[集成协议](integration.md)、[动作签署](app-action.md) 与 [浏览器 v4](browser-v4.md)。不提供 commerce 1 的订单、会员变更或买断回退接口。

## 权威与身份

产品负责本身的账号/项目、USD 账单、角色、共同合同和交付回执；dMsg user 核验精确账号/设备批准；commerce 核验真实资金；membership 维护神经元资格、全局占用与预算。浏览器和 Worker 不是这些事实的权威。

受益主体是 `(product_id, authority_canister, subject_schema, subject_bytes)`。dMsg 账户、产品操作人、受益主体与实际经济钱包是不同字段。TokenList 使用八字节大端 ProjectId，dMsg 自用账号和参考账号产品分别使用自己的十二字节 schema。长度不能决定账号归属。

`BillingOffer` 固定完整美元微单位金额、`[S,E)`、SKU、原条款摘要、业务版本、operation ID、quote authority 和 adapter。服务回读已登记产品权威；前端传入金额和日期不能替代它。报价不占区间或成功申请名额。

## 现金

生产只支持 ckUSDT `cngnf-vqaaa-aaaar-qag4q-cai` 与 ckUSDC `xevnm-gaaaa-aaaar-qafnq-cai`。资产身份由 ledger 确定，不由 symbol 推断。两者要求验证 decimals=6、ICRC-1/3、费用和真实 transfer 块格式。价格观察有明确 authority、最多 30 分钟有效期及 1% 脱锚守卫，没有隐式一美元价格。

`amount_atomic = ceil(amount_usd_micros × 10^decimals / price_usd_micros)`，中间乘积使用大整数，只在最后向上取整。报价冻结商户、账本、payer、收款子账户、价格、网络费上限/储备及资金期限。之后发布的新价格不会使已接受报价失效：只要报价自带的价格观察仍在有效期内、当前观察同样可用，且账本、资产类型、精度、费用和启用状态等非价格条款未变，`open_checkout` 仍按原报价开单。

`quote_checkout → approve_application + 产品批准 → open_checkout → AwaitingFunding → 钱包原转账 → check_checkout_funding → Apply/原回执`。

账本与其认证 archive callback 提供入账事实；去重键为 `(ledger, block_index)`。只有选定资产、预期付款人、足額及时的一笔入账可以开通。错资产、错来源、不足、多付、重复及晚到资金留作实际来源的退款义务。

每笔订单分别维护每个账本：

`incoming = refundable + service_reserve + fee_reserve + outgoing`。

商户只能领取自己原订单的已赚部分，从 `max(实际交付时间,S)` 起算。另一商户或另一资产不能替它支付。未开始的现金续费可凭产品取消回执退款；已开始的服务没有新增任意退款承诺。

转出记录冻结 ledger、来源、接收人、金额、fee、memo 和纳秒时间。Unknown 只能用原参数或可信原区块对账；不能创建第二笔付款。已知失败的费用修订也需要接收者明确批准并受原 cap 约束，不增加总资金义务。超限继续 FeeBlocked。

## PANDA 全额抵扣

`required_stake_e8s = ceil(amount_usd_micros × R_num × 10^8 / (10^6 × R_den))`。

金额是完整订阅区间价格；不按天数年化或按剩余期限降低门槛。R、预算、门槛、申请期限和原 E 固定到接受的条款。只抵扣全额订阅费，客户现金本金为零；没有折扣 bps、部分抵扣、买断或可提现余额。

服务验证固定 SNS root/governance/ledger、八位 PANDA 精度和已审核 governance 模块 hash。未知语义或无法核验的响应为 Unverifiable。实际 actor 必须拥有需要的经济控制权限，其他经济控制者会使申请不合格；仅投票权限不够。净本金足额且最早解锁时间不得早于 E。dMsg 不取得 SNS 原生托管权。

首次实际合格观察后至少冷却 65 分钟，再用新鲜账号/产品批准和神经元观察确认原条款。一个全局占用表阻止同一神经元跨产品/主体复用；同一主体只可同时持有当前与连续下一期引用。

Apply 准备之前可取消；Apply Unknown 不按超时释放。Apply 成功后，即使未来续费尚未开始，也不可提前退出、Replace、Buyout 或 Close。解绑、产品结束、失格和权益终止不缩短 `committed_until=原 E`。每个到期引用只释放一次，不误清连续下一期。

资格租约最多一小时且不越过 E。已知失格停止新权益并进入七天修复；Unverifiable 暂停修复时钟、不延长旧租约。终止权益永不复活，占用和预算仍到原 E。SNS pin 在外部查询期间变化时不发放新租约；审核升级后由治理更新 pin 并重新验证，不清空承诺。

## 产品适配与资源

两种方式共用 `ProductBook` 的区间预留和业务 CAS；Unknown Apply 不会被 prune。固定 callback caller、完整报价/来源、当前角色和版本都要核对。原决定、合同和回执原子保存；ACK 丢失查询/重放原决定。资格观察只增加 lease revision。

TokenList 保留首次结算的 Included 期、年度档位和续费窗口；外部订阅历史保存实际 dMsg 来源，不在本地再次 transfer_from 或增加原生可提现收入。Worker 付费起草通过 registry 只读共识方法核对已认证 signer 与当前来源租约。

dMsg 自用产品也通过同一服务获得合同。现金升级和存储增购投影为资源，不能退出 PANDA。月度执行额度按完整、无重叠 UTC 时间线加权，账户创建前无额度；资格刷新和退款不重置已用/预留单位。根派生与恢复使用独立安全预算。

公开记录、定期报告和重大变更披露不会被计费故障阻断。缺失或过期租约不授予新增值权益。

## 恢复与发布

运营界面通过有界分页列出当前身份可读的原订单、转账、每账本义务、过期资格和原到期承诺。没有清空 Unknown 的按钮。暂停只阻止新申请，旧对账、原路退款、资格刷新与到期释放继续可用。

Rust、SDK、真实 PocketIC、两个产品类型与临时 Chrome profile 有独立验收。真实 II/钱包 origin、经济权限、治理 R/预算及真实资产付款/退款需生产证据，不能用本地 fixture 代替。历史 Statement 三个 profile 的字节不变；消息投递仍使用独立 delivery profile 2，不构成订阅旧接口或 PANDA 退出通道。
