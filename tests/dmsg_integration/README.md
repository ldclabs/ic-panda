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
