# dMsg 源码安全审查与修复报告

日期：2026-11-08（Asia/Shanghai）

初次审查基线：`605538c5d0cf4df0f5301cf19accf8c9be8ee40b`

修复前基线：`b81a02daecbd5a3e96bf67e68d0d853ddd0c38b0`（两者仅 iOS 构建号不同）

复核与修复：Codex；按项目要求由同一个 agent 独立顺序完成，没有使用 subagents。

## 1. 当前结论

本次逐项复核原报告的四个候选，均已确认、修复并通过定向回归。C1/C3/C4 已完成修复前后运行对照；C2 的钱包投票与 hub 创建两个入口也完成了相同 Rust 1.98.1 工具链下的 PocketIC 对照。首次修复见提交 `f8379127`，本报告补充其后的固定工具链验证结果。

| 编号 | 问题 | 当前处理 | 运行证据 |
| --- | --- | --- | --- |
| C1 | `vault.list` 搜索受限内容 | 已修复并验证 | 原版本 notes-only 查询返回条目；修复后查询只匹配可见元数据 |
| C2 | 钱包消费已过期的设备批准 | 已修复并验证 | 原版本到期后仍记票/创建；修复后拒绝，及时批准及已提交订单重放成功 |
| C3 | 无效回调提前消费登录会话 | 已修复并验证 | 原版本被拒绝请求移除 session；修复后 session 保留，合法取消只消费一次 |
| C4 | JWKS 在大小检查前完整缓冲 | 已修复并验证 | 原版本超过预算后仍等待 EOF；修复后提前返回，恰好 64 KiB 的正常响应仍通过 |

本报告记录公开源码边界审查与四项问题的修复验证，审查范围及覆盖记录见第 4–5 节。运行测试使用临时工作区、虚拟账户、测试账本和本地服务；线上部署、真实资产及平台发布物验证不在本轮范围内。

## 2. 四项复核详情

### C1：Agent 列表搜索泄露正文匹配结果

Fingerprint：`dmsg/agent/vault-list-content-oracle`。

**边界和根因：**只有 `vault.list`、没有目标标签的 `vault.read` 的 Agent，可传入查询；旧 `dmsg_vault_list` 调用 owner 的全文搜索，再投影元数据。`Item::matches` 包含 notes、字段标签和普通字段值，条目 ID 是否返回因此成为正文子串的判断接口。concealed/OTP 值和 reserved 条目不在该结论内。

**修复前实测：**创建 title=`Public title`、tag=`work`、notes=`private-note-marker` 的虚拟条目。直接读取返回 `FORBIDDEN`，但查询该 notes 标记返回包含条目 ID 的数组，而预期为空。

**修复：**`src/dmsg_cli/src/agents.rs:1210` 不再把 Agent 查询传入全文检索，而只匹配列表可披露的 title/type/tags；保留大小写不敏感、空白修剪和 reserved 排除。`src/dmsg_agent/src/tools.rs` 同步明确工具搜索合同。owner 全文检索仍按原合同工作。

**回归：**`agents::tests::audit_vault_list_queries_only_disclosed_metadata` 覆盖 notes、普通字段标签/值、隐藏值、保留条目标签、无匹配查询；正向覆盖标题、类型、标签、大小写及空查询，并检查 owner 仍能检索正文、Agent 直接读取仍被拒绝。修复前失败，修复后通过。

### C2：钱包消费者未在最终写入前复查批准截止

Fingerprint：`dmsg/wallet/consumer-authorization-expiry`。

**边界和根因：**调用方必须已有合法成员身份和真实设备批准。`dmsg_user/src/external.rs` 将授权截止设为应用批准和设备批准的较早期限，验证成功后返回 `ApplicationAuthorization.valid_until_ms`。旧钱包投票及 hub 创建丢弃了该回执；等待期间批准可能到期，而提案或报价尚未到期。既有成员、阈值、报价、付款和配额校验不能替代批准的截止校验。

**修复：**

- `src/dmsg_wallet/src/api.rs:1419` 保留验证回执；返回后重新读取时间，在写票前要求 `at < authorization.valid_until_ms`。此路径同时服务 approve/reject/veto，现有成员与提案重读继续保留。
- `src/dmsg_wallet_hub/src/lib.rs:371` 保留回执，在最后的 ledger balance 等待返回后、保存 Paid 订单和占用配额前执行同一截止检查；已提交订单的原样幂等重放仍走既有恢复路径。

