# 会员与商业合同

[English](commerce.md) | 简体中文

本文档描述公开实现，并非部署或定价公告。`membership/1` 是产品中立的；`dmsg-commerce/1` 提供 dMsg 资源合同。`src/membership`、`src/dmsg_commerce`、`src/dmsg_user` 和 `src/dmsg_payment` 中的可执行 Candid 是方法与类型的基准。规范 CBOR 由 `dmsg_protocol` 定义；[commerce_vectors.json](../../src/dmsg_types/tests/commerce_vectors.json) 冻结了代表性字节和算术规则。

## 边界

- `membership` 负责 PANDA 资格、不可变阈值策略、全局 `(SNS governance, neuron ID)` 占用索引、短期资格租约、补贴预留以及持久产品决定。`ProductConfig` 锚定适配器、接受的授权 canister、主体 schema 与主体字节长度。此 canister 中不包含任何 dMsg 套餐或 AccountId 解释。
- `dmsg_commerce` 负责现金订单、独立存储附加包、产品合同、退款、收入储备与资源投影。现金激活与资金分类在同一个消息中提交。SNS 决定在本地提交并返回已保存的回执；重试同一决定绝不会重复应用。
- `dmsg_user` 负责设备精确批准的商业意图与月度执行用量。商业批准绝不添加认证绑定。`dmsg_cose` 仅接受其固定的 user home；资源证书不能授权签名。
- `dmsg_payment` 继续负责投递托管。会员订单不进入投递状态机。`dmsg_handle` 保留其独立的 PANDA 定价。

## 编码与算术

ID 为 32 字节的字节串；dMsg 账户主体为原始 12 字节的 AccountId。`Beneficiary` 包含 `product_id`、`authority_canister`、`subject_schema` 与 `subject_bytes`。dMsg 使用 `dmsg` / `dmsg-account-v1`。Principal 使用现有协议表示。金额为 u128（JSON 十进制字符串）；资源计数器为 u64。业务时间为 UTC Unix 毫秒。ICRC 转账 `created_at_time_ns` 保持纳秒。

`digest(domain, value)` 为规范 CBOR `(1, domain, value)` 的 SHA-256。规范 map 按键编码字节排序。以下所有经认证路径均为单个原始 32 字节分段：

| 值 | 域与输入 |
| --- | --- |
| 资源视图 | `dmsg/commerce/entitlement-key/v1`, Beneficiary |
| 已发布目录 | `dmsg/commerce/catalog-key/v1`, `"dmsg"` |
| 商家订单 | `dmsg/commerce/order-key/v1`, order ID |
| 不可变 PANDA 策略 | `membership/policy-key/v1`, policy version |
| PANDA 认领 | `membership/claim-key/v1`, claim ID |
| 执行用量 | `dmsg/commerce/usage-key/v1`, `(account_id, YYYYMM)` |

意图与现金订单域为 `dmsg/commerce/intent/v1` 和 `dmsg/commerce/order/v1`。PANDA 操作与决定域为 `membership/claim-action/v1` 和 `membership/decision/v1`。新操作需要新的申请身份/批准。对于同一不可变申请，短时间批准可以续期，同时保留其冷静期起始时间。已签名的账户命令与意图的申请截止时间是分离的。

年度周期结束于下一年相同的 UTC 日期/时间，2 月 29 日截断为 2 月 28 日。所有周期均为 `[start,end)`。阈值计算使用任意精度中间乘积及经过检查的 u128 结果：`ceil(price_cents * R_num * 10^8 / (100 * R_den))`。现金价格向上取整；退款向下取整。执行额度在整个无重叠月份时间线上对 `duration_ms * monthly_units` 求和，并除以整月持续时间一次。账户创建前的时间不产生额度。最多接受 64 个分段。退款或续费绝不清空已收取或持有的额度。

## 方法权限与状态

治理变更需要配置的 SNS governance 调用者，而不是 `is_controller`，也不接受调用者提供的提案号。普通策略发布需要提前 30 天通知。预定的目录和投递费策略为仅追加（append-only）：其版本号和生效时间都必须递增。准入暂停会影响新的承诺；既有的资格刷新和退款依然可用。策略和产品记录不可变；版本号或产品 ID 不能被重新挪用。

在准入前，治理调用 `verify_sns_configuration` / `verify_ledger_configuration`。这些方法核对 root 的 SNS 身份和配置的 ledger 小数位数，并单独记录账本当前费用，与冻结的售卖目录区分。部署必须独立锚定生产 SNS Wasm/Candid 及预期的 ledger ID；小数位数匹配并不等同于代币身份确立。实现的 ledger 适配器为 DFINITY ICRC-3 `1xfer` / `2xfer` 区块格式，包含经认证的归档回调。

