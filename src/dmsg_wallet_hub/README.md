# dmsg_wallet_hub

团队钱包的工厂，接管保留的主网 ID `ghcje-dyaaa-aaaas-qg7va-cai`（产品设计 D-1）。它按 dMsg 会员套餐的配额出售一次性激活、创建 [dmsg_wallet](../dmsg_wallet/README.md) 实例并作为其唯一 controller，保存 SNS 登记的钱包模块，只在实例自己接受后升级或锁定它，并维护成员到钱包的索引。hub 不读取也不移动任何钱包的资金。

公开合同见 [wallet_zh.md](../../docs/protocol/wallet_zh.md)，接口见 [dmsg_wallet_hub.did](dmsg_wallet_hub.did)。

## 接口

| 类别 | 方法 | 调用者 |
| --- | --- | --- |
| 激活 | `quote_activation_preview(account_id)`（query）、`quote_activation(account_id)`、`create_wallet(CreateWallet)`、`claim_activation_refund(quote_id, to)`；`reconcile_activation(quote_id)` | 报价调用者（客户端为本机钱包密钥）；续做任何人可调 |
| 收入 | `collectable_activations(after, take)`（query）、`collect_revenue(quote_id)` | 任何人 |
| 实例 | `upgrade_wallet`、`lock_wallet`、`wallet_health`、`sync_wallet` | 任何人（只按实例自己的决定行事） |
| 查询 | `get_hub_config`、`get_user_homes`、`list_versions`、`hub_stats`、`get_quote`、`get_order`、`get_wallet_record`、`list_wallets_of(member)`、`list_wallets(after, take)`、`staged_wasm` | 任何人 |
| 模块 | `upload_wasm_chunk`、`clear_wasm_staging` | controller、governance 或 `admin_set_uploader` 指定的上传者 |
| 治理 | `admin_register_version`、`admin_retire_version`、`admin_remove_version`、`admin_set_pricing`、`admin_set_revenue`、`admin_set_uploader`、`admin_add_user_home`，各有同参数 `validate_*` query | controller 或 SNS governance |

钱包的 cycles 由实例自己的 `refuel()` 补充（任何人可调）；看门狗用 `list_wallets` 遍历实例，`wallet_health` 读取只有 controller 才能看到的 `canister_status`。

## 激活

1. `quote_activation_preview` 只读 hub 的定价、版本与各套餐的剩余额度，不写报价；客户端用它和 commerce 的套餐、账本手续费一起快速显示“获取报价”。额度为 0 时返回剩余 0，不报错。
2. `quote_activation` 向 commerce 的 `refresh_entitlement` 读取账户的基础套餐（从未付费的账户为 Free），套餐配额减去该账户已占用的激活数；报价 15 分钟有效，收款子账户为 `deposit_subaccount(hub, quote_id)`。按当前账本手续费，收入的两笔转账付不出（`price ≤ share + 2·fee` 或 `share ≤ fee`）时不出报价。额度用完时仍返回报价（`remaining = 0`），由 `create_wallet` 和客户端拒绝，用户能看到套餐限制。
3. `create_wallet` 核对调用者、报价未过期且未退款、未暂停（`pricing.paused`）、规格（`validate_spec`）、创建者的 `WalletCreate` 设备批准（经账户所在 user home 核验）与收款子账户余额，跨 canister 调用返回后以新读取的时间重查期限、退款、暂停与配额，记为 `Paid` 并占用配额；随后创建 canister（controllers = [hub]，冻结阈值 365 天，日志公开，附 `cycles_to_wallet`），把模块分块上传到实例的 chunk store 后 `install_chunked_code`，最后清空 chunk store。每步都写订单状态，同一订单的并发调用返回 `Pending`。订单已存在时，同一规格的 `create_wallet` 继续未完成的激活，Ready、Failed、Refunded 原样返回。
4. 创建 canister 失败（`Failed`）的订单与过期未用的报价由报价调用者以 `claim_activation_refund(quote_id, to)` 退款：收款扣一次账本手续费后转到调用者指定的 `to`，须在 hub 之外，所有者不能是匿名或管理 canister。客户端以本机钱包密钥领取，`to` 为付款身份的默认账户。第一次领取固定账户与 `created_at_time`，之后的调用只完成这笔转账并返回同一区块；订单记为 `Refunded`。已就绪订单的收款是收入，领取一律返回 `Forbidden`，不会推进收入转账。
5. 订单就绪即进入 `collectable_activations`；维护任务对每个 ID 调 `collect_revenue`，把收款分两笔转出：`cycles_share_e8s` 以 ICRC-1 memo `TPUP` 转入 CMC 的 hub 子账户并 `notify_top_up`，其余转入 `revenue`。每笔的参数与 `created_at_time` 固定保存，重试由账本去重；CMC 退回时记为铸得 0 cycles 并结束。拆分与收入账户在余额读取返回后读取。`hub_stats.uncollected` 是待归集数。

