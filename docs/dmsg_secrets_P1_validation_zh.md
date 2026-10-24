# dMsg Secrets P1 实现与验证

日期：2026-10-24。实现分支：`codex/secrets-p0`；P0 基线：`be486681`。本文件区分代码实现、测试证据和发布门禁；不代表已部署。团队共享秘密库仍暂缓，不引入 macFUSE。

## 实现范围

| 工作包 | 入口与实现 | 边界 |
| --- | --- | --- |
| P1-01 | Apple FSKit 原生模块和独立合成探针；Linux `fuse3` 异步 FUSE 回调（包含内核 interrupt） | macOS 当前源码针对 macOS 27 / Xcode 27；没有验证旧 SDK。正式模块仅配置开发签名，按用户要求未启用 |
| P1-02 | `dmsg run --env-file .env.refs --file NAME=URI -- executable args...` | owner CLI 单次领取；无 renderer/MCP 领取接口；终端与后代可以保留已交付值 |
| P1-03 | `secrets mount prepare/save/start/status/revoke/stop` | Linux 固定句柄；macOS 固定文件、版本、UID、至多 60 秒窗口；每次读取校验当前来源 |
| P1-04 | 统一使用中心的文件映射、显式项目选择、项目根目录软链接、窗口状态与撤销 | 不覆盖现有文件；来源修改后停止并要求重新启动；不承诺 IDE 自动刷新或热重启 |
| P1-05 | 私有 Unix SSH Agent socket；Ed25519 身份与逐次 SSH/Git 签名审批 | SSH 必须验证未转发的 session-bind；Git 只接受 SSHSIG `git` namespace；不支持加密私钥、转发和任意消息签名 |
| P1-06 | 字段/备注/固定附件选择、独立密钥快照包、加密上传、一次发布、原 ID 对账及撤销 | 默认全未选。当前入口为链接；格式可作为普通频道文件承载，未新增团队库或自动同步 |
| P1-07 | 浏览器完整解密校验后逐字段显示/逐附件保存；客户端凭链接选择导入 | 新本地 ID；附件仅本地；不提供浏览器整包明文下载；领取会消耗原合同的下载会话 |
| P1-08 | 本机健康观察、明确目录扫描、导入审批和轮换记录 | 不上传扫描内容，不跟随符号链接，不删除源文件；上游撤销只记录用户报告 |
| P1-09 | 1pux 选定附件流式加密、逐项遗漏、源文件摘要、Vault/Files/outbox/收据事务 | Passkey、历史和无法识别的字段明确报告；不宣传完整迁移 |
| P1-10 | 桌面使用中心、所有秘密审批活动、CLI 与公开合同 | `lost`、`revoke_pending`、来源变更、平台不可用均保留失败含义 |

## 权限与结果边界

- 取密仅发生在获批的交付边界。MCP 没有进程领取、扫描路径、挂载读取、SSH 签名或分享密钥接口；桌面 renderer 不能提交任意扫描/挂载路径，也不能调用进程领取。目录选择由 Electron main 的系统对话框产生，保存使用原会话中的一次性预览。
- 进程领取在返回值之前写入不可重复领取状态。可执行文件身份、完整 SHA-256、参数、工作目录、refs 文件及当前凭据版本固定；解释器、动态库、项目代码、子进程输出不由可执行文件摘要证明。
- SSH 私钥留在核心。签名绑定原连接、用途、公钥、当前 revision 和解锁会话；无效或转发的 session-bind 关闭连接，不能回退到 Git 签名。签名记录只证明签名交付，不证明登录成功。
- macOS IPC 验证扩展的 bundle ID、FSKit entitlement 和与 daemon 相同的签名团队。此身份是文件系统模块身份，**不是读取项目文件的应用身份**。窗口内同 UID 的其他程序也可能读取，审批必须明确告知。
- macOS 和 Linux 的期限使用包含休眠时间的单调时钟。读、seek、重开不会续期。锁定先关闭内容权限，再尝试 OS 卸载；卸载或缓存撤销未确认时返回错误，不能显示为已安全卸载。
- 快照的 share/upload/snapshot/file-version ID 与密钥在准备时固定；发布尝试前持久化提交点，任何不确定响应只查询原 share ID。通用审批和存储恢复器不得再次发布。并发撤销会隐藏链接并保留待撤销状态，直到云端确认。
- 接收完整外层 AEAD/SHA-256 与内层 manifest/附件摘要校验成功后才预览。导入的明确选择、内容提交和收据绑定；重复选择返回原收据，改变选择不能复用已经提交的导入。外部导出也按完整源摘要及选择保存事务收据。
- 云端分享次数是会话次数，不是阅读次数。浏览器领取与客户端导入各占一个会话。撤销不能收回已下载副本，也不承诺立即使已签发下载 URL 失效。

