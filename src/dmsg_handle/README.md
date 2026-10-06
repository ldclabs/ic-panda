# dmsg_handle

dMsg 的名称注册表，是“规范化名称 → 稳定 `AccountId`”映射的唯一写入方。它负责新名称的 PANDA 收费注册、旧 `ic_message` 冻结名称的导入与免费认领、双方批准的转移，以及权属的认证查询。名称只是指向账户的可变入口：转移名称不会移动账户、内容、签名身份或外部应用权限，账户也不需要名称。

完整接口见 [dmsg_handle.did](dmsg_handle.did)，公开类型见 [dmsg_types::handle](../dmsg_types/src/handle.rs)，规范化与定价见 [dmsg_protocol::handle](../dmsg_protocol/src/handle.rs)。

## 架构设计

### 组件关系

```mermaid
flowchart LR
  A["账户客户端"] -- "mutate_account<br/>AuthorizeHandle" --> U["dmsg_user<br/>（home_user）"]
  C["付款人 / 旧 owner / 转交方"] -- "register / claim / transfer" --> H["dmsg_handle"]
  H -- "consume_handle_authorization<br/>consume_handle_transfer_authorizations" --> U
  H -- "icrc2_transfer_from<br/>icrc3_get_blocks" --> L["PANDA 账本"]
  O["controller / SNS governance"] -- "导入旧名快照<br/>维护手续费、提取收入" --> H
  V["验证者"] -- "resolve_handle_certified" --> H
```

| 组件                             | 在名称流程中的职责                                                                                      |
| -------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `dmsg_handle`                    | 名称表、旧名预留、注册收费操作与锁、权属事件、认证树                                                    |
| `dmsg_user`                      | 校验账户的认证 caller 与管理员设备，保存短期的精确名称意图；handle 只消费这些意图，不读取账户的其他状态 |
| PANDA 账本                       | ICRC-2 扣款；ICRC-3 区块用于对账                                                                        |
| `ic_message`、`ic_name_identity` | 冻结后提供带证书的名称与角色快照，只在迁移时使用                                                        |

handle 不保存设备、密钥、资料或旧名称区块，注册也不创建旧 Profile 或 COSE namespace。

### 接口与调用者

| 类别 | 方法                                                                                                                                                             | 调用者                       |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- |
| 注册 | `register_handle`、`commit_handle`                                                                                                                               | 付款人                       |
| 对账 | `reconcile_handle_charge`                                                                                                                                        | 任何人，以账本区块为证据     |
| 认领 | `claim_legacy_handle`                                                                                                                                            | 冻结 owner 或冻结管理员      |
| 转移 | `transfer_handle`                                                                                                                                                | 任何人，需携带双方批准       |
| 查询 | `resolve_handle_certified`、`snapshot_certified`、`get_handle_event`、`get_handle_operation`、`get_legacy_reservation`、`snapshot_progress`、`get_handle_config` | 任何人                       |
| 管理 | `begin_legacy_snapshot`、`import_legacy_handles`、`seal_legacy_snapshot`、`update_ledger_fee`、`admin_collect_token`                                             | controller 或 SNS governance |
| 预演 | 与每个管理方法同参数的 `validate_*`，如 `validate_import_legacy_handles`                                                                                         | 任何人，只读 query           |

管理方法接受本 canister 的 controller 或初始化固定的 `governance`。`validate_*` 按当前状态执行与对应方法相同的校验，通过时返回给提案投票者看的说明文字，失败时返回错误名；它不修改状态，可用于 SNS 通用提案的验证方法，也可供 controller 提交前预演。

### 授权模型

账户发起的名称操作分两步授权：

1. 账户在 `dmsg_user` 调用 `mutate_account`，命令为 `AuthorizeHandle { intent }`。user 要求 caller 是该账户的认证绑定、账户处于 Active，并由具有 `RootManage` 能力的管理员设备签名整条命令；`intent.account_id` 必须是该账户，`intent.handle_canister` 必须是配置的 handle。批准最长有效 60 秒，每个账户最多同时保留 32 条。
2. handle 处理业务调用时，向初始化固定的 `home_user` 调用 `consume_handle_authorization`；转移改用 `consume_handle_transfer_authorizations`，一次核对双方。user 只接受配置的 handle 作为调用者，要求同一 `op_id` 下存有完全相同且未过期的意图。这一步只读，不再检查设备撤销或账户状态；重复执行由 handle 按 `(account_id, op_id)` 去重。

