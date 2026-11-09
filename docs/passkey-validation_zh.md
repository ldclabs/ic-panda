# Passkey 登录验证记录

日期：2026-11-09。范围：`ic-panda@13109976` 之后本次提交的 `dmsg_auth`、共享 Rust 会话、桌面/CLI、登录网页与 iOS 接入。源码与本地测试结果不等于生产发布或真实硬件验收。

## 已执行的检查

| 范围     | 结果与边界                                                                                                                                                                                                        |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust     | `dmsg_auth` 4 项、共享客户端 601 项、CLI 120 项、mobile 28 项、protocol 89 项及其集成测试 20 项、文档测试 3 项通过。客户端 5 项和 CLI 6 项原有忽略测试未执行。                                                    |
| Clippy   | 五个修改包，以及启用 `pocketic-tests` 的全部集成测试目标，均按 `-D warnings` 检查通过。                                                                                                                           |
| canister | release Wasm 构建、Candid 与提交文件一致性、2 项真实 PocketIC 测试通过；覆盖 IC 的 P-256 ingress 验签、canister → P-256 → Ed25519 委托链、targets 限制、升级、180 天后续期、16 会话上限及退出后重放拒绝。         |
| 账户绑定 | 既有 PocketIC 回归 `a_login_binds_only_by_accepting_an_administrator_approval` 通过，验证管理员批准和新身份接受后仍路由到原 AccountId；不代表两端真实 Passkey 首次登录已验收。                                    |
| 共享会话 | 并发失败只执行一轮续期、后续操作可重试、网络失败保留本机内容和凭据、锁定时不等待网络且拒绝迟到结果、未绑定工作区不保存长期登录均有回归覆盖。                                                                      |
| 桌面     | `pnpm check`、188 项测试、控件审查和构建通过。Electron 隔离 fixture 检查主登录、II 选择、创建失败重试、原账户绑定入口、退出全部、满额后的新设备退出入口、过期登录提示及键盘操作。                                 |
| 桌面视觉 | 检查中文明暗主题、法语长文案、阿拉伯语 RTL，包含 760×540 最小窗口及登录失败对话框；截图均已人工查看。fixture 不调用真实 Passkey。                                                                                 |
| 网页     | 类型检查、127 项测试及构建通过，2 项原有测试跳过。新增的 4 项 Passkey 测试含 Rust 生成的 CBOR 请求与回调逐字节互操作；调整满额退出流程后重新运行该组与类型检查、构建。                                            |
| iOS 构建 | 设备和模拟器 Rust slice、UniFFI、XCFramework、源码指纹校验、App 模拟器构建与启动通过。                                                                                                                            |
| iOS 测试 | 115 项通过、0 失败、0 跳过：全部 `dMsgTests` 加两项相关 UI 测试。覆盖原生初始化失败重试、桥接、本地化，以及英/中/法/阿拉伯语 Passkey 入口、满额退出入口和过期后的本机复制；使用 iOS 18.4 的小屏模拟器与合成数据。 |
| 配套准入 | 私有 Core 的类型检查、准入及配置 9 项测试、Worker dry-run 构建通过。票据字节不变，auth issuer 单独准入，不加入账户 user-home 信任列表。                                                                           |

iOS 新增文案位于原有 XML `.strings` 字典内；五个新增键在全部 23 个语言文件中检查，并由实际 Bundle 读取测试覆盖。验证过程发现并修正了 Candid 枚举重命名、只读工作区的重置顺序、公开名称查询的登录前置检查，以及移动端续期失败时应保留的会话切换保护。最初因磁盘不足中断的构建不计为通过；清理的仅为生成的构建缓存，最终 Rust 检查使用 `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`，保留测试断言和 CLI 既有优化配置。

## 可复核的局部测量

命令：

```sh
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_auth
cargo test --locked -p dmsg_integration --features pocketic-tests --test passkey -- --nocapture
```

环境：仓库锁定依赖，Rust 1.98.1、PocketIC 16.1.0，NNS 根子网与 application 子网；测试签名钥匙为固定的合成软件钥匙。

