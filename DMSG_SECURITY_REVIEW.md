# dMsg 源码安全审查与修复复核

## 2026-11-08 修复记录

审查基线为 `faa3e06e8e963938a247d7823ad7638095bc6633`；修复版本为本报告所在的 Git 提交。下方原始审查的严重性、证据行和“未运行测试”描述属于修复前基线，当前验证结论以本节为准。原有 `DMSG_SECURITY_AUDIT_REPORT.md` 保持不变。原始插件扫描的封存产物没有改写。

结论：两项发现均已完成代码修复及定向运行验证（`fixed`）；原问题在基线中复现，在修复后的相同回归中不再复现。

### 实现

- **Linux WebDAV 凭据**：Linux 与 macOS 一样生成独立随机 Basic 密码；GIO URL 只带公开用户名，密码经私有 stdin 交付，不进入 argv、环境变量或挂载状态。URL token 单独不能读取、写入或查询文件元数据，挂载与卸载都不再暴露完整授权。保留 GIO 正常读写、锁定撤销与卸载流程，修正 token 仅在 daemon 内存中的旧文档描述。
- **充值未知结果**：`TopUp.unsent` 在第一次网络发送前持久清除；旧作业缺字段时保守视为已发送。未知后的任何拒绝保留原参数与作业，不生成替代付款。发送、通知和对账按作业互斥，await 后仍重读和核对状态。首次明确拒绝仍可正常结束。
- **充值恢复**：增加仅非 Viewer 成员可调用的 `reconcile_top_up(id, block)`；`id=0` 为自动充值，其他 ID 为已批准的 `Refuel` 提案。只从固定账本读取真实区块，核对付款账户/子账户、CMC/目标子账户、金额、memo、时间和无 spender，读取后重查权限、配置与原作业。补记区块后由原 `refuel()` / `reconcile(id)` 继续通知，不再次付款。原请求使用默认手续费，不以当前手续费替代历史付款证明。
- **CMC 未确认结果**：`InvalidTransaction` 和 `TransactionTooOld` 不再被当作退款；仅明确 `Refunded` 能清理为退款终态。这也使共享适配器的 hub 收入归集保留未确认付款。该语义已对照 [CMC 公开合同](https://github.com/dfinity/ic/blob/master/rs/nns/cmc/cmc.did)；并未据此声称线上模块已经升级。

Linux 密码交付方式核对了 [GIO 的 stdin 认证处理](https://github.com/GNOME/glib/blob/main/gio/gio-tool-mount.c) 与 [GVFS 的 WebDAV 认证](https://github.com/GNOME/gvfs/blob/master/daemon/gvfsbackenddav.c)。私有设计仅在本地核对，未复制正文或路径。

### 验证记录

验证环境：macOS arm64、Rust 1.98.1；PocketIC server 16.0.0（Rust 客户端锁定为 16.1.0）；隔离的 Debian bookworm arm64 容器、Rust 1.98.1、GIO 2.74.6 / GVFS 1.50.3。全部账户、文件和资金均为本地合成测试数据。

| 验证 | 结果 |
| --- | --- |
| 修复前钱包对照 | 自动充值、提案充值两个测试均复现 `InvalidInput(TooOld)`，而预期应保留 `ExecutionUnknown`；窗口内正常去重通过 |
| 修复前 Linux 对照 | 真实 `gio mount` 成功后，UID 65534 仅凭 URL 得到 HTTP 200，回归测试按预期失败 |
| 修复后充值回归 | 8 通过，包含显式执行旧 Wasm → 新 Wasm 的未知付款升级测试 |
| 既有钱包 PocketIC 集成 | 30 通过、3 ignored；包含普通转账恢复、多签、升级、CMC 充值与 hub 收入归集 |
| Rust 包单元测试 | 122 通过：protocol 85、runtime 21、wallet 13、wallet_hub 3 |
| macOS CLI 常规测试 | 120 通过、6 ignored |
| Linux GVFS 修复后对照 | 1 通过：UID 65534 的 URL-only 读取为 401；缺失/错误密码及 token-as-password 的 GET/PUT/PROPFIND 被拒绝，真实 GIO 读写正常，锁定与卸载撤销访问 |
| Linux 既有挂载测试 | 10 通过、1 ignored；该 ignored 原生测试已在上一行显式运行通过 |
| 相关包 Clippy | `--all-targets`、PocketIC 测试 feature、`-D warnings` 通过 |
| 格式与合同 | 相关包 `cargo fmt --check`、`git diff --check` 通过；当前钱包 Wasm 的 Candid 导出与提交合同逐字节一致 |

定向回归覆盖：未知付款跨去重窗口与升级后仍保留原参数；反复调用不再次扣款；并发发送/对账返回 `Pending`；真实区块恢复成功；错误 sender、付款子账户、收款人、CMC 子账户、金额、memo、时间以及上一笔自动充值的证明被拒绝；非成员不能对账；首次明确失败和窗口内 Duplicate 保持正常行为；CMC 未退款错误不删除已付款作业。

主要命令（从仓库根运行）：

```sh
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_wallet -p dmsg_wallet_hub -p dmsg_test_cmc
cargo test --locked -p dmsg_protocol -p dmsg_runtime -p dmsg_wallet -p dmsg_wallet_hub --lib
cargo test --locked -p dmsg_cli --bin dmsg
DMSG_PRE_TOP_UP_WALLET_WASM="$PWD/target/wasm32-unknown-unknown/release/dmsg_wallet_before_top_up.wasm" cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane top_up_ -- --include-ignored --test-threads=2
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane wallet:: -- --skip top_up_ --test-threads=4
cargo clippy --locked -p dmsg_cli -p dmsg_wallet -p dmsg_wallet_hub -p dmsg_runtime -p dmsg_integration -p dmsg_test_cmc --all-targets --features dmsg_integration/pocketic-tests -- -D warnings
cargo fmt --check -p dmsg_cli -p dmsg_wallet -p dmsg_runtime -p dmsg_integration -p dmsg_test_cmc
```

PocketIC 所需其他 canister / ledger / SNS fixture 的 Wasm 先由同一锁文件构建。`dmsg_wallet_before_top_up.wasm` 是修改前在审查基线构建并保存的模块，不是当前模块的复制品。其 SHA-256 为 `3e8176a5b615f4481eaf796e89c756567a88fd47333226333f49ca65db3ffa19`；本次修复钱包 Wasm 为 `dc769de82a38a8329736e0c4b4e6649aaa50df228f31de5bdafecab9c44d2044`。

Linux 对照在容器内从基线源码加同一回归测试构建，再替换修复源码并强制更新 mtime 重建，避免复制保留旧时间戳导致 Cargo 复用旧二进制。命令为：

```sh
dbus-run-session -- cargo --config profile.test.package.dmsg_cli.opt-level=0 test --offline --locked -p dmsg_cli native_linux_mount_requires_a_private_password -- --ignored --nocapture --test-threads=1
```

Linux 首次默认 `opt-level=1` 编译因耗时主动中断，未计为通过；实际对照只将 CLI 包的测试优化级别设为 0，保留 debug assertions，未修改仓库构建配置。测试编写中修正过遗漏 `Memo` 导入、错误假定账本首块非零以及旧模块环境变量相对路径；以上表格仅计最终运行结果。

### 复核范围与剩余限制

当前 agent 按项目要求独立完成边界调查与修改后反证复核，未使用 subagents。复核了所有 `run_top_up` 和共享 CMC 适配器调用方、旧稳定存储布局、默认/正常输入及并发等待后的状态。相关实现位于 [Linux 挂载](src/dmsg_cli/src/mount.rs)、[充值执行与对账](src/dmsg_wallet/src/execute.rs)、[充值状态与互斥](src/dmsg_wallet/src/ledger.rs)、[CMC 适配器](src/dmsg_runtime/src/ledger.rs)；回归位于 [挂载测试](src/dmsg_cli/src/mount_tests.rs)、[钱包回归](tests/dmsg_integration/tests/control_plane/wallet_review.rs) 和 [CMC 故障替身](tests/dmsg_test_cmc/src/lib.rs)。公开中英文钱包合同、Candid、相关 README 同步更新。

未执行线上部署/真实资金测试、完整 workspace 集成、真实生产 ledger/CMC 互操作、Windows 原生验证、完整 Linux 桌面/FUSE 挂载路径验收、Electron/iOS 验收及 CLI 中显式 ignored 的平台测试。Linux 对照使用真实 GIO/GVFS，但不代表发布优化二进制或完整桌面验收。新增对账方法需要钱包升级到本提交构建的模块。本轮没有继续扩大到原始审查列出的全部未覆盖代码，也未处理普通 ICRC 未知结果后其他拒绝类型的独立复核项；CMC 误判退款项已在此次修复范围内处理。


修复与复核署名：Codex <noreply@openai.com>。

---

以下为修复前的静态审查记录；保留原始发现、限制和后续审查范围。

# Security Review: ic-panda

## Scope

2026-11-08（Asia/Shanghai），审查本地 ic-panda@faa3e06e8e963938a247d7823ad7638095bc6633 的 dMsg 源码。范围包括账户/COSE/名称/目录/支付/商业/会员/团队钱包 canisters，客户端、daemon/CLI/MCP、桌面与 iOS/移动安全入口、connect/SDK、旧版迁移及发布配置。按项目约束由当前 agent 独立顺序执行；这是安全边界优先的广泛定向静态审查，非独立多 worker Deep Scan，亦非逐文件逐行全量证明。

- Scan mode: repository
- Target kind: git_revision
- Target ID: target_sha256_53e196e4f3953bbee338ec0a461c3704f42989990ed3878f930e8585cac4a04e
- Revision: faa3e06e8e963938a247d7823ad7638095bc6633
- Inventory strategy: repository
- Included paths: .
- Excluded paths: none
- Artifacts reviewed: AGENTS.md, Cargo.toml, Cargo.lock, docs/protocol/agent_zh.md, docs/protocol/wallet_zh.md, docs/dMsg_wallet_zh.md, docs/dmsg_architecture_zh.md, DMSG_SECURITY_AUDIT_REPORT.md, src/dmsg_cli/README.md, src/dmsg_agent/README.md, src/dmsg_wallet/README.md, .github/workflows/dmsg-release.yml, .github/workflows/test.yml, tests/dmsg_integration/tests/control_plane/wallet_review.rs, tests/dmsg_test_ledger/src/lib.rs
- Scan context: 初始及最终源码基线检查均为干净工作区；根目录已有 DMSG_SECURITY_AUDIT_REPORT.md 记录此前四项修复，本轮保留原件并另存 DMSG_SECURITY_REVIEW.md。历史测试记录没有计入本轮验证。私有 PRD/实施/商业/Agent 基线的本地版本和日期已核对，未核对远端，未复制私有正文、Worker 实现或凭据。

Limitations and exclusions:
- 本轮源码覆盖为 partial：重点追踪安全入口、共享控制和敏感消费者；没有逐行审完所有实现、生成桥接、所有 UI/本地化、稳定存储迁移或第三方 SDK/密码库。
- 两项报告发现均经过源码验证，部署条件和运行可靠性仍需定向复现；未把静态检查表述为运行测试通过。
- 私有 Worker、线上模块/控制者/系统 ACL、实际发布包与签名身份不在本轮公开源码证据内；生产已上线为用户提供背景，未实测部署一致性。
- 工作区原有历史审查报告保持不变；源码不做修复。未使用 subagents，未运行独立基线复核。
- 离线源审查未进行在线依赖漏洞库查询；没有把锁文件审查写成依赖 CVE 扫描通过。
- Excluded archived/\*\*: 历史参考资料，不用它代替当前 dMsg 实现。
- Excluded src/ic_panda_luckypool/\*\*: 非本轮 dMsg 产品实现。
- Excluded src/cli_airdrop/\*\*: 独立空投工具，不属于 dMsg 请求范围。
- Excluded node_modules/\*\*; target/\*\*; dist/\*\*; release/\*\*: 未审计构建缓存、第三方依赖全量源码或实际发布二进制；只按锁定版本读取与候选反证相关的本地依赖代码。

### Scan Summary

| Field | Value |
| --- | --- |
| Scan outcome | completed |
| Reportable findings | 2 |
| Severity mix | low: 2 |
| Confidence mix | medium: 2 |
| Coverage | partial |
| Validation mode | 当前源码数据流/授权/异步状态机/异常恢复追踪，逐个候选反证复核，核对精确证据行与锁定依赖的相关解析防护。 |

Canonical artifacts: `scan-manifest.json`, `findings.json`, and `coverage.json`. This report is a deterministic projection of those files.

## Threat Model

dMsg 在 Rust 客户端中管理加密内容与设备密钥，通过本地 daemon 向 Electron、CLI 和受限 Agent 提供能力，并与 ICP 账户/名称/支付/商业/钱包 canister 和云端密文中继交互。本轮针对当前公开源码进行单 agent 顺序静态审查；架构阅读与漏洞验证分别记录。

### Assets

- 账户登录绑定、设备签名与解密密钥、内容根、审批硬件密钥及恢复权限。
- 秘密库、频道与文件的明文、授权范围、完整性和持久化游标。
- 来信托管、商业订单、多签钱包资产及出金幂等状态。
- 本机文件、进程、系统凭据以及发布更新的代码执行权限。

### Trust Boundaries

- 登录 Principal → user home：src/dmsg_user/src/account.rs:106 的 check_device 验证绑定、当前设备、能力、epoch、sequence、期限、签名和硬件证明；src/dmsg_user/src/api.rs:350 的 mutate_account 在外部等待后重读账户。
- 恢复登录 → 新管理员设备：src/dmsg_user/src/recovery.rs:12 验证恢复策略、登录绑定和新设备持有证明，complete_recovery 在延迟窗口内替换设备；撤销不能追回已复制的密钥。
- 应用批准 → 第三方消费者：src/dmsg_user/src/external.rs:394 的 approve_application 和 :481 的 verify_application 按配置消费精确 application，返回 valid_until_ms；消费者必须保留绑定与新鲜度。
- 本机调用方 → daemon：src/dmsg_cli/src/daemon.rs:383 只绑定 127.0.0.1；:1921 拒绝所有带 Origin 的 bearer 请求；:1997 将 owner 与 Agent 方法分离；:2126 的 WebSocket 只接受 owner。
- Electron renderer → 主进程 → daemon：src/dmsg_desktop/src/main/policy.ts:494 过滤方法、删除 human 与任意 attest 文件路径；主进程承担系统选择器与确认，仍须逐个检查参数与敏感入口。
- 客户端 → IC/云端/外链：src/dmsg_client/src/config.rs:167 校验 HTTPS、生产根和固定 login/connect 地址；配置来自本机而非请求；src/dmsg_client/src/hold.rs:30 把同步 Client 访问与网络分开，await 返回后须重新核对会话和状态。
- 钱包成员/公开维护调用 → ICP/外链资产：src/dmsg_wallet/src/api.rs:1388 重查提案与成员并消费设备批准；execute.rs:269 起根据已批准意图执行，:974 对计划/协签和 nonce/输入替换约束签名；:1238 的自动 refuel 对非成员开放，须独立保持未决付款语义。
- 本机不同 UID → Files WebDAV：src/dmsg_cli/src/mount.rs:138 把 capability 放入 loopback URL，:272 的 Linux 挂载将其放入 gio argv；:366 仅 macOS 另设 Basic 密码。库只在解锁账户内开放，token 泄露不能被 0700 工作区权限弥补。
- 公开内容/导入文件 → 解密和本地存储：src/dmsg_client/src/codec.rs:101 先扫描深度、节点与长度；root.rs:534 校验已认证根承诺后解密；library/request.rs:183 绑定接收清单；vault/import.rs:267 检查 zip 路径、成员类型与解压大小。
- OS 系统扩展 → 用户显式导出的快照：src/dmsg_ios/Shared/SystemExports.swift 管理独立 App Group、限时快照与在场保护的 key；不向扩展交付整个账户内容根。

### Attacker Capabilities

- 匿名或自建身份可调用公开 IC 方法、浏览器协议及公共内容入口，但没有被害者设备私钥。
- 受限 Agent 持有自己的 MCP 凭据与用户授予的 scopes，不能等同 owner。
- 恶意频道成员、共享文件/导入数据提供者、受绑定远端服务可控制其输入或响应。
- 同机不同权限进程与被污染 renderer 是条件攻击者；不假定攻击者已控制 owner token、治理身份或用户 OS 账户。

### Security Objectives

- 授权必须绑定账户、设备、资源、用途、请求摘要和截止，异步返回后不使用失效授权或错账户状态。
- 解密与签名仅对授权对象执行；列表、搜索、预览和错误不能泄露被拒绝的正文。
- 账本未知结果保持固定参数及不可变决定，不重复交付或支付。
- 外部内容不能越过主进程文件、进程执行与更新发布边界；网络和解码需合理有界。

### Assumptions

- 审查提交 faa3e06e8e963938a247d7823ad7638095bc6633，开始时工作区干净；只核对本地版本，未核对远端或线上部署。
- 已读取项目要求的本地私有设计相关章节，仅作设计边界核对；公开报告不复制内部正文、实现或凭据。
- 不使用 subagents，因此 baseline 与架构复核均由同一 agent 顺序执行，不属于独立多 worker Deep Scan。
- 不假定诚实云端保证能抵抗恶意中继隐藏未锚定历史；生产配置、实际权限、原生平台和第三方依赖实现须分别标明验证范围。
- Agent connector 的 agents.sqlite 保存已读房间和信件为明文，README 明确此边界；没有将驻留 Agent 已持有数据可在锁定后保留误报为 vault 解锁绕过。
- 私有设计文件仅核对本地版本/日期和工作区；没有同步远端。报告结论以当前公开实现证据为主，不把私有目标设计或历史审查当作现网证明。
- 本轮是广泛安全边界定向审查，覆盖记录为 partial；没有把盘点到的文件或搜索命中算作逐行审查，也没有声称审完所有生成代码、本地化和第三方库。

## Findings

| Finding | Severity | Confidence | Detailed write-up |
| --- | --- | --- | --- |
| [钱包充值重试 TooOld 会丢弃已付款但未确认的作业](#finding-1) | low | medium | inline below |
| [Linux 挂载把唯一 WebDAV 凭据放入进程参数](#finding-2) | low | medium | inline below |

### Confidence Scale

| Label | Meaning |
| --- | --- |
| high | Direct evidence supports the finding with no material unresolved blocker. |
| medium | Evidence supports a plausible issue, but material runtime or reachability proof remains. |
| low | Evidence is incomplete and the item is retained only for explicit follow-up. |

<a id="finding-1"></a>

### [1] 钱包充值重试 TooOld 会丢弃已付款但未确认的作业

| Field | Value |
| --- | --- |
| Severity | low |
| Confidence | medium |
| Confidence rationale | 源码与现有账本 fixture 明确支持提交后丢响应及过期重试的组合，并能逐分支推出作业删除；本轮未运行 PocketIC，也未核对生产 ICP ledger/CMC 的现场行为。 |
| Category | Incorrect recovery from an ambiguous financial operation |
| CWE | CWE-754 |
| Affected lines | src/dmsg_wallet/src/execute.rs:603-607, src/dmsg_wallet/src/execute.rs:1236-1270, src/dmsg_runtime/src/ledger.rs:173-182, src/dmsg_wallet/src/execute.rs:635-638, src/dmsg_wallet/src/lib.rs:18-30 |

#### Summary

当充值转账已在账本提交但响应丢失，`run_top_up` 保留 `block=None`。若之后跨过账本去重窗口再重试，`TooOld` 被映射为 `Sent::Refused`，随即删除原充值作业并停止通知 CMC；自动充值的下一次调用又能用新时间创建新转账。无需钱包成员资格的已认证调用方可触发这一错误恢复路径，使原付款脱离自动对账并可能产生额外充值支出。

#### Root Cause

`TopUp` 仅记录可选 block，没有记录某次付款已发出且结果未定。`ledger::send` 的 Refused 被当作整个充值从未发生，而它只表示这次重试未成功。原款已提交且 block 响应丢失时，后来的 TooOld 仍删除原作业；自动 refuel 因看不到作业再生成新 created_at_time，原付款不能沿该作业继续查账和通知 CMC。

**非成员也可调用 refuel** — `src/dmsg_wallet/src/lib.rs:18-30`

入口明确将 refuel 列为开放方法，只拒绝匿名 ingress；不要求调用方属于钱包成员。

```rust
#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let method = ic_cdk::api::msg_method_name();
    let open = matches!(
        method.as_str(),
        "join" | "unbind" | "bootstrap" | "refuel" | "prune"
    );
    let member =
        store::with(|w| caller == w.settings.hub || w.members.iter().any(|m| m.binds(&caller)));
    if caller != Principal::anonymous() && (open || member) {
        ic_cdk::api::accept_message();
    }
```

**有作业时重试，无作业时建立新付款** — `src/dmsg_wallet/src/execute.rs:1236-1261`

已存在作业直接进入 run_top_up；一旦它被删除，下一次调用可在既有政策内用新的 created_at_time 创建充值。

```rust
/// Top up cycles under the policy's refuel rule; anyone may call it.
#[ic_cdk::update]
async fn refuel() -> Result<Option<u128>> {
    let me = ic_cdk::api::canister_self();
    if store::job(REFUEL_JOB).is_none() {
        let at = now();
        let mut w = store::wallet();
        let Some(rule) = w.policy.refuel.clone() else {
            return Ok(None);
        };
        if ic_cdk::api::canister_cycle_balance() >= rule.min_cycles {
            return Ok(None);
        }
        w.roll_day(at);
        ensure(w.refuels_today < rule.max_per_day, Error::QuotaExceeded)?;
        w.refuels_today += 1;
        let job = TopUp {
            from: icp_subaccount(rule.from).map(Hash::new),
            amount: rule.icp_e8s,
            created_at_time: at * NANOS_PER_MILLISECOND,
            block: None,
        };
        store::save_job(REFUEL_JOB, &Job::TopUp(job));
        store::save(w);
    }
    match run_top_up(REFUEL_JOB, me).await? {
```

**TooOld 与先前 Unknown 被分开处理** — `src/dmsg_runtime/src/ledger.rs:172-183`

账本响应丢失归 Unknown；TooOld 等应用错误归 Refused。这个结果只描述当前尝试，不能证明更早的尝试没有转账。

```rust
/// Send one transfer; a duplicate answers with the original block.
pub async fn send(ledger: Principal, arg: TransferArg) -> Sent {
    let reply: std::result::Result<std::result::Result<Nat, TransferError>, CallFailure> =
        call_classified(ledger, "icrc1_transfer", (arg,)).await;
    match reply {
        Ok(Ok(n)) | Ok(Err(TransferError::Duplicate { duplicate_of: n })) => {
            block_index(n).map_or(Sent::Unknown, Sent::Block)
        }
        Ok(Err(TransferError::TemporarilyUnavailable)) => Sent::Unknown,
        Ok(Err(e)) => Sent::Refused(format!("{e:?}")),
        Err(_) => Sent::Unknown,
    }
```

**重试拒绝无条件删除充值作业** — `src/dmsg_wallet/src/execute.rs:587-620`

充值只以 block 是否已知决定是否发送；没有保存先前可能已执行的标记，任何 Refused 都删除作业并跳过后续 CMC 通知。

```rust
async fn run_top_up(key: u64, me: Principal) -> Result<Option<TopUpEnd>> {
    let (cmc, ledger) = store::with(|w| (w.settings.cmc, w.settings.icp_ledger));
    let Some(Job::TopUp(mut job)) = store::job(key) else {
        return Err(Error::NotFound);
    };
    if job.block.is_none() {
        let sent = ledger::send(ledger, job.arg(cmc, me)).await;
        let Some(Job::TopUp(stored)) = store::job(key) else {
            return Ok(None);
        };
        job = stored;
        match sent {
            Sent::Block(block) => {
                job.block = Some(block);
                store::save_job(key, &Job::TopUp(job.clone()));
            }
            Sent::Refused(reason) => {
                store::remove_job(key);
                return Ok(Some(TopUpEnd::Refused(reason)));
            }
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        }
    }
    let block = job.block.expect("sent");
    let end = match ledger::notify_top_up(cmc, block, me).await {
        Notified::Cycles(cycles) => TopUpEnd::Minted { block, cycles },
        Notified::Refunded(reason) => TopUpEnd::Refused(reason),
        Notified::Pending => return Err(Error::Pending),
    };
    if store::job(key).is_none() {
        return Ok(None);
    }
    store::remove_job(key);
    Ok(Some(end))
```

**提案充值也被记为确定失败** — `src/dmsg_wallet/src/execute.rs:635-638`

同一 helper 服务 Refuel 提案；Refused 会以 release=true 结束提案并清理作业，丢失本应保留的恢复状态。

```rust
        Some(TopUpEnd::Refused(reason)) => {
            fail(&mut w, &mut p, reason.clone(), true, at);
            finish(w, &p);
            return Err(Error::InvalidInput(reason));
```

**普通转账保留过期重试的不确定性** — `src/dmsg_wallet/src/execute.rs:499-519`

相邻普通 ledger 作业在已发送后的 TooOld 保留 Executing 并等待真实 block 对账，说明充值缺少相同保护。

```rust
            Sent::Refused(reason) => {
                let mut w = store::wallet();
                let mut p = store::proposal(id)?;
                let blocks: Vec<u64> = job.items.iter().filter_map(|x| x.block).collect();
                // A first attempt rejected as TooOld never moved funds. Only
                // retries can hide a payment outside the deduplication window.
                let unknown = !first_attempt && reason.starts_with("TooOld");
                if unknown {
                    p.failure = Some(format!(
                        "item {i}: outcome unknown after the deduplication window; check the ledger"
                    ));
                    p.updated_at_ms = at;
                    store::save_proposal(&p);
                    return Err(Error::ExecutionUnknown);
                }
                let reason = format!("item {i}: {reason}");
                let release = blocks.is_empty();
                p.outcome = Some(Outcome::Ledger { blocks });
                fail(&mut w, &mut p, reason.clone(), release, at);
                finish(w, &p);
                return Err(Error::InvalidInput(reason));
```

#### Validation

确认自动 refuel 与成员 Refuel 提案均调用 run_top_up；确认 Unknown 分支保留 block=None、TooOld 映射 Refused、随后 remove_job，并确认下一次 refuel 可建立新时间戳。检查 reconcile_transfer 后发现它只接受 Job::Ledger，不能给被删 TopUp 补入真实 block。现有普通转账回归只保护 Ledger 路径，不能覆盖此充值路径。

Validation method: 静态状态机、调用者检查和现有故障 fixture 源码复核

- **Status:** source_validated
- **Disposition:** reportable

**有作业时重试，无作业时建立新付款** — `src/dmsg_wallet/src/execute.rs:1236-1261`

已存在作业直接进入 run_top_up；一旦它被删除，下一次调用可在既有政策内用新的 created_at_time 创建充值。

```rust
/// Top up cycles under the policy's refuel rule; anyone may call it.
#[ic_cdk::update]
async fn refuel() -> Result<Option<u128>> {
    let me = ic_cdk::api::canister_self();
    if store::job(REFUEL_JOB).is_none() {
        let at = now();
        let mut w = store::wallet();
        let Some(rule) = w.policy.refuel.clone() else {
            return Ok(None);
        };
        if ic_cdk::api::canister_cycle_balance() >= rule.min_cycles {
            return Ok(None);
        }
        w.roll_day(at);
        ensure(w.refuels_today < rule.max_per_day, Error::QuotaExceeded)?;
        w.refuels_today += 1;
        let job = TopUp {
            from: icp_subaccount(rule.from).map(Hash::new),
            amount: rule.icp_e8s,
            created_at_time: at * NANOS_PER_MILLISECOND,
            block: None,
        };
        store::save_job(REFUEL_JOB, &Job::TopUp(job));
        store::save(w);
    }
    match run_top_up(REFUEL_JOB, me).await? {
```

**TooOld 与先前 Unknown 被分开处理** — `src/dmsg_runtime/src/ledger.rs:172-183`

账本响应丢失归 Unknown；TooOld 等应用错误归 Refused。这个结果只描述当前尝试，不能证明更早的尝试没有转账。

```rust
/// Send one transfer; a duplicate answers with the original block.
pub async fn send(ledger: Principal, arg: TransferArg) -> Sent {
    let reply: std::result::Result<std::result::Result<Nat, TransferError>, CallFailure> =
        call_classified(ledger, "icrc1_transfer", (arg,)).await;
    match reply {
        Ok(Ok(n)) | Ok(Err(TransferError::Duplicate { duplicate_of: n })) => {
            block_index(n).map_or(Sent::Unknown, Sent::Block)
        }
        Ok(Err(TransferError::TemporarilyUnavailable)) => Sent::Unknown,
        Ok(Err(e)) => Sent::Refused(format!("{e:?}")),
        Err(_) => Sent::Unknown,
    }
```

**重试拒绝无条件删除充值作业** — `src/dmsg_wallet/src/execute.rs:587-620`

充值只以 block 是否已知决定是否发送；没有保存先前可能已执行的标记，任何 Refused 都删除作业并跳过后续 CMC 通知。

```rust
async fn run_top_up(key: u64, me: Principal) -> Result<Option<TopUpEnd>> {
    let (cmc, ledger) = store::with(|w| (w.settings.cmc, w.settings.icp_ledger));
    let Some(Job::TopUp(mut job)) = store::job(key) else {
        return Err(Error::NotFound);
    };
    if job.block.is_none() {
        let sent = ledger::send(ledger, job.arg(cmc, me)).await;
        let Some(Job::TopUp(stored)) = store::job(key) else {
            return Ok(None);
        };
        job = stored;
        match sent {
            Sent::Block(block) => {
                job.block = Some(block);
                store::save_job(key, &Job::TopUp(job.clone()));
            }
            Sent::Refused(reason) => {
                store::remove_job(key);
                return Ok(Some(TopUpEnd::Refused(reason)));
            }
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        }
    }
    let block = job.block.expect("sent");
    let end = match ledger::notify_top_up(cmc, block, me).await {
        Notified::Cycles(cycles) => TopUpEnd::Minted { block, cycles },
        Notified::Refunded(reason) => TopUpEnd::Refused(reason),
        Notified::Pending => return Err(Error::Pending),
    };
    if store::job(key).is_none() {
        return Ok(None);
    }
    store::remove_job(key);
    Ok(Some(end))
```

**提案充值也被记为确定失败** — `src/dmsg_wallet/src/execute.rs:635-638`

同一 helper 服务 Refuel 提案；Refused 会以 release=true 结束提案并清理作业，丢失本应保留的恢复状态。

```rust
        Some(TopUpEnd::Refused(reason)) => {
            fail(&mut w, &mut p, reason.clone(), true, at);
            finish(w, &p);
            return Err(Error::InvalidInput(reason));
```

**普通转账保留过期重试的不确定性** — `src/dmsg_wallet/src/execute.rs:499-519`

相邻普通 ledger 作业在已发送后的 TooOld 保留 Executing 并等待真实 block 对账，说明充值缺少相同保护。

```rust
            Sent::Refused(reason) => {
                let mut w = store::wallet();
                let mut p = store::proposal(id)?;
                let blocks: Vec<u64> = job.items.iter().filter_map(|x| x.block).collect();
                // A first attempt rejected as TooOld never moved funds. Only
                // retries can hide a payment outside the deduplication window.
                let unknown = !first_attempt && reason.starts_with("TooOld");
                if unknown {
                    p.failure = Some(format!(
                        "item {i}: outcome unknown after the deduplication window; check the ledger"
                    ));
                    p.updated_at_ms = at;
                    store::save_proposal(&p);
                    return Err(Error::ExecutionUnknown);
                }
                let reason = format!("item {i}: {reason}");
                let release = blocks.is_empty();
                p.outcome = Some(Outcome::Ledger { blocks });
                fail(&mut w, &mut p, reason.clone(), release, at);
                finish(w, &p);
                return Err(Error::InvalidInput(reason));
```

**现有 fixture 可表达先提交后丢响应** — `tests/dmsg_test_ledger/src/lib.rs:285-305`

测试账本先执行转账，再跨消息提交并模拟响应丢失。这是已有源码证据，本轮没有执行该 fixture。

```rust
async fn icrc1_transfer(a: TransferArg) -> std::result::Result<Nat, TransferError> {
    let at = ic_cdk::api::time();
    let result = transfer(
        Account {
            owner: ic_cdk::api::msg_caller(),
            subaccount: a.from_subaccount,
        },
        &a,
        None,
        at,
    );
    let lose = config().lose;
    if lose && result.is_ok() {
        configure(|c| c.lose = false);
        // Commit funds in one message before losing the subsequent response.
        let _: () = stable::call(ic_cdk::api::canister_self(), "barrier", ())
            .await
            .unwrap();
        ic_cdk::trap("injected lost response after committing the transfer");
    }
    delay_response().await;
```

**现有 fixture 对过期原参数返回 TooOld** — `tests/dmsg_test_ledger/src/lib.rs:219-234`

同一 fixture 在去重检查前处理过期时间；组合前一事件即可表达当前候选的输入序列。

```rust
    at: u64,
) -> std::result::Result<Nat, TransferError> {
    // Like the ICRC-1 ledger, deduplication spans 24 hours plus the permitted drift.
    const WINDOW_NS: u64 = (24 * 3600 + 60) * 1_000_000_000;
    if a.created_at_time
        .is_some_and(|t| t.saturating_add(WINDOW_NS) < at)
    {
        return Err(TransferError::TooOld);
    }
    let key = digest("transfer", &(from, a, spender));
    if let Some(block) = DUPLICATES.with_borrow(|t| t.load(key.as_slice())) {
        return Err(TransferError::Duplicate {
            duplicate_of: block.into(),
        });
    }
    let rejects = config().rejects;
```

Assertions:
- 先前结果未知的充值与从未发送的充值没有持久状态区分。
- TooOld 后执行 remove_job，不到达 notify_top_up。
- 自动充值随后重新建单；原充值不再有该作业的恢复入口。

Counterevidence and remaining uncertainty:
- 去重窗口内收到 Duplicate 时可以正常取得原 block 并通知 CMC。
- 新充值仍检查 min_cycles/max_per_day，收款地址固定为 CMC。
- 原账本记录仍存在；能否通过外部运维手动通知/退款未作结论。

Limitations:
- 未构建或执行新增 PocketIC 回归，故不声称运行复现。
- 普通 ICRC 作业对 BadFee/InsufficientFunds 的未知重试处理仍需固定账本合同和定向实验，未扩展为已确认的支出额度绕过。

#### Dataflow

公开 `refuel` / 已批准 Refuel 提案 → `run_top_up` → 固定参数转账 → Unknown → 原参数过期重试 → Refused → remove_job → 新 refuel 作业

- **Source:** 无需钱包成员资格的 refuel 调用与先前未决充值状态

- **Sink:** 充值作业持久化状态和后续账本付款

- **Outcome:** 原付款退出自动恢复，可能增加充值支出和人工对账工作

**非成员也可调用 refuel** — `src/dmsg_wallet/src/lib.rs:18-30`

入口明确将 refuel 列为开放方法，只拒绝匿名 ingress；不要求调用方属于钱包成员。

```rust
#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let method = ic_cdk::api::msg_method_name();
    let open = matches!(
        method.as_str(),
        "join" | "unbind" | "bootstrap" | "refuel" | "prune"
    );
    let member =
        store::with(|w| caller == w.settings.hub || w.members.iter().any(|m| m.binds(&caller)));
    if caller != Principal::anonymous() && (open || member) {
        ic_cdk::api::accept_message();
    }
```

**有作业时重试，无作业时建立新付款** — `src/dmsg_wallet/src/execute.rs:1236-1261`

已存在作业直接进入 run_top_up；一旦它被删除，下一次调用可在既有政策内用新的 created_at_time 创建充值。

```rust
/// Top up cycles under the policy's refuel rule; anyone may call it.
#[ic_cdk::update]
async fn refuel() -> Result<Option<u128>> {
    let me = ic_cdk::api::canister_self();
    if store::job(REFUEL_JOB).is_none() {
        let at = now();
        let mut w = store::wallet();
        let Some(rule) = w.policy.refuel.clone() else {
            return Ok(None);
        };
        if ic_cdk::api::canister_cycle_balance() >= rule.min_cycles {
            return Ok(None);
        }
        w.roll_day(at);
        ensure(w.refuels_today < rule.max_per_day, Error::QuotaExceeded)?;
        w.refuels_today += 1;
        let job = TopUp {
            from: icp_subaccount(rule.from).map(Hash::new),
            amount: rule.icp_e8s,
            created_at_time: at * NANOS_PER_MILLISECOND,
            block: None,
        };
        store::save_job(REFUEL_JOB, &Job::TopUp(job));
        store::save(w);
    }
    match run_top_up(REFUEL_JOB, me).await? {
```

**重试拒绝无条件删除充值作业** — `src/dmsg_wallet/src/execute.rs:587-620`

充值只以 block 是否已知决定是否发送；没有保存先前可能已执行的标记，任何 Refused 都删除作业并跳过后续 CMC 通知。

```rust
async fn run_top_up(key: u64, me: Principal) -> Result<Option<TopUpEnd>> {
    let (cmc, ledger) = store::with(|w| (w.settings.cmc, w.settings.icp_ledger));
    let Some(Job::TopUp(mut job)) = store::job(key) else {
        return Err(Error::NotFound);
    };
    if job.block.is_none() {
        let sent = ledger::send(ledger, job.arg(cmc, me)).await;
        let Some(Job::TopUp(stored)) = store::job(key) else {
            return Ok(None);
        };
        job = stored;
        match sent {
            Sent::Block(block) => {
                job.block = Some(block);
                store::save_job(key, &Job::TopUp(job.clone()));
            }
            Sent::Refused(reason) => {
                store::remove_job(key);
                return Ok(Some(TopUpEnd::Refused(reason)));
            }
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        }
    }
    let block = job.block.expect("sent");
    let end = match ledger::notify_top_up(cmc, block, me).await {
        Notified::Cycles(cycles) => TopUpEnd::Minted { block, cycles },
        Notified::Refunded(reason) => TopUpEnd::Refused(reason),
        Notified::Pending => return Err(Error::Pending),
    };
    if store::job(key).is_none() {
        return Ok(None);
    }
    store::remove_job(key);
    Ok(Some(end))
```

**普通转账保留过期重试的不确定性** — `src/dmsg_wallet/src/execute.rs:499-519`

相邻普通 ledger 作业在已发送后的 TooOld 保留 Executing 并等待真实 block 对账，说明充值缺少相同保护。

```rust
            Sent::Refused(reason) => {
                let mut w = store::wallet();
                let mut p = store::proposal(id)?;
                let blocks: Vec<u64> = job.items.iter().filter_map(|x| x.block).collect();
                // A first attempt rejected as TooOld never moved funds. Only
                // retries can hide a payment outside the deduplication window.
                let unknown = !first_attempt && reason.starts_with("TooOld");
                if unknown {
                    p.failure = Some(format!(
                        "item {i}: outcome unknown after the deduplication window; check the ledger"
                    ));
                    p.updated_at_ms = at;
                    store::save_proposal(&p);
                    return Err(Error::ExecutionUnknown);
                }
                let reason = format!("item {i}: {reason}");
                let release = blocks.is_empty();
                p.outcome = Some(Outcome::Ledger { blocks });
                fail(&mut w, &mut p, reason.clone(), release, at);
                finish(w, &p);
                return Err(Error::InvalidInput(reason));
```

**TooOld 与先前 Unknown 被分开处理** — `src/dmsg_runtime/src/ledger.rs:172-183`

账本响应丢失归 Unknown；TooOld 等应用错误归 Refused。这个结果只描述当前尝试，不能证明更早的尝试没有转账。

```rust
/// Send one transfer; a duplicate answers with the original block.
pub async fn send(ledger: Principal, arg: TransferArg) -> Sent {
    let reply: std::result::Result<std::result::Result<Nat, TransferError>, CallFailure> =
        call_classified(ledger, "icrc1_transfer", (arg,)).await;
    match reply {
        Ok(Ok(n)) | Ok(Err(TransferError::Duplicate { duplicate_of: n })) => {
            block_index(n).map_or(Sent::Unknown, Sent::Block)
        }
        Ok(Err(TransferError::TemporarilyUnavailable)) => Sent::Unknown,
        Ok(Err(e)) => Sent::Refused(format!("{e:?}")),
        Err(_) => Sent::Unknown,
    }
```

**提案充值也被记为确定失败** — `src/dmsg_wallet/src/execute.rs:635-638`

同一 helper 服务 Refuel 提案；Refused 会以 release=true 结束提案并清理作业，丢失本应保留的恢复状态。

```rust
        Some(TopUpEnd::Refused(reason)) => {
            fail(&mut w, &mut p, reason.clone(), true, at);
            finish(w, &p);
            return Err(Error::InvalidInput(reason));
```

#### Reachability

攻击者无需治理或成员权限，但需要钱包已存在未决充值并越过去重期限；提案充值另需合法成员执行。

- **Attacker:** 非成员的已认证 IC 调用方，或执行已批准提案的成员

- **Entry point:** refuel；成员 execute/reconcile 的 TopUp 分支

- **Outcome:** 推进错误失败终态并移除原充值恢复状态

Preconditions:
- 充值转账可能已提交而返回 Unknown。
- 去重窗口结束前没有取回原 block。
- 后续新充值还需满足现有政策和余额条件。

Existing controls:
- 固定参数及 Duplicate 处理在正常窗口内仍有效；每日充值次数与固定 CMC 收款人限制影响。

#### Severity

**Low** — 这是资金恢复与可用性问题，需先发生已提交但响应未知的充值，并长期未在去重窗口内恢复。公开调用者可以推进错误终态，但不能选择新的收款人，后续新充值仍受配置阈值和每日次数限制，因此定为低风险。

若实测显示生产充值经常出现该未知窗口、后续调用持续造成额外扣款或旧款无法人工找回，应上调；若有独立部署且可靠的对账服务在窗口结束前处理所有充值，则实际发生概率降低。

Impact assessment:
- **Level:** medium
- **Why:** 影响已付资金与 cycles 的恢复、记账和服务可用性，可能开始额外充值。

Likelihood assessment:
- **Level:** low
- **Why:** 要求先前未知付款与长时间未恢复；调用者不能仅凭一个普通请求制造这些前提。

#### Remediation

为 TopUp 持久保存发送/未知状态；此前可能执行过的作业在任何重试拒绝（尤其 TooOld）后继续保持未知，禁止丢弃并用新时间戳自动重开。增加按真实 ledger block 核对原来源、CMC 收款子账户、金额、memo 和时间的充值恢复入口，再通知 CMC。仅确定未执行或确认最终退款后才能释放作业。

Tests:
- 复用 lose_next_response：先验证充值 ICP 已扣除，推进超过 fixture 去重窗口，再调用 refuel，期望仍保留原作业、返回 ExecutionUnknown，后续调用不新增付款。
- 对已批准 Refuel 提案运行同一序列，确认不进入确定 Failed、不提前释放账务状态。
- 以真实原 block 对账后只通知一次 CMC；错误 block、金额、memo、收款子账户必须拒绝。
- 覆盖未知后明确拒绝及并发回复，确认稍晚的失败不能删除另一路已记录成功的作业。

Preventive controls:
- 所有资金调用区分本次拒绝与此前付款的最终结果；复用项目中已经采用的保留 Unknown 原则。

<a id="finding-2"></a>

### [2] Linux 挂载把唯一 WebDAV 凭据放入进程参数

| Field | Value |
| --- | --- |
| Severity | low |
| Confidence | medium |
| Confidence rationale | 源码完整证明 URL 含凭据、进入 argv、Linux 无第二凭据以及 DAV 消费路径；未运行 Linux 多用户/GVFS 实机复现，OS 可见性是明确前提。 |
| Category | Sensitive credential exposure through process arguments |
| CWE | CWE-214 |
| Affected lines | src/dmsg_cli/src/mount.rs:271-277, src/dmsg_cli/src/mount.rs:138-146, src/dmsg_cli/src/mount.rs:365-368, src/dmsg_cli/src/mount.rs:598-632, src/dmsg_cli/src/mount.rs:291-298 |

#### Summary

在允许其他 UID 读取进程命令行的 Linux 主机上，用户开启并挂载 dMsg Files 时，另一本机用户可从短暂运行的 `gio mount` 参数取得完整 DAV URL。Linux 路径没有独立 Basic 密码，取得 URL 即可在账户解锁且挂载有效期间读写文件库，无需 owner token 或 Agent grant。

#### Root Cause

本应只交给本用户挂载客户端的 capability 被序列化到 `gio` 的公开进程参数。Linux 生成的 `password` 为 None，DAV 服务又把 URL token 本身作为完整授权，操作系统的命令行可见性因此跨过 dMsg 的账户文件权限边界。

**Linux 只生成 URL token** — `src/dmsg_cli/src/mount.rs:365-368`

每次挂载生成随机 token；独立 Basic 密码仅在 macOS 存在。Linux 的 token 因而是完整访问凭据。

```rust
                let token = hex::encode(dmsg_client::crypto::random_id());
                let password = cfg!(target_os = "macos")
                    .then(|| hex::encode(dmsg_client::crypto::random_id()));
                let (prefix, url) = share_location(self.port(), &token, &name);
```

**凭据嵌入 URL 路径** — `src/dmsg_cli/src/mount.rs:138-146`

`share_location` 将 token 编进 loopback URL，后续 scheme 替换不会移除它。

```rust
fn share_location(port: u16, token: &str, name: &str) -> (String, String) {
    let prefix = format!("/dav/{token}/{name}");
    let mut url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).expect("loopback URL");
    url.path_segments_mut()
        .expect("HTTP path")
        .clear()
        .extend(["dav", token, name]);
    (prefix, url.into())
}
```

**完整 URL 进入 gio argv** — `src/dmsg_cli/src/mount.rs:271-277`

`system_mount` 把携带 token 的 URL 作为子进程参数传给 `gio`；同样的 URL 也进入 Linux unmount 参数。

```rust
#[cfg(target_os = "linux")]
async fn system_mount(url: &str, _token: &str, _password: Option<&str>) -> Result<Option<String>> {
    run(
        tokio::process::Command::new("gio").args(["mount", &gvfs_uri(url)]),
        None,
    )
    .await?;
```

**凭 URL 即可通过 Linux DAV 授权** — `src/dmsg_cli/src/mount.rs:598-620`

路由按 token 取得处理器；`password=None` 时跳过 Basic 验证，仅检查被害者账户仍解锁且允许挂载。TCP loopback 没有附带对端 UID 授权。

```rust
    let token = params.get("token").cloned().unwrap_or_default();
    let name = params.get("volume").cloned().unwrap_or_default();
    let Some((handler, password, account)) = daemon.mount_of(&token, &name) else {
        return status(StatusCode::NOT_FOUND);
    };
    if let Some(password) = &password
        && !authorized(request.headers(), password)
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"dMsg\"")],
        )
            .into_response();
    }
    {
        let client = daemon.client().lock().await;
        if client.session_state().unlocked_account() != Some(account.as_str()) {
            return status(StatusCode::FORBIDDEN);
        }
    }
    if !daemon.files_mount.load(Ordering::SeqCst) {
        return status(StatusCode::FORBIDDEN);
    }
```

**授权后提供读写并刷新活动时间** — `src/dmsg_cli/src/mount.rs:621-632`

请求被交给整个账户的 LibraryFs，支持 GET 与写操作；GET/写操作还调用 `note_activity`，捕获者可在未显式锁定时延长空闲窗口。

```rust
    let method = request.method().as_str().to_string();
    let changed = matches!(
        method.as_str(),
        "PUT" | "MKCOL" | "DELETE" | "MOVE" | "COPY"
    );
    if changed || method == "GET" {
        daemon.note_activity();
    }
    let fs = LibraryFs::new(daemon.client_handle(), account).for_method(&method);
    let response = handler
        .handle_with(DavConfig::new().filesystem(Box::new(fs)), request)
        .await
```

#### Validation

追踪从 token 生成、URL 构造、GIO 启动到 DAV 路由消费，确认攻击者无需访问 0700 工作区、Keychain 或 owner token。核对相邻 macOS 分支发现独立密码通过 stdin 传给 NetFS，故本结论限定 Linux。

Validation method: 静态源码数据流与平台分支复核

- **Status:** source_validated
- **Disposition:** reportable

**Linux 只生成 URL token** — `src/dmsg_cli/src/mount.rs:365-368`

每次挂载生成随机 token；独立 Basic 密码仅在 macOS 存在。Linux 的 token 因而是完整访问凭据。

```rust
                let token = hex::encode(dmsg_client::crypto::random_id());
                let password = cfg!(target_os = "macos")
                    .then(|| hex::encode(dmsg_client::crypto::random_id()));
                let (prefix, url) = share_location(self.port(), &token, &name);
```

**凭据嵌入 URL 路径** — `src/dmsg_cli/src/mount.rs:138-146`

`share_location` 将 token 编进 loopback URL，后续 scheme 替换不会移除它。

```rust
fn share_location(port: u16, token: &str, name: &str) -> (String, String) {
    let prefix = format!("/dav/{token}/{name}");
    let mut url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).expect("loopback URL");
    url.path_segments_mut()
        .expect("HTTP path")
        .clear()
        .extend(["dav", token, name]);
    (prefix, url.into())
}
```

**完整 URL 进入 gio argv** — `src/dmsg_cli/src/mount.rs:271-277`

`system_mount` 把携带 token 的 URL 作为子进程参数传给 `gio`；同样的 URL 也进入 Linux unmount 参数。

```rust
#[cfg(target_os = "linux")]
async fn system_mount(url: &str, _token: &str, _password: Option<&str>) -> Result<Option<String>> {
    run(
        tokio::process::Command::new("gio").args(["mount", &gvfs_uri(url)]),
        None,
    )
    .await?;
```

**凭 URL 即可通过 Linux DAV 授权** — `src/dmsg_cli/src/mount.rs:598-620`

路由按 token 取得处理器；`password=None` 时跳过 Basic 验证，仅检查被害者账户仍解锁且允许挂载。TCP loopback 没有附带对端 UID 授权。

```rust
    let token = params.get("token").cloned().unwrap_or_default();
    let name = params.get("volume").cloned().unwrap_or_default();
    let Some((handler, password, account)) = daemon.mount_of(&token, &name) else {
        return status(StatusCode::NOT_FOUND);
    };
    if let Some(password) = &password
        && !authorized(request.headers(), password)
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"dMsg\"")],
        )
            .into_response();
    }
    {
        let client = daemon.client().lock().await;
        if client.session_state().unlocked_account() != Some(account.as_str()) {
            return status(StatusCode::FORBIDDEN);
        }
    }
    if !daemon.files_mount.load(Ordering::SeqCst) {
        return status(StatusCode::FORBIDDEN);
    }
```

**授权后提供读写并刷新活动时间** — `src/dmsg_cli/src/mount.rs:621-632`

请求被交给整个账户的 LibraryFs，支持 GET 与写操作；GET/写操作还调用 `note_activity`，捕获者可在未显式锁定时延长空闲窗口。

```rust
    let method = request.method().as_str().to_string();
    let changed = matches!(
        method.as_str(),
        "PUT" | "MKCOL" | "DELETE" | "MOVE" | "COPY"
    );
    if changed || method == "GET" {
        daemon.note_activity();
    }
    let fs = LibraryFs::new(daemon.client_handle(), account).for_method(&method);
    let response = handler
        .handle_with(DavConfig::new().filesystem(Box::new(fs)), request)
        .await
```

Assertions:
- URL 在 `gio` 存活时出现在其 argv。
- 非 macOS 分支将 `password` 留为 None；匹配 token 的请求无需 Authorization header。
- DAV LibraryFs 检查的是当前解锁账户，不是连接发起者的 OS UID。

Counterevidence and remaining uncertainty:
- 挂载默认关闭，账户锁定或切换后访问被拒绝。
- hidepid 等 OS 策略可能阻止其他 UID 读取 argv。
- macOS 独立 Basic 密码使单独泄露 URL 不足以访问。

Limitations:
- 没有运行 gio/GVFS、读取其他真实用户进程或访问真实文件库。
- Windows 分支也把 URL 放入参数，但未核对其跨用户可见性，不将其作为已验证受影响平台。

#### Dataflow

随机 token → `share_location` → `gvfs_uri` → `gio mount` argv → 本机攻击者 → `/dav/{token}/{volume}` → `LibraryFs`

- **Source:** 跨 UID 可见的挂载子进程 argv

- **Sink:** 解密后的账户文件库

- **Outcome:** 未经 dMsg owner 或 Agent 授权读取、修改挂载文件

**完整 URL 进入 gio argv** — `src/dmsg_cli/src/mount.rs:271-277`

`system_mount` 把携带 token 的 URL 作为子进程参数传给 `gio`；同样的 URL 也进入 Linux unmount 参数。

```rust
#[cfg(target_os = "linux")]
async fn system_mount(url: &str, _token: &str, _password: Option<&str>) -> Result<Option<String>> {
    run(
        tokio::process::Command::new("gio").args(["mount", &gvfs_uri(url)]),
        None,
    )
    .await?;
```

**Linux 只生成 URL token** — `src/dmsg_cli/src/mount.rs:365-368`

每次挂载生成随机 token；独立 Basic 密码仅在 macOS 存在。Linux 的 token 因而是完整访问凭据。

```rust
                let token = hex::encode(dmsg_client::crypto::random_id());
                let password = cfg!(target_os = "macos")
                    .then(|| hex::encode(dmsg_client::crypto::random_id()));
                let (prefix, url) = share_location(self.port(), &token, &name);
```

**凭 URL 即可通过 Linux DAV 授权** — `src/dmsg_cli/src/mount.rs:598-620`

路由按 token 取得处理器；`password=None` 时跳过 Basic 验证，仅检查被害者账户仍解锁且允许挂载。TCP loopback 没有附带对端 UID 授权。

```rust
    let token = params.get("token").cloned().unwrap_or_default();
    let name = params.get("volume").cloned().unwrap_or_default();
    let Some((handler, password, account)) = daemon.mount_of(&token, &name) else {
        return status(StatusCode::NOT_FOUND);
    };
    if let Some(password) = &password
        && !authorized(request.headers(), password)
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"dMsg\"")],
        )
            .into_response();
    }
    {
        let client = daemon.client().lock().await;
        if client.session_state().unlocked_account() != Some(account.as_str()) {
            return status(StatusCode::FORBIDDEN);
        }
    }
    if !daemon.files_mount.load(Ordering::SeqCst) {
        return status(StatusCode::FORBIDDEN);
    }
```

**授权后提供读写并刷新活动时间** — `src/dmsg_cli/src/mount.rs:621-632`

请求被交给整个账户的 LibraryFs，支持 GET 与写操作；GET/写操作还调用 `note_activity`，捕获者可在未显式锁定时延长空闲窗口。

```rust
    let method = request.method().as_str().to_string();
    let changed = matches!(
        method.as_str(),
        "PUT" | "MKCOL" | "DELETE" | "MOVE" | "COPY"
    );
    if changed || method == "GET" {
        daemon.note_activity();
    }
    let fs = LibraryFs::new(daemon.client_handle(), account).for_method(&method);
    let response = handler
        .handle_with(DavConfig::new().filesystem(Box::new(fs)), request)
        .await
```

**凭据嵌入 URL 路径** — `src/dmsg_cli/src/mount.rs:138-146`

`share_location` 将 token 编进 loopback URL，后续 scheme 替换不会移除它。

```rust
fn share_location(port: u16, token: &str, name: &str) -> (String, String) {
    let prefix = format!("/dav/{token}/{name}");
    let mut url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).expect("loopback URL");
    url.path_segments_mut()
        .expect("HTTP path")
        .clear()
        .extend(["dav", token, name]);
    (prefix, url.into())
}
```

#### Reachability

条件为 Linux、挂载已开启、账户解锁、同一主机网络空间和跨 UID 进程参数可见性；攻击者只需普通本机账户。

- **Attacker:** 不同 OS 用户的本机进程

- **Entry point:** gio 进程参数与 loopback DAV 端点

- **Outcome:** 访问当前挂载库的明文与写接口

Preconditions:
- 用户执行 files/mount。
- 攻击者能够读取该 gio 进程的 argv。
- 被害者尚未显式锁定、切换账户或卸载。

Existing controls:
- 默认关闭；只监听 loopback；锁定/切换账户使文件系统拒绝访问。

#### Severity

**Low** — 影响可达当前挂载文件库的明文读取和内容修改，但要求启用默认关闭的 Linux 挂载、与被害者共享主机及网络命名空间、具备跨 UID 命令行可见性并捕捉短暂子进程。按受限本机路径降为低风险。

若在目标 Linux 桌面证明 token 持续出现在跨 UID 可读的 GVFS 状态或可稳定捕获 argv，风险应上调；若部署强制隔离进程可见性与网络命名空间，实际可利用性降低。

Impact assessment:
- **Level:** high
- **Why:** 完整文件库的保密性和完整性受影响。

Likelihood assessment:
- **Level:** low
- **Why:** 可选功能、共享 Linux 主机、OS 参数可见性及捕获窗口均是前提。

#### Remediation

为 Linux DAV 增加不出现在 URL/argv 的独立凭据，经私有 stdin/FD 和 GIO 原生认证接口交给挂载客户端；不能在 helper 中再把秘密拼回 gio 命令行。或在具备此交付方式前停用 Linux 挂载。同步修正 token 只在 daemon 内存中的文档描述。

Tests:
- 在隔离 Linux 双 UID 环境挂载虚拟库，读取挂载进程参数，确认其中没有可单独授权的秘密。
- 仅使用公开挂载 URL 的 GET/PROPFIND/PUT 必须被拒绝；系统挂载提供独立凭据后正常工作。
- 显式锁定、切换账户和卸载后，旧凭据及进行中的读取均失效。

Preventive controls:
- 进程参数、挂载名称和可枚举的系统元数据不能充当私密 capability 的唯一存放位置。

## Reviewed Surfaces

| Surface | Risk Area | Outcome | Notes |
| --- | --- | --- | --- |
| 完整实现与生成代码覆盖 | coverage | Needs follow-up | 本轮主要阅读安全入口及相关片段，不能把所有 dmsg 源码视为逐行完成。尚未逐行完成 client 的全部同步/迁移/存储分支、daemon 全方法、全部平台桥接和稳定存储迁移；自动生成内容与 UI 本地化仅盘点/检索。 |
| 普通 ICRC 未知后其他拒绝与 CMC 错误分类 | financial recovery | Needs follow-up | execute.rs:505 只对未知重试 TooOld 特判；runtime/ledger.rs:246 将 InvalidTransaction/TransactionTooOld 映射 Refunded。需要固定生产 ledger/CMC 合同和定向故障实验再判断这些条件，未把它们扩大成确认的重复支付或资金永久丢失。 |
| 账户设备、恢复、正式认证与应用批准 | authentication and authorization | No issue found | 已追踪 src/dmsg_user/src/account.rs:106、:246、:360，api.rs:350、:536、:611，external.rs 与 recovery.rs 的实际授权/提交/重放路径。账户变更在候选副本完成校验；AddDevice/attest 在 await 后重读；根恢复必须是已完成恢复的设备。读取旧回执不视作重新执行。 |
| 来信资金托管与名称收费 | money lifecycle | No issue found | 已审查 dmsg_payment/src/api.rs 的开单、查账、结算、退款、重试与对账以及 dmsg_handle/src/api.rs 的认领/注册/转移/收费。payment/model.rs:394 与 handle/api.rs:667 保留先前 Unknown，固定账本参数并按真实 block 对账。user home 对名称授权的消费是该路径声明的线性化点，未把已批准精确意图在返回后继续提交单独判为绕过。 |
| Agent/owner、本地 API 与受限数据访问 | capability isolation | No issue found | daemon.rs:1921 拒绝 Origin；:1997 owner 与 Agent 路由分离；agents.rs:1209 起 vault.list 只检索可披露字段，vault.read/write 排除 reserved/隐藏值，并按标签/文件夹授权；library.rs:319 核对版本属于目标文件。held.rs 钉住账户，Unlocked 钉住解锁代次。 |
| 浏览器桥接与证明校验 | cross-origin and cryptographic binding | No issue found | connect/page.ts:93 起检查 opener、source、origin、一次性 nonce；bridge.ts:343 以 P-256 验证连接/操作证明，:581 以认证设备 HPKE 解回复。SDK ic.ts 核对独立信任根、canister、路径/叶值及时间；公开证明的权威配置不能由 bundle 自报。 |
| 设备传输、文件导入与接收 | untrusted files and session binding | No issue found | transfer/session.rs 检查账户/设备/session、签名、HPKE、序号与manifest；失败连接继续监听，record 仅按批准清单发送且接收端核对摘要。vault/import.rs:139、:267 对源文件、zip成员/总大小/解压大小有界校验；library/request.rs:183 绑定上传计划，另存新文件且名字经 safe_name。 |
| iOS 平台安全与系统导出 | platform credentials | No issue found | AppleSecurity.swift 使用 ThisDeviceOnly Keychain、Secure Enclave 和私有 LAContext token；系统导出只持有显式导出快照，Credentials 扩展拒绝无交互取密，Files 扩展检查快照/取消并清理不完整输出。此结论为源码阅读，未运行设备验证。 |
| Linux 文件库 WebDAV 挂载凭据 | credential exposure | Reported | 候选 linux-dav-argv 经静态反证复核保留，见 dmsg.linux-dav-token-argv。仅 Linux 跨 UID argv 可见场景；macOS 有独立 stdin 密码，Windows 可见性未验证。 |
| 钱包充值未知结果恢复 | financial recovery | Reported | 候选 wallet-unknown-retry 收窄到 TopUp 的 TooOld 恢复路径，见 dmsg.wallet-topup-unknown-discard。普通 ICRC 的其他错误次序未升级为确认漏洞。 |
| 钱包审批、协签与交易构造 | fund authorization | No issue found | 除已报告充值恢复外，api.rs:528/637/1388 的成员绑定/解绑/投票均检查精确用途与当前状态；execute.rs:974 在签名之前及返回时核对计划版本；chains/btc.rs 固定输出/找零与 sighash，evm.rs 固定 calldata/chain/fee，sol.rs 只构造限定指令。未运行链节点或签名互操作测试。 |
| 结账、PANDA 与 App Store 权益 | billing and cross-service authorization | No issue found | checkout.rs:260 同时核对账户/产品权威，返回后检查有效期、摘要与当前配置；membership/claims.rs:323 在最终 SNS 等待后检查 authorization_until；app_store.rs:306 限定 verifier 并绑定 beneficiary/environment/原交易与单调状态。sns.rs:79 允许列入权限表的 actor 是明示当前合同，未按旧经济独占设计误报 hotkey 绕过。 |
| 根、同步与频道控制 | cryptographic integrity and replay | No issue found | root.rs:534 将当前根绑定认证 bundle digest/account/environment/generation，previous 链检查摘要和承诺；sync_pull.rs:92 验证签名/对象字段/单调页 cursor；channel_log.rs:538 起重放角色和 ACL 变化，历史授权必须可读；channel.rs:1040 起核对签名 upload plan。恶意中继隐藏历史的既有信任边界未当作成员签名伪造。 |
| Electron 主进程、文件输入与更新 | renderer and code execution boundary | No issue found | main/index.ts 的 trust/handle 按窗口身份分权；policy.ts:494 去掉 human 与任意 attest 文件路径，preload/index.ts 用真实 File 提取路径。系统选择器和确认仍由 main 执行。release-feed.ts 校验完整版本资产，updater.ts 固定 feed/版本并禁降级；release.rs:207 在 macOS 核对签名团队。未验收真实平台安装包。 |
| 秘密使用、进程交付、SSH 与机密挂载 | secret execution | No issue found | secrets/github.rs 固定 api.github.com、禁跳转/重试/代理且结果受限；run.rs 在发送前持久标记并以 Unlocked 绑定；process.rs:415 先消费一次 claim，再重查精确源/会话；process_command.rs 在 spawn 前检查文件身份。ssh_agent.rs 校验0700 home、0600 socket、peer UID；secret_mounts.rs 使用路径约束、cap-std 与 peer 校验。未运行 FUSE/FSKit/SSH/Git。 |
| 旧版冻结与迁移 | legacy authority | No issue found | ic_message/Profile/Channel 的业务写入口通过 migration_canister.rs:73/86 的冻结票据；异步 callback 依票据重读状态，drain/seal 由 controller 执行；旧 name-identity 撤销和只读/登录路径与扩权写路径分开。handle 认领按 frozen owner/admin，不按同名显示身份授权。迁移读取器有格式/大小/清单校验；未复验全部线上冻结来源。 |
| 嵌入凭据与私钥搜索 | embedded secrets | No issue found | 对当前树代码/配置/文档/测试离线检索常见私钥/token格式并检查命中上下文。命中为 PEM 识别器、明确 synthetic 内容和 ssh_tests.rs:51 的测试生成钥匙；未找到授予外部权限的凭据证据。未查询 Git 历史或使用任何候选秘密。 |
| CBOR 嵌套耗尽假设 | parser resource limits | Rejected | client/codec.rs 在反序列化前限制24层/节点/字节；直接证书 witness 解码进一步核对 Cargo.lock 中 cbor2 1.1.6 的本地依赖源码，默认递归上限256，未把缺少应用级扫描器直接判为无限递归漏洞。依赖文件只作控制反证，不计产品覆盖。 |
| Agent connector 明文缓存 | documented local data boundary | Rejected | dmsg_agent/runtime.rs:903 把 connector snapshot 写入 agents.sqlite；README:89 明确它包含 Agent 已读房间和信件并支持锁定后驻留工作，邮件私钥 checkpoint 则由宿主封存。未找到从此处取得账户 vault/root 私钥的链，不按全盘加密承诺误报。 |

## Open Questions And Follow Up

- 在 faa3e06e8e963938a247d7823ad7638095bc6633 上，Linux 挂载 token 是否可由另一普通 UID 稳定捕获并访问虚拟库？
  - Follow-up prompt: 针对 src/dmsg_cli/src/mount.rs，在隔离 Linux 双用户环境完成 gio/GVFS 凭据暴露与锁定撤销回归，不访问真实文件库。
- TopUp 的 Unknown → TooOld 是否在固定 PocketIC/ledger/CMC 夹具下重现作业删除和后续新扣款？
  - Follow-up prompt: 针对 src/dmsg_wallet/src/execute.rs:587，复用 lose_next_response 夹具给自动 refuel 和 Refuel 提案增加确定性跨去重窗口测试。
- 普通 ICRC 与 CMC 错误分类是否还能错误消除历史不确定性？
  - Follow-up prompt: 复核 src/dmsg_wallet/src/execute.rs:499 与 src/dmsg_runtime/src/ledger.rs:233；固定实际账本/CMC版本后测试 Unknown 后的非 TooOld 拒绝及非退款通知错误。
- 未逐文件逐行审完全部实现、生成代码和依赖；已完成各主要安全边界的定向追踪，剩余路径需后续细化覆盖。
  - Follow-up prompt: Review deferred unit remaining-source-detail and close its stated proof gap. Paths: src/dmsg_client/src, src/dmsg_cli/src, src/dmsg_desktop/src, src/dmsg_mobile/src, src/dmsg_ios, src/dmsg_runtime/src, src/dmsg_types/src, src/dmsg_commerce/src/stable_codec.rs, src/dmsg_user/src/stable_codec.rs.
- 本轮未执行运行测试。两项发现需 Linux 双 UID/GVFS 虚拟库和 PocketIC 充值故障回归；完整 cargo/pnpm/Electron/iOS/外链验证未运行。
  - Follow-up prompt: Review deferred unit runtime-validation and close its stated proof gap. Paths: src/dmsg_cli/src/mount.rs, src/dmsg_wallet/src/execute.rs, tests/dmsg_integration/tests/control_plane/wallet_review.rs.
- 需核对所接入生产 ledger/CMC 的固定合同，验证未知后 BadFee/InsufficientFunds 与 CMC 非退款错误能否到达并影响账务。
  - Follow-up prompt: Review deferred unit icrc-cmc-other-errors and close its stated proof gap. Paths: src/dmsg_wallet/src/execute.rs, src/dmsg_runtime/src/ledger.rs.