## 挂载能力与测试边界

| 环境 | 已取得的证据 | 尚不能据此推断 |
| --- | --- | --- |
| macOS 27.0.1，Xcode 27.1，自研合成 FSKit 探针 | 有效 FSKit Module 描述文件；通过系统“按类别 → 文件系统扩展”启用；只读挂载和真实大小 1,350,028 字节；首次读取等待约 5.337 秒；同 UID 第二进程窗口内成功；随机读取成功；期限后旧 fd 被拒绝，新打开被拒绝，写入返回只读错误 | 正式 Vault/daemon 模块的内核联调；macOS 跨 UID 测试（本机 sudo 需要交互密码，未运行）；所有 IDE / Vite / Next.js 的读取模式 |
| macOS mmap 探针 | 到期后已返回页仍可读取；此前未触及的远端页在撤销后产生 SIGBUS | mmap **不列入支持接口**；不能声称已交付内存被清除 |
| 隔离 Linux，OrbStack Linux 7.0.14，独立 FUSE 探针 | 首句柄与第二句柄分别等待约 5 秒；到期拒绝；另一 UID 拒绝；随机读取和实际 size 正确 | 正式 daemon 后端的测试须单列，不用探针替代 |
| 正式 Apple FSKit 模块 | Swift 编译及 Xcode 开发签名完成；有效模块描述文件 | 用户明确要求“仅配置签名，暂不启用”；正式模块未启用、未挂载。Developer ID 分发、公证和已安装模块升级/卸载未运行 |

项目只创建可选的根目录别名，例如 `.env → .dmsg-secrets/.env`、`service.json → .dmsg-secrets/service.json`。删除或停止挂载后别名可能悬空；不会写明文替代文件。请将挂载目录和别名加入 Git、IDE 索引及备份排除规则。dMsg 无法阻止同用户备份器在已授权窗口内复制内容，也不擅自重写项目配置。

## 自动化检查

未写明通过的项目不得视为通过。最终执行记录见下表。

- Rust：核心权限、版本、单次领取、SSH 协议、挂载范围、连续时钟、快照容器、发布响应丢失和导入收据；daemon 与 CLI；`fmt` / `clippy`。
- 桌面：类型检查、权限桥接单测及真实 Electron 对本地合成 daemon 的审批/分享/导入测试。
- 浏览器：Rust/TypeScript 共用向量、容器边界和纯文本展示；Svelte 检查与构建。
- OS：隔离 Linux 正式 daemon/FUSE、Node/Python/Rust 读者、OpenSSH 登录、Git SSH 签名及 `verify-commit`；macOS Git 验证。

## 执行结果与复现命令