`HandleIntent` 固定 handle canister、动作、账户、目标账户、预期版本、`op_id` 和 `terms_digest`，`terms_digest` 再固定各动作的具体条款：

| 动作                          | `terms_digest` 绑定的内容                                  | handle 对 caller 的要求                      |
| ----------------------------- | ---------------------------------------------------------- | -------------------------------------------- |
| `Register`                    | 账本、付款账户、金额、手续费（`charge_terms_digest`）      | 必须是 `payer.owner`                         |
| `ClaimLegacy`                 | 快照 ID、完整冻结记录、目标账户                            | 冻结 owner；名称由名称账户持有时为冻结管理员 |
| `Transfer` / `AcceptTransfer` | handle canister、名称、原 owner、接收方、预期版本、`op_id` | 不限                                         |

因此 handle 不检查 caller 与 `account_id` 的绑定：账户归属在批准时由 user 校验，handle 检查的是该动作需要的另一方。付款人可以不是账户的认证身份，客户端用单独连接的钱包付款；由他人代付时，也必须由账户批准一个写明该付款人的意图。

### 注册与收费

名称为 1–20 字节的 ASCII 字母、数字或下划线，不能以下划线开头，统一转为小写。价格只取决于字节长度：

| 长度  |         1 |       2 |    3–4 |    5–6 |  7–20 |
| ----- | --------: | ------: | -----: | -----: | ----: |
| PANDA | 1,000,000 | 200,000 | 50,000 | 20,000 | 1,000 |

付款人共支付名称价格：其中 `price − ledger_fee` 转入 handle canister 的默认账户，`ledger_fee` 是账本手续费。付款人须预先给 handle canister 不低于价格的 ICRC-2 allowance。

`register_handle(Registration)` 在一次调用内完成授权、加锁和扣款：

1. 校验意图、caller 与付款人、手续费和 `terms_digest`，并检查名称不在活跃名称、旧名预留或扣款锁中，全局 `max_pending` 未满，该账户没有未决扣款。旧快照封存前返回 `LegacyWriteDisabled`。
2. 向 `home_user` 核对授权。
3. 回调后先查重放，再重读配置、锁和容量并重新校验，不复用等待前的快照。
4. 在发出扣款前，同一消息内持久化名称锁、账户锁、`Charging` 操作和未决计数。
5. 以固定的付款账户、金额、手续费、memo（`digest("dmsg/handle-memo/v1", (canister, op key))`）和 `created_at_time` 调用 `icrc2_transfer_from`，在回调中提交结果。

| 阶段                  | 含义与后续                                                                                      |
| --------------------- | ----------------------------------------------------------------------------------------------- |
| `Charging`            | 扣款已发出，或回调因升级丢失。锁保留，`commit_handle` 用原参数重试                              |
| `ChargeUnknown`       | 扣款结果未知。锁保留，`commit_handle` 用原参数重试，或用 `reconcile_handle_charge` 提交真实区块 |
| `Committed`           | 权属、事件、操作终态和锁释放在同一回调中提交                                                    |
| `Rejected { reason }` | 首次扣款被账本明确拒绝或确定未执行。立即释放锁，该 `op_id` 作废，重放返回同一终态               |

- 重试使用完全相同的账本参数，账本去重窗口内返回 `Duplicate` 即视为已付款。窗口过后只能用实际区块对账；不会刷新时间戳，也不会因本地超时释放名称。
- 出现过未知结果后，后续拒绝不能证明之前没有扣款，操作保持 `ChargeUnknown`。
- `commit_handle` 只接受付款人。`reconcile_handle_charge` 任何人可调用，但只在区块的付款账户、收款账户、金额、memo、`created_at_time` 和 spender 全部匹配时提交。
- 内存中的调用 guard 使同一操作的扣款、重试与对账互斥，执行中返回 `Pending`。升级会清空 guard，遗留的 `Charging` 按结果未知处理。
- `get_handle_operation(account_id, op_id)` 返回操作的当前状态，客户端据此恢复原流程。

### 旧名认领