`quote_order` 为非变更性草案。`open_order` 需要实际付款人以及 user home 当前的批准。已开立的订单具有不可变收款子账户、价格、付款人、费用储备、注资截止时间和激活截止时间。`check_order_funding` 自行读取 ledger。每个区块仅认领一次；仅有一笔合规支付能激活订单。少付、多付、来源错误、重复支付和超时支付依然可退还给其实际来源。`reconcile_order`、`claim_deposit_refund`、`process_transfer` 和 `reconcile_transfer` 可以在没有私有服务参与的情况下推进。公开推进仅返回 `OrderProgress`（订单 ID/状态）或 `TransferProgress`（订单/转账 ID、状态、替代腿 ID）。订单详情/凭证需要付款人、对应 user home 或治理者权限；存款详情也对原始存款人可见，转账详情也对该笔收款人可见。推进响应不包含账户、金额和授权意图。

开单授权核验是只读操作：`open_order` 在授权成功且回调重验当前状态后才持久化订单，并消耗一次每日成功订单额度。并发或重复开单返回同一订单。授权尝试单独限制为每调用者每分钟 10 次、全站每分钟 200 次；失败不遗留订单，也不消耗成功订单额度。账本操作使用独立的每分钟 400 次预算，权益刷新使用每分钟 200 次预算；同 schema 升级保留计数。

现金操作支持新年度会员、30 天内续费、保留年度终点的升级、存储附加包以及对剩余周期的 SNS 买断。原始年度锚点在升级后保留。退款涵盖 7 天内的首笔现金基础购买以及尚未开始的续费取消。首周期的退款同时关闭其现金升级；每笔贡献订单保留其原始付款人和退款分支。存储产品以年价报价，仅允许当前资格有效的付费会员购买；按照报价时原始基础合同剩余期限折算并向上取整，与该基础合同同日到期。购买成功后，即使基础合同提前关闭，存储包仍独立有效。

关闭（Closing）会停止从受影响来源颁发租约，并记录最新的未结租约截止时间。该截止时间过后，对账会确定未履约本金，终止未来的权益，并记录退款负债。网络费零头保持记录。出账费用不能超过订单冻结的费用储备；账本费率超过该上限时阻断转账，已知超出上限的费用也会阻止新增开单。明确失败的转账可根据已记录的 `BadFee.expected_fee` 在原上限内修订，无需等待新售卖目录。`get_transfer.last_error` 保留账本的具体拒绝原因；未知执行结果不能据此修订。出账转账在调用账本前持久化确切的来源、目标、金额、费用、memo 和纳秒时间戳。未知结果复用这些参数或对账匹配的区块。只有已知被拒绝的转账可以修改，被取代的分支不能派发或对账。收益归集仅释放已赚取服务储备并支付给配置的国库。

PANDA 认领通过注册的适配器对经济 Principal 进行身份认证并验证账户批准。它接受独立控制的 neuron：必需拥有 ConfigureDissolveState、ManagePrincipals、Disburse 和 Split 权限；其他 principal 最多只能拥有 Vote。已知质押/控制/锁定不足则不符合资格；未知权限、无法识别的状态或调用失败均视为无法验证。有效质押为 `max(cached_neuron_stake_e8s - neuron_fees_e8s, 0)`；不计入 maturity。若溶解中 neuron 的固定解锁时间戳覆盖合同终点，则可予以接受。

新认领至少冷却 65 分钟，从首次验证合规且有资金的观察开始。激活会重新检查批准、principal、质押和最终区间。短期租约锚定在观察开始时刻，且绝不超过 1 小时。Applying/Closing 决定跨 await 和升级持久化。丢失的产品确认（ACK）使用固定的决定 ID 进行对账；绝不会仅因超时流逝就释放占用。`cancel_application` 在原始执行者权限下释放未应用的请求。`reconcile_claim` 推进已准备的决定并清理过期的承诺/申请。产品回执固定权益区间和承诺截止时间。已替换/已买断的认领使用 `release_replaced_claim`；它们的旧租约必须在重新使用前到期。已知不符合资格具有 7 天的产品修复期；无法验证会暂停该计时器。已释放的认领无法重新恢复。SNS 升级/换绑使用决定中固定的同一时间结束旧区间、开始新区间，不因投递延迟改变切换点；既有租约释放截止仍保留。

membership 在初次账户批准通过后才创建 Claim、消耗申请额度及预留补贴；授权失败不遗留 Claim。授权尝试按每主体每分钟 10 次、全局每分钟 200 次独立限流；资格检查与活跃会员刷新各有每分钟 200 次预算。未激活申请确认不合格后释放，补足本金须以新申请重新冷却；进入 Applying 前在回调后重新检查准入暂停。`sweep_expired_claims()` 与通过授权的准入各从稳定到期索引清理至多 32 条记录；结果未知的 Apply/Close 不按时间释放。重复关闭或对账不会恢复 Released。首次直接投递不可变决定，恢复时查询原回执。membership 配置采用开发 schema 2，Claim 编码仍为 schema 2；高频计数通过 `pre_upgrade` 持久化，同 schema 升级不能跳过该 hook。初始产品校验、生效政策索引与性能样本见 [membership README](../../src/membership/README.md)。

## 认证消费与执行

