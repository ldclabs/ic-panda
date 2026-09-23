# dmsg_payment

ICP 上的最小资金托管和结算实现。当前支持公开的 `profiles::delivery` 付费投递协议；它不是任意服务收据或任意代币的通用结算器。

## 使用

完整接口见 [dmsg_payment.did](dmsg_payment.did)。

1. 取得固定 `Quote` 和收款主体批准的 `PaymentOffer`，调用 `open_escrow`。
2. 向返回订单的独立 subaccount 付款，用账本 block 调用 `check_funding`。
3. 提交有效 `SignedReceipt` 进入结算；超过受理期限可直接 `expiry_refund`，不需要中继同意。
4. `process_transfer` 推进出金；未知响应通过原请求或 `reconcile_transfer` 对账。

`EscrowInfo` 是公开资金视图，不含内部转账 ID 分配器和 outbox 计数。`FundsDecision` 只表示资金方向；到账状态必须检查转账记录。`list_my_escrows`、`get_deposit`、`list_transfers` 支持恢复和对账。

## 保证

结算与退款互斥。收款账户、净额和费用上限由已批准条款固定。收据需要通过具体投递协议的大小、保留期限、承诺和 signer 检查。链上不证明中继执行了最新屏蔽规则，也不证明内容永久可下载。

所有入金只认领一次；少付、多付、迟到款归实际原出资账户。已确认入金始终等于负债、已转出和网络费之和。未知转账保持原始参数；只有明确拒绝的转账才允许受限修订。

收款方 `account_id` 为 12 字节 `AccountId`；账本 Principal、32 字节 subaccount、操作 ID 和 SHA-256 保持各自类型，不随 Xid 缩短。

## 实现

`api.rs` 负责外部调用和本地提交，`model.rs` 验证报价、收据及资金转换，`state.rs` 保存内部资金记录，`store.rs` 保存稳定表和公开认证视图。schema 6 的私有 `stable_codec.rs` 为配置和 escrow 使用 CBOR 整数 map key，共用 compact representation 覆盖报价、入金、出金与 signer；公开支付摘要和认证叶编码不变。`dmsg_runtime::ledger` 是独立账本适配器。

## 执行成本与恢复边界

- 开单先检查去重与配额，只计算一次报价摘要并验证一次报价签名。`verify_payment_offer` 返回后重新检查启用状态、报价/offer 期限、signer 撤销、当前费率版本、网络费预留和每日成功开单额度。报价内容及同一 epoch 的 signer 公钥不可变，无需再次验签。已接受订单保留原绝对金额。
- `calls.rs` 对正在执行的只读跨 canister 工作去重：同一付款方或报价开单、同一入金 block 查账、同一转账腿对账只允许一个请求进行，其余返回 `Pending`，完成后可重试。全局最多保存 128 个占用键，每次开单占两个键；失败、正常结束及 CDK 取消任务时释放。它不代替稳定 outbox，也不清除未知出金状态。
- 有界配置和预算计数保存在 heap，随普通消息和 `await` 提交；初始化与 `pre_upgrade` 才编码到原有 StableCell。正常升级保留开关、每日成功开单数及每分钟授权/账本操作预算，**升级不可跳过 `pre_upgrade`**。资金记录、入金认领和出金 outbox 仍直接写稳定表，不在升级时整体序列化。
- 授权尝试按每调用方 10 次/分钟、全局 200 次/分钟限流，计数映射最多 200 项；账本操作使用独立的 400 次/分钟预算。失败授权不消耗每日成功开单额度；所有预算在同 schema 升级后保留。预算变化仅更新 heap，不重算公开配置认证叶。
- `get_configuration_certified` 默认按当前时间选择有效费率，与 `get_fee_policy` 一致；显式版本可查询历史政策。初始政策必须在安装时已生效，后续政策的版本和生效时间严格递增；查询反向查找，调度读取最后一项，政策表最多 256 项并使用紧凑编码。
- controller 可通过 `set_ledger_fee` 维护已核对的网络手续费，不能超过初始化的 `max_fee`。它更新认证配置和后续新转账的预期费用，不重写原 Quote 或已准备的出金腿。旧单预留不足仍返回 `FeeBlocked`；未知结果不能因更新手续费而重建，明确拒绝才允许受限修订。
- `MemoryManager` 使用 16 页（1 MiB）分配桶，库的 32,768 桶上限对应约 32 GiB；该数字是地址容量，不是业务容量承诺。
- 领取退款、领取剩余手续费及修订拒绝转账只修改内部预留/出金记录，不重算未变化的 `EscrowInfo` 认证叶。资金方向、入金和实际支付发生变化时才更新认证树；重复成功回调直接返回已有结果。
- 无心跳、轮询 timer 或后台账本扫描。结算提交不额外查账；出金保留原始去重参数，未知响应仍须原参数重试或账本对账。

认证树仍保存在 heap，升级时扫描全部 escrow 重建，只在完成后发布一次根。该路径仍随订单数增长，订单与入金记录尚未压缩归档。下述容量测试覆盖指定样本；持续入金/出金历史、真实资产和主网负载需独立验收。

实践依据：[ICP 跨 canister 调用与回调恢复](https://docs.internetcomputer.org/guides/security/inter-canister-calls/)、[Rust 稳定存储](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[CDK 取消任务与 Drop 清理](https://docs.rs/ic-cdk/0.20.2/ic_cdk/futures/index.html)。本实现仅对有界配置采用升级 hook，不将此方式扩展到订单集合。

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

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_payment
```

`payment_regressions.rs` 覆盖 256 条费率政策的生效边界和认证查询、手续费维护/并发变更、独立授权预算、最后一个每日名额的并发竞争、拒绝原因及升级保留。模型测试逐项拒绝错误收据绑定、大小、保留期限、签名和 signer 区间；出金结果表覆盖全部 ICRC 拒绝类别与历史 Unknown。

`payment_optimization.rs` 另覆盖并发开单/入金/对账去重、失败后的重试、启用与 signer 撤销竞争、退款/费用修订的认证视图、升级后配额保留及费用余额的单次领取。既有集成测试继续覆盖未知支付结果、重复回调、手续费修订与直接到期退款。

真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。

Quote/AdmissionReceipt 使用 delivery profile 2。平台费由固定 SNS governance 发布的版本化比例/最低费政策计算，订单保留接受时的绝对原子金额。参见 [commerce contract](../../docs/protocol/commerce.md)。开发稳定 schema 为 6。

配置中的费用政策与历史政策表均使用独立紧凑表示。`TransferLeg.last_failure` 保存有界拒绝原因，包括过期、余额不足、临时不可用及传输不确定性；不保存账本提供的任意长度文本，成功后清空。该诊断不改变资金方向，也不把历史 Unknown 变成明确拒绝。
