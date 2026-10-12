# dmsg_integration

基于 PocketIC 16.0.0 的真实 Wasm 联调，覆盖四个 dMsg canister 和故障注入账本。测试只使用生成的测试身份、固定测试 seed、临时本地实例，不调用生产服务。

```sh
make build-dmsg
cargo build --release --target wasm32-unknown-unknown -p dmsg_test_ledger
POCKET_IC_BIN=/path/to/pocket-ic cargo test -p dmsg_integration --features pocketic-tests --test control_plane -- --test-threads=1
```

可设置 DMSG_WASM_DIR 指向 Wasm 目录，默认读取 target/wasm32-unknown-unknown/release。完整一致性检查使用 `bash scripts/test-dmsg.sh`。

覆盖：设备授权与登录恢复、根 CAS 与接收者摘要、设备 Ed25519 认证与认证回执、Xid 原子发号/重试/升级、恢复 vetKD 派生与 IBE 往返、独立认证树检查、名称认领/转移/收费、入金/结算/退款、丢失响应、BadFee、迟到资金与升级恢复。测试通过公开 AccountInfo/EscrowInfo 查询，不读取 canister 内部状态。

这些测试不代替真实账本部署、扩展 UI、私有 Worker、TSA 信任验证和容量验收。

`user_execution_retention_survives_a_full_window_and_upgrade` 覆盖 user 执行窗口满额、拒绝无写入、到期清理、旧批准防重放及升级后的认证回执。另有显式运行的 `user_cycles_profile`，以 4 KiB 正文和 0/4/8 条已完成签名历史比较 user canister 的 cycles：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane user_cycles_profile -- --ignored --nocapture
```

余额差只包含 user canister，不包含 COSE 和管理 canister 的阈值调用费用。两次比较必须使用相同的依赖、构建配置与 PocketIC 版本。

`control_plane/handle.rs` 覆盖名称注册锁、并发扣款、快照导入原子性与批次顺序、事件日志和认证查询的升级恢复，以及 governance 管理调用、`validate_*` 预演和收入提取。另有显式运行的 `handle_cycles_profile`，比较注册历史为 0/8/16 时的预留、额度拒绝、扣款和重试费用，并记录稳定内存与升级费用：

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


The PANDA cases in `control_plane/commerce.rs` cover fresh post-cooling approval, no early exit, one-minute observation reuse, cross-product exclusivity, contiguous terms, lost Apply ACK, an actor that is only a hotkey beside the neuron owner, and claim capacity recovered by cancellation. The SNS test double is fault-injection only and never a production admission path.

`control_plane/membership_review.rs` adds temporary product-reservation failures, per-actor retry limits, recovery during admission pause, reentrant reservation/cancellation, the post-cooling observation boundary, a neuron read returning after its SNS verification expired, concurrent SNS verification, policy retry/version identity and terminal compaction across upgrades. Run it with `cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane membership_review:: -- --test-threads=1` after building the current Wasm. The separate native `membership` history profile measures 1,000/10,000 records and reader indexes. The ignored `membership_capacity_profile` loads 100,000 and 1,000,000 `Active` claims built on the host by membership's `capacity_image` and measures the upgrade, refreshes and sweeping; see the [membership README](../../src/membership/README.md#验证).


`control_plane/user_review.rs` 覆盖伪造服务身份的前置拒绝、恢复完成回执的升级与幂等重试、全局绑定表满后的过期回收，以及独立执行清理后的未知结果保留、账务不变和认证不存在证明。第三方批准的 32 条/小时 60 次限制及 Agent 事件、controller、nonce 的前置检查在对应模块中覆盖。

混合升级样本包含 64 个账户、768 条历史月账及 512/2048 条当前认证叶。使用每个版本自身的新实例与同一份测试，不将不同 schema 的旧状态原地升级：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane user_mixed_upgrade_profile -- --ignored --test-threads=1 --nocapture
```

升级样本打印 user cycles、stable bytes 和升级日志中的指令数、Wasm 线性内存分配高水位。它们不能推断生产吞吐或可升级账户总量。AppAction 与 controller 注册的同配置 user cycles 还可从以下两项测试输出比较：

```sh
DMSG_WASM_DIR=/path/to/wasm cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane -- hosted_principal_publishes external_action_authority_signature --test-threads=1 --nocapture
```

## 桌面客户端验收（W1–W4a）

`tests/desktop.rs` 在同一个 PocketIC 部署（与扩展的 `account_fixture` 共用 `tests/support`）和私有中继（workerd）上启动两个 `dmsg daemon`，各用一个 `--home`，配置为 `secure_storage = "file"`，不触碰系统凭据库。测试扮演 dmsg.net 登录页：从 URL fragment 读出请求，用生成的 Ed25519 登录身份签发 delegation 链，再经 daemon 的回调页提交；daemon 没有测试专用路径。