| 检查 | 命令/入口 | 结果 |
| --- | --- | --- |
| Rust 格式 | `cargo fmt -p dmsg_client -p dmsg_cli -- --check`、`git diff --check` | 通过 |
| macOS Rust 静态检查 | `cargo clippy -p dmsg_client -p dmsg_cli -p dmsg_agent --all-targets -- -D warnings` | 通过 |
| 核心 | `cargo test -p dmsg_client --lib` | 304 通过，5 个向量生成/大规模性能测试按原标记忽略 |
| Agent 核心与协议向量 | `cargo test -p dmsg_agent`（与客户端组合运行） | 71 通过 |
| macOS daemon/CLI | `cargo test -p dmsg_cli --bin dmsg -- --skip enclave::tests::creates_keys_without_asking` | 70 通过；3 项忽略；硬件测试单独运行，不计作本行通过 |
| macOS Secure Enclave | 单独运行 `enclave::tests::creates_keys_without_asking` | 单独重跑通过；全量运行中曾失败一次，见下文 |
| macOS Git | `cargo test -p dmsg_cli --bin dmsg secret_integration_tests::real_git_ssh_signature_verifies_with_the_public_key -- --ignored --nocapture` | 真实 `git commit -S` 与 `git verify-commit` 通过；临时合成密钥 |
| Linux 静态检查及 CLI | 容器内 `cargo clippy -p dmsg_cli -p dmsg_client --all-targets -- -D warnings`；`cargo test -p dmsg_cli --bin dmsg` | 静态检查通过；68 项通过，3 项 OS 集成测试另行运行 |
| Linux OS 集成 | `cargo test -p dmsg_cli --bin dmsg secret_integration_tests -- --ignored --nocapture` | 3 项通过：真实 OpenSSH 登录、Git 签名/验签、正式 daemon/FUSE 的项目 `.env` / 普通文件读取、阻塞批准、UID 与取消/撤销 |
| 桌面 | `pnpm --dir src/dmsg_desktop check/test/build` | 检查与构建通过；106 项测试通过 |
| Electron | 构建后在桌面目录运行 `node scripts/secrets-smoke.mjs` | 真实窗口和本地合成 daemon 通过：完整原文、按住批准、未知结果、选择字段发送/导入、同用户窗口提示、renderer 边界 |
| 浏览器 | `pnpm --dir src/dmsg_frontend check`；分享/i18n 的 Vitest；`pnpm ... build` | 检查/构建通过；31 项通过，真实云端链接测试因未提供 live fixture 跳过 |
| 正式 FSKit | `native/fskit/build-module.py --output <外部目录> --team <团队>` | Xcode 构建、有效开发描述文件及 codesign 核验通过；没有启用/挂载 |

Linux 实测为隔离容器中的 root 测试账户：Linux `7.0.14-orbstack`、Rust `1.98.1`、Python `3.11.2`、Node `18.20.4`、OpenSSH `9.2p1`、Git `2.39.5`。测试没有挂载真实用户 home 或访问真实秘密库。正常非 root 发行版、Windows 编译、其他 macOS/SDK 组合未运行；不能把该容器结果扩展为所有发行版认证。

两个原有 macOS WebDAV 原生测试仍按原标记忽略；本轮没有用它们替代 FSKit 测试。`fuse3` 的内核 interrupt 与原请求使用相同取消标记；实际进程退出后批准不能生成旧读取 grant。

## 验收映射与剩余检查

