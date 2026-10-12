# dmsg_integration

基于 PocketIC 16.0.0 的真实 Wasm 联调，覆盖四个 dMsg canister 和故障注入账本。测试只使用生成的测试身份、固定测试 seed、临时本地实例，不调用生产服务。

```sh
make build-dmsg
cargo build --release --target wasm32-unknown-unknown -p dmsg_test_ledger
POCKET_IC_BIN=/path/to/pocket-ic cargo test -p dmsg_integration --features pocketic-tests --test control_plane -- --test-threads=1
```

可设置 DMSG_WASM_DIR 指向 Wasm 目录，默认读取 target/wasm32-unknown-unknown/release。完整一致性检查使用 `bash scripts/test-dmsg.sh`。

覆盖：设备授权与登录恢复、根 CAS 与接收者摘要、设备 Ed25519 认证与认证回执、Xid 原子发号/重试/升级、恢复 vetKD 派生与 IBE 往返、独立认证树检查、名称认领/转移/收费、入金/结算/退款、丢失响应、BadFee、迟到资金与升级恢复。测试通过公开 AccountInfo/EscrowInfo 查询，不读取 canister 内部状态。

这些测试不代替真实账本部署、客户端 UI、私有 Worker、TSA 信任验证和容量验收。

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

## 旧部署快照（显式运行）

`legacy_fixture` 在 PocketIC 中安装并升级已核对的旧发布模块（`DMSG_LEGACY_RELEASES` 指向这些 `.wasm.gz`），冻结后提供本地网关；`src/dmsg_legacy/tests/snapshot-gateway.test.ts` 启动它，在 Node 中核对冻结快照的 ingress 证明、共享频道的管理者同意与冻结归档比对，并用 `src/dmsg_legacy/scripts/legacy-name-plan.mjs` 生成名称导入计划：

```sh
DMSG_LEGACY_RELEASES=/path/to/releases pnpm --dir src/dmsg_legacy test
```

## 桌面客户端验收（W1–W8）

`tests/desktop.rs` 在同一个 PocketIC 部署（`tests/support`）和私有中继（workerd）上启动两个 `dmsg daemon`，各用一个 `--home`，配置为 `secure_storage = "file"`，不触碰系统凭据库。测试扮演 dmsg.net 登录页：从 URL fragment 读出请求，用生成的 Ed25519 登录身份签发 delegation 链，再经 daemon 的回调页提交；daemon 没有测试专用路径。

