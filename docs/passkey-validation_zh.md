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

## 尚未执行

- 签名真机上的首次 Passkey 创建/登录、AuthenticationServices 的实际 origin、Keychain/Secure Enclave 跨重启和系统锁定后的恢复。
- 真实桌面与 iOS 设备之间的 Passkey 同步、原账户绑定与设备批准整条链路。
- 测试网或主网的目标容量、并发吞吐与长期运行测量，以及签名发行包发布验收。
- 生产安装、升级或发布。只读核对已登记的 `mpld4-maaaa-aaaas-qhepa-cai` 时，`module_hash` 为空；客户端 `production.toml` 的 `auth` 仍为注释，没有启用生产入口。

部署顺序与公开合同见 [dmsg-passkey/1](protocol/passkey-v1_zh.md)。本次未修改 `dmsg_user` 的业务接口或状态布局，未部署私有 Worker，未重新声明历史互操作 fixture 的来源。