## 模块与升级

模块（建议为 release 产出的 `.wasm.gz`，≤ 4 MiB，最多保存 16 个版本，含已退役）先分块上传到暂存区，再由治理以模块哈希 `admin_register_version` 登记；新钱包使用最新未退役版本。提案投票期间暂存区不能再动，否则执行时哈希不符。版本号单调分配，`admin_remove_version` 只删已退役的版本及其模块字节以腾出名额，号码不再复用。

`upgrade_wallet` 读取实例的 `accepted_upgrade()`，核对版本已登记、哈希一致且与已装版本不同，然后 stop → 分块 install（`Upgrade`，参数为版本号）→ start。停止失败（例如在途调用使其超时）或安装失败都会重新启动实例；安装成功即记下新版本，即使随后启动失败。读取 `accepted_upgrade()` 失败时先 `start_canister`（对运行中的实例无影响），所以上一次没能启动的实例在下一次调用时恢复运行。`lock_wallet` 在实例接受 `LockCode` 后把 controllers 设为空。

## 存储

stable 记录用整数键的紧凑编码（`dmsg_runtime::stable_types::wallet` 与 [stable_codec.rs](src/stable_codec.rs)），`post_upgrade` 校验 `STABLE_SCHEMA`。订单只存规格与进度，报价单独存一份：一次激活（报价 + 订单）由 1,250 B 降为 669 B（3 名成员的规格）。报价不再改写 hub 记录；`sync_wallet` 成员不变时不写，变化时只改动增删的索引项。

## 部署与上线

1. 部署 hub（`HubInit`：environment、SNS governance、ICP 账本、CMC、commerce、issuer namespace、user homes、`key_1`、`Mainnet`、定价与配额、hub 以外的收入账户），给 hub 充足 cycles。保留 ID 上没有模块（2026-10-18 核对），首次安装即普通安装。
2. 治理在 commerce 登记产品 `dmsg-wallet`（`quote_authority = beneficiary_authorities = adapter = hub`，subject schema `dmsg-wallet-v1`，subject 10 字节）与应用 `dmsg-wallet`（origins 含 `https://dmsg.net`，capabilities `[Checkout]`），并把 hub 的 `admin_*` 方法与对应 `validate_*` 注册为 SNS generic function。
3. 上传 release 中的 `dmsg_wallet.wasm.gz` 并以其 SHA-256 提案登记。
4. ICPanda DAO 团队钱包由 Max 套餐成员创建，成员只有团队成员（产品设计 D-6：SNS governance 不作为成员，需要 SNS 治理的资金留在 SNS 国库）；hub 的收入账户可经治理改为该钱包的 ICP 地址。
5. 定期运行维护：`collectable_activations` + `collect_revenue`，`list_wallets` + 实例 `refuel()`。

## 验证

`cargo test -p dmsg_wallet_hub` 检查 hub 记录、订单与 payout 的紧凑编码（往返、整数键、体积）；`dmsg_runtime` 检查报价、规格、钱包记录与配置的 repr。行为由 PocketIC 控制面 `wallet::*` 覆盖：激活、报价预览、配额（含额度用完后报价 `remaining = 0` 且不能创建）、错误调用者、规格与批准不符、未付款、安装中断后续做、治理参数校验与 validator 文本、升级、版本退役与删除、锁定、过期报价与创建失败退到付款人账户、收入可归集列表与归集、已就绪订单拒绝退款。CMC 由测试替身 [dmsg_test_cmc](../../tests/dmsg_test_cmc/src/lib.rs) 代替。主网部署、真实 CMC 汇率与 SNS 提案流程尚未验收。