- `two_desktop_devices_share_an_account`（W1）：设备 A 建账户、写条目并同步；设备 B 用同一登录发起配对，A 批准并换根，换根前写下未同步的条目被换到新根后发出；B 解锁时同步，读到并解密 A 的条目；B 写入后 A 读到；锁定后重新登录解锁；另一个登录身份、带外部 `Origin` 的登录回调与无凭证或带 `Origin` 的 `/daemon/status` 都被拒绝。
- `desktop_content_and_sync`（W2）：两台设备并发编辑同一条目，两边都得到同一冲突副本，解决后两边一致；删除后另一台的旧编辑被中继拒绝、留作本机冲突副本，墓碑保持，只有显式恢复（采用该副本）带回条目；100 MiB 文件在 A 导入、在 B 校验导出；A 的 daemon 在上传另一个 64 MiB 文件时被杀，重启并登录后续传完成，修订数与写入次数一致；公开资料只含发布的字段，可由他人读取并核对设备签名；锁定后读取、签名与同步返回 `LOCKED`，A 的 home 中找不到任何明文标记；开启“后台在线”的 B 锁定后仍能发送已封存的版本、但不能读取。合成目录把 Free 套餐的存储放大到 1 GiB，以容纳这些文件及其加密开销。`tests/support` 自行启动 PocketIC 服务器并把空闲 TTL 设为 10 分钟：经 HTTP 网关的请求不算服务器活动，默认 60 秒会在 daemon 仍在使用时停掉服务器。
- `desktop_approvals_and_attestation`（W3）：无凭证访问 `/app/v1`、`/ws/app/v1`、`/daemon/status` 一律 401；`/ws/app/v1` 用 owner 凭证初始化并收到 `approval/changed` 事件；脚本（`--json`）创建的认证请求只能等待，CLI 在非交互模式与 RPC 不带 `human` 时都拒绝批准，摘要不符被拒；没有 `FormalApprove` 时 user home 拒绝且不签名；单设备管理员为自己开启 `FormalApprove`；文件声明在 A 批准、导出为证据包，B 的 home 在 daemon 停止的情况下用 `dmsg attest verify` 独立核验（签名、认证回执、文件摘要、设备当前状态），文件改动后核验失败；请求与批准之间文件变化被拒；仅摘要的声明核验为 `digest_only`；拒绝与撤回不执行；锁定后审批列表返回 `LOCKED`，待审批数仍可见。
- `desktop_web_connect`（W4a）：注册测试应用 `token-list`（`http://localhost:5173`）后，由无头桥接页（`src/dmsg_connect/scripts/headless.ts`，Node）扮演浏览器。B 邀请、桥接页提交候选，钓鱼页用伪造邀请提交的候选因 MAC 不符被设备排除、无法授予，未经人确认不签发 grant（DT18）；锁定的 A 经提示通道收到无内容提示，开启“后台在线”的 B 只取回密封信封，解锁后才显示来源与载荷（DT26）；页面先取消则批准失败（DT23）；A 离线时创建、B 批准，A 上线后标为“由其他设备处理”（DT22），页面确认后为 `returned`；两台设备同时批准只有一台执行（DT23）；第二个浏览器提交同一操作只产生一个审批，两边收到同一结果（DT31）；桥接页丢失记录后经 `read` 从设备取回快照，未知操作为 `NOT_FOUND`（DT29）；B 在授权后崩溃，重启对账不重复签名（DT24）；撤销 B 后，第二个浏览器（由 A 配对）看到 B 钉定的请求为 `failed` + `DEVICE_REVOKED`（DT30）；B 签发的配对失效，A 签发的仍可用（DT25）。DT19、DT32（中继替换密钥、伪造回复）由 Rust 与桥接页的向量测试覆盖。
- `desktop_browser_connect`（W4b）：为本次部署构建桥接页（`src/dmsg_connect`，Local 配置内联 PocketIC 网关、根公钥与中继）和测试应用页（`src/dmsg_connect/e2e/app`），由 `scripts/serve.mjs` 按 `.ic-assets.json5` 的响应头与路径别名提供；`scripts/browser.ts` 经 Playwright 在 `DMSG_TEST_BROWSERS`（逗号分隔，默认 `chromium`）中逐个运行：核对 connect-v1 §2.2 的响应头；B 邀请后在浏览器打开配对页，设备看到的标签与浏览器一致，人确认后配对生效；未登记的 origin 握手被拒（DT20）；应用页在点击中打开桥接窗口、完成握手，签署文本声明，桥接窗口给出 `dmsg://connect/request` 链接，Safari（WebKit）先要求在窗口内点击“继续”（§4.1）；B 批准后页面取得结果并确认为 `returned`；关闭桥接窗口后页面调用返回 `DISCONNECTED`（DT27）。DT20 的其余情形（错误 source、opener 关闭或替换）与 DT21（篡改命令、连接与 nonce，nonce 重放与过期）由 `src/dmsg_connect/tests/page.test.ts` 覆盖。

- `desktop_channels`（W5）：A 建频道（第一纪元即生效），邀请前发出文本与 vault 文件；B 接受邀请。纪元激活提交后回复丢失（`EXECUTION_UNKNOWN`），重试按请求 ID 对账后进入纪元 2；B 只读到纪元 2 的消息，纪元 1 由中继跳过，未授权的历史无法读取；A 的新消息经提示通道（WebSocket）由 B 的 daemon 自行读取；A 把纪元 1 的密钥封装给 B 的设备后，B 以 `--backfill` 读到历史并下载、校验附件；所有权在 A、B 之间转移并转回（提议、目标设备接受、原所有者提交），计费转给 B；B 的新设备 C 配对后只经账户的同步内容拿到频道记录与纪元密钥，下一纪元起可读写，中继异步完成计费迁移期间以 `VERSION_CONFLICT` 拒绝的消息用同一消息 ID 重试；C 撤销 B 的旧设备后新纪元排除它；移除 B 后新纪元隔离 B；C 的频道状态在 daemon 重启后保留。
- `desktop_commerce`（W5）：登记应用 `dmsg`（origin `https://dmsg.net`，计划 D-19），PocketIC 转为实时后重新发布已过期的结算价格。登录身份 42 是有余额的钱包、神经元热键和旧名称 `legacy_probe` 的冻结所有者。ckUSDC 现金结账：商户开单回复丢失、账本转账回复丢失，每次重试先读服务再重发同一字节，钱包只付一次，订单 `Applied`、套餐变为 Plus；已应用订单不能取消（`FORBIDDEN`）。旧名称认领回复丢失，由认证所有者对账完成。另一个账户用同一身份的 PEM 钱包（`wallet use`）以 ckUSDT 付款。PANDA 会员由钱包的神经元担保：受理后处于冷却期（激活要等 65 分钟冷却，本测试不运行）。
- `desktop_paid_letters`（W5）：收件人设备先经审批取得 `PaymentOffer`，发布付费联系策略，策略提交后回复丢失，重试找到生效的策略而不发布第二版；发件人写信得到中继报价，同一订单再次提交返回原状态；付款经审批中心执行，托管入金的账本回复与中继受理回复先后丢失，每次重试读回已提交的结果，钱包只付一次（扣款为金额加转账手续费），托管为 `SettlementCommitted`；收件人用同步的收件箱密钥读到来信。