| ID | 已覆盖证据 | 剩余边界 |
| --- | --- | --- |
| B01–B04 | `process_refs_reject_shell_duplicates_limits_and_unknown_fields`、`process_claim_is_once_bound_to_approval_revision_and_session`、`executable_and_reference_file_changes_are_checked_before_spawn`、`inherited_pipe_is_sequential_and_exit_code_is_preserved` | Ctrl-C/后代进程的完整人工矩阵未运行；程序及其依赖受信，摘要不是代码执行证明 |
| B05–B08 | SSH 协议/签名/断连/源变更测试；Linux 真实 SSH 登录；macOS/Linux 真实 Git 验签 | Windows named pipe 暂缓；转发和通用签名不支持 |
| B09–B13 | Rust/TS 包向量、坏边界/摘要；所选内容；来源改版；`lost_publication_response_queries_original_id_without_create_replay`；导入选择/收据/会话测试 | 真实云端上传、计费、会话限额及撤销完整联调未运行；不以 HTTP mock 代替 |
| B14–B16 | 风险规则、revision 绑定忽略、轮换步骤；受限扫描链接/源变更/取消测试 | 上游凭据创建、验证与撤销仍为用户报告 |
| B17–B19 | 1pux 选定附件流、逐条遗漏、路径拒绝、源摘要；Vault/Files/outbox/收据事务与故障回滚；索引不泄露源摘要测试 | 不支持的 Passkey/历史等不会被悄悄迁移；没有生产导出文件进入测试 |
| B20 | 统一 UI、CLI、Electron、权限桥接及失败状态 | 复杂真实网络恢复需与云端联调 |
| B21–B24 | Linux 正式 FUSE 读者与真实 size；macOS 合成 FSKit 延迟读取；dotenv/路径核心测试 | 正式 FSKit 被用户要求保持未启用；不能认定其项目内核联调已通过 |
| B25–B30 | 核心版本/UID/句柄/窗口/连续时钟/取消/会话测试；Linux 内核 UID/取消/撤销；macOS 合成缓存/mmap 探针 | 正式 macOS 缓存、跨 UID、描述符转交、崩溃重启矩阵未运行；mmap 不支持 |
| B31–B32 | 不覆盖路径和别名；签名及模块打包验证；明确排除/复制边界 | IDE/Vite/Next.js、正式分发公证、升级/卸载、非 root Linux 发行版矩阵未运行 |
| C01–C05 | 共享向量、旧功能回归、读取期间不持网络锁、metadata-only 桥接、密文持久化/索引与原结果对账测试；本报告保留全部失败和未运行边界 | 不声称所有生产发布门禁完成 |

## 发布门禁

代码和 mock 通过不等于全部 B01–B32 通过。发布 macOS 挂载前，仍需在用户同意启用正式模块后执行跨 UID、真实审批阻塞/取消、失联与缓存故障、来源改版、休眠、升级和卸载矩阵，并完成分发签名与公证。其他 OS/SDK 组合没有相应证据时应保持不可用。不会为测试降低 macOS 启动安全策略。

本轮不触及 `dmsg_user` 或 `dmsg_directory` 的稳定存储布局，没有生产迁移或部署。


## 额外审查结论

- Linux 生产适配器使用支持 `interrupt(request_unique)` 的 `fuse3`，将内核取消直接连接到核心审批/读取检查的原子取消标记；不使用旧合成探针所用 `fuser` 的取消行为作为生产证据。未知 request unique 返回 EAGAIN，让内核按协议处理取消先于原请求的竞争。内核语义参考 [Linux FUSE 文档](https://www.kernel.org/doc/html/latest/filesystems/fuse/fuse.html)。
- 所有挂载共享最多五个待审批名额，给其他 owner 操作保留全局队列空间；文件系统回调仍受独立并发与 30 秒等待限制。
- 进程领取和云端客户端禁用隐式 HTTP 重放与跳转；新解锁会话不能接续旧的快照导入工作。扫描导入也在读取源文件前再次验证原审批会话。
- 原生 FSKit 的 access/open 回调提供 UID/GID，read 回调不提供独立读取者身份。已打开描述符的主动跨 UID 转交未验证，不把普通路径权限测试扩展成对该场景的保证。
- 本轮出现一次原有 `enclave::tests::creates_keys_without_asking` 的 `Unavailable("secure enclave")`；单独重跑通过，当前出现不稳定结果，根因未确定。该次失败仍保留，不能据重跑宣称 Secure Enclave 发布门禁已经通过。

- 文件附件沿用 Files 文件夹授权；导入明确选择目的文件夹并提示已有 Agent 读取权限。机密挂载不会替代或收窄原 Files 的授权。这与 Vault 隐藏字段不可通过普通 MCP 读取是两个独立边界。

- 外部导入收据以 LDK 派生的索引查找，不将可猜测源文件的无密钥摘要放入 SQLite 明文 key；重复导入仍返回原事务收据。