查询返回 `CertifiedBatch` schema 1，附带精确叶子字节。验证预期的 canister、certificate、witness、请求路径及叶子。缺失证明仅在具有完整缺失证明加上经认证的 Free 策略时方可使用。未知/已修剪的 witness 和失败请求不等于 Free。查询方法绝不会创建或延长租约。`refresh_catalog` 发布生效的预定目录；消费者必须在预定的转换时刷新，而不是延长先前的已发布策略。

资源投影区分业务修订版本与租约修订版本。刷新租约不会使已支付订单的业务 CAS 失效。资源叶不包含 neuron ID 或付款人身份。未知的 SNS 资格不会发放可复用的兜底租约。已知无效资格仅授予 Free 资源并保留修复信息。独立存储附加包在基础会员终止后依然有效。无法核验的刷新具有持久化的一分钟重试冷却；返回原过期视图不会延长资格。

`get_execution_entitlement` 是仅限对应配置 user home 访问的副本响应；它不需要查询证书，也绝不会回调 user canister。user 提供其不可变账户创建时间，重新计算完整月份的额度，拒绝版本回退，并将额度单位与执行序号一同预留。`ExecutionGrant.commerce` 绑定预留 ID、月份、单位数、策略版本和过期时间；COSE 授权摘要为 `dmsg/cose-execution/v3`。正式 Statement 和批准字节保持不变。显式 `refresh_execution_entitlement` 即使本地旧租约仍有效也会获取当前商业权益，同时保留已用和预留计数。已完成和已过期的历史输出消耗其原始单位；COSE 已记录的失败释放单位；Unknown 保留预留。传输未发出、或 COSE 尚未记录终态的业务拒绝会保留 grant、序号与预留，使用同一请求对账，过期后也须补齐终态序号。根派生使用受保护的预算而非商业单位。COSE 从正式签名中为安全操作预留 20% 的部署执行/cycles 上限，同时保留硬性总上限。

公开错误新增 `MembershipStale`、`MembershipIneligible` 和 `MembershipClosing`。继续根据记录的操作使用 `VersionConflict`、`IdempotencyConflict`、`QuotaExceeded`、`Pending`、`Expired`、`FeeBlocked` 和 `ExecutionUnknown`。绝不要为了解决未知结果而创建另一笔付款。

## 投递 profile 2

`Quote` 增加 `fee_policy_version`。在生效策略下，其服务费必须等于 `max(ceil(recipient_net * rate_bps / 10000), minimum_atomic)`。净额保持不变。Quote 和 AdmissionReceipt 域变为 `dmsg/quote/v2` 和 `dmsg/admission-receipt/v2`；回执 `protocol` 为 2。现有已开立订单保留其固定的费用和结算/退款决定。私有客户端在使用这些开发 canister 之前必须显式采用此版本。

投递 payment 另提供仅 controller 可调用的 `set_ledger_fee`，在部署时批准的上限内维护预期网络费；它不修改治理发布的平台服务费政策，也不重写已接受报价或已准备转账。默认配置认证查询选择查询时已生效的政策，历史版本仍可单独查询。转账记录增加有界 `last_failure` 诊断，不改变 Unknown 的恢复规则。

## 验证与部署限制

`make test-dmsg` 涵盖原生测试、Clippy、所有六个生产 Wasm 模块、Candid 提取比对、独立 Rust/JavaScript 向量以及带故障注入的 ledger/SNS fixture PocketIC 测试。这些 fixture 不是生产 SNS 或钱包的验收证明。未硬编码任何生产汇率 R；5000 PANDA/USD 仅在测试中出现。产品策略与生产经济 principal 入口连接需要部署验证。本仓库未实现 TokenList 的适配器、私有资源记账、跨链支付适配器或自动借记授权。

`list_catalogs(after_version)` 返回可选游标之后至多 64 个版本。目录查询使用有界有序缓存；执行权重与前一排期版本比较，变更只能在 UTC 月初生效。

Commerce 本地配置/主体记录及修订后的转账格式使用开发稳定 schema 2。配置与限流预算在内存中更新，初始化和 `pre_upgrade` 时持久化，升级不可跳过该 hook。增长测量与边界见 [commerce README](../../src/dmsg_commerce/README.md)。开发稳定 schema 为全新实例 schema；同 schema 升级保留状态。这些改动不迁移遗留生产服务或根。共享服务仅强制注册消费者；TokenList 必须切换到相同的已部署授权中心，然后才能宣传跨产品独占性。

商业化前的公开基线审查（`65a2635` → `b319e21`）发现 `user.rs`、`cose.rs`、`payment.rs` 和 `profiles/delivery.rs` 中的改动均为注释/格式化/尾逗号。协议 README 也澄清了共享 Xid 分配和历史执行证明的新鲜度。本次实现刻意更改了上面列出的商业和投递合同；它不是对该基线的仅按哈希重新锁定。

构建和测试后生成精确的移交快照：

```sh
python3 scripts/export-commerce-snapshot.py --output /tmp/commerce-source.json --fixtures /tmp/dmsg-commerce-fixtures
```

快照显式标明工作树及其基准 commit。它不声称新的发布 commit 或私有服务兼容性。接收方仓库在更新自身源码锁定前，必须先审查变更的合同和向量。
