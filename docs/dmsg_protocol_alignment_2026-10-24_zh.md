# dMsg 协议对齐与使用流程审查 · 2026-10-24

## 审查基线

本轮以本地已提交的 Agent Protocols `13257c1` / SDK `0.12.6` 为规范基线，对照本地 dmsg-cloud checkout 的公开接口行为，审查 ic-panda `01b1de4b` 上的 dMsg canister、Rust 客户端、daemon/MCP、TypeScript SDK 与桌面入口。未核对远端部署，结论不代表生产环境已升级。

公开规范的八份 JSON、来源提交、文件摘要及 MIT 许可保存在 [规范向量目录](../tests/vectors/agent-protocols/README.md)。本轮接续工作区已有的 SDK 升级；未改变 canister Candid 或稳定布局。

## 已修复的问题

| 问题与实际影响 | 修复与验证位置 |
| --- | --- |
| 依赖已升级但规范向量与边界测试仍旧，Identity nonce 边界出现失败，后续 SDK 升级容易漏同步。 | 同步共享向量与上游测试；新增离线版本/摘要校验，纳入 CI 与 `make test-dmsg`。 |
| Mail 发布生成密钥后才由宿主保存；进程在中继接受后退出，会留下无法解密的邮箱。 | 发布前原子提交连接器与加密密钥检查点，重启先恢复，正常密钥存储成功后清除；TLS 故障注入覆盖发布前失败、接受后失败与工作区删除。 |
| 带 `host` 的 owner mailbox inventory 被当作本地 inspection，恢复记录不能可靠保存，实际密钥可用性也未传入只读清单。 | 远程清单走鉴权、串行与持久化路径，本地清单只读并传入真实密钥；真实 daemon 用例覆盖元数据丢失、分页恢复与再次重启。 |
| 桌面没有恢复入口，缺钥邮箱仍能点击续期，后台也可能为不可收取的邮箱订阅推送。 | 新增逐页恢复、重复 cursor 防护和取消保护；显示缺钥/待发布状态，缺钥时支持轮换或关闭，后台仅订阅可用的开放邮箱。六种语言同步。 |
| 主人查看公开 Agent Profile 的路径未持久保存最大 nonce，重启或切换服务后可能展示旧资料。 | 严格验证最新签名 envelope，持久保存 pin，拒绝回退、同 nonce 冲突与跨 origin discovery；多服务和重启测试覆盖。 |
| controller 在多设备 nonce 冲突后直接使审批失败，要求用户重复审批相同操作。 | 明确的 `409 nonce_not_greater` 与合法有界 header 最多触发一次重签；重试决定先写 journal，审批摘要保持不变，未知结果仍重投原字节。旧 journal 可读。 |

Mail 检查点使用既有加密原语和按工作区、Agent、用途隔离的密钥，不把明文私钥写进连接器数据库或 MCP 输出。恢复中继记录不恢复历史明文；解密密钥缺失时仍需恢复本地密钥或轮换后接收新邮件。

## 协议覆盖

- Identity：Agent ID、签名、严格 JSON、重放窗口和 nonce 上界；宿主继续禁止重定向。
- Agent Delegation：principal/controller 文档、状态验证、审批 journal、确定拒绝与未知提交结果的不同恢复方式。
- Agent Profile：完整 envelope 验证、持久 pin、多服务读取与已配置 origin。
- Agent Discourse：共享记录链与边界向量；真实 daemon 的房间、草稿、head 冲突、权限和 MCP 会话流程。
- Agent Knowledge：共享 schema/vector 与批量类型导入边界；真实 daemon 的上传、引用与读取。
- Agent Mail / local connector：邮箱发现与 owner inventory、密钥持久化、未知发布恢复、收发、阅读状态、webhook 与后台 MCP 通知。

## 验证记录