- 完整续期 update（含稳定记录更新、签名表与 certified data）：**7,213,730 cycles**。
- 一个身份、曾建立 16 个会话的 fixture：**513 stable pages，32.0625 MiB**。这包含 MemoryManager 的初始分配粒度，不能据此推算每个真实用户的空间。
- Wasm：**1,294,409 bytes**，SHA-256 `0220e54e385fc868483d4c847cbbc2d8f6e910da6f1ec480267b2c7db11bba9f`。

样本规模很小，测量未包含大量同时待取签名、网络延迟或主网目标吞吐。不承诺千万级容量、固定单次费用或每天最多两次续期。预算由部署测量确定。

## 2026-11-09 限流与升级签名恢复回归

本节记录基于 `ic-panda@db276843` 的修复工作区，前文保留首次功能验收的数据。修改范围为 `dmsg_auth` 交互限流、升级后缺失 witness 的补发及对应测试；稳定记录格式和公开方法签名不变。

- `cargo test --offline --locked -p dmsg_auth --lib`：8 项通过。覆盖无效 challenge 不消耗共享额度、准备请求为完成请求保留空间、单 caller 限流、跨分钟保留已有 challenge 的计数位置、计数表上限，以及补发不改变稳定会话且计入续期预算。
- `cargo test --offline --locked -p dmsg_protocol --lib passkey`：4 项通过。
- `dmsg_auth --all-targets` 与启用 `pocketic-tests` 的 `dmsg_integration --test passkey` 均通过 Clippy `-D warnings`；相关包格式检查通过。
- release Wasm 构建通过，`candid-extractor` 输出与提交的 Candid 文件逐字节一致。
- `cargo test --offline --locked -p dmsg_integration --features pocketic-tests --test passkey -- --nocapture`：3 项通过。验证真实入口下的完成额度、升级后立即补发且保持原到期时间、重复补发被拒绝、退出全部后再升级不能恢复会话，并保留 P-256 ingress、嵌套委托与 180 天后续期检查。

环境：Rust 1.98.1，锁定的 PocketIC 客户端 16.1.0，显式使用本地 PocketIC server 16.0.0；NNS 根子网与 application 子网。集成测试及其 Clippy 使用 `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`，不关闭测试断言。

- 完整常规续期 update：**7,324,315 cycles**。
- 一个身份、曾建立 16 个会话的 fixture：**513 stable pages，32.0625 MiB**。
- Wasm：**1,329,544 bytes**，SHA-256 `d307ea023420a13f81ee1d0e647c1c0631a95b469fae3ac271b47f06973cafb1`。

上述测量仍为小规模本地 fixture，不能推算生产吞吐或容量；未重跑桌面、iOS 或真实硬件流程，也未执行生产部署或主网压测。

## 2026-11-09 注册预检查与 iOS 登录入口回归

本节基于 `ic-panda@a6f7a2a2` 的修复工作区。注册先取得准入票据，再启动系统凭据创建；票据签发的临时网络失败按安全请求有界重试，不再误报未知注册结果。iOS 将创建与登录收进同一个入口，并说明新凭据与原账户绑定的区别；切换账号期间隐藏且抑制原账号本机解锁。

- `cargo test --offline --locked -p dmsg_client --lib passkey`：7 项通过，包含注册预检查失败不进入创建 challenge。
- `cargo test --offline --locked -p dmsg_client --lib cloud::tests`：11 项通过，包含准入请求临时失败重试、明确拒绝，以及普通写请求仍保持未知结果语义。
- `cargo test --offline --locked -p dmsg_auth --lib`：8 项通过；`cargo clippy --offline --locked -p dmsg_client --lib --tests -- -D warnings` 通过。本次修改的四个 Rust 文件单独格式检查通过。
- 重建设备/模拟器 Rust slice、UniFFI 与 XCFramework，源码指纹校验通过；Debug 链接仍有 `__eh_frame` 过大的警告，构建成功。
- iPhone SE 小屏模拟器、iOS 18.4：App 构建与启动通过，123 项 Swift 单元测试和 5 项 UI 测试通过，0 失败、0 跳过。测试结果保存在本次运行的 `dmsg-passkey-tests.xcresult`。
- 检查英文、简体中文、法语长文案、阿拉伯语 RTL 的合成界面与截图，覆盖浅色/深色、账号切换及取消、登录与创建入口、会话满额、普通限流和未注册凭据提示。另人工检查中文浅色页面。23 种语言资源检查通过。

