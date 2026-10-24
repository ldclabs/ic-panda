# dMsg commerce 2

[English](commerce.md) | 简体中文

状态：开发合同已实现。生产配置与真实资金验收须单独留证，本地 fixture 不证明这些事实。各自的认证边界见[集成](integration_zh.md)、[应用动作](app-action_zh.md)和 [dmsg-connect/1](connect-v1_zh.md)。

## 权威与身份

产品负责自身账户/项目、价格、角色核验、订阅合同与交付回执。dMsg user 核验精确的批准账户/设备；commerce 负责现金结算；membership 负责神经元资格、全局占用和全额抵扣承诺。Worker 或浏览器都不是付款或产品交付的权威。

`Beneficiary = (product_id, authority_canister, subject_schema, subject_bytes)`。十二字节主体不隐含 dMsg AccountId。dMsg 自用适配器解释 `dmsg-account-v1`，TokenList 适配器解释八字节大端项目 ID，独立 `sample-account-v1` 适配器有自己的所有者。产品操作者、批准的 dMsg 账户与经济钱包分别核验，可以不同。

权威 `BillingOffer` 固定完整 USD 微单位金额、`[S,E)`、SKU、产品条款摘要、业务版本、操作 ID、报价权威与适配器。只有登记的报价权威能确定条款，浏览器金额或期限不能替代。报价不预留区间或消耗成功订单/认领容量；账户批准和产品授权都通过后才准入。

## 版本、编码与承诺

摘要为确定性 CBOR `(1, domain, value)` 的 SHA-256。整数使用明确的资产原子单位、USD 微单位、毫秒或纳秒，不使用浮点。类型/CDDL/SDK 从 `integration*.rs` 生成，拒绝未知字段和版本。

| 值 | Domain |
| --- | --- |
| 完整产品账单 | `dmsg/commerce/offer/v2` |
| 完整现金报价 | `dmsg/checkout/quote/v2` |
| 现金订单身份 | `dmsg/checkout/order/v2`，`(home, app_id, operation_id)` |
| 完整 PANDA 申请 | `dmsg/panda/application/v2` |
| PANDA 认领身份 | `dmsg/panda/claim/v2`，`(home, app_id, operation_id)` |
| 产品授权 | `dmsg/product-authorization/v2` |
| 应用配置叶 | `dmsg/registration/app/v1` |
| 产品配置叶 | `dmsg/registration/product/v2` |
| PANDA 认领叶 | `dmsg/panda/claim-certificate/v2` |

现金订单、出金腿和资产政策没有认证叶。它们是经访问控制的展示/恢复 query；资金只依账本证据与冻结报价移动。`open_checkout` 对照已发布的价格历史，重验报价中的精确资产政策。

原资源目录、权益和执行用量叶保持 v1，因为它们描述 dMsg 资源，而不是已删除的 commerce 1 订单/会员变更接口。没有商业 v1 回退。

## 现金结账

Local 之外只支持 ckUSDT `cngnf-vqaaa-aaaar-qag4q-cai` 与 ckUSDC `xevnm-gaaaa-aaaar-qafnq-cai`。两者须有六位小数、经验证的 ICRC-1/ICRC-3 能力和实际受支持的转账块。资产由固定账本识别，不由 symbol 推断。治理登记政策并核验账本元数据；controller、governance 或单独配置的价格权威发布 USD 微单位参考价格。治理更新已登记资产只替换非价格条款，保留最新价格并取下一个政策版本，避免提案等待期间的价格发布使其过时。新报价要求资产启用、价格窗口最多一年且相对一美元偏差不超过 1%。治理可设 `enabled = false` 暂停新报价而不改变价格，已有订单继续；没有隐式美元锚定。

`amount_atomic = ceil(amount_usd_micros * 10^decimals / price_usd_micros)`，中间任意精度，最后只取整一次。`CheckoutQuote` 冻结商户/产品登记、付款人、账本、价格、入金子账户、最高手续费、费用准备、入金及激活期限。入金手续费另计；客户端付款前展示精确价格、准备金和费用。后续发布价格不使已接受报价失效：原观察仍有效、当前观察可用、全部非价格条款（账本、种类、精度、费用、启用状态）未变时，`open_checkout` 保留原观察。报价 `(ledger, policy_version)` 必须与稳定内存中的权威发布快照完全一致，客户端价格/时间/版本不是证据。每账本最多保留 256 个未过期快照；发布时先移除过期项，未过期历史已满则拒绝，不能驱逐仍有效报价。