没有新建授权抽象或改动状态格式；截止来自固定 user home 的回执，已涵盖原始应用批准期限，不需要重新定义钱包授权协议。相邻 join/unbind 本来就执行同类检查。

**回归设计：**`tests/dmsg_integration/tests/control_plane/wallet_review.rs` 新增：

- `audit_wallet_decisions_recheck_device_approval_expiry_after_await`：钱包与 user home 位于不同子网，逐轮提交/暂停/推进至设备批准的排他截止；应用批准仍有效，待完成调用应返回 `Expired` 且不留票，同时验证及时完成的调用成功。
- `audit_wallet_creation_rechecks_approval_after_balance_and_preserves_replay`：hub 与 home/ledger 跨子网，逐轮越过批准期限；尚未提交的创建不得保存订单或占配额，及时提交的创建完成后，批准过期的原样重放仍成功。

**修复前实测：**两个回归均在暂停一轮、将时间推进至设备批准的排他截止后失败：钱包返回带有一票的 `Ok(Proposal)`，hub 返回 `Ok(ActivationOrder)` 且状态为 `Ready`，而预期均为 `Err(Expired)`。应用批准和报价此时仍有效，证明缺少的是设备批准记录的最终消费期限检查。

**修复后实测：**相同 Rust 1.98.1、依赖特征、测试程序和真实 user home 验证路径，仅替换修复后的钱包/hub Wasm，两个回归均通过。过期调用不留票、不留订单、不占配额；及时完成的投票/创建及批准到期后对已提交创建的幂等重放均通过。钱包相邻 PocketIC 测试合计 30 通过、0 失败、3 ignored。

### C3：不匹配的登录回调使待登录会话失效

Fingerprint：`dmsg/daemon/login-callback-prevalidation-consumption`。

**边界和根因：**`/auth/callback` 是专用公开回环入口。没有 owner token 或随机 state 的本机调用方可发送请求；旧实现先 `take()` session，再校验 Origin、Content-Type 和 state。请求虽被拒绝，用户正在完成的登录却被消费。该结论是本机登录可用性问题，不等于登录身份绕过；未声称普通网页可绕过浏览器网络限制。

**修复前实测：**向 pending session 提交无 Origin/Content-Type 的空回调，收到拒绝后 `login.session.is_some()` 断言失败。

**修复：**`src/dmsg_cli/src/daemon.rs:2213` 先验证头，再在登录锁内调用 `LoginSession::accept` 校验正文和当前 session。只对成功结果或已校验 version/state 的登录页错误消费会话并设置 checking；无效内容、错误 state 不改变 pending/outcome。合法取消仍被消费，重复或竞争回调只有一个能取得会话。

**回归：**`daemon::tests::audit_invalid_login_callbacks_preserve_the_pending_session` 覆盖无效头、畸形 CBOR、错误 state、合法 user_cancelled 和重复回调。修复前失败，修复后通过；既有 `login_result_answers_while_the_callback_checks` 等临近测试也通过。

### C4：JWKS 响应大小检查发生在全量缓冲之后

Fingerprint：`dmsg/mcp-events/jwks-postbuffer-limit`。

**边界和根因：**绑定的低信任 MCP 上游，或持有相应 endpoint 管理能力的 Agent，可影响 JWKS 下载来源。旧 `response.bytes().await` 先收集正文，再检查 64 KiB；10 秒超时、16 个 endpoint 和一小时缓存不能代替字节预算。不是无凭证互联网调用方直接访问 daemon。

**修复前实测：**私有 loopback fixture 发送 64 KiB + 1 字节后保持连接，旧实现未提前结束，2 秒测试期限到达；恰好 64 KiB 的合法 JWKS 对照通过。未进行大流量、OOM 或进程崩溃试验。

**修复：**`src/dmsg_cli/src/mcp_events.rs:108` 改为逐 chunk 读取；每次扩展缓冲前检查剩余预算，超额立即丢弃响应并返回空 key 集合。没有依赖 Content-Length 判定；保留超时、禁止跳转、失败/成功缓存与验签语义。

