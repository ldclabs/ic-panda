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

`api.rs` 负责外部调用和本地提交，`model.rs` 验证报价、收据及资金转换，`state.rs` 保存内部资金记录，`store.rs` 保存稳定表和公开认证视图。`dmsg_runtime::ledger` 是独立账本适配器。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_payment
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