验证后工作区出现并行的收件箱相关修改，已保留，未纳入本次 Passkey 改动。整包 `cargo fmt --check -p dmsg_client` 随后在这些文件上报告格式差异；原生库最终指纹检查也因后续源码变化不再通过。上述构建与测试对应本轮已构建的 Passkey 版本，合并并行修改后须重新构建原生库与校验指纹。本轮没有执行真实系统 Passkey 注册、原账户绑定链路、生产部署或客户端发布。

## 2026-11-09 桌面入口与失败恢复补齐

本节基于 `ic-panda@e8bcc601` 的修复工作区。共享 Rust 注册预检查已由 `0f2fb3b3` 提供；本轮补齐桌面欢迎页和账号菜单的单一 Passkey 入口。统一登录对话框先区分使用已有凭据与创建独立身份，保留 II 入口；预检查失败可重试，已打开浏览器的注册结果未确认则先尝试使用已保存凭据，不直接重复创建。绑定入口的恢复保留绑定语义，登录对话框打开时不自动解锁原账户。

- 桌面 `pnpm check`：0 错误、0 警告；`pnpm test`：192 项通过、0 失败。
- `pnpm audit:controls`：0 个未经允许的用法；`pnpm build` 通过。
- `DMSG_DESKTOP_SMOKE=passkey node scripts/smoke.mjs` 通过。使用隔离 Electron 配置与 mock daemon，覆盖打开入口不启动认证、明确选择后才注册、注册预检查重试、未知结果转为登录、未注册凭据说明、账号菜单、II fallback、原账户绑定恢复、会话满额/普通限流区别、退出全部、续期过期以及键盘与 Escape 焦点返回。
- 人工检查中文明暗主题、法语长文案、阿拉伯语 RTL 的入口与错误恢复截图，包含 760×540 最小窗口。23 种语言齐全，截图在本次 `src/dmsg_desktop/test-results/passkey-*.png`。

本轮未修改 Rust 或登录网页协议，未重跑真实系统浏览器 WebAuthn、Secure Enclave 或跨设备同步；未生成或发布安装包。发布桌面新版时必须包含更新后的 Rust daemon，单独替换 renderer 不会把注册预检查带到已安装的旧 daemon。

## 2026-11-09 iOS 绑定中断恢复与重复创建防护

本节基于 `ic-panda@462b0c21` 的修复工作区。用户确认系统已保存凭据，但没有设备日志，因此无法断定原始超时发生在哪个网络请求。代码检查确认绑定在系统认证后未显式恢复 host，网络失败又会丢弃认证响应与已验证身份，下一次操作重新创建凭据。