旧名称以 `LegacyReservation` 永久预留，不能被新注册。`claim_legacy_handle(intent, snapshot_id)` 要求快照已封存且 ID 一致、记录未隔离（否则返回 `Locked`），并按冻结记录检查 caller：

- 普通名称：caller 必须是冻结的 `legacy_owner`。
- 由名称账户持有的名称（`legacy_name_principal == legacy_owner`）：caller 必须是冻结管理员之一，名称账户本身不能认领。没有冻结管理员的这类名称由导入计划标为隔离。

认领不收费，提交版本 1，事件的 `from` 为空。由于还需要目标账户批准，旧 owner 只能把名称认领到同意接收的账户。

### 转移

`transfer_handle(from, accept)` 需要原 owner 的 `Transfer` 意图和接收方的 `AcceptTransfer` 意图：两者的名称、预期版本、`op_id` 和 `terms_digest` 相同，账户互为对方的目标。handle 先核对当前 owner 和版本，再一次调用 user 核对双方授权，回调后要求记录未变，然后提交版本 +1。

认领和转移按 `(account_id, op_id)` 保存请求摘要与原始权属回执：原请求重放返回原结果，即使名称后来又被转移；同一 ID 换了参数返回 `IdempotencyConflict`。

### 认证查询与事件

认证树驻留 heap，只有两类叶：

- 名称 → `HandleRecord` 的规范 CBOR，包含当前 owner、版本和产生该权属的事件摘要 `event_tip`。
- `_legacy_snapshot` → `SnapshotProgress`。名称不能以下划线开头，所以该键不会与名称冲突。

`resolve_handle_certified` 每次查询 1–64 个名称，返回包含或不存在证明；验证者须核对信任根、canister ID、证书时间、witness 和叶字节。`snapshot_certified` 证明导入进度。旧名预留不进入认证树，旧名数量不影响升级成本。

每次权属变化追加一条 `HandleEvent`（`StableLog`，序号即追加时的日志长度），事件以 `previous` 串成全局摘要链，摘要为 `digest("dmsg/handle-event/v1", event)`，并记录所属快照 ID。`get_handle_event(sequence)` 是普通 query，`HandleRecord.event_tip` 可用来核对读到的事件。

`get_legacy_reservation`、`snapshot_progress`、`get_handle_operation`、`get_handle_config` 也是普通 query。认领会在链上重新核对完整冻结记录，伪造的预留查询结果只会让认领失败。

### 存储

稳定布局为 schema 8（相对 schema 7 只在配置中增加 `governance`）。`post_upgrade` 遇到其他 schema 直接失败，布局变化须编写显式迁移。`MemoryManager` 使用 16 页（1 MiB）分配桶。

| Memory | 内容                                                                                 |
| -----: | ------------------------------------------------------------------------------------ |
|      0 | 配置 `StableCell`：`HandleInit`（含 `governance`）、快照进度、事件链 tip、未决扣款数 |
|      1 | `NAMES`：名称 → `HandleRecord`                                                       |
|      2 | `LEGACY`：名称 → `LegacyReservation`                                                 |
|      3 | `OPS`：op key → 注册收费操作                                                         |
|      4 | `LOCKS`：名称 → 持锁操作的 op key                                                    |
|   5、8 | `EVENTS`：`StableLog` 的索引与数据                                                   |
|      6 | `ACTIVE_ACCOUNTS`：有未决扣款的账户                                                  |
|      7 | `OWNERSHIP`：op key → 认领或转移回执                                                 |

op key 为 `digest("dmsg/handle-operation/v1", (account_id, op_id))`。私有的 `stable_codec.rs` 用 CBOR 整数 map key 保存记录，不影响公开 Candid、摘要和认证叶编码。heap 只保存已解码的配置（每次变更同步写入 stable cell）、认证树和调用 guard。没有 `pre_upgrade`；升级时从 `NAMES` 和快照进度重建认证树，只发布一次根哈希。操作、回执和事件只增不删，以保持幂等语义。

### 容量与成本

- 活跃名称与未决扣款合计不超过 100,000（`MAX_ACTIVE_NAMES`）。新注册和认领在授权前后都检查，为已发出的扣款保留提交位置；达到上限返回 `QuotaExceeded`，转移、重放、重试和对账不受影响。
- 全局未决扣款不超过 `max_pending`（1–10,000），每个账户同时最多一笔。
- 旧名快照不超过 1,000,000 条，每批导入 1–256 条，每条最多 16 个冻结管理员。