1. `quote_checkout` 核验权威报价及所选资产。
2. 设备签用途隔离的 `ApplicationApproval`；产品授权同一受益主体、报价与操作者。
3. `open_checkout` 先保存 `Reserving`，请产品预留区间，再进入 `AwaitingFunding`；开单不移动资金。
4. 钱包在唯一一次转账前持久化原账本、账户、memo、金额、纳秒时间与费用；结果未知不创建另一笔付款。
5. `check_checkout_funding` 读取账本，只跟随账本认证的 archive 回调；去重键为 `(ledger, block_index)`。
6. 合格的一次性付款在任何适配器 await 前写入余额与不可变 Apply 决定；适配器在同一消息提交合同与回执。ACK 丢失后 `reconcile_checkout` 查询/重放原决定，仅超时不能授权退款。

只有选定资产、预期付款人、及时且足额的入金可以支付价格。错账本、错来源、不足、多付、重复与迟到资金都形成向实际来源退款的义务。PANDA 不生成虚构付款引用。

每单按**每个账本**保存独立余额：

`incoming = refundable + service_reserve + fee_reserve + outgoing`。

收入从 `max(actual_delivery, S)` 起累计。服务实际结束后，剩余已赚金额自动付给订单冻结的商户，不能用其他商户准备金或其他账本资产垫付。`claim_checkout_refund` 汇总调用者选择的同账本、同来源真实入金，不能替换目的地。现金取消要求持久产品回执：未开始期间可取消；dMsg 自用产品允许首次现金基础会员在开始后七天内退未用时间。权益停止时间不早于已发租约；其他已开始现金期间不承诺任意退款。

出金腿固定来源、账本、收款人、净额、手续费、memo 和时间戳。`Unknown`/`InFlight` 只能通过同一转账或精确可信账本块解决。只有明确拒绝可以被替换：收款人显式重新签发新的账本时间，保留原费用或采用账本期望费用，但仍在**原始**最高手续费内。

每单在服务结束后归集一次，最多使用一笔账本 fee 的准备金。完成全部已赚收入的归集带走不足以独立退还的小额余款；更大余款在收入全部归集后自动退回冻结付款人，也保留任何人可触发的恢复入口。结清后余额归零，可归档；这也恢复首次派发已超出账本 24 小时去重窗口的出金腿。总义务保持不变。超上限费用保留阻塞，不从他人本金或准备金扣除。每腿在途 guard 排斥并发派发、对账和替换，不因原回调耗时过长而失效。升级丢弃 guard，但保留 InFlight 与固定参数；只有账本结果能裁决未知状态。

### 结账取消展示

`CheckoutView.cancellation` 独立于现金结算状态保存产品取消回执。提前退款时，商户账簿可继续为 Applied，以便归集已赚收入。客户端用成功取消回执的 `cancelled_at_ms` 作为服务终点，此前显示 closing，之后 cancelled。`RefundCommitted` 只表示可领取退款，不证明已转出；只有 `Succeeded` 出金证明付款，拒绝/未知出金仍须保留恢复入口。

## PANDA 全额费用承诺

`required_stake_e8s = ceil(amount_usd_micros * R_num * 100_000_000 / (1_000_000 * R_den))`。金额是接受区间的全额价格，不换算为年费或再次按时长折算质押。零值或溢出失败；PANDA 不变成折扣基点、部分付款、现金余额或可提现返利，用户现金本金为零。

不可变报价记录政策/版本、报价时间、所需质押、申请期限和原 E。首份政策可发布即生效；后续 R 政策须提前至少 15 天。没有 USD 补贴预算；准入由并发认领容量和每小时申请限额约束。新申请在资格核验前预留全局 `(SNS governance, neuron_id)` 占用。

新政策版本严格增加，包括清理被覆盖记录后。保留版本以相同业务字段重试时返回原发布时间，不重新开始通知期。

服务核对固定 SNS root/governance/ledger 和八位小数，不固定 governance 模块：SNS 可升级到 NNS 批准的版本，无法解码的 `get_neuron` 返回 Unverifiable。经济操作者只须以任意权限出现在神经元 Principal 列表，例如 hotkey；其他 Principal 包括 owner 可持任意权限，全局占用防止一个神经元支持两份权益。净本金须满足报价，最早可解锁时间不早于 E，覆盖整个 `[start,E)`。这只是资格，不是 SNS 本金托管。认领可复用观察一分钟，之后再查 SNS。

合格申请使用原批准立即 Apply；等待期间设备批准过期时，客户端为同一不可变条款重新批准。激活在最后一个外部 await 后、本地决定前重新调用一次 `get_neuron`，不使用此前的一分钟缓存。返回后重查准入及两项权威回复的期限；过期须重新授权，之后才准备 Apply。同一神经元最多有当前期和连续下一期引用，必须属于同一受益主体；不同产品不能并发复用，由服务全局占用表强制执行。

