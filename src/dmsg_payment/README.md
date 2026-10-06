# dmsg_payment

ICP 上的最小资金托管和结算实现。当前支持公开的 `profiles::delivery` 付费投递协议；它不是任意服务收据或任意代币的通用结算器。

## 使用

完整接口见 [dmsg_payment.did](dmsg_payment.did)。

1. 取得固定 `Quote` 和收款主体批准的 `PaymentOffer`，调用 `open_escrow`。
2. 向返回订单的独立 subaccount 付款，用账本 block 调用 `check_funding`。
3. 提交有效 `SignedReceipt` 进入结算；超过受理期限可直接 `expiry_refund`，不需要中继同意。
4. `process_transfer` 推进出金；未知响应通过原请求或 `reconcile_transfer` 对账。

`EscrowInfo` 是公开资金视图，不含内部转账 ID 分配器和 outbox 计数。`FundsDecision` 只表示资金方向；到账状态必须检查转账记录。`list_my_escrows`、`list_deposits`、`get_deposit`、`list_transfers` 支持恢复和对账。`quote_refund` 预览选定入金和已释放预留，`claim_refund` 原子准备一笔同源退款，再由 `process_transfer` 执行。

## 保证

结算与退款互斥。收款账户、净额和费用上限由已批准条款固定。收据需要通过具体投递协议的大小、保留期限、承诺和 signer 检查。链上不证明中继执行了最新屏蔽规则，也不证明内容永久可下载。

所有入金只认领一次；少付、多付、迟到款归实际原出资账户。已确认入金始终等于负债、已转出和网络费之和。未知转账保持原始参数；只有明确拒绝的转账才允许受限修订。

收款方 `account_id` 为 12 字节 `AccountId`；账本 Principal、32 字节 subaccount、操作 ID 和 SHA-256 保持各自类型，不随 Xid 缩短。

## 多 user home 与治理

`PaymentInit` 以 `environment`、`issuer_namespace` 和只增不减的 `user_homes` 取代固定的 `home_user`。收款账户 ID 的第 4..9 字节是分配它的 home 的指纹，`open_escrow` 据此把 `verify_payment_offer` 发给账户所在的 home；不在列表中的 home 的账户返回 `NotFound`，且不消耗授权预算。认证配置叶（schema 2）以 `user_homes` 取代 `home_user`，客户端核对自己的 home 在列表中。

管理方法接受 controller 和初始化时固定的 `governance`，每个方法都有同参数的 `validate_*` query，供 SNS 通用提案按当前状态预演并渲染载荷：

| 方法 | 作用 |
| --- | --- |
| `admin_add_user_home(home)` | 追加 user home，最多 64 个，指纹不得重复；新 `dmsg_user` 须以本 canister 为 `payment_canister` |
| `set_orders_enabled(enabled)` | 开关新订单；已有订单的入金、退款和出金不受影响 |
| `set_ledger_fee(fee)` | 更新预期网络费，不超过 `max_fee` |
| `rotate_receipt_signer(signer)` | 启用更高 epoch 的收据 signer；旧 epoch 不自动撤销 |
| `revoke_receipt_signer(epoch)` | 撤销某个 epoch 并关闭新订单 |
| `schedule_fee_policy(policy)` | 预告至少 30 天后生效的服务费政策 |

## 实现

`api.rs` 负责外部调用和本地提交，`model.rs` 验证报价、收据及资金转换，`state.rs` 保存内部资金记录，`store.rs` 保存稳定表和公开认证视图。schema 9 的私有 `stable_codec.rs` 为配置和 escrow 使用 CBOR 整数 map key，共用 compact representation 覆盖报价、入金、出金与 signer；付款方和报价索引只保存键，已归零的付款方开放订单计数删除；出金块索引保存 `(escrow_id, leg_id)`；Quote/AdmissionReceipt 摘要和 EscrowInfo 认证叶格式不变；配置认证叶新增总容量，退款腿记录合并选择。`dmsg_runtime::ledger` 是独立账本适配器。

## 执行成本与恢复边界