升级成本随活跃名称线性增长，主要花在重建认证树。以下为 2026-10-02 在 schema 7、Rust 1.98.1、PocketIC 16.0.0、release 配置下的 `handle_active_name_capacity_profile` 实测；每个样本另有 1 条未认领旧名，未模拟同规模的收费操作、事件或回执历史。schema 8 未改变名称表和认证树，未重新测量：

| 活跃名称数 |      升级指令数 |     升级 cycles |  heap 字节 | stable 字节 | 64 名称响应字节 |
| ---------: | --------------: | --------------: | ---------: | ----------: | --------------: |
|      1,000 |     657,682,610 |   4,022,926,949 |  1,572,864 |   3,211,264 |          50,118 |
|     10,000 |   8,636,811,833 |  12,002,056,319 |  3,997,696 |   4,259,840 |          63,038 |
|    100,000 | 106,721,845,931 | 110,087,090,634 | 28,114,944 |  23,134,208 |          75,426 |

指令数取自 `post_upgrade` 的 `performance_counter(0)` 差值。响应字节包含证书和成功 Result 封装；单次 64 名称查询的 host 耗时 6–19 ms，含 PocketIC 开销，不代表主网延迟或吞吐。100,000 名称样本还验证了满容量时注册、认领的本地拒绝和双方授权转移。上限取已实测规模，提高前须重新测量升级预算和内存。

同日 `handle_cycles_profile` 用同一 host 程序及 user、ledger Wasm，对比 schema 6 与 schema 7 的 handle（256 条旧名，逐步注册到 17 个活跃名称）：

| 项目                            |            schema 6 |            schema 7 |
| ------------------------------- | ------------------: | ------------------: |
| 首次注册 cycles                 |          22,700,188 |          22,689,030 |
| 已有 8 次注册后的新注册 cycles  |          22,803,734 |          22,761,180 |
| 已有 16 次注册后的新注册 cycles |          22,919,303 |          22,859,037 |
| 同一请求重放 cycles             | 8,358,503–8,429,106 | 8,355,829–8,428,322 |
| 17 个活跃名称的升级 cycles      |       3,308,105,352 |       3,371,517,203 |
| Wasm 字节                       |           1,611,468 |           1,642,673 |

注册 cycles 下降 0.05%–0.26%，重放基本持平；新增的恢复保护、回执和容量检查使 Wasm 增加 31,205 字节，小规模升级费用增加约 1.9%。cycles 只统计 handle，升级费用包含 Wasm 安装成本，不能代替 hook 指令数。`handle_scale_profile` 另以公开接口完成 `1,000 旧名 + 17 活跃`、`10,000 旧名 + 17 活跃`、`1,000 旧名 + 1,017 活跃` 三组写入、转移和重复升级；最后一组注册约 24.5M cycles、转移约 16.3M cycles。2026-09-25 的 schema 6 规模样本见 Git 历史。

## 部署流程

### 初始化参数

| `HandleInit` 字段 | 要求                                                                                                                                                                                                |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `home_user`       | 本部署的 `dmsg_user` canister ID，初始化后不可修改                                                                                                                                                  |
| `ledger`          | PANDA 账本，须为支持 ICRC-2 与 ICRC-3 `icrc3_get_blocks` 的 DFINITY ICRC 账本；生产 ID 见 [sns_canister_ids.json](../../sns_canister_ids.json) 的 `ledger_canister_id`。初始化后不可修改            |
| `ledger_fee`      | 账本当前的 `icrc1_fee`，必须小于 1,000 PANDA；之后用 `update_ledger_fee` 维护                                                                                                                       |
| `max_pending`     | 全局未决扣款上限，1–10,000                                                                                                                                                                          |
| `governance`      | 除 controller 外可以执行管理方法的 SNS governance canister；生产 ID 见 [sns_canister_ids.json](../../sns_canister_ids.json) 的 `governance_canister_id`，本地可填部署者 principal。初始化后不可修改 |