Apply 前未使用申请可取消；准备 Apply 后 Unknown 不可按定时取消或释放。成功 Apply 后，包括尚未开始的续期，`committed_until_ms = original E` 不可撤回，Upgrade/Replace/Buyout/提前 Close/治理编辑均不能缩短。已终止认领保持占用到失格观察前发出的租约到期，之后释放；原承诺终点本身不继续占用。到期逐引用只释放一次，释放当前期不会删除已承诺下一期。

资格租约最多一小时且不超过 E。已知失格立即停止新权益并终止认领；Unverifiable 停止新权益但不终止，也不延长旧租约。终止期间不复活；原租约到期且资格恢复后可重新申请。已终止产品合同不再预留旧账单区间，原条款与回执保留为历史证据。SNS 配置验证超过一小时或失效后才返回的神经元读取，不发租约，也不清承诺。`refresh_panda_claim`/`reconcile_panda_claim` 保留原认领/决定。

暂时的产品预留失败保留已接受申请供对账。reserve、回执查询、Apply、release 仅在真正派发时消耗产品调用预算：全局 `product_calls_per_minute` 和每经济操作者 10 次。报价和申请共享 `authorizations_per_minute`；激活有同大小的独立额度，报价不能挤占已接受申请。三项由 PandaServiceConfig 治理配置，配置前均为 200。同一认领只能有一个在途产品调用序列，重叠重试返回 Pending；暂停准入不停止恢复。SNS 配置核验同时只派发一个，并发调用者返回 Pending，重试可复用其缓存。

完整终态保存到终态转换时间与申请截止较晚者之后 30 天。有界承诺 sweep 同时压缩最多 32 个到期终态，保留不可变操作摘要；返回释放与压缩数量之和，维护调用直到零。压缩后返回 ResultExpired，同 ID 异条款仍冲突；开放承诺和未知 Apply 不压缩。完整记录上限 1,000,000，累计保留操作身份上限 10,000,000，均非生产容量声明。分页用读者索引，只有更多可读完整记录时才给游标；归档认领移除认证叶。

## 产品适配器与有界权益

产品登记的受益主体权威列表只增不减。subject 指定其中一个权威，checkout/membership 向它调用 `authorize_product_billing`。dMsg 账户产品列出全部 user home，让新增 home 的账户可购买。报价条款摘要等于登记 `terms_hash`；条款随登记生效，独立于目录排期。

现金与 PANDA 共用 `ProductBook`、业务版本 CAS 和单一未提交区间预留，永不清理未知 Apply。产品实现 `verify_billing_offer`、`authorize_product_billing`、`reserve_product_billing`、`release_product_billing`、`apply_product_decision`、`get_product_decision` 及两项现金取消方法。回调固定结算调用者、完整报价、所选来源与原回执。外部 await 后重验当前角色/主体和业务版本；租约观察推进独立 lease revision，不推进业务 CAS。

`get_checkout_for_product` 和 `get_panda_claim_for_product` 是受限的共识来源读取，不递归刷新/Apply 正在调用适配器的决定。回执不可变且幂等，明确拒绝同样保存；没有回执表示 Unknown，不表示没有交付。

TokenList 保留首次结算 Included 期间、年度档位/续费和合规读取规则。外部订阅历史记录 dMsg 订单/认领及交付引用，不增加原生 FeeLedger 可提现余额；未开始现金期间的取消恢复此前日历游标。新增付费功能要求有界的当前来源租约，公开记录、定期报告与重大变更披露继续可用。Worker 购买付费起草前向 registry 查询共识角色/租约决定，浏览器或会话声明不能自行授权扣款。

dMsg 自用账户也使用这些服务，基础套餐和存储资源是已接受合同的投影。现金升级在交付时结束旧现金资源视图，不能退出 PANDA 承诺。升级保持原年度，差价和存储增购按完整年度剩余时间计价；过期增购不占 64 项上限。不可核验期间暂停付费执行时间，已知失格立即终止权益。月执行额度对完整、无重叠 UTC 月时间线积分，截去账户创建前时间，最后向下取整一次。Free/付费分段遵循当月实际生效目录，不追溯替换旧分段；目录边界结束旧资源租约，刷新投影推进 lease revision。

既有付费合同保留价格、期限和套餐快照；实际资源额度取快照与当前同档目录逐项较大值，执行额度仅在权重相同时比较。上调从目录实际生效时刻开始，不重置已扣单位。治理可用 `schedule_policy`、`effective_at_ms = 0` 立即发布纯额度上调，由 canister 记录当前时间并认证目录；价格、条款、执行权重、存储产品必须不变，限额不得降低，且不能已有未来排期。其他更新提前七天。