**回归：**`audit_jwks_stops_reading_at_the_limit` 分别覆盖声明较大 Content-Length 以及无 Content-Length 的 chunked 流，二者均在超额后、不等 EOF 即返回；`audit_jwks_accepts_exact_limit_and_caches_valid_keys` 覆盖合法边界和成功缓存，另验证失败缓存。修复前超额用例失败，修复后两项通过。64 KiB 限制针对累计正文长度，不声称 Vec 容量、HTTP/TLS 栈及单个网络 chunk 完全零额外内存。

## 3. 实际验证记录

首轮执行环境为本机已有的 Linux 容器和本地依赖缓存：禁止外网、独立 loopback、只读源码/依赖/工具链、空环境显式 allowlist，HOME/TMPDIR/cache/构建输出限定于 scratch。容器上限为 4 CPU、8 GiB 内存及同值 memory-swap、256 PID、6 GiB scratch、单文件 2 GiB、每进程 CPU 900 秒；各构建另有 900 秒 wall-clock timeout，禁用额外 capabilities，启用 no-new-privileges。未下载依赖或新工具链。

首轮容器使用 Rust **1.99.1**；其结果与后续固定工具链结果分别记录。用户授权原生离线验证后，使用本机现有 **Rust 1.98.1**、Wasm 标准库、**PocketIC server 16.0.0** 和锁定依赖，在临时源码副本重新构建并执行。原生执行清空环境、禁止外网，仅允许回环与测试目录内 Unix socket；写入限制在临时目录，每条构建/测试命令有 900 秒超时。没有下载依赖、工具链或 PocketIC，也没有在用户实际 dMsg 工作区运行服务。

| 检查 | 结果 |
| --- | --- |
| 修复前 `cargo test --offline --locked -p dmsg_cli audit_ -- --test-threads=1` | 1 通过、3 按预期失败；分别复现 C1/C3/C4 |
| 修复后相同四个 `audit_` 用例 | 4 通过、0 失败 |
| 修复后 `cargo test --offline --locked -p dmsg_cli -- --test-threads=2` | 111 通过、1 失败、5 ignored；不是全套通过 |
| 初次 CLI 唯一失败的处理 | 既有 socket 夹具 rename 到硬编码 `/tmp` 时出现 `EXDEV`；只将临时镜像中的 `/tmp` 指向 scratch/tmp，复跑同一测试二进制，无源码豁免 |
| 调整测试容器布局后 CLI 全套复跑 | **112 通过、0 失败、5 ignored**；复跑上限 2 CPU / 2 GiB / 256 PID / 256 MiB scratch / 120 秒，其他隔离条件不变 |
| CLI ignored | Electron、真实 FUSE、Git SSH、sshd 需要专用环境；另有一个作为子进程入口的 helper，父测试已实际调用并通过 |
| `cargo check --offline --locked -p dmsg_wallet -p dmsg_wallet_hub -p dmsg_integration --test control_plane --features pocketic-tests` | 通过；包括新增 PocketIC 测试的编译检查，不代表运行集成测试 |
| `cargo test --offline --locked -p dmsg_wallet -p dmsg_wallet_hub --lib` | 钱包 12 项、hub 3 项全部通过；覆盖编码/存储与交易算法对照，不替代跨调用期限回归 |
| 固定 Rust 1.98.1 Clippy：受影响五个包，`--all-targets --features pocketic-tests -- -D warnings` | 通过；首次发现 JWKS fixture 未处理分段请求读取，补为 4 KiB 有界完整头读取后复跑通过 |
| 固定 Rust 1.98.1 的 rustfmt（仅修改文件）及 `git diff --check` | 通过 |
| Rust 1.98.1 / PocketIC：修复前 `audit_wallet_` | 0 通过、2 按预期失败；两个入口均复现到期后成功消费批准 |
| Rust 1.98.1 / PocketIC：修复后 `audit_wallet_` | 2 通过、0 失败；同时覆盖及时批准和合法幂等重放 |
| Rust 1.98.1 / PocketIC：全部 `wallet::` 集成测试 | 30 通过、0 失败、3 ignored |
| Rust 1.98.1：`cargo test --offline --locked -p dmsg_cli audit_ -- --test-threads=1` | 4 通过、0 失败；包含本次补正后的 JWKS fixture |
| 官网审计状态文案清理：`pnpm check`、`pnpm test`、`pnpm build` | 类型检查 0 错误/0 警告；heartbeat 4 项与 Vitest 123 项通过，2 项在线测试 skipped；生产构建和公开内容校验通过；23 个语言目录及构建产物均无已移除的旧状态文案 |
| `cargo build --offline --locked --release --target wasm32-unknown-unknown`（钱包、hub 及十个集成夹具模块） | Rust 1.99.1 首轮通过；Rust 1.98.1 的 12 个修复后模块及修复前对照构建均通过 |

