# 凭据受控使用 P0：交付与验证记录

日期：2026-10-24。实现分支：`codex/secrets-p0`，起点 `01b1de4b`。

P0-01～P0-07 的代码、界面和公开合同已完成。本地自动化验证如下；**没有发布部署，不宣称真实 GitHub、系统在场确认和完整私有云端集成已验收。** P1 的原生 Apple FSKit、Linux FUSE、CLI 注入、SSH 与分享尚未实现，团队共享秘密库仍暂缓。

## 交付对应

| 工作包 | 实现入口 |
| --- | --- |
| P0-01 引用合同 | [secrets 核心](../src/dmsg_client/src/secrets/mod.rs)、[独立编码向量](../tests/vectors/client/secrets-v1.json) |
| P0-02 Agent 收敛 | [daemon 工具](../src/dmsg_cli/src/agents.rs)、[capability](../src/dmsg_client/src/agents.rs)、旧字段/标签回归测试 |
| P0-03 本地策略 | LDK 封存、版本、配置审批、可选 grant 原子提交、禁用；[owner RPC](../src/dmsg_cli/src/secret_methods.rs) |
| P0-04 审批与执行 | [原审批中心](../src/dmsg_client/src/approval.rs)、[锁外执行及不重放](../src/dmsg_client/src/secrets/run.rs) |
| P0-05 固定适配器 | [GitHub adapter](../src/dmsg_client/src/secrets/github.rs)：固定 origin、单次 POST、响应白名单 |
| P0-06 MCP 与 UI | [工具 schema](../src/dmsg_agent/src/tools.rs)、[CLI](../src/dmsg_cli/src/secret_commands.rs)、[策略/记录页](../src/dmsg_desktop/src/renderer/views/secrets/Policies.svelte)、[审批全文](../src/dmsg_desktop/src/renderer/views/secrets/Review.svelte) |
| P0-07 测试与文档 | [核心行为测试](../src/dmsg_client/src/secrets/tests.rs)、[Electron smoke](../src/dmsg_desktop/scripts/secrets-smoke.mjs)、[公开合同](protocol/secrets_zh.md)、各包 README 和中英文架构 |

没有修改 canister 或稳定布局；没有新增同步内容类型、独立执行账本或 MCP 使用会话握手。`misc` 方案继续遵循仓库忽略设置，没有强制加入 Git。

## 首次 P0 实际执行

本节为首次实现的历史记录；最新提交前审查与完整运行结果见文末。

环境：macOS 27.0.1，Rust 1.98.1，Node 26.8.1。桌面复用已有依赖目录，没有做全新依赖安装；本地生成的构建和截图不作为源代码提交。

| 命令（仓库根目录，另注者除外） | 结果 |
| --- | --- |
| `cargo check -p dmsg_cli` | 通过 |
| `cargo test -p dmsg_client -p dmsg_cli -p dmsg_agent -- --skip enclave::tests::creates_keys_without_asking` | 413 项通过，7 项既有 ignored；明确跳过下述 1 项失败的硬件测试。包括 client 277、CLI 65 及 Agent/协议向量测试 71 |
| `cargo test -p dmsg_client secrets` | 最近专项运行 16 项通过（含 1 项既有 OTP 测试）；其后完整回归也通过 |
| `cargo clippy -p dmsg_client -p dmsg_cli -p dmsg_agent --all-targets -- -D warnings` | 通过 |
| `cargo fmt -p dmsg_client -p dmsg_cli -p dmsg_agent --check` | 通过 |
| `cargo check -p dmsg_integration --test desktop --features pocketic-tests` | 通过；只编译，未启动 PocketIC/私有云端服务。首次漏 feature 的命令未执行检查，已以正确 feature 重跑 |
| `pnpm --dir src/dmsg_desktop check` | 通过，Svelte 0 errors / 0 warnings，TypeScript 检查通过 |
| `pnpm --dir src/dmsg_desktop test` | 104 项通过 |
| `pnpm --dir src/dmsg_desktop build` | 通过 |
| `node scripts/secrets-smoke.mjs`（在 `src/dmsg_desktop`） | 真实 Electron 窗口与 IPC 对接本地 mock，通过 |
| 公开网站六语言目录的 TypeScript 语法与旧 capability 文案核对 | 通过；未运行整站完整构建 |
| `git diff --check`、方案 JSON/代码围栏/55 个验收编号检查 | 通过 |