`home_user`、`ledger` 和 `governance` 不能是匿名或管理 canister principal，参数不合法时安装失败。`dmsg_user` 的 `UserInit.handle_canister` 必须指向本 canister。两者互相引用，所以先创建 canister ID，再分别安装。

### 步骤

以下命令在仓库根目录执行，以 `--network ic` 为例；本地开发去掉该参数。

1. 在待部署的提交上完成验证。脚本会构建 release Wasm，并核对 Candid、协议向量和 PocketIC 回归；工作区应没有未提交的改动：

   ```sh
   POCKET_IC_BIN=/path/to/pocket-ic make test-dmsg
   ```

2. 创建 `dmsg_user`、`dmsg_handle`，以及 `UserInit` 引用的其他 canister ID：

   ```sh
   dfx canister create dmsg_user --network ic
   dfx canister create dmsg_handle --network ic
   ```

3. 以 `handle_canister = $(dfx canister id dmsg_handle --network ic)` 安装 `dmsg_user`，其他字段见 [dmsg_user](../dmsg_user/README.md)。

4. 查询账本手续费，然后安装 handle：

   ```sh
   dfx canister call --network ic <PANDA 账本 ID> icrc1_fee --query
   dfx deploy dmsg_handle --network ic --argument "(record {
     home_user = principal \"$(dfx canister id dmsg_user --network ic)\";
     ledger = principal \"<PANDA 账本 ID>\";
     ledger_fee = <icrc1_fee> : nat;
     max_pending = 1000 : nat32;
     governance = principal \"<SNS governance ID>\";
   })"
   dfx canister info dmsg_handle --network ic
   ```

   `dfx deploy` 按 dfx.json 以 `optimize: cycles` 和 gzip 构建；用 `dfx canister info` 记录实际模块哈希和 controllers。

5. 导入并封存旧名快照，步骤见下文“迁移流程”。封存前注册返回 `LegacyWriteDisabled`、认领返回 `VersionConflict`。没有旧名的本地或测试部署也要封存一个空快照：

   ```sh
   ZERO=$(printf '\\00%.0s' $(seq 32))
   ID=$(printf '\\01%.0s' $(seq 32))
   dfx canister call dmsg_handle begin_legacy_snapshot "(record {
     source_canister = principal \"$(dfx identity get-principal)\";
     snapshot_id = blob \"$ID\";
     freeze_version = 0 : nat64;
     event_tip = blob \"$ZERO\";
     count = 0 : nat64;
     entries_digest = blob \"$ZERO\";
   })"
   dfx canister call dmsg_handle seal_legacy_snapshot
   ```

   空快照的 `entries_digest` 必须是 32 个零字节；`snapshot_id` 任意非零，`source_canister` 可以是任意非匿名、非管理 canister 的 principal。

6. 在 `src/dmsg_app/dmsg.config.json` 填写 `canisters.user` 与 `canisters.handle`，重新构建客户端，见 [dmsg_app](../dmsg_app/README.md)。

7. 部署后检查：`get_handle_config` 与安装参数一致；`snapshot_progress` 显示已封存且 `imported` 等于快照 `count`；`snapshot_certified` 和 `resolve_handle_certified` 的证书能通过客户端验证。生产部署和真实 PANDA 账本仍需另行验收。

### SNS 治理

交给 SNS 管理时，先用 `AddGenericNervousSystemFunction` 提案为每个管理方法登记通用函数，目标和验证方法都在 handle canister 上：

| 目标方法                | 验证方法                         |
| ----------------------- | -------------------------------- |
| `begin_legacy_snapshot` | `validate_begin_legacy_snapshot` |
| `import_legacy_handles` | `validate_import_legacy_handles` |
| `seal_legacy_snapshot`  | `validate_seal_legacy_snapshot`  |
| `update_ledger_fee`     | `validate_update_ledger_fee`     |
| `admin_collect_token`   | `validate_admin_collect_token`   |

之后用 `ExecuteGenericNervousSystemFunction` 提案执行，载荷是目标方法的 Candid 参数，可用 `didc encode -d src/dmsg_handle/dmsg_handle.did -m <method> '<参数>'` 生成；迁移计划中的 `candid_hex` 可直接作为载荷。