首轮 Rust 1.99.1 报告既有 `notifications.rs` 的 `Atomic::fetch_update` 弃用警告。原生构建初期补齐了本机 LLVM 动态库路径，最终命令成功；隔离下的 xcrun 缓存写入产生工具告警，Clippy 检查本身退出码为 0。未为这些环境告警修改无关产品代码。

钱包集成测试的三个既有 ignored 项分别需要独立 Anvil、独立 Solana validator、已发布的 v3.1.2 钱包 Wasm；这些扩展环境没有运行。

复现命令（先用同一工具链构建修复前/后 Wasm，再分别设置本地 `POCKET_IC_BIN` 与 `DMSG_WASM_DIR`）：

```sh
cargo test --offline --locked -p dmsg_integration --features pocketic-tests --test control_plane audit_wallet_ -- --test-threads=1
cargo clippy --offline --locked -p dmsg_cli -p dmsg_agent -p dmsg_wallet -p dmsg_wallet_hub -p dmsg_integration --all-targets --features pocketic-tests -- -D warnings
```

上述 PocketIC 定向命令在修复前失败、修复后通过。相邻测试使用 `wallet::` 过滤器和 `--test-threads=2`；结果与具体忽略条件如上表。

## 4. 审查范围与方法

公开范围为 `src/dmsg_*` canisters、Rust 客户端、daemon/CLI/MCP、Electron、iOS/移动核心、connect、legacy，以及 membership、SDK、相关协议/测试和发布配置。初次盘点为 1,315 个代码文件、429,389 行（含测试、生成及本地化内容），记录了 140 个文件的定向源码阅读；盘点不是逐行覆盖，也不是测试覆盖率。

已读取 AGENTS.md 指定的本地私有 PRD、实施、商业化及 Agent Protocols 基线，复核本地提交与工作区状态；没有核对远端，不声称本地文档最新。公开报告没有复制私有设计正文、Worker 实现或凭据。用户已有 iOS 构建号改动已由其他提交进入修复基线，本次不包含 iOS 文件修改。

调查按入口、主体、资源/操作绑定、生命周期和 await、最终写入/披露点顺序追踪；修复后顺序复读失败路径、相邻入口、正常行为和重放语义。按项目要求，源码追踪、反证复读与运行验证均由当前 agent 顺序完成。

## 5. 初次源码覆盖及仍存在的缺口

以下防护表和 44 单元账本是初次审查基线的历史记录，不将其改写为本轮全量运行通过。原统计为 covered=32、candidate=5、blocked=3、deferred=1、out_of_scope=3；五个 candidate 单元对应四个根因，C2 有两个消费者。候选的当前状态以第 1–3 节为准。

### 初次定向阅读中的有效防护


以下结论限所列源码路径，均不是运行测试通过声明。