- 开单先检查去重与配额，只计算一次报价摘要并验证一次报价签名；仅读取 `PaymentInit`，不复制授权/查账/出金预算映射。`verify_payment_offer` 返回后重新检查启用状态、报价/offer 期限、signer 撤销、当前费率版本、按最高网络费计算的预留、每日成功开单额度和总订单容量。报价内容及同一 epoch 的 signer 公钥不可变，无需再次验签。已接受订单保留原绝对金额。user 观察时间只要求与本地时间相差不超过 60 秒，不假设两个子网时钟同向；入金块时间也不与本地时间比较，只用于 `fund_by` 判定。
- `calls.rs` 对正在执行的跨 canister 工作去重：同一付款方或报价开单、同一入金 block 查账、同一转账腿的出金或对账只允许一个请求进行，其余返回 `Pending`，完成后可重试。全局最多保存 128 个占用键，每次开单占两个键；失败、正常结束及 CDK 取消任务时释放。它不代替稳定 outbox，也不清除未知出金状态。
- 出金腿在 await 前持久化为 `InFlight`，但并发锁是上述内存占用键，不是该状态。升级或回调 trap 丢失账本回复后，`process_transfer` 可用冻结的 memo 与 `created_at_time` 重发同一腿，账本按去重窗口返回 `Duplicate` 或新块；`reconcile_transfer` 仍可用真实块完成它。`Superseded` 腿返回 `VersionConflict`。
- 有界配置和预算计数保存在 heap，随普通消息和 `await` 提交；初始化与 `pre_upgrade` 才编码到原有 StableCell。正常升级保留开关、每日成功开单数及每分钟授权/账本操作预算，**升级不可跳过 `pre_upgrade`**。资金记录、入金认领和出金 outbox 仍直接写稳定表，不在升级时整体序列化。
- 授权、账本读取、账本出金分别使用独立预算，每类全局 200 次/分钟；每调用方分别为 10、20、40 次/分钟，每张计数映射最多 200 项。匿名调用共享匿名主体的限额，无效查账不能占用出金额度。失败授权不消耗每日成功开单额度；所有预算在同 schema 升级后保留。预算变化仅更新 heap，不重算公开配置认证叶。
- `get_configuration_certified` 默认按当前时间选择有效费率，与 `get_fee_policy` 一致；显式版本可查询历史政策。初始政策必须在安装时已生效，后续政策的版本和生效时间严格递增；查询反向查找，调度读取最后一项，政策表最多 256 项并使用紧凑编码。
- controller 或 governance 可通过 `set_ledger_fee` 维护已核对的网络手续费，不能超过初始化的 `max_fee`。它更新认证配置和后续新转账的预期费用，不重写原 Quote 或已准备的出金腿。报价预留至少覆盖实际结算腿数乘以 `max_network_fee`，最多为三倍该上限；两笔结算同时涨到已批准上限仍可完成。未知结果不能因更新手续费而重建，明确拒绝才允许受限修订。
- `MemoryManager` 使用 16 页（1 MiB）分配桶，库的 32,768 桶上限对应约 32 GiB；该数字是地址容量，不是业务容量承诺。
- 合并领取退款/剩余手续费及修订拒绝转账只修改内部预留/出金记录，不重算未变化的 `EscrowInfo` 认证叶。资金方向、入金和实际支付发生变化时才更新认证树；重复成功回调直接返回已有结果。
- 无心跳、轮询 timer 或后台账本扫描。结算提交不额外查账；出金保留原始去重参数，未知响应仍须原参数重试或账本对账。

认证树仍保存在 heap，升级时扫描全部 escrow 重建并只发布一次根哈希；认证叶缓存自身哈希，写入时不再重复哈希祖先节点的叶值。该路径仍随订单数增长；部署必须设置 `max_escrows`（1–10,000），达到上限仅停止新订单，已有订单入金、退款、转账和重试继续可用。终态订单也计入容量，入金历史没有另行硬截断。订单与入金记录尚未压缩归档。下述容量测试覆盖指定样本；持续入金/出金历史、真实资产和主网负载需独立验收。