- 共享核心在系统创建前完成部署目标发现。重试先查询原硬件会话的授权；仅当原提交尚未确认、授权确实不存在且原 challenge 未过期时，才重新递交完全相同的证明。服务端已有的一次性 challenge 与 caller 检查保证重复递交不会创建另一个身份或恢复已退出会话，本轮没有修改认证服务生产逻辑。
- iOS 在系统认证返回后显式恢复 host；临时失败保留原响应和已验证身份，继续操作不再次调用系统创建。锁定、取消或离开页面会取消任务并丢弃待办，迟到结果不能恢复旧状态。等待图标位于绑定按钮内。
- iOS 与桌面的添加入口均支持使用已保存的凭据；重新创建是独立选择。iOS 与网页的新凭据名称带 8 位十六进制短标识。旧密码管理器条目不自动修改或删除。
- `cargo test --offline --locked -p dmsg_client -p dmsg_mobile -p dmsg_auth --lib`：分别 611、30、9 项通过，客户端原有 5 项忽略测试未运行。覆盖绑定读取失败后复用身份、取消网络任务、拒绝异源回调、失效 challenge、注册结果查询失败不重复提交，以及相同注册证明重复递交只产生一次效果。
- 四个修改包（client、mobile、CLI、auth）的 `--all-targets` Clippy `-D warnings` 与修改 Rust 文件格式检查通过。测试使用 `CARGO_INCREMENTAL=0`。
- 桌面 `pnpm check`、192 项测试、控件审查（0 个未经允许的用法）、构建与 Electron Passkey fixture 通过。绑定选择页不提前开始认证，已有凭据使用 `login + bind`，未知创建结果保留绑定语义；检查中文明暗主题、法语长文案和阿拉伯语 RTL 的 760×540 窗口截图及键盘操作。
- 网页类型检查与构建通过；Passkey 逻辑及页面的两组测试共 21 项通过，包含可区分的名称与不变的 user ID。未重跑网页全部测试。
- 最终 Debug 设备/模拟器 Rust slice、UniFFI 与 XCFramework 重建通过，源码指纹校验通过。Debug 链接仍有 `__eh_frame` 大小警告。早先 Release 构建期间源码发生变化，指纹检查拒绝该产物，故不计作最终 Release 验证。23 种语言资源检查通过。
- iPhone SE 小屏模拟器、iOS 18.4：129 项 Swift 单元测试和 4 项相关 UI 测试通过，0 失败、0 跳过。绑定测试补强了弹窗可点击与转场结束等待；发现旧测试缓存后，使用全新构建目录与重新安装的测试 runner 单独重跑该项，通过英文浅色、中文明暗、法语长文案、阿拉伯语 RTL 五个场景，并查看最终截图。故障 fixture 验证原响应重试和审批页导航，不代表真实系统认证或完整链上审批；结果包分别为本次 `dmsg-binding-tests.xcresult` 与 `dmsg-binding-ui-fresh.xcresult`。

本轮没有真机系统 Passkey、Secure Enclave 或跨设备同步验收，也没有生产部署或发行包发布。桌面新增凭据名称依赖登录网页更新；现有失败凭据是否已注册，不能只从系统列表的名称或顺序判断。

## 2026-11-09 复核修正

本节基于 `ic-panda@65fd390d` 的复核工作区，修正上一节的实现与说明。

- 上一节把“绑定在系统认证后未显式恢复 host”列为原因之一，复核不成立：host 每次暂停都会推进会话代际，而待完成的绑定固定在开始时的代际，被暂停的 host 只会以界面不提示的 `LOCKED` 结束。截图中的“暂时无法连接”是 `NETWORK_UNAVAILABLE`，来自真实网络错误。iOS 两条绑定路径新增的 `resume` 不起作用，已移除。
- 签发方的证书签名在签发一分钟后会被其他签发修剪，此后重试的 `get_delegation` 只返回 `Unavailable`，直到挑战过期。重试取授权遇到这种情况时，先对同一硬件会话调用 `renew` 再读取。整个重试仍限于原挑战有效期内，过期后改用“使用已有通行密钥”。
- “注销所有设备”不派生委托，不再在系统认证前读取部署 targets，避免为逃生入口增加查询与失败点。
- iOS“继续”只续用已保存的尝试，不会落回创建凭据，续用时保留原操作。会话代际变化（后台暂停、锁定）或开始“注销所有设备”时丢弃已无法完成的尝试，不再显示会静默失败的“继续”；之后由“使用已有通行密钥”关联已保存的凭据。
- `cargo test --offline --locked -p dmsg_client -p dmsg_mobile --lib`（独立 `CARGO_TARGET_DIR`、`CARGO_INCREMENTAL=0`）：分别 611、30 项通过，客户端原有 5 项忽略测试未运行。签名修剪后的续签路径没有自动化测试；本地模拟需要可签名的查询响应或 PocketIC 网关，本轮未搭建。
- 两个包的 `--all-targets` Clippy `-D warnings` 与 `cargo fmt --check` 通过。
- iPhone 16e 模拟器：`PasskeyBindingTests` 6 项（新增“继续不会落回创建凭据”）与绑定 UI 测试通过，覆盖英文浅色、中文明暗、法语、阿拉伯语 RTL，并查看中文浅色的保留尝试截图。本轮只改 Swift 与客户端 Rust，未改 UniFFI 接口；模拟器使用上一节构建的 XCFramework，未重建 Rust slice，因此续签逻辑未进入这次模拟器运行。没有真机 Passkey 验收。