| 路径 | 源码中实际存在的防护/反证 |
| --- | --- |
| 账户设备/硬件批准 | `src/dmsg_user/src/account.rs:106` 集中检查登录绑定、未撤销设备、能力、epoch、sequence、期限和硬件政策。`SetApprovalKey` 受 `AccountControl` 约束，不能仅凭“任何设备可设置自己的 key”认定绕过。 |
| 恢复与 COSE | `recovery.rs` 检查恢复开关、绑定登录、设备持有证明及等待期限；`dmsg_cose/src/api.rs` 检查配置 home 与 AccountId 分配器、执行序号和预算。旧证据或未知执行没有被当作新授权无条件复用。 |
| 正式认证与商业批准 | `attest_statement` 在跨服务返回后重查账户/时间/配置；`checkout::authorize` 和 `membership::claims::authorize` 校验回执身份、摘要、期限与一分钟新鲜度。它们是 C2 的直接对照。 |
| 来信托管与退款 | 固定报价收款人/平台账户，入金由真实 ledger/archive reply 验证 block/index；退款只回原账户，Unknown 保留不可变 memo/created_at_time，后来的拒绝不能证明早先未知调用未成功。 |
| 名称与 PrincipalDocument | 旧名按冻结 owner/admin 认领；同版本异内容拒绝。directory 校验账户 home，发布回调重读当前 state，旧发布不能回退新文档。 |
| daemon 普通 API | `/app/v1` bearer 拒绝所有 Origin，Agent 与 owner 方法分离，WS 为 owner-only。C3 限独立 callback 路由，不能扩大成所有 API 无认证。 |
| Agent 数据访问 | Vault reserved 条目排除；concealed/OTP 遮罩；写入不改变 tags、旧 head 或把隐藏字段改成明文。文件检索按可读 folder 过滤。C1 限 list 的内容谓词。 |
| 连接器 origin | `runtime.restore` 清空快照 hosts，再按配置与 grant 的 reach 重建；read/list 过滤其他 origin 的 room/draft。未审完第三方 SDK 内部，不能宣称整个 connector 全面无缺陷。 |
| MCP webhook 签名 | 测试明确允许缺少 `v1a` 的记录以 `verified.v1a=false/null` 留存，失败的非空签名才丢弃。未找到消费方把 false/null 升为已认证授权的完整链，故未报告为已确认验签绕过。 |
| Electron | sandbox/contextIsolation、禁用 Node、CSP、sender 角色及方法白名单；drop 路径由 preload 从真实 File 获取。仅看 main handler 接受 path 数组，不能推导网页可任意读主机文件。 |
| connect / SDK | 精确 origin/source、一次 nonce、command/operation digest、回复 HPKE auth 与设备证据；SDK 验证显式传入的 home/root/witness。未把正常已授权 pairing 当成通用签名权。 |
| 锁定/会话 | `Client::lock` 清 login/keys/root，递增 generation；Hold/Unlocked/ScopedHost 防止旧步骤写入新会话。显式后台在线保留签名能力是产品选择，不声称锁定删除已持有内容。 |
| 同步与频道 | 验证签名字段、历史 control_head、ACL/epoch 和当前接收者可读范围；页序号与游标绑定，下载/应用后提交。恶意 relay 的可用性/隐藏历史边界与伪造成员授权分开。 |
| 文件与归档 | R2 URL allowlist + HTTPS + 禁止跳转 + 流式上限/摘要；RequestFile 对 request/upload/object/version/chunk 做完整绑定；1pux 拒绝 traversal、符号链接、重复成员和超量展开。 |
| 原生秘密挂载 | 补查 `FSKitPeer.swift` 的真实代码签名身份，FUSE 的 kernel uid/fh、取消和 direct IO，以及 core 的窗口/会话/source 重验。签名包和内核缓存/mmap 行为仍未运行验证。 |
| iOS 扩展 | 主动导出的独立 AES-GCM 副本、Keychain ACL、当前 snapshot/有效期检查；FileProvider 逐块及完成前复查会话。真机 entitlement/签名/缓存行为未验证。 |
| 发布 | macOS 构建强制签名公证；运行时同团队验证；更新绑定所选版本/资产、拒绝降级，停止 daemon 未确认不安装。远端 policy 与实际包签名仍未知。 |


### 初次覆盖账本