- 提交提案时 SNS 调用验证方法，按当前状态预演；有先后依赖的提案（导入批次、封存）要等前一个执行后再提交。导入批次带有快照位置，提前提交的后续批次在验证时即返回 `VersionConflict`。
- 执行时 SNS 只判断调用是否得到回复，不解析返回的 `Result`。执行后用 `snapshot_progress`、`get_handle_config` 或账本记录确认结果。

### 运维与升级

- 账本手续费变化时，由 controller 或 governance 调用 `update_ledger_fee(fee)`，新值仍须小于 1,000 PANDA。它只影响新订单：已有操作保留原金额、fee、memo 和账本时间，客户端按新配置重新准备报价。
- 注册收入留在 handle canister 的默认账户，由 controller 或 governance 调用 `admin_collect_token(to, amount)` 提取：转给 `to` 的金额为 `amount`，账本手续费另从 handle 账户扣除，成功时返回账本区块号。该转账不带 `created_at_time`，账本不会去重；返回 `ExecutionUnknown` 时先在账本核对，再决定是否重新提交。

  ```sh
  dfx canister call --network ic dmsg_handle admin_collect_token \
    '(record { owner = principal "<收款 principal>"; subaccount = null }, <amount> : nat)'
  ```

- 升级要求 schema 不变。先停止 canister，让在途的账本回调完成，再升级、启动并查看日志：

  ```sh
  dfx canister stop dmsg_handle --network ic
  dfx deploy dmsg_handle --network ic --argument-type raw --argument 4449444c0000
  dfx canister start dmsg_handle --network ic
  dfx canister logs dmsg_handle --network ic
  ```

  Candid 服务声明了 `HandleInit` 初始化参数，dfx 升级时不带参数会报错；`post_upgrade` 不读取参数，传空 Candid 参数 `()`（hex `4449444c0000`）即可。不要在升级时重新提供 `HandleInit`，它不会修改配置。日志中的 `handle_upgrade names=… instructions=…` 记录本次重建的名称数和指令数。若升级打断了扣款，操作停在 `Charging` 或 `ChargeUnknown`，由付款人 `commit_handle` 或任何人 `reconcile_handle_charge` 恢复。

- 升级所需 cycles 随活跃名称增长（100,000 名称约 110B cycles），升级前确认余额。

## 迁移流程

旧名称来自两个冻结后的旧 canister（ID 见 [canister_ids.json](../../canister_ids.json)，以盘点结果为准）：

| 来源               | 快照范围      | 每行内容                                                                                            |
| ------------------ | ------------- | --------------------------------------------------------------------------------------------------- |
| `ic_message`       | `Names`       | `(名称, owner, 名称账户 principal)`；页上下文为旧名称区块的 `(next_block_height, next_block_phash)` |
| `ic_name_identity` | `Authorities` | `(名称, 名称账户 principal, [(principal, role)])`，在 seal 时固定                                   |

角色表中 `role = 1`（名称账户 owner）的 principal 成为 `frozen_admins`；`role = 0` 的普通成员和 `role = -1` 的停用者不能认领。冻结与快照的服务端合同见 [legacy_freeze_zh.md](../../docs/protocol/legacy_freeze_zh.md)。

### 步骤

1. **盘点**：用 [legacy-inventory.mjs](../dmsg_app/scripts/legacy-inventory.mjs) 确认两个来源的 canister ID、模块哈希、controllers 和名称数量，见 [旧部署只读盘点](../../docs/legacy_inventory_zh.md)。

2. **冻结来源**：用 [legacy-freeze-plan.mjs](../dmsg_app/scripts/legacy-freeze-plan.mjs) 生成分阶段、绑定模块哈希的 controller 调用。按顺序：`admin_legacy_acknowledge_baseline` 确认基线 → `admin_legacy_drain(cutover_id)` 停止新写 → 用 `admin_legacy_pending`、`admin_legacy_resolve` 收敛在途写入 → `admin_legacy_seal(cutover_id)` 进入 ReadOnly。名称注册与转移随之停止，`ic_name_identity` 在 seal 时固定角色表。两个来源使用同一个 cutover ID。

