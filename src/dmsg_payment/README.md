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

`api.rs` 负责外部调用和本地提交，`model.rs` 验证报价、收据及资金转换，`state.rs` 保存内部资金记录，`store.rs` 保存稳定表和公开认证视图。schema 3 的私有 `stable_codec.rs` 为配置和 escrow 使用 CBOR 整数 map key，共用 compact representation 覆盖报价、入金、出金与 signer；公开支付摘要和认证叶编码不变。`dmsg_runtime::ledger` 是独立账本适配器。

## 执行成本与恢复边界

- 开单先检查去重与配额，只计算一次报价摘要并验证一次报价签名。`verify_payment_offer` 返回后重新检查启用状态、报价/offer 期限与 signer 撤销；报价内容、支付配置和同一 epoch 的 signer 公钥不可变，无需再次验签。以后若增加修改这些字段的入口，必须同时更新回调校验。
- `calls.rs` 对正在执行的只读跨 canister 工作去重：同一付款方或报价开单、同一入金 block 查账、同一转账腿对账只允许一个请求进行，其余返回 `Pending`，完成后可重试。全局最多保存 128 个占用键，每次开单占两个键；失败、正常结束及 CDK 取消任务时释放。它不代替稳定 outbox，也不清除未知出金状态。
- 固定大小的配置和预算计数保存在 heap，随普通消息和 `await` 提交；初始化与 `pre_upgrade` 才编码到原有 StableCell。正常升级保留开关、每日开单尝试数及每分钟账本操作预算，**升级不可跳过 `pre_upgrade`**。资金记录、入金认领和出金 outbox 仍直接写稳定表，不在升级时整体序列化。
- 领取退款、领取剩余手续费及修订拒绝转账只修改内部预留/出金记录，不重算未变化的 `EscrowInfo` 认证叶。资金方向、入金和实际支付发生变化时才更新认证树；重复成功回调直接返回已有结果。
- 无心跳、轮询 timer 或后台账本扫描。结算提交不额外查账；出金保留原始去重参数，未知响应仍须原参数重试或账本对账。

认证树仍保存在 heap，升级时扫描全部 escrow 重建，只在完成后发布一次根。该路径仍随订单数增长，订单与入金记录也尚未压缩归档；这轮优化不代表已经完成大规模容量验收。大数据量上线前需测量重建指令数、heap 峰值及长期稳定存储增长。

实践依据：[ICP 跨 canister 调用与回调恢复](https://docs.internetcomputer.org/guides/security/inter-canister-calls/)、[Rust 稳定存储](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[CDK 取消任务与 Drop 清理](https://docs.rs/ic-cdk/0.20.2/ic_cdk/futures/index.html)。本实现仅对固定大小的配置采用升级 hook，不将此方式扩展到订单集合。

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

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_payment
```

`payment_optimization.rs` 另覆盖并发开单/入金/对账去重、失败后的重试、启用与 signer 撤销竞争、退款/费用修订的认证视图、升级后配额保留及费用余额的单次领取。既有集成测试继续覆盖未知支付结果、重复回调、手续费修订与直接到期退款。

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