| 单元 | 范围 | 初次状态 | 当时的源码结论或缺口 |
| --- | --- | --- | --- |
| `account` | 账户/设备变更 | `covered` | check_device 集中检查；successor 限定旧设备用途；AddDevice 跨 commerce await 后重新读取账户和时间；SetApprovalKey 受 AccountControl 硬件政策约束。 |
| `agent` | Agent/MCP 工具授权 | `blocked` | vault 保留内部条目排除、历史 head CAS、隐藏字段不降级、tags 不允许迁移；文件读写按当前 folder 授权；网络 MCP 调用按授予 origin 限制。 runtime.restore 丢弃 snapshot hosts，按配置与 reach 重建；本地 room/draft 列表过滤 origin。 未完全追踪第三方 connector 内部，保持 partial。 |
| `algorithm_matrix` | 多链钱包全部算法/故障矩阵 | `blocked` | 补查：BTC 绑定 address/unique UTXO、checked totals、change回本地址、max fee 与首签输入集合；EVM 固定 nonce/chain；Sol 无 durable nonce 禁重签、nonce mode 固定；general CanisterCall 需要所有受保护 ICP 地址的协签。现有Sol/外链对照向量只阅读未运行。 未完成：运行向量/模拟链时序验证。 |
| `approval` | 客户端审批绑定 | `covered` | approval digest 绑定 id/source/action，决策校验期限/状态/presence；敏感 secret action 限 GUI 或硬件在场 CLI；SecretRun 固定计划、AbortHandle 与 Lost 终态。 |
| `appstore` | Apple 权益回调 | `covered` | 固定治理 verifier；transaction/original ID 使用环境参与摘要；app_account_token 绑定 beneficiary；新增沙盒只允许主动配置年度订阅。状态更新反重放及私有 Apple JWS 验证尚待完整核对。 TRANSACTIONS 检查 signed_at、checked_at 单调；撤销不能重开。 |
| `attest` | 正式认证/执行预算 | `covered` | attest_statement 在 commerce 和 app 授权返回后重新读账户、时间、配置，check_attestation 重做；结果、月度额度、账户及认证叶同步提交。 |
| `channel` | 频道控制/消息 | `covered` | advance 检查 head/ACL、角色、邀请时效、移除换代与 history 可读范围；消息 receipt/digest 与签名 payload 对齐，按历史 control_head 校验作者，按当前 ledger 校验接收人；页序号覆盖完整。 |
| `commerce` | 商业结账/退款 | `covered` | authorize 校验双权威回执与新鲜度；退款合并仅同一原账户，原 block/operation ID 幂等；auth 时间检查优于 wallet 的缺口，作为对照。 |
| `connect` | 网页应用接入 | `covered` | 双方精确 event.source/origin；每连接 nonce 单次使用；operation key 绑定 origin/app/op ID，read 对 session public key 校验；回复 HPKE auth/设备证据及 request/digest/pinned device 校验。 |
| `daemon` | 回环网关与会话 | `covered` | bearer 拒绝任意 Origin；WS owner-only；dispatch 模块与 Session::Agent 双向匹配；Shared 固定首次 with 的账户，Unlocked 再固定 generation。 |
| `dependencies` | 锁定依赖全量源码/漏洞库 | `deferred` | Lockfiles inventoried; reqwest buffering implementation read; no dependency advisory database or exhaustive third-party source audit. |
| `desktop` | Electron 桥接/文件 | `covered` | sandbox/contextIsolation/no nodeIntegration；主窗口与审批窗口 sender 分开；文件 drop 的路径由 preload webUtils 从真实 File 解析。单看 main 接受路径不足以确认任意读取。 CSP default-src none、无网络 connect-src；没有识别到远程内容到 IPC 的可达链。 |
| `external` | 外部应用批准 | `covered` | 固定 service/consumer，registration、home、account、device、epoch 和 record expiry 联合验证；commerce/membership 消费方核对 approval ID/hash、verified_at 和 valid_until。 |
| `files` | 文件加密/下载/导入 | `covered` | R2 host HTTPS 固定、无 redirect、流式大小与 digest；RequestFile 验证 request/upload/object/version 及所有 chunk，标题 safe_name；读取逐块会话检查和总摘要；导出 create_new partial。 |
| `handle` | 名称注册/转移/迁移 | `covered` | 冻结导入先校验再写；旧名 caller 为冻结 owner/admin；精确重放回执；注册 consume 后重读配置/locks，未知扣费保留原请求；转移双方批准且重读名称版本。 |
| `ios` | iOS 扩展/系统导出 | `covered` | AES-GCM 绑定 header/name；Keychain userPresence/biometry；manifest、文件块和 session 复查当前 export ID 与截止；FileProvider 每块及完成前检查会话。签名包/真机验证未执行。 |
| `ipc_files` | WebDAV/SSH 本机服务 | `covered` | SSH home 0700、socket0600、peer uid、32连接与帧大小；签名经过批准；WebDAV 每次检查随机 token、macOS password、当前解锁账户和开启偏好；读取逐块 generation。FSKit/FUSE 另列覆盖缺口。 |
| `legacy` | 旧数据迁移与冻结 | `covered` | 个人 archive 有独立有界 parser、一时 nonce/文件摘要与接收密钥；迁移记录不授权设备/名称；旧名按冻结 owner/admin 校验。旧 TS reader 与 legacy canisters 仅做定位/关联追踪，未逐路径审完。 冻结证明校验 request_id、caller/method/arg、证书 reply、页完整性；旧证明不要求近期属预期合同。 |
| `legacy_full` | 旧网站/冻结 canister 全量路径 | `covered` | 补查：四个 legacy canister 的 update 入口分类，业务入口经 FreezeState begin/writable，Draining 拒绝新票、pending 清零才 seal；例外为只读 token、撤销、登录及 validator。频道读取检查成员，profile 私有投影只给本人；TS reader 固定服务/方法名单，bridge 绑定 origin/target/fingerprint/帧链。结论限冻结/恢复/迁移边界，不宣称旧 Active 业务或所有 UI 页面逐行审完。 |
| `login` | 登录/设备配对 | `covered` | 登录 callback state/末端 session key/TTL/target narrowing；真实 IC query 签名检查；仅固定 loopback 或 iOS HTTPS callback；pairing mailbox 固定 relay，返回密文不自动授予设备。daemon 未授权提前消费会话另列候选。 |
| `login_callback` | 未认证登录回调的状态消费 | `candidate` | callback 独立公共路由；check_callback 在校验 Origin/Content-Type/state 前 take 登录会话，拒绝请求仍消费。 |
| `mcp_jwks` | MCP 上游公钥响应预算 | `candidate` | Agent 可绑定自身 endpoint 的 HTTPS server；McpEvents::keys 使用 response.bytes 完整收集后才比较 MAX_JWKS。10秒 timeout/16 endpoints/1小时缓存不构成字节界限。 |
| `membership` | PANDA 资格与租约 | `covered` | claims.authorize 检查双授权与时间；qualify 用 generation 抵御旧返回，源代码明确任何已列出的 neuron principal 可申请，不把已明确的产品政策误报成缺少经济控制权。未逐一执行 lease 故障矩阵。 |
| `mobile` | 移动核心/授权 | `covered` | ScopedHost transition 先 fence 并限制旧 handle；硬件返回前后 generation 核对；decide UI digest 与当前记录重验；私有授权 token 只在 native adapter/Rust 内部流转。真机/打包证据未执行。 |
| `native_mount_deep` | FSKit/FUSE 原生挂载深度与 OS 验证 | `blocked` | 补查：FSKit socket0700/0600、peer uid/PID 与同团队+bundle ID+entitlement 校验；Linux 使用 kernel uid/fh 与 direct IO；core callback 绑定 session/source generation/取消/单调期限，每块和释放前重查；FSKit 无缓存与 revoke ACK 失败后停止卷。 未完成：平台签名包及内核行为验证。 |
| `other_products` | 无关 PANDA 功能 | `out_of_scope` | Not dMsg source scope. |
| `payment` | 来信托管支付 | `covered` | offer 对账后重查 signer/fee/enablement/deadline；入金 block 唯一；退款绑定原账户；Unknown 维持原时间戳和参数；精确账本交易校验完成对账。 |
| `principal` | 公开身份发布 | `covered` | controller PoP 绑定账户/generation/能力；directory 限 home 与单调版本；同版本异内容拒绝；publish 回调只推进当前记录的发布水位。 |
| `private_cloud` | 私有云端实现 | `out_of_scope` | Public repository audit only; private documents read as baseline, private Workers not audited. |
| `production` | 线上模块/签名发布物/平台政策 | `out_of_scope` | No production probes, accounts, payments, release or hosted configuration checks authorized by this source audit. |
| `protocol` | 编码/证书/签名 | `covered` | CBOR scanner 限深度/节点/长度、拒绝尾字节和不保留字段；AAD/HPKE 分域；认证精确 canister/key/value/root；ledger 原始受信回复索引及最多四跳归档；解码失败保留 Unknown。 公钥拒绝 weak，Ed25519 verify_strict；budget checked_add 且按原预留日结算。 |
| `recovery` | 延迟恢复/根派生 | `covered` | begin/complete 检查 enabled、发起登录、到期区间和设备持有证明；RemoveAuth 撤销待恢复；完成恢复清理旧设备与硬件批准。COSE 派生另待追踪。 COSE execute 同时检查配置 home、account allocator、home_user/home_cose、序号与截止，未知调用不复用新参数。 |
| `release` | CI/打包/依赖 | `covered` | pull_request 与 tag release 分开；Rust 缓存只 main 保存；同次 workflow artifacts；macOS builder 强制签名/公证。mutable action tag/远端 environment policy/依赖 CVE 清单未验证，不作为已确认漏洞。 |
| `secrets` | 秘密交付/挂载/进程 | `covered` | 固定 GitHub HTTPS host/path、no_proxy、禁止 redirects/retry、响应流上限；进程计划固定 argv/hash/cwd/claim/session；扫描使用 cap-std/no-follow、字节/文件数量界限及导入复核。FSKit/FUSE 实机权限未验证。 |
| `storage` | 工作区/锁定/根密钥 | `covered` | LDK/VRK 分域封装；root bundle 对承诺/context/history；lock 清 keys/root/login 并递增 generation；只有显式 background_online 可保留 signing；ScopedHost/Unlocked 复查 generation。 |
| `sync` | 账户同步/本地缓存 | `covered` | 逐条验证签名与签名字段相等，页 cursor 严格递增/through 固定，vault plan 要求恰一块；apply_incremental_page 先下载核对再事务应用。恶意 relay 可隐藏/分叉的设计边界不计作漏洞。 |
| `transfer` | LAN 设备传输与继任交接 | `covered` | 批准绑定目标设备、session 和 manifest；HPKE 封 session key，Ed25519 签 header，方向/序号参与AEAD；仅白名单 lib 前缀转移，钥匙和审批原记录 stay；收尾检查活跃设备。 |
| `updates` | 桌面/CLI 更新 | `covered` | SHA256、macOS 同团队校验先于安装；desktop 精确绑定 feed version/asset、禁止 downgrade；stop 未确认不安装。Windows unsigned 模式显式存在，未检查发布物签名/远端保护。 |
| `vault` | 保险库导入/导出 | `covered` | 1pux 拒绝 traversal/反斜杠/符号链接/重复成员，展开长度及总附件上限；selected archive digest 与 picks 固定；隐藏字段不可通过 Agent 编辑降级，reserved 排除。vault.list 搜索另列独立候选。 |
| `vault_search` | Agent 列表搜索与条目读取隔离 | `candidate` | dmsg_vault_list 把 query 传入 owner 全文匹配，item.matches 包含 notes/非隐藏 field value，read 却需要 tagged grant。 |
| `wallet` | 钱包成员/提案权限 | `candidate` | decide 丢弃 verify_account 返回的 valid_until_ms，随后只检查提案和成员；join/unbind 有相邻的显式截止检查。形成待验证候选 wallet-approval-expiry。 |
| `wallet_hub` | 钱包工厂/升级 | `candidate` | create_wallet 丢弃验证回执，再 await ledger balance，只检查报价过期，不检查 approval/authorization 期限。与 wallet-approval-expiry 同根因，独立入口需回归。 |
| `wallet_sign` | 钱包交易/协签 | `covered` | execute gate 校验非 Viewer、状态、votes；sign_plan 限预算、计划版本与冻结；EVM 构建固定转账 calldata 且费用乘法 checked；SOL sole signer，BTC 绑定 outpoint/amount/script/output。逐算法向量、原生协签及真实广播未执行。 |
| `web` | 网页/SDK 验签与渲染 | `covered` | SDK 明确传入受信 home/root，验证精确 witness 与时间；跨窗口 source+origin；legal Markdown html:false，外链 noopener；未把构建期 SVG/manifest 的 @html 误报为用户 XSS。其他旧网站页面只做入口检索。 |


未完成的全仓库检查仍包括：第三方 connector/依赖源码与漏洞库全量审查、全平台原生挂载/签名包/内核缓存行为、旧 Active 业务和所有历史 UI 路径、完整算法/故障矩阵、私有 Worker 实现、真实发布物及线上政策核验。本次没有执行 Electron/Swift/iOS 真机检查或模糊测试，也没有据此新增放行结论。

署名：Codex。