资源租约按权益是否仍可收回确定：首购现金退款窗口内最长一小时；窗口外已开始现金不可退款，未开始续期位于下一边界之后，因此现金、到期和 Free 最多 30 天且在下一已知限额变更处截止。PANDA 可能失格，租约最长一小时。commerce 复用租约至剩余 10 分钟后再签新租约，membership 在同一窗口重新核验合格申请，让提前几分钟续期的消费者获得更晚期限。Pending/QuotaExceeded 不读取 SNS，旧租约继续且不触发付费时间暂停。user home 至少每小时复核执行额度，不依赖资源租约长度。

从未购买的账户没有 commerce 记录或认证权益叶；不存在证明加认证目录表示 Free，其执行权益由目录与 home 的创建时间计算，lease revision 为 0，已存主体从 1 开始。工作历史只保留当前月及仍有效/未来的资源区间，并保存最后服务截止用于到期投影。资格刷新、退款或降级不重置预留/已扣单位。根恢复/派生使用独立于付费执行单位的安全预算。

## 操作恢复与部署

私有分页 `checkout_operations`、`checkout_transfers`、`panda_operations` 复用读者/owner/商户/收款人/适配器或治理权限，包括订单冻结的商户。稳定读者索引只返回可读记录，仅有更多可读记录时给游标。`checkout_deposits(order_id, after, take)` 以不透明 hash 游标返回最多 128 笔入金。这些页面展示按账本义务、Unknown 决定、费用阻塞、过期租约与承诺，不是交付回执；客户端不得提供清空 Unknown 或提前结束已 Apply PANDA 承诺的操作。

商户 `collectable_checkouts(after, take)` 按期间终点分页监控 Applied 且收入未结订单。定时器每分钟有界处理：服务实际结束后为冻结商户分配已赚收入、退回未用费用准备并派发固定出金腿。`collect_checkout_revenue` 已删除，常规结算无须商户批准。未知回复仅在账本窗口内用原参数重试；明确费用拒绝和过期未知结果沿现有恢复方法。`commerce_stats` 提供实时记录数、stable 大小和 cycles，与 `get_commerce_limits` 对照。

入金/产品/出金共用每 UTC 分钟治理全局额度（初始 400）和 `calls_per_caller`（初始 40）；账本读取包含有界 archive 遍历。授权独立限额初始全局 200、每 caller 10。资格刷新初始全局 200、公开 caller 20；已登记 user home 可为其账户使用全局额度。每次实际产品调用都消耗额度，包括回执查询与必要 Apply；本地退款分配、未变终态读取及被 guard 拒绝的并发重试不消耗 RPC 预算。同时最多 128 个受 guard 保护的在途操作。

`sweep_checkout_history` 每次最多归档 32 单和 32 条出金腿。订单须无准备/退款义务、无未完成转账，且产品结果确定、预留已释放；保留到最后资金变化与服务/激活终点较晚者之后 30 天。成功或被替换的出金腿在最后变化后至少保留完整参数 30 天；Unknown/Rejected/Pending 继续保留。冷订单去除冗余授权/决定载荷，保留原输入摘要、报价、各账本余额、终态回执与入金引用。账本/区块去重和读者索引不删除，直接读取与历史分页继续可用。新核实的迟到入金把原单恢复为热记录，沿原来源退款，不重新授益。治理在固定上界内配置主体、热订单、累计身份、每日订单和每分钟调用上限；容量 profile 记录实测成本。

可部署多个 commerce，每个列出互不重叠的 user home，自持登记、资产、目录与订单，并作为相应 dmsg_user 的 commerce_canister。共享 membership 以只增不减的 `PandaServiceConfig.commerce_homes` 映射 home，`qualifications_per_minute` 限制 SNS 读取；私有云端按账户 home 对应的 commerce 核验目录/权益证据。

暂停准入不影响原订单对账、原路退款、既有转账恢复、资格刷新和到期释放。不能以重装解决已接受财务/承诺状态错误。开发 schema 可从空状态初始化，但没有旧商业 API 迁移层；协调已接受操作的发布必须保留原记录。

`make test-dmsg` 覆盖原生规则、真实 Wasm、编码向量与 SDK。独立[账户产品](../../examples/dmsg-account-product/README.md)验证不改核心即可支持第二种主体。外部产品负责自己的回调、权威与发布验收。[集成指南](../../tests/dmsg_integration/README.md)说明本地检查；真实浏览器使用临时配置，生产钱包 origin、神经元控制权、已批准 R/预算及有界真实资金转账须另留部署证据。

独立消息投递服务保持 delivery profile 2 与 `dmsg/quote/v2` / `dmsg/admission-receipt/v2`，不作为订阅回退或 PANDA 提前退出路径。