- `desktop_agents`（W6）：部署中装有真实的 `dmsg_directory`（`principal_origin` 为 `https://id.dmsg.test`，`delegation_service` 为 `https://agents.dmsg.test`）。A 建账户并开启 `FormalApprove`，经审批启用 principal、登记 controller（私钥写入秘密库），user home 把文档发布到 directory；托管两个 Agent ID（`host`、`claude`）。给 `claude` 的授权在本机签名后提交，第一次回复丢失（`EXECUTION_UNKNOWN`），对账原样重提同一信封后被接收；按依赖方的方式核验（经网关取 directory 的认证文档、经服务取凭证与已接收记录，SDK `verify_delegation_credential`）对 `https://dmsg.net` 为 `usable`，对其他依赖方不可用。`claude` 以自己的凭证运行 `dmsg mcp`（stdio，50 个工具）：它的凭证调不了审批、`ap/call`、能力授权与其他方法，读不了 `/daemon/status`；配置外的宿主 `permission_denied`，未知工具被拒（DT01、DT02）。`host` 经 `dmsg ap` 在测试宿主（`tests/adp`，按 SDK 规则验签、分配序号与链式哈希、校验 nonce 与 head）上开房间，`claude` 经 MCP 加入并发言；房间话题变更推进 head 后，`claude` 的下一条消息被 `held` 并带回变更（DT07）；宿主报告另一服务见过的 nonce，在界内时连接器重同步并重签一次，超界时报错而不采用（DT06）；没有能力授权时 `dmsg_vault_list` 为 `FORBIDDEN`，授权后可列出（不含秘密字段），`dmsg_attest_request` 只生成来源为 `agent:<did>` 的待审批请求，撤销授权后再次被拒；daemon 重启并登录后，暂存的草稿、读位置与 nonce 都在，提交草稿发出（DT09）；最后撤销凭证，核验为不可用，服务端状态 `revoked`。
- `desktop_mail_knowledge`（W7）：私有仓库的 `dmsg-mail` 与 `dmsg-knowledge` 以本地 workerd 在 HTTPS 上运行（公开的 localhost 测试证书 `tests/fixtures/localhost-tls`，daemon 经仅限 local 的 `agent_ca` 信任它；Agent Mail 与 Agent Knowledge 只接受 HTTPS 源），两者都是 `agent_hosts`。A 建账户并托管两个 Agent ID：`bob` 经 `dmsg ap` 在中继开邮箱；`alice` 以自己的凭证经 `dmsg mcp` 开回信邮箱，并附上回信卡片给 `bob` 写信。中继（TypeScript SDK）接受 daemon（Rust SDK）签名的卡片与封装的密文包（DT08 的跨语言部分，向量部分见 `src/dmsg_agent/tests`）。`bob` 同步后读到信、收件箱出现 `mail.received`，按未读读取并标记已读，不给地址、凭父信的回信卡片回复；`alice` 同步收到回复，线程与 `in_reply_to` 正确。`alice` 再写一封后重启 A 的 daemon，登录后 `bob` 仍能打开：连接器状态里没有密钥，`bob` 的邮件密钥从封存存储取回。Knowledge：`alice` 发布贡献，`bob` 按作者查询、按 ID 读取并检查状态为 `active`，服务未宣告排序搜索时 `knowledge_search` 报 `unsupported_search_mode`；`alice` 撤回后状态为 `retracted`。
- `desktop_mcp_events`（W7b）：中继的 hooks 主机（本地 `HOOKS_ORIGIN` 为 `https://hooks.dmsg.local`）经一个把 `Host` 改为它的代理对外；测试里的上游服务器替身按 MCP Events 草案与 Standard Webhooks 投递（`v1` 用订阅方密钥，`v1a` 用自己的 Ed25519 钥匙，并在回环 HTTP 上发布 `/.well-known/mcp-webhook-jwks.json`）。没有 `events.endpoint` 能力的 Agent ID 建不了端点（A8）；`anda` 获授权后经 `dmsg mcp` 看到 `events/list` 的 `dmsg.relay.delivery`（poll 与 push，A1），建端点得到回调 URL 与 `whsec_` 密钥并带 `context`（A2）。中继直接回显 `verification` 挑战（DT14）；`dmsg_events_bind` 登记上游服务器、订阅与很近的 `refreshBefore` 后，`events/stream` 先发 `active`，再推送验证记录、带端到端签名的事件（`verified` 为 `{v1: true, v1a: true}`，带回 `context` 与订阅 ID）和 `refresh_due`。伪造签名、过期时间戳 401，超大正文 413，重复的 `webhook-id` 照常 200 但只投递一次（DT12）；`v1` 正确、`v1a` 不符的投递（只有持有 `v1` 密钥的中继能造出）被 daemon 丢弃并计入 `dmsg events status`（DT16）。取消流后再投递，从上次 cursor 新开的流不丢不重，安静一轮后有心跳（DT17）；`events/poll` 读到同样的记录，`dmsg_events_ack` 后只剩之后的。daemon 停机期间的投递留在中继，重启登录后新的 MCP 会话不显式指定时读不到别的会话的端点，按 `context.session` 指定时从 cursor 继续读到（A3、A5）。轮换后新旧密钥都可投递（A6）；删除端点后上游得到 410，读者收到最后一条 `terminated`（DT13）。中继的保留期丢弃与 `relay_gap`（DT15）由 dmsg-core 的 workerd 测试覆盖。
- `desktop_release`（W8）：测试在回环 HTTP 上按 GitHub 的布局发布“release”（`/latest` 给出 tag，`/download/<tag>/<asset>` 与 `.sha256`），资产是被测的 `dmsg` 本身，经 `DMSG_RELEASE_URL` 指过去。`dmsg install --dir` 首次 `installed`、再次 `current`，之后用装好的副本。最新 release 是 canister 的 `v*` tag 时 `update --check` 报 `NOT_FOUND`；同版本不算更新；`.sha256` 不符时 `update --force` 报 `INTEGRITY_FAILED`，二进制与 daemon 实例都不变。DT10：一个等人决定的请求、一条新条目，再让一次认证的提交回复丢失（`EXECUTION_UNKNOWN`、`unknown`）并锁定（锁定时不能在后台对账），`update --force --wait 3` 等满 3 秒后报 `BUSY`，二进制与实例不变；登录、对账为 `done` 后更新停掉 daemon、换掉二进制、启动新 daemon 并核对版本，新 daemon 已锁定、仍有 1 个待决定的请求，登录后条目仍在。`restart --wait 0` 遇到未对账的请求报 `BUSY`，`--interrupt` 照样重启，登录后对账为 `done`。`dmsg stop` 在 daemon 退出后返回，`status --json` 退出码 3。