实践依据：[ICP 跨 canister 调用与回调恢复](https://docs.internetcomputer.org/guides/security/inter-canister-calls/)、[Rust 稳定存储](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[CDK 取消任务与 Drop 清理](https://docs.rs/ic-cdk/0.20.2/ic_cdk/futures/index.html)。本实现仅对有界配置采用升级 hook，不将此方式扩展到订单集合。

## 退款与小额余额

`quote_refund(escrow_id, blocks, include_reserve)` 和 `claim_refund` 使用同一选择规则：最多 32 个入金区块，内部排序并拒绝重复；所有来源必须是同一完整账本账户（含 subaccount）。`include_reserve=true` 选择原始托管余额或结算剩余费用，目的地固定为报价付款账户；退款决定后可领取原始托管余额，结算则须等全部受益人出金完成。空区块列表可单独领取预留；`false` 可在资金决定前退还多付/少付款，但不会释放主入金。

预览返回 `available/fee/amount/to`。`available > 0 && amount == 0` 表示余额暂时不足手续费；保留归属，可与同源入金合并，不能计作平台收入。一次合并只支付一笔网络费。领取成功会先清空选定分配并保存出金腿，重复领取返回 `FeeBlocked`；响应丢失用 `list_transfers` 找回原腿，不新建资金分配。`list_deposits` 按区块号返回最多 32 条，游标为上一页最后区块号。

公开客户端提供入金分页、退款预览、同源选择、转账重试与明确拒绝后的修订。未知结果始终保留原参数。原 `claim_deposit_refund`、`claim_fee_reserve` 已移除，`LegKind::Refund` 记录选定区块和是否包含预留。

## Cycles 实测

2026-09-10，Rust 1.98.1、PocketIC 16.0.0，release 默认 `opt-level = 's'`；基线为公开提交 `14ae7b3`。固定其余 canister Wasm，只替换 payment，在独立新实例中各执行 8 次相同的开单、入金、结算、两笔支付、多付款退款流程。下表为每方法 cycles 中位数（百万 cycles），不是主网报价或吞吐承诺。

| 方法 | 优化前 | 优化后 | 降幅 |
| --- | ---: | ---: | ---: |
| `open_escrow` | 25.306 | 21.656 | 14.42% |
| `check_funding` | 16.553 | 16.409 | 0.86% |
| `finalize_receipt` | 13.902 | 13.904 | -0.02% |
| `process_transfer`（收件人/平台） | 17.018 | 16.883 | 0.80% |
| `claim_deposit_refund` | 9.648 | 9.021 | 6.50% |
| `process_transfer`（退款） | 17.048 | 16.914 | 0.79% |

8 次完整流程合计从 928,369,665 降至 890,402,267 cycles，减少约 4.09%。只统计 payment 的余额差，不包括 user/ledger 自身的执行费、代币网络费或长期存储租金；这些顺序执行场景也不包含并发去重节省的外部调用。小于 1% 的差异应视为小幅改善，结算提交成本基本不变。

Wasm 从 1,606,851 增至 1,624,049 字节；8 单升级场景从 3,303,426,176 增至 3,338,625,102 cycles（约增加 1.07%）。减少热路径开销没有带来升级总成本下降。

实测 payment Wasm SHA-256：

- 基线：`779e2e50eaf893cf2ecf562bdf052d9627e8dfa1f1152afc056de58fd1495ddb`
- 优化：`010e550207f1866cf8e23d929732d5667351a6606e7b46d37c33a6718b6889d9`

复现时分别将两次构建产物放入独立目录，并保持其余 Wasm 相同；用同一份 host 测试运行：

```sh
POCKET_IC_BIN=/path/to/pocket-ic DMSG_WASM_DIR=/path/to/wasm \
  cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane payment_cycles_profile -- --ignored --nocapture
```

## 2026-09-23 审查修复与测量

基线为公开提交 `e2d6abd`，PocketIC 16.0.0，release `opt-level = 's'`。两次使用同一份 host 测试和其他 canister Wasm，仅替换 payment；各运行 8 次完整流程。下面是每方法 cycles 中位数（百万），只统计 payment 的余额变化：

| 方法 | 基线 | 本次 | 变化 |
| --- | ---: | ---: | ---: |
| `open_escrow` | 22.017 | 21.823 | -0.88% |
| `check_funding` | 16.847 | 16.579 | -1.59% |
| `finalize_receipt` | 14.130 | 14.133 | +0.02% |
| `process_transfer`（收件人/平台） | 17.348 | 17.056 | -1.68% |
| `claim_deposit_refund` | 9.194 | 9.360 | +1.81% |
| `process_transfer`（退款） | 17.312 | 17.079 | -1.35% |

完整流程合计从 916,297,527 降到 907,395,846 cycles，约减少 0.97%；属于小幅变化。退款预留成本略增；本次也增加了转账诊断字段。8 单升级从 3,583,708,814 增到 3,635,894,369 cycles（+1.46%），不宣称升级总成本下降。

小样本完成两笔支付后，稳定内存从 92,340,224 字节（88.0625 MiB）降到 11,599,872 字节（11.0625 MiB），约减少 87.44%。这是分配桶变化，不能据此推算长期订单存储按同一比例减少。

付款客户端只允许尚未发送的 `prepared` 请求在费用核对通过后采用维护后的手续费。曾发送过的 Unknown/Rejected 请求保持原编码参数，不因配置更新重建付款。

容量测试通过实际 `open_escrow` 入口建立独立付款人、共享收件人的未付款订单，验证升级后的订单数据及认证树。它测量 Quote、订单和索引增长，不把未付款样本当成同量入金/退款历史的容量结果。

| 订单数 | 整次升级 cycles | 已分配 stable memory | 升级后 Wasm 线性内存 |
| ---: | ---: | ---: | ---: |
| 1,000 | 6,321,862,622 | 7,405,568 字节 | 2,752,512 字节 |
| 10,000 | 42,769,854,390 | 16,842,752 字节 | 15,728,640 字节 |

两档均核对保留样本及其认证证明。整次升级费用包含安装成本，不能当作重建指令数；本次未单独采集指令计数或分配器的 live heap 峰值。10,000 单结果表明升级成本仍明显随历史增长，因此先以目标部署规模做测量，再决定压缩终态记录或限制容量。

实测 payment Wasm SHA-256：基线 `279ad16b41884b444e5d352fd375256757134030c2f2b382d38eae165f2435c2`，本次 `f92b0799d3343b5cf927460eb51d6c8fcc6b25edb850307af681415ab91e2192`。

复现命令：

```sh
POCKET_IC_BIN=/path/to/pocket-ic DMSG_WASM_DIR=/path/to/wasm \
  cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane payment_scale_profile -- --ignored --nocapture
```

## 2026-10-02 支付审查修复与测量

schema 8 对应本轮预算隔离、最高结算费预留、同源合并退款、入金分页和总订单容量限制。`payment_review.rs` 验证单调用方及全局预算耗尽后其他资金路径可用、两笔出金同时涨费、不同 subaccount 隔离、32 条分页、重复领取、升级恢复和最后一个容量名额的并发竞争。模型测试同时核对总额守恒及“未分配预留 + 可退款入金 + 未完成出金分配 = 负债”。

Rust 1.98.1、PocketIC 16.0.0、release `opt-level='s'`。基线 payment 来自 `cac48ae` 工作区（该模块与此前版本一致）；同一份 host 测试、相同报价金额、固定其他 canister Wasm，只替换 payment。各序列重复 8 次，下面是百万 cycles 的中位数：

| 操作 | 基线 | 本次 | 变化 |
| --- | ---: | ---: | ---: |
| 开单 | 21.488 | 21.443 | -0.21% |
| 查账 | 15.967 | 15.982 | +0.09% |
| 结算提交 | 13.560 | 13.562 | +0.02% |
| 收件人/平台出金 | 16.270 | 16.339 | +0.43% |
| 准备仅多付款退款 | 8.950 | 9.000 | +0.56% |
| 执行退款 | 16.285 | 16.384 | +0.60% |
| 预算已有 198 个主体时开单 | 21.560 | 21.412 | -0.69% |

保持原来单笔多付款退款流程时，8 次合计从 869,651,968 到 871,013,005 cycles（+0.16%）；细粒度预算和退款选择带来少量开销，不能宣称所有热路径都加速。将多付款和费用余量完整退回时，基线需要两笔退款，本次合并为一笔：8 次从 1,069,635,654 降到 871,013,805 cycles（-18.57%），每单退款网络费从 20 降至 10 原子单位。只统计 payment 的 update 余额变化，不包含 user/ledger 执行费、长期存储或主网吞吐。

8 单升级从 3,663,858,805 到 3,761,566,455 cycles（+2.67%）。Wasm 从 1,785,759 到 1,833,798 字节，新增接口和客户端能力不代表升级成本降低。测量 Wasm SHA-256：基线 `9def3511c2ac9a69bfe63c4d05d0a41cbb3b4099ee4ba9669a3354051017f1bd`，本次 `213558b9184926ae9e65e3e431074ad74beb640ee1278b2b80b11bf0817bb9af`。

复现普通/合并退款对比时，分别指定独立 Wasm 目录；旧版本加 `DMSG_PAYMENT_BASELINE=1`，合并场景两侧均加 `DMSG_COMBINE_REFUNDS=1`。新版本不设置 baseline 标志：

```sh
POCKET_IC_BIN=/path/to/pocket-ic DMSG_WASM_DIR=/path/to/wasm \
  cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane -- payment_cycles_profile payment_dense_open_cycles_profile \
  --ignored --nocapture --test-threads=1
```

容量测试通过实际开单、入金、两笔结算和合并退款建立全部历史；每 20 单再加入 5 次明确拒绝后的修订。10,000 单包含 10,000 条入金、30,000 笔成功转账及 2,500 条被替代的出金历史。两档升级后核对保留订单、入金、转账和认证证明，并确认达到总容量后新单被拒绝。

| 已完成订单 | payment 已分配 stable memory | 升级后 Wasm 线性内存 |
| ---: | ---: | ---: |
| 1,000 | 12,648,448 字节 | 2,752,512 字节 |
| 10,000 | 37,814,272 字节 | 16,056,320 字节 |

该长测约 38 分钟。运行期间共享构建目录被其他构建刷新，因此这轮容量结果用于确认恢复、账务历史和内存边界，不作为同一 Wasm 的升级 cycles 对比；固定构建的成本比较使用上面的隔离目录。线性内存不是分配器 live heap 峰值，10,000 单也不覆盖无限迟到入金、真实账本或主网吞吐。准入上限不会拒绝已存在订单的后续入金认领/退款。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_payment
```

`payment_regressions.rs` 覆盖 256 条费率政策的生效边界和认证查询、手续费维护/并发变更、独立授权预算、最后一个每日名额的并发竞争、拒绝原因及升级保留。模型测试逐项拒绝错误收据绑定、大小、保留期限、签名和 signer 区间；出金结果表覆盖全部 ICRC 拒绝类别与历史 Unknown。

`payment_optimization.rs` 另覆盖并发开单/入金/对账去重、失败后的重试、启用与 signer 撤销竞争、退款/费用修订的认证视图、升级后配额保留、费用余额的单次领取，以及升级丢失账本回调后 `InFlight` 腿的单次重发。既有集成测试继续覆盖未知支付结果、重复回调、手续费修订与直接到期退款。

真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic cargo test --locked -p dmsg_integration \
  --features pocketic-tests --test control_plane -- payment_ \
  escrow_settlement_duplicate_callbacks_fee_repair_and_direct_refunds \
  bounded_failed_history_keeps_the_latest_transfer_recoverable \
  clean_transport_rejection_preserves_earlier_unknown_payouts --test-threads=1
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。

Quote/AdmissionReceipt 使用 delivery profile 2。平台费由固定 SNS governance 发布的版本化比例/最低费政策计算，订单保留接受时的绝对原子金额。参见 [commerce contract](../../docs/protocol/commerce.md)。开发稳定 schema 为 9。

配置中的费用政策与历史政策表均使用独立紧凑表示。`TransferLeg.last_failure` 保存有界拒绝原因，包括过期、余额不足、临时不可用及传输不确定性；不保存账本提供的任意长度文本，成功后清空。该诊断不改变资金方向，也不把历史 Unknown 变成明确拒绝。
