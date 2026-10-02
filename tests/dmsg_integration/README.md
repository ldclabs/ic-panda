# dmsg_integration

基于 PocketIC 16.0.0 的真实 Wasm 联调，覆盖四个 dMsg canister 和故障注入账本。测试只使用生成的测试身份、固定测试 seed、临时本地实例，不调用生产服务。

```sh
make build-dmsg
cargo build --release --target wasm32-unknown-unknown -p dmsg_test_ledger
POCKET_IC_BIN=/path/to/pocket-ic cargo test -p dmsg_integration --features pocketic-tests --test control_plane -- --test-threads=1
```

可设置 DMSG_WASM_DIR 指向 Wasm 目录，默认读取 target/wasm32-unknown-unknown/release。完整一致性检查使用 `bash scripts/test-dmsg.sh`。

覆盖：设备/恢复授权、根 CAS、标准 COSE 的 Ed25519/ES256K、Xid 原子发号/重试/升级、认证执行回执与签名绑定、vetKD、独立认证树检查、名称认领/转移/收费、入金/结算/退款、丢失响应、BadFee、迟到资金与升级恢复。测试通过公开 AccountInfo/EscrowInfo 查询，不读取 canister 内部状态。

这些测试不代替真实账本部署、扩展 UI、私有 Worker、TSA 信任验证和容量验收。

`user_execution_retention_survives_a_full_window_and_upgrade` 覆盖 user 执行窗口满额、拒绝无写入、到期清理、旧批准防重放及升级后的认证回执。另有显式运行的 `user_cycles_profile`，以 4 KiB 正文和 0/4/8 条已完成签名历史比较 user canister 的 cycles：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane user_cycles_profile -- --ignored --nocapture
```

余额差只包含 user canister，不包含 COSE 和管理 canister 的阈值调用费用。两次比较必须使用相同的依赖、构建配置与 PocketIC 版本。

`control_plane/handle.rs` 覆盖名称注册锁、并发扣款、快照导入原子性、事件日志和认证查询的升级恢复。另有显式运行的 `handle_cycles_profile`，比较注册历史为 0/8/16 时的预留、额度拒绝、扣款和重试费用，并记录稳定内存与升级费用：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane handle_cycles_profile -- --ignored --nocapture
```

该余额差只统计 handle canister；比较时固定其他 canister 的 Wasm。样本结果及取舍见 [dmsg_handle README](../../src/dmsg_handle/README.md)。


## Commerce fixtures

The suite also builds `membership`, `dmsg_commerce` and `dmsg_test_sns`. The mock SNS has the pinned governance record shape and can vary principal permissions, net stake and dissolve state. It is never a production admission bypass.

To export certified artifacts from actual PocketIC Wasm executions:

```sh
DMSG_COMMERCE_FIXTURE_DIR=/tmp/dmsg-commerce-fixtures cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane commerce:: -- --test-threads=1
```

`cash-active.cbor` is canonical CBOR `(1, "dmsg-commerce/1", now_ms, root_key_DER, user_canister, commerce_canister, account_id, order_batch, entitlement_batch, catalog_batch)`. `sns-active.cbor` is `(1, "membership/1", now_ms, root_key_DER, membership_canister, account_id, claim_batch, policy_batch)`. These certificates contain the real local root and witnesses and are fixed samples, not fresh production authority. Do not extend their resource leases based on the time they are imported. Pair them with the exact source/Wasm SHA-256 snapshot and generated Candid.

`control_plane/user.rs` 增加未记录终态的 COSE 拒绝/序号补齐、模拟丢失成功回调后的跨月计费、名称授权续期、政策变更保留内容根，以及商业回调前并发冻结的回归。`user_upgrade_profile` 是显式运行的小样本升级/稳定内存基准，涵盖 1/16/64 个账户和最多 12 个已刷新月份；不代表生产容量验收。


The PANDA cases in `control_plane/commerce.rs` cover fresh post-cooling approval, no early exit, one-minute observation reuse, cross-product exclusivity, contiguous terms, lost Apply ACK, a module pin changed during the SNS read, `canister_info` module/controller verification and claim capacity recovered by cancellation. The SNS test double is fault-injection only and never a production admission path.

`control_plane/membership_review.rs` adds temporary product-reservation failures, per-actor retry limits, recovery during admission pause, reentrant reservation/cancellation, the post-cooling observation boundary, concurrent SNS verification, policy retry/version identity and terminal compaction across upgrades. Run it with `cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane membership_review:: -- --test-threads=1` after building the current Wasm. The separate native `membership` history profile measures 1,000/10,000 records and reader indexes; it does not establish production Wasm/cycles capacity.


`control_plane/user_review.rs` 覆盖伪造服务身份的前置拒绝、恢复完成回执的升级与幂等重试、全局绑定表满后的过期回收，以及独立执行清理后的未知结果保留、账务不变和认证不存在证明。第三方批准的 32 条/小时 60 次限制及 Agent 事件、controller、nonce 的前置检查在对应模块中覆盖。

混合升级样本包含 64 个账户、768 条历史月账及 512/2048 条当前认证叶。使用每个版本自身的新实例与同一份测试，不将不同 schema 的旧状态原地升级：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane user_mixed_upgrade_profile -- --ignored --test-threads=1 --nocapture
```

升级样本打印 user cycles、stable bytes 和升级日志中的指令数、Wasm 线性内存分配高水位。它们不能推断生产吞吐或可升级账户总量。AppAction 与 controller 注册的同配置 user cycles 还可从以下两项测试输出比较：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane -- hosted_principal_publishes external_action_authority_signature --test-threads=1 --nocapture
```