3. **采集快照证明**：用非匿名身份分页调用两个来源的 `legacy_snapshot`（update，每页最多 64 条），保留每页 ingress 的 `request_status` 证书。[@dmsg/legacy](../dmsg_legacy/src/snapshot.ts) 的 `collectSnapshot(agent, canister, scope, [canister])` 负责分页、验证证书和滚动摘要。把两组 proofs 写成 `{ "names": [...], "authorities": [...] }`，其中 `args`、`nonce`、`requestId`、`certificate`、`reply` 编码为小写 hex。仓库没有独立的采集命令，完整的采集和编码示例见 [legacy-probe.ts](../dmsg_app/e2e/legacy-probe.ts)。

4. **生成导入计划**（离线执行，不联网、不签名）：

   ```sh
   node src/dmsg_app/scripts/legacy-name-plan.mjs \
     --input name-proofs.json \
     --root-key-hex <IC 根公钥 DER 的 hex> \
     --source <ic_message ID> \
     --identity <ic_name_identity ID> \
     --count <盘点得到的名称数> \
     --cutover-hex <cutover ID 的 hex> \
     --output name-plan.json
   ```

   [legacy-name-plan.mjs](../dmsg_app/scripts/legacy-name-plan.mjs) 逐页核对证书、调用者、范围、游标、ReadOnly 状态和 cutover，要求页数据闭合、名称数等于 `--count`（不接受空快照），角色表中的每个名称都能对应到名称快照。然后生成：

   - 按名称排序的 `LegacyReservation`；owner 是名称账户且没有冻结管理员的条目标为隔离。
   - `entries_digest`：从 32 个零字节开始，逐条计算 `digest("dmsg/legacy-entry/v1", (前值, entry))`。
   - `snapshot_id`：由来源与身份服务 ID、两边的冻结 epoch、cutover ID、旧名称区块 tip 和 `entries_digest` 派生；`event_tip` 为旧名称区块 tip，`freeze_version` 为名称来源的冻结 epoch。

   输出格式为 `dmsg-legacy-name-plan/1`，包含一次 `begin_legacy_snapshot`、每 256 条一次 `import_legacy_handles(snapshot_id, offset, entries)` 和一次 `seal_legacy_snapshot`，参数均为 Candid hex，`offset` 是该批第一条在快照中的位置。输出文件不覆盖已有文件，权限为 0600。

5. **审核计划**：核对 `count`、`quarantined` 名单、`snapshot`（快照 ID）和调用顺序，与盘点及冻结记录一致后再提交。

6. **提交**：按计划顺序逐条提交，每步后查询 `snapshot_progress`。controller 可直接调用，提交前可用对应的 `validate_*` 预演；由 SNS 管理时，每条调用作为一个通用函数提案，等前一条执行后再提交下一条（见“SNS 治理”）。

   ```sh
   dfx canister call --network ic --type raw dmsg_handle <method> <candid_hex>
   ```

   三个入口都可安全重试：同一快照重复 `begin` 返回成功；导入批次中位于已导入位置的条目必须与已存条目完全相同，作为重放跳过，新条目必须从当前位置开始并按名称递增；`offset` 超过当前进度的批次返回 `VersionConflict`，不会留下缺口；一批中任何条目校验失败则整批不写入；重复封存返回当前进度。一个实例只接受一个快照，已开始后提交不同快照返回 `IdempotencyConflict`，封存后不能再导入。

7. **封存后核对**：`snapshot_progress` 显示 `sealed`，`imported` 等于 `count`，`rolling_digest` 等于 `entries_digest`；`snapshot_certified` 可通过验证。此后开放新注册和认领。

### 用户认领

用户在客户端的名称设置中连接冻结记录里的旧身份。客户端先用 `snapshot_progress`、`get_legacy_reservation` 预检，再构造 `ClaimLegacy` 意图，由新账户批准后以旧身份调用 `claim_legacy_handle`；结果为版本 1 的 `HandleRecord`。传输失败或结果未知时重试原请求，被明确拒绝后才重新准备。实现见 [services/handle.ts](../dmsg_app/src/lib/services/handle.ts)。

### 不迁移的内容

- 不续写旧名称区块链。新事件链从零摘要开始，每条事件记录快照 ID，快照保存旧区块 tip 作为来源上下文。
- 旧 `ic_message` 的 PANDA 余额、在途扣款和 allowance 不会转移，handle 也不使用对旧服务的 allowance；新注册需要对 handle 重新 approve。
- 未认领的旧名没有期限，永久预留。