实际 Cargo 命令复用了已有 target 缓存；表中省略机器上的绝对缓存路径。首次回归发现的工具总数测试（75→79）、policy ID 样本和审批索引读取回归已经修复并重跑通过，没有把失败的初次执行计为通过。

## 失败与未完成的检查

未排除硬件测试的完整命令中，`enclave::tests::creates_keys_without_asking` 失败：其 `PresenceKey::create` 返回 `Unavailable("secure enclave")`。该测试文件未修改；没有把失败改成 ignored 或改动系统安全设置。**该历史失败在本轮提交前复核中通过，见文末。** 测试创建密钥不等于真实 Touch ID/系统密码确认。

没有使用真实 token，也没有创建任何真实 GitHub issue。未运行 `desktop_agents` 的完整私有服务/PocketIC 集成、生产 TLS/代理环境实测、完整 Windows/Linux 桌面回归或发布包签名/公证验收。生产发布前仍需这些适用的人工及环境验收；当前结果仅证明所列本地测试。

## 验收证据与边界

下表记录已有自动化证据，不把一个测试名字当作该验收项所有人工组合都已完成。

| 方案验收 | 实际证据 |
| --- | --- |
| A01、A02、C01 | `reference_vector_has_independent_cbor_and_json_encoding` 比较独立生成的 CBOR/摘要与 URI；引用、当前 revision、删除、跨账户/保留项、冲突测试 |
| A03、A04 | daemon `agent_vault_write_never_shows_a_hidden_value`：按 ID/label 转明文被拒，既有标签变更被拒，旧 secret=true/false 被拒，换值/删字段/新建仍可用；capability 明确拒绝旧 vault.secret 与新 secret.use 通配符 |
| A05、A06 | `request_ownership_limits_recovery_and_configuration_are_enforced`、daemon owner/MCP 流程：无 grant 拒绝、另一 Agent 不能查询/取消、伪造 actor 输入被拒；未运行新流程的两个真实 stdio MCP 子进程场景 |
| A07、A09 | `stale_revision_and_session_stop_before_dispatch`：改字段、锁定后解锁、凭证轮换、grant 撤销、策略禁用都不能进入发送函数；既有 Hold 会话隔离测试也通过 |
| A08、A11 | 完整计划摘要变更测试、模拟在场确认拒绝；Electron 显示长正文全文、HTML 原样文本、自述理由标签、高风险按住确认；真实系统在场确认仍未通过环境验收 |
| A10 | 同键同输入返回原请求，异参冲突，重复批准拒绝；待审批来源限额、同 Agent 执行限额与完成后释放 |
| A12、A13、C03 | 慢发送期间客户端仍可锁定，取消网络等待；SQLite trigger 注入提交点前写失败与成功响应后的结算失败；原审批 started/state 和对账不重放断言；未做完整生产进程/网络故障演练 |
| A14、A15 | 本地 TCP HTTP mock 检查真实请求头、201、307 不跳转、503 不重试、超大 Content-Length、超时；固定生产 origin/path/header、无环境代理及 TLS 配置经源码核对；未联系真实 GitHub |
| A16、A17、C04 | HTTP 回显阻止、Debug 中认证头敏感标记、白名单结果、MCP 输出不含注入值、真正 SQLite/WAL 文件扫描；元数据请求没有取密结果 |
| A18 | Electron 创建策略→配置批准→使用批准→lost→人工核查→立即禁用；renderer 无权调用 approval/respond；本设备记录与不重试说明可见 |
| C02 | 策略/grants/结果封存与另一设备无本地策略测试；现有 Vault/Files 核心回归；没有改动云端备份、共享和恢复数据模型 |
| C05 | 上述 fmt/clippy/单测、桌面检查/构建/Electron 和集成编译记录；环境失败和未运行项见上一节 |