| 检查 | 结果 |
| --- | --- |
| `python3 scripts/sync-agent-protocols.py --check --source /path/to/agent-protocols` | 通过，八份规范 JSON 与 MIT 许可一致。 |
| `cargo test --locked -p dmsg_agent` | 84 项通过，含 HTTP/TLS 故障与重启用例。 |
| `cargo test --locked -p dmsg_client -p dmsg_protocol` | 客户端 263 项、协议 100 项与 3 项 doctest 通过；客户端 5 项环境相关用例仍为 ignored。 |
| `cargo test --locked -p dmsg_cli --bin dmsg` | 后续排查后 66 项通过、2 项挂载用例 ignored；真实 Secure Enclave 创建测试保持启用并通过，见下方补充。 |
| `cargo clippy --locked -p dmsg_agent -p dmsg_client -p dmsg_protocol -p dmsg_cli -p dmsg_integration --features dmsg_integration/pocketic-tests --all-targets -- -D warnings` | 通过。 |
| `make test-dmsg`（未分片） | 通过：原生测试、Clippy、release Wasm、Candid 对照、字节向量及 PocketIC。control_plane 156 通过 / 24 ignored；directory 4 通过 / 1 ignored；signin_with 4 通过。 |
| 真实 daemon + 本地服务的 6 项 desktop 集成测试 | Agent 授权/Discourse、Mail/Knowledge、webhook/native events、后台收信/MCP 通知、PrincipalDocument 均通过。连接器用例修正旧 CLI 参数后定向重跑通过，含邮箱元数据恢复与第二次重启。 |
| TypeScript `dmsg-sdk` build/test 与真实 directory Wasm 证明 | 通过；单独导出 PrincipalDocument 证明后，对应独立验证器 8 项全部通过，补齐常规 suite 中需要该 fixture 的跳过项。 |
| 桌面 `check`、`test`、`build` 与 Electron smoke | 通过，104 项单元测试；smoke 显式关闭 macOS 原生全屏检查，检查了恢复页面截图。修正了一处测试等待 WebSocket session 事件的竞态。 |
| `dmsg-connect` check/test/build | 通过，构建使用测试配置。 |
| `dmsg_frontend` check/test | 现有未提交的 `HeroArt.svelte` 改动导致 1 个 `exactOptionalPropertyTypes` 错误，及 i18n coverage 的 7 个缺失文案；测试 93 通过 / 1 失败 / 1 跳过。首页文件未修改、未纳入本提交。 |

六项真实服务用例名称为 `connector::desktop_connector_profile_and_mail`、`desktop_agents`、`desktop_mail_knowledge`、`desktop_mail_webhook_and_native_events`、`desktop_mail_background_receives_and_notifies_mcp`、`desktop_principal_documents`，通过 `cargo test -p dmsg_integration --features pocketic-tests --test desktop -- --ignored --exact <names> --test-threads=1` 显式启用。使用 `DMSG_BIN`、`DMSG_WASM_DIR` 和项目约定的 `DMSG_CLOUD_DIR` 指向本机测试构建。

所有服务集成测试使用临时本地服务与测试账户。未执行生产环境冒烟、部署、发布、系统凭据交互或原生挂载/全屏验证；其他显式 ignored 场景不能视为通过。

## Secure Enclave 失败补充排查

原失败时段的 macOS CryptoTokenKit 日志明确记录：测试进程创建 SEP 密钥时，`ctkd` 返回 `0xe00002e2`（`kIOReturnNotPermitted`）。同一次调用的生物识别能力检查为 YES，因此“设备支持”不能证明“此刻允许创建”。日志能确认系统拒绝发生在创建密钥阶段，但不足以确定具体是锁定、系统服务状态还是其他底层条件；未将推测写成锁屏结论。

在未修改实现的情况下，原硬件测试连续五次通过，完整 CLI suite 65 项通过；相同 Swift 编译配置下，主线程与后台线程的密钥协商/签名密钥创建也通过。因此本轮没有削弱访问控制、改用软件密钥、加入自动重试或跳过原硬件测试。

修复了诊断信息被吞掉的问题：Swift 桥接保留 `NSOSStatusErrorDomain` 的负数状态，Rust 仍按 `Unavailable` 处理并携带状态码；其他 domain 不会误占成功、用户取消或数据完整性错误的桥接值。没有输出错误的 `userInfo`、密钥或句柄。真实密钥创建测试、原生错误映射回归、完整 CLI suite 与 Clippy 在修改后再次通过；文档增加了硬件测试和日志定位方法。交互式 Touch ID/密码解锁、签名仍未验证，本次只验证无交互创建与错误边界。