## 当前限制

- 隔离名称（`quarantined`）没有解除或处置接口。这类名称在旧系统中由名称账户自己持有，冻结角色表里又没有 role 1 的管理员，没有人能证明控制权；认领返回 `Locked`，名称永久保留。
- 每个实例只能导入一个旧快照；更换快照需要新实例。
- 生产部署、真实 PANDA 账本，以及超过 100,000 个活跃名称的容量都未验收。

## 实现

| 文件                  | 内容                                                                                      |
| --------------------- | ----------------------------------------------------------------------------------------- |
| `src/api.rs`          | Candid 入口、注册扣款状态机、认领、转移、快照导入、治理入口与 `validate_*` 预演、收入提取 |
| `src/store.rs`        | 稳定表、配置缓存、回执与 op key、容量常量                                                 |
| `src/calls.rs`        | 扣款调用 guard                                                                            |
| `src/stable_codec.rs` | 配置、名称、导入、操作、事件和回执的紧凑 CBOR 表示                                        |

选择依据参考 ICP 官方的 [Stable structures](https://docs.internetcomputer.org/languages/rust/stable-structures/)、[消息执行原子性](https://docs.internetcomputer.org/references/message-execution-properties/)和[性能测量建议](https://docs.internetcomputer.org/guides/canister-management/optimization/)。

## 验证

```sh
cargo test -p dmsg_handle
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
POCKET_IC_BIN=/path/to/pocket-ic \
  cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane handle_active_name_capacity_profile -- --ignored --nocapture
```

`handle_scale_profile`、`handle_cycles_profile` 用同样方式显式运行。[handle.rs](../../tests/dmsg_integration/tests/control_plane/handle.rs) 与相关 PocketIC 回归覆盖：

- allowance 不足、临时失败和明确拒绝后立即释放名称、账户和全局额度，作废操作 ID 的重放，手续费更新只影响新订单。
- 并发注册只扣一次，额度提前拒绝，锁、事件日志和认证树的升级恢复，未知扣款跨时间和升级不释放名称。
- 成功或拒绝的扣款回复到达前升级，执行中的重试与对账互斥，账本去重窗口过期后的并发对账，付款账户、收款账户、金额、memo、时间戳或 spender 不符的对账证据被拒绝。
- 快照批量导入失败不部分写入、重叠导入重试、跳过前一批的导入被拒绝、冻结管理员与隔离名称、匿名管理员拒绝。
- 认领回执跨转移和升级重放，双方授权过期与并发转移，名称包含与不存在证明、快照认证叶及查询批量上限。
- governance 执行快照导入、封存、手续费更新和收入提取，非管理者被拒绝；`validate_*` 按当前状态预演且不改状态，提前提交的后续导入批次在验证和执行时都被拒绝；提取后账本余额准确变化，超额提取被账本拒绝。

测试账本只模拟 allowance 校验与扣减、固定参数去重和 sender 时间窗口；`approve_test` 只用于测试设置，不模拟真实 approve 的手续费和事件。名称导入工具由 [legacy-snapshot.spec.ts](../dmsg_app/e2e/legacy-snapshot.spec.ts) 覆盖，需要 `DMSG_LEGACY_RELEASES` 指向已核对的旧发布工件。

2026-10-06 在加入治理与收入提取（schema 8）的版本上完整运行 `scripts/test-dmsg.sh` 通过，包括 Rust 单元与文档测试、Clippy `-D warnings`、Wasm/Candid 比对、跨语言协议向量、105 项 PocketIC 回归（`control_plane` 101 项、`directory` 4 项）和 26 项 SDK 测试；`dmsg_app` 的类型检查与 171 项单元测试也通过。默认忽略的 16 项 profile 本轮未运行，三项 handle profile 最近一次运行为 2026-10-02（schema 7）。部署流程中的安装（含 `governance`）、空快照、raw 提交、`validate_*` 预演、手续费更新和“停止 → 升级 → 启动”命令，已用 release Wasm 在隔离的 dfx 0.32.0 本地副本上实测；本地没有账本，`admin_collect_token` 只核对了命令格式，转账行为由 PocketIC 回归覆盖。主网部署、SNS 提案流程和名称导入工具的 e2e 未运行。