Electron mock 负责模拟 GitHub 结果不明，GitHub adapter 由独立 Rust HTTP mock 验证。两者不是一次真实 GitHub 端到端演练。SQLite 测试使用唯一人工 token，不使用生产凭据；明文只出现在测试发送端预期检查的内存。

## 操作与后续

桌面进入“签名与授权 → 凭据使用”，选择隐藏字段、精确 GitHub owner/repo，以及可选的 Agent。配置批准只授予持续至撤销的**申请权限**；每次发布仍单独高风险审批。Agent 经四个新 MCP 工具列策略、申请、查询或取消。`lost` 必须先在 GitHub 人工核查，不自动再次提交。

P1 的平台选择与授权语义仍由本地 `misc` 方案记录，不是本 P0 实现的公开能力；不能从 P0 的通过记录推断挂载后端已实现或已验证。

本地完整运行日志保留在本 worktree 的 `misc/p0-validation/`（Git 忽略），公开文档只保留命令、结果与必要边界。


## 2026-10-24 提交前审查

范围为本分支相对 `01b1de4b` 的全部变更（包含此前未跟踪的新文件），另沿调用链检查 daemon 鉴权、Vault 版本操作、审批中心、capability 和桌面 IPC。由当前 agent 独立完成，未使用 subagents；这不是对整个仓库或生产部署的安全证明。

确认并修复两处问题：

1. **历史 base 绕过当前隐藏规则。** 已知旧 revision 且持有对应标签读写能力的 Agent，可通过编辑历史明文字段版本取得当前已经隐藏的值。最小本地测试在修复前失败，修复后通过。Agent 的 `base` 现为当前 revision 的并发校验，过期返回 `VERSION_CONFLICT`，不再借写入读取历史内容；owner 的历史查看和冲突处理不变。正常当前版本编辑、隐藏值遮罩及非法 id/base 类型均有回归断言。
2. **旧待审批请求可在恢复权限后重新有效。** 暂停再恢复，或撤销后重新授予同名 capability，原实现只检查当前权限，不能证明请求属于原授权。使用计划现在固定具体 grant ID 和暂停代次；恢复/重授不使旧请求恢复，新的使用必须创建新计划。暂停/恢复的反例在修复前复现，修复后两种变更都经过测试。

另作一项防御性收紧：分开发送前失败与发送后结算。发送后除了适配器明确识别的 HTTP 拒绝，未识别错误一律为 `lost/EXECUTION_UNKNOWN`；不让未来适配器错误变化误报“确定失败”。本轮未发现当前生产路径自动重发 POST 的反例；通过注入未知错误、遗留 executing/unknown、重复幂等键及人工核查标记，验证它们均不会重新发送。

新增真实回环 HTTP 鉴权测试：Agent 凭证访问全部九个 owner secrets RPC、approval/respond、grants/add 均被拒；伪造 human/source 不改变结果；带网页 Origin 的 owner 请求也被拒。普通字段/备注仍是明确授权的可读内容，工具说明不再泛称所有“秘密”都能被自动识别和隐藏。

审查后最终运行 `cargo test -p dmsg_client -p dmsg_cli -p dmsg_agent`（不使用 skip）：Rust **419 项通过**、7 项既有 ignored、0 项失败、0 项额外过滤，包含 Secure Enclave 测试。桌面 **104 项通过**，类型检查、构建、Electron 流程、严格 Clippy、格式检查及桌面集成测试编译均通过。前述首次 P0 的 413 计数及曾排除硬件的审查中间轮 418 计数保留为历史记录；本段是最终提交前结果。

单独重跑的 Secure Enclave 测试本次通过；原先的 `Unavailable("secure enclave")` 作为历史结果保留，没有修改该测试来跳过失败。测试通过不等于真实 Touch ID/系统密码交互已经验收。真实系统在场确认、真实 GitHub 与完整私有云端/PocketIC 运行仍未验收。原始失败、修复后日志与本次审查覆盖记录保留在被忽略的 `misc/p0-review/`，未提交任何生产凭据或私有设计正文。
