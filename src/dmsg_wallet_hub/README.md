# dmsg_wallet_hub

团队钱包的工厂，接管保留的主网 ID `ghcje-dyaaa-aaaas-qg7va-cai`（产品设计 D-1）。它按 dMsg 会员套餐的配额出售一次性激活、创建 [dmsg_wallet](../dmsg_wallet/README.md) 实例并作为其唯一 controller，保存 SNS 登记的钱包模块，只在实例自己接受后升级或锁定它，并维护成员到钱包的索引。hub 不读取也不移动任何钱包的资金。

公开合同见 [wallet_zh.md](../../docs/protocol/wallet_zh.md)，接口见 [dmsg_wallet_hub.did](dmsg_wallet_hub.did)。

## 接口

| 类别 | 方法 | 调用者 |
| --- | --- | --- |
| 激活 | `quote_activation(account_id)`、`create_wallet(CreateWallet)`、`reconcile_activation(quote_id)`、`claim_activation_refund(quote_id)` | 报价调用者（付款人） |
| 收入 | `collect_revenue(quote_id)` | 任何人 |
| 实例 | `upgrade_wallet`、`lock_wallet`、`refuel_wallet`、`wallet_health`、`sync_wallet` | 任何人（只按实例自己的决定行事） |
| 查询 | `get_hub_config`、`get_user_homes`、`list_versions`、`hub_stats`、`get_quote`、`get_order`、`get_wallet_record`、`list_wallets_of(member)`、`staged_wasm` | 任何人 |
| 模块 | `upload_wasm_chunk`、`clear_wasm_staging` | controller、governance 或 `admin_set_uploader` 指定的上传者 |
| 治理 | `admin_register_version`、`admin_retire_version`、`admin_set_pricing`、`admin_set_revenue`、`admin_set_uploader`、`admin_add_user_home`，各有同参数 `validate_*` query | controller 或 SNS governance |

## 激活

1. `quote_activation` 向 commerce 的 `refresh_entitlement` 读取账户的基础套餐（从未付费的账户为 Free），套餐配额减去该账户已占用的激活数；报价 15 分钟有效，收款子账户为 `deposit_subaccount(hub, quote_id)`。
2. `create_wallet` 核对调用者、报价未过期且未退款、规格（`validate_spec`）、创建者的 `WalletCreate` 设备批准（经账户所在 user home 核验）与收款子账户余额，记为 `Paid` 并占用配额；随后创建 canister（controllers = [hub]，冻结阈值 365 天，日志公开，附 `cycles_to_wallet`），把模块分块上传到实例的 chunk store 后 `install_chunked_code`，最后清空 chunk store。每步都写订单状态，同一订单的并发调用返回 `Pending`。
3. 创建失败的订单与过期未用的报价可由付款人退款（扣一次账本手续费）。
4. `collect_revenue` 把已就绪订单的收款分两笔转出：`cycles_share_e8s` 以 ICRC-1 memo `TPUP` 转入 CMC 的 hub 子账户并 `notify_top_up`，其余转入 `revenue`。每笔的参数与 `created_at_time` 固定保存，重试由账本去重。

## 模块与升级

模块（建议为 release 产出的 `.wasm.gz`，≤ 4 MiB，最多 16 个版本）先分块上传到暂存区，再由治理以模块哈希 `admin_register_version` 登记；新钱包使用最新未退役版本。`upgrade_wallet` 读取实例的 `accepted_upgrade()`，核对版本已登记、哈希一致且与已装版本不同，然后 stop → 分块 install（`Upgrade`，参数为版本号）→ start；安装失败也会重新启动实例。`lock_wallet` 在实例接受 `LockCode` 后把 controllers 设为空。

## 部署与上线

1. 部署 hub（`HubInit`：environment、SNS governance、ICP 账本、CMC、commerce、issuer namespace、user homes、`key_1`、`Mainnet`、定价与配额、收入账户），给 hub 充足 cycles。
2. 治理在 commerce 登记产品 `dmsg-wallet`（`quote_authority = beneficiary_authorities = adapter = hub`，subject schema `dmsg-wallet-v1`，subject 10 字节）与应用 `dmsg-wallet`（origins 含 `https://dmsg.net`，capabilities `[Checkout]`）。
3. 上传 release 中的 `dmsg_wallet.wasm.gz` 并以其 SHA-256 提案登记。
4. ICPanda DAO 团队钱包由 Max 套餐成员创建，SNS governance 作为 Recovery 成员；`dmsg_handle` 与 `dmsg_payment` 的收入账户经治理改为该钱包的 ICP 地址。

## 验证

`cargo test -p dmsg_wallet_hub` 只编译；行为由 PocketIC 控制面 `wallet::*` 覆盖（激活、配额、错误调用者、规格与批准不符、未付款、升级、锁定、退款、收入归集），CMC 由测试替身 [dmsg_test_cmc](../../tests/dmsg_test_cmc/src/lib.rs) 代替。主网部署、真实 CMC 汇率与 SNS 提案流程尚未验收。