## 2026-11-10 满额恢复目标与绑定语义

本节基于 `ic-panda@4363743c`。满额恢复固定到失败认证返回的凭据，清理其会话后仍用同一凭据继续登录或原账户绑定；保留当前工作区，普通“注销所有设备”仍退出当前登录。桌面拒绝没有目标凭据的恢复请求，已被替换的认证回调不能覆盖新流程的目标；iOS 的 II 账户也有对应恢复入口。

- 共享客户端新增目标固定回归：当前记住 A 时，恢复与后续登录都限制为 B，A 的凭据保持不变，错误的 A 回调被拒绝。移动核心验证恢复后账户、解锁状态和代际不变，过期的完成结果被拒绝；普通注销仍结束本机会话。
- 当前依赖组合的完整 Rust 回归：client 612 项、mobile 32 项、CLI 122 项通过，原有忽略测试分别为 5、0、6 项，未执行这些忽略项。三个包的 `--all-targets` Clippy `-D warnings` 及修改文件格式检查通过。隔离检出还验证了提交时的旧依赖基线；daemon 最后增量的 29 项回归在两种依赖组合下均通过。
- 新增真实 PocketIC 回归：注册响应丢失后推进 61 秒，由另一设备签发触发清理，再由实际客户端 `complete` 调用 `renew` 并取回授权。重复完成仍使用同一身份，最终只有原有 1 个身份、2 个会话。Passkey 集成测试共 4 项通过，使用合成 P-256 钥匙及本轮构建的 auth Wasm，不调用生产服务。
- iOS Debug 的设备/模拟器 Rust slice、UniFFI、XCFramework、源码指纹及 App 构建启动通过。131 项 Swift 单元测试和 4 项 UI 测试共 135 项通过，0 失败、0 跳过。MCP 等待超时后，直接读取已完成的 `dmsg-capacity-tests.xcresult` 核实结果。查看英文、中文明暗、法语长文案、阿拉伯语 RTL 的小屏模拟器截图，覆盖 II 账户绑定满额、按钮内等待及重置后进入原绑定审批页；fixture 不代替真机系统认证或链上绑定验收。
- 桌面类型检查、192 项测试、控件审查（0 个未经允许的用法）及构建通过。Electron fixture 验证目标凭据跨恢复和登录保持不变，绑定标志不丢失，原账户保持解锁，并覆盖键盘、中文明暗、法语、RTL 和 760×540 最小窗口。使用提交基线的 Electron 45.2.1 官方缓存包，已按官方 SHA-256 校验；用户依赖升级中的 45.2.3 下载失败，未将该版本算作运行验证。
- 按用户追加要求兼容 SQLite backup 回调类型；同时显式实现 WebDAV 虚拟文件系统原有的“没有符号链接、查询相同元数据”行为，兼容 dav-server 新版移除默认实现。依赖版本文件保留用户已有改动，不纳入本次提交。新旧依赖基线分别检查，未进行 Linux FUSE 或真机发布验收。

本轮没有发布安装包、部署 canister 或执行真实跨设备 Passkey 操作；最终原生验证为 Debug，Release 发行构建仍需按发布脚本重建。

## 尚未执行

- 签名真机上的首次 Passkey 创建/登录、AuthenticationServices 的实际 origin、Keychain/Secure Enclave 跨重启和系统锁定后的恢复。
- 真实桌面与 iOS 设备之间的 Passkey 同步、原账户绑定与设备批准整条链路。
- 测试网或主网的目标容量、并发吞吐与长期运行测量，以及签名发行包发布验收。
- 本轮未执行生产安装、升级或客户端发布。首次功能验证时 issuer 尚未安装；2026-11-09 本轮只读查询 `auth_stats` 已返回 `ready=true`、`admission_required=true`，源码 `production.toml` 也已启用生产 issuer。这不代替真实硬件注册与登录验收。

部署顺序与公开合同见 [dmsg-passkey/1](protocol/passkey-v1_zh.md)。本次未修改 `dmsg_user` 的业务接口或状态布局，未部署私有 Worker，未重新声明历史互操作 fixture 的来源。