W5 用例在 daemon 与 IC 网关、中继之间各放一个原始 TCP 代理（`Proxy`），`lose_next(method, path)` 让下一个匹配请求照常转发、但在回复前断开连接，以制造“已执行、回复丢失”。中继的 `API_ORIGIN` 设为代理的 origin，因为请求证明的 audience 必须等于 `url.origin`。`DMSG_TEST_LOGS=1` 显示 daemon 的 stderr。

```sh
make build-dmsg
cargo build -p dmsg_cli
POCKET_IC_BIN=/path/to/pocket-ic DMSG_CLOUD_DIR=/path/to/dmsg-cloud DMSG_WASM_DIR=$PWD/target/wasm32-unknown-unknown/release \
  cargo test -p dmsg_integration --features pocketic-tests --test desktop -- --ignored --nocapture --test-threads=1
```

`desktop_mail_knowledge` 另由私有仓库的 `dmsg-mail/scripts/probe-server.mjs` 与 `dmsg-knowledge/scripts/probe-server.mjs` 启动两个 Worker：测试传入端口与证书、私钥的 PEM 文件，Worker 以 `https://localhost:<port>` 为自己的源，不持久化，每次运行使用新的密钥，按 `PROBE_TTL_MS` 自行退出。

`DMSG_BIN` 可指定 `dmsg` 二进制，默认 `target/debug/dmsg`；release 构建（`cargo build --release -p dmsg_cli`）加解密 100 MiB 更快。中继由私有仓库的 `dmsg-core/scripts/desktop-probe-server.mjs` 启动（W5 另传 `api_origin` 与 `paid_contacts`；W6 另传 `agent_origin`、`principal_origin` 与 `principal_gateway`：本地没有自定义域，中继经 PocketIC 网关读取 directory 的 principal 文档，daemon 的 `agent_service` 指向一个把 `Host` 改为 `agents.dmsg.test` 的代理），只允许本次分配的桥接页 origin（`connect_origin`，也是 daemon 的 `connect_url`），按启动参数 `ttl_ms`（桌面用例 20 分钟）自行退出。`desktop_web_connect` 与 `desktop_browser_connect` 还需要 Node 与 `pnpm install`（`src/dmsg_connect` 及其依赖 `dmsg-sdk` 的构建）；后者还需要 Playwright 浏览器（`pnpm --dir src/dmsg_connect exec playwright install chromium webkit firefox`），`DMSG_TEST_CHROME` 可指向已安装的 Chrome for Testing。