- `two_desktop_devices_share_an_account`（W1）：设备 A 建账户、写条目并同步；设备 B 用同一登录发起配对，A 批准并换根，换根前写下未同步的条目被换到新根后发出；B 解锁时同步，读到并解密 A 的条目；B 写入后 A 读到；锁定后重新登录解锁；另一个登录身份、带外部 `Origin` 的登录回调与无凭证或带 `Origin` 的 `/daemon/status` 都被拒绝。
- `desktop_content_and_sync`（W2）：两台设备并发编辑同一条目，两边都得到同一冲突副本，解决后两边一致；删除后另一台的旧编辑被中继拒绝、留作本机冲突副本，墓碑保持，只有显式恢复（采用该副本）带回条目；100 MiB 文件在 A 导入、在 B 校验导出；A 的 daemon 在上传另一个 64 MiB 文件时被杀，重启并登录后续传完成，修订数与写入次数一致；公开资料只含发布的字段，可由他人读取并核对设备签名；锁定后读取、签名与同步返回 `LOCKED`，A 的 home 中找不到任何明文标记；开启“后台在线”的 B 锁定后仍能发送已封存的版本、但不能读取。合成目录把 Free 套餐的存储放大到 1 GiB，以容纳这些文件及其加密开销。`tests/support` 自行启动 PocketIC 服务器并把空闲 TTL 设为 10 分钟：经 HTTP 网关的请求不算服务器活动，默认 60 秒会在 daemon 仍在使用时停掉服务器。
- `desktop_approvals_and_attestation`（W3）：无凭证访问 `/app/v1`、`/ws/app/v1`、`/daemon/status` 一律 401；`/ws/app/v1` 用 owner 凭证初始化并收到 `approval/changed` 事件；脚本（`--json`）创建的认证请求只能等待，CLI 在非交互模式与 RPC 不带 `human` 时都拒绝批准，摘要不符被拒；没有 `FormalApprove` 时 user home 拒绝且不签名；单设备管理员为自己开启 `FormalApprove`；文件声明在 A 批准、导出为证据包，B 的 home 在 daemon 停止的情况下用 `dmsg attest verify` 独立核验（签名、认证回执、文件摘要、设备当前状态），文件改动后核验失败；请求与批准之间文件变化被拒；仅摘要的声明核验为 `digest_only`；拒绝与撤回不执行；锁定后审批列表返回 `LOCKED`，待审批数仍可见。
- `desktop_web_connect`（W4a）：注册测试应用 `token-list`（`http://localhost:5173`）后，由无头桥接页（`src/dmsg_connect/scripts/headless.ts`，Node）扮演浏览器。B 邀请、桥接页提交候选，钓鱼页用伪造邀请提交的候选因 MAC 不符被设备排除、无法授予，未经人确认不签发 grant（DT18）；锁定的 A 经提示通道收到无内容提示，开启“后台在线”的 B 只取回密封信封，解锁后才显示来源与载荷（DT26）；页面先取消则批准失败（DT23）；A 离线时创建、B 批准，A 上线后标为“由其他设备处理”（DT22），页面确认后为 `returned`；两台设备同时批准只有一台执行（DT23）；第二个浏览器提交同一操作只产生一个审批，两边收到同一结果（DT31）；桥接页丢失记录后经 `read` 从设备取回快照，未知操作为 `NOT_FOUND`（DT29）；B 在授权后崩溃，重启对账不重复签名（DT24）；撤销 B 后，第二个浏览器（由 A 配对）看到 B 钉定的请求为 `failed` + `DEVICE_REVOKED`（DT30）；B 签发的配对失效，A 签发的仍可用（DT25）。DT19、DT32（中继替换密钥、伪造回复）由 Rust 与桥接页的向量测试覆盖。

```sh
make build-dmsg
cargo build -p dmsg_cli
POCKET_IC_BIN=/path/to/pocket-ic DMSG_CLOUD_DIR=/path/to/dmsg-cloud DMSG_WASM_DIR=$PWD/target/wasm32-unknown-unknown/release \
  cargo test -p dmsg_integration --features pocketic-tests --test desktop -- --ignored --nocapture --test-threads=1
```

`DMSG_BIN` 可指定 `dmsg` 二进制，默认 `target/debug/dmsg`；release 构建（`cargo build --release -p dmsg_cli`）加解密 100 MiB 更快。中继由私有仓库的 `dmsg-core/scripts/extension-probe-server.mjs` 以 `client: "desktop"` 启动，不允许任何浏览器 origin，按启动参数 `ttl_ms`（桌面用例 20 分钟）自行退出。`desktop_web_connect` 还需要 Node 与 `pnpm install`（`src/dmsg_connect` 及其依赖 `dmsg-sdk` 的构建）。
