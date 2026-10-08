# dMsg Chrome 扩展

`src/dmsg_app` 是 dMsg 唯一的完整客户端：Svelte 5 + TypeScript + Vite 构建的 Manifest V3 扩展。它在本机生成并使用内容根、对象与文件密钥，负责加解密、设备签名与批准、本地 IndexedDB、outbox 和云端同步；ICP canisters 只保存账户、设备、根承诺、名称、认证回执与资金等权威状态，私有云端只保存和投递密文。整体分工见 [技术架构](../../docs/dmsg_architecture_zh.md)，加密机制见 [加密设计](../../docs/dmsg_encryption_zh.md)。

## 当前状态

核对日期：2026-10-08。

- **已接线**：账户与设备、登录解锁与 PRF 快速解锁、内容根（RootBundle v2）与登录恢复、秘密库与文件、云端内容同步与公开资料、正式频道、设备签名的正式认证、外部应用接入（`dmsg-extension/4`）、名称认领/购买/转移、套餐与会员、付费来信与资金恢复、Agent Delegation、旧版个人迁移与共享继承。
- **生产构建**：`environment: "production"` 时，[vite.config.ts](vite.config.ts) 要求名称注册表、固定的 user home、COSE 及其公钥 pin、payment、commerce、membership 与 `relayOrigin` 都已配置，缺任一项就拒绝构建。仓库里的 `dmsg.config.json` 不含任何服务 ID，生产配置另行提供。
- **尚未验收**：正式扩展 ID 下的 Internet Identity 派生 origin 与 Principal 连续性、扩展 origin 上的 WebAuthn PRF、主网 canister 与 `key_1`、真实资金、私有云端生产部署、容量与安全审计。本地测试只使用合成账户、合成资金和合成旧数据。
- **没有口令、恢复码和离线备份**：本机数据密钥由账户服务按登录身份发放的解锁秘密保护；所有设备丢失后只能凭登录身份经等待期恢复。云端同步是唯一的设备外副本：每次登录解锁、绑定、批准后读取根或恢复完成后都会自动同步一次，侧栏显示待同步的版本数。

## 快速开始

在仓库根目录执行（Node ≥ 22，pnpm 10）：

```sh
pnpm install --frozen-lockfile
pnpm --dir packages/dmsg-sdk build    # 扩展从 dist 引用 dmsg-sdk
pnpm --dir src/dmsg_app check
pnpm --dir src/dmsg_app test
pnpm --dir src/dmsg_app build
```

在 `chrome://extensions` 打开开发者模式，选择“加载已解压的扩展程序”，加载 `src/dmsg_app/dist`。最低 Chrome 120。默认配置不含任何服务 ID，此时只能创建临时工作台，登录按钮保持禁用。

`DMSG_CONFIG` 指定另一份配置文件，例如 `DMSG_CONFIG=dmsg.production.json pnpm --dir src/dmsg_app build`（路径相对 `src/dmsg_app`）；构建出的扩展只读取这份配置。扩展版本号取自 `package.json`。

`pnpm --dir src/dmsg_app dev` 在 `http://127.0.0.1:5176` 提供网页预览。预览使用自己的 IndexedDB，没有扩展 API、CSP、外部端口和窗口行为；涉及这些的验证必须加载实际构建。

| 入口                   | 职责                                                                    |
| ---------------------- | ----------------------------------------------------------------------- |
| `index.html`           | 全页工作台：秘密库、消息（正式频道）、签名与授权、身份、设置            |
| `sidepanel.html`       | 窄屏工作台，默认打开签名与授权；不读取当前网页                          |
| `popup.html`           | 锁定状态、待确认请求数、打开全页或侧栏、立即锁定；不加载工作台代码      |
| `approve.html?id=…`    | 外部请求的独立审核窗口，只读取扩展内部请求库                            |
| `service_worker.js`    | 外部端口、请求入库与过期、徽标、锁定后的密文提交；不持有内容密钥        |
| `src/crypto-worker.ts` | 解锁页面独占的 dedicated Worker，持有解锁后的密钥并执行全部加解密与签名 |

## 构建配置

[dmsg.config.json](dmsg.config.json)（或 `DMSG_CONFIG` 指定的文件）在构建时内联，只放公开参数；修改后必须重新构建。权限和 CSP 由 [vite.config.ts](vite.config.ts) 从这些字段生成，运行时无法扩大；构建时还会校验 canister ID 与 origin 的格式。

| 字段                                            | 含义                                                                                                                                                   |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `environment`                                   | `local`/`staging`/`production`，进入数据库名、AAD、根上下文与应用登记核对。只有 `local` 允许回环地址并从本地副本取 root key                            |
| `icHost`                                        | IC 网关。非 `https://icp-api.io` 时只允许 `local` 的回环地址                                                                                           |
| `canisters.handle`                              | 名称注册表，也是 user home 列表与注册入口的权威；为空时不能登录                                                                                        |
| `canisters.userHomes`                           | 构建时固定信任的 user home。注册表返回的列表必须包含它们；未固定的 home 以注册表为准                                                                   |
| `canisters.cose` / `coseRootPublicKey`          | 内容根 vetKD 服务及其派生公钥 pin（base64url，96 字节）。pin 用 `dmsg_protocol` 的 `cose_pins` 示例离线计算                                            |
| `canisters.payment` / `commerce` / `membership` | 付费来信托管、套餐结账、PANDA 会员。客户端每类只连一个实例，所有 home 须指向同一组                                                                     |
| `relayOrigin`                                   | 私有云端 API 的精确 origin；进入 `host_permissions` 与 CSP                                                                                             |
| `principalOrigin` / `agentOrigin`               | Agent Delegation 的 principal 文档前缀与 delegation 服务                                                                                               |
| `externalOrigins`                               | 允许连接扩展的精确 HTTPS origin；为空时不生成 `externally_connectable`                                                                                 |
| `derivationOrigins`                             | Internet Identity 派生 origin。`https://dmsg.net` 与旧版 `https://panda.fans` 产生不同 Principal；绑定时选定的 origin 记入工作台，之后的登录默认使用它 |
| `rootDerivationMaxCycles`                       | 登录恢复时一次 vetKD 派生的 cycles 上限，客户端不自动加价                                                                                              |
| `cloudProtocol`                                 | 云端 wire 版本，必须为 `dmsg-cloud/1`                                                                                                                  |
| `legacy.channels/identity/buckets`              | 主网旧服务的 canister ID，用于冻结快照核对                                                                                                             |
| `legacy.cutover`                                | 已审核的冻结批次；为空时共享继承与冻结对照不可用                                                                                                       |

`host_permissions` 只包含 `icHost`、`relayOrigin`、`principalOrigin` 与 `agentOrigin`；II 登录走弹窗与 postMessage，不需要访问 II 或派生站点。权限只有 `storage`、`alarms`、`sidePanel`，`clipboardWrite` 与 `unlimitedStorage` 按需申请。CSP 只允许扩展自身脚本，`connect-src` 列出上述 origin，并为中继加上频道活动提示所用的 `wss://`（`https:` 来源不放行 `wss:`）。

使用真实 II 登录前，派生 origin 的站点必须在 `/.well-known/ii-alternative-origins` 列出 `chrome-extension://<正式扩展 ID>`；仓库中 [dmsg_frontend](../dmsg_frontend/static/.well-known/ii-alternative-origins) 与 [ic_panda_frontend](../ic_panda_frontend/static/.well-known/ii-alternative-origins) 的这两个文件目前都还没有扩展 origin。

## 运行架构

- **页面与 Crypto Worker**：每个解锁的页面拥有自己的 dedicated Worker，RPC 方法有白名单并串行执行。IndexedDB 中的 `crypto-owner` 租约和递增 fence 保证同一时刻只有一个页面持有解锁会话：在另一个页面（侧栏、审核窗口或另一个标签页）解锁会先递增 fence 并广播锁定，原页面随即锁定。租约每 5 秒续期，20 秒未续期只表示另一页面可以接手；后台标签页的计时器被节流时，持有者在被接手前仍然有效。15 分钟无操作、锁定、关闭或刷新页面、扩展安装与浏览器重启都会结束会话；锁定先终止 Worker（连同 II 会话私钥），再释放租约并清除明文视图、缓存的登录和 Blob URL。
- **Service Worker**：只做调度。它校验外部端口的真实来源，把请求用设备 HPKE 公钥加密后入库，打开审核窗口，并每 30 秒提交解锁页面预先签好的密文版本（每批最多 25 个，授权最长 45 秒）。它不保存内容密钥，也不能产生新内容或新签名。
- **存储**：每个工作台一个数据库 `dmsg:<environment>:<subjectId>:<deviceId>`，另有 `dmsg:registry:1` 记录当前工作台。对象与文件块、文件任务、控制日志、请求和 outbox 中的私密载荷都已加密；outbox 行在云端确认后只保留回执，不再保存密文副本；对象外层、任务索引、PRF 凭据 ID 和绑定前的临时键是明文。完整清单见 [加密设计 5.3 节](../../docs/dmsg_encryption_zh.md)。
- **链上调用**：IC agent 开启 query 验签，生产只用内置主网 root key。认证数据经 [certified.ts](src/lib/services/certified.ts) 校验证书、canister 与 witness，证书时间最多领先本机时钟 10 秒，60 秒有效期仍从证书时间起算。II 会话私钥只在 Worker 内存，delegation 目标限定为配置与注册表中的 canisters，有效期 15 分钟，不写入任何存储。

## 账户、解锁与恢复

1. **建立工作台**：生成设备 ID、Ed25519 签名与 X25519 HPKE 种子；本机数据密钥先由明文保存的临时键封装。绑定账户前拒绝写入任何内容。
2. **绑定账户**：登录 II 后使用该登录已有的账户，或在注册入口 home 新建账户（该 home 配置了准入公钥时，先向云端 `/v1/account-admission` 取准入票据）；取得 `unlock_secret(account, device)` 重新封装本机数据密钥、删除临时键并记下登录 origin；随后打开或创建内容根、激活工作区并同步一次。
3. **日常解锁**：登录取回解锁秘密，解锁后自动同步；启用平台认证器后可用 WebAuthn PRF 解锁（不会自动同步），但距上次登录解锁超过 7 天必须重新登录。PRF 解锁后，客户端按证书验证过的设备表核对本机状态，确认已被撤销时清除本机数据库。
4. **新设备**：新设备生成批准请求，已有管理员设备批准并随即换根，新设备再读取封装给自己的根。撤销设备同样会立即换根；换根完成前全账户的内容写入暂停。每个账户最多 16 台设备。
5. **新增登录身份**：新登录在设置页生成绑定请求（只存在本机，不上链），管理员设备核对后批准，新登录在 10 分钟内点“管理员批准后完成绑定”接受。
6. **所有设备丢失**：用绑定过的登录申请恢复，等待期默认 3 天（可设 1–7 天），期间任一旧设备可取消；到期后这台设备替换旧设备与绑定，做一次 vetKD 派生打开恢复信封，并立即换根。登录身份与全部设备都丢失时内容不可恢复。

字节格式与断点语义见 [账户与根合同](../../docs/protocol/account_root_zh.md)。所有链上变更先把签好的请求写入加密日志再提交，结果未知时按原请求对账，不换新 ID 重发。

## 功能与代码位置

| 功能                 | 界面                                                                                                                   | 主要实现                                                                                                                           | 合同                                                                                          |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| 秘密库与文件         | [Vault](src/lib/components/Vault.svelte)                                                                               | [engine.ts](src/lib/crypto/engine.ts)                                                                                              | [加密设计](../../docs/dmsg_encryption_zh.md) 6–7 节                                           |
| 账户、设备、根、恢复 | [Onboarding](src/lib/components/Onboarding.svelte)、[AccountSettings](src/lib/components/AccountSettings.svelte)       | [account.ts](src/lib/services/account.ts)、[account-root.ts](src/lib/services/account-root.ts)、[root.ts](src/lib/crypto/root.ts)  | [account_root_zh](../../docs/protocol/account_root_zh.md)                                     |
| 云端同步与公开资料   | [SyncSettings](src/lib/components/SyncSettings.svelte)                                                                 | [content.ts](src/lib/services/content.ts)、[relay.ts](src/lib/services/relay.ts)、[background.ts](src/lib/services/background.ts)  | [cloud_zh](../../docs/protocol/cloud_zh.md)                                                   |
| 正式频道             | [FormalChannels](src/lib/components/FormalChannels.svelte)                                                             | [services/channel.ts](src/lib/services/channel.ts)、[crypto/channel.ts](src/lib/crypto/channel.ts)                                 | [channel_migration_zh](../../docs/protocol/channel_migration_zh.md)                           |
| 正式认证             | [Signatures](src/lib/components/Signatures.svelte)、[DocumentApproval](src/lib/components/DocumentApproval.svelte)     | [signing.ts](src/lib/services/signing.ts)、[cose.ts](src/lib/services/cose.ts)                                                     | [protocol](../../docs/protocol/README_zh.md)、[app-action](../../docs/protocol/app-action.md) |
| 外部应用接入         | `approve.html`                                                                                                         | [external-port.ts](src/lib/external-port.ts)、[bridge-requests.ts](src/lib/bridge-requests.ts)、[requests.ts](src/lib/requests.ts) | [browser-v4](../../docs/protocol/browser-v4.md)                                               |
| 名称                 | [HandleSettings](src/lib/components/HandleSettings.svelte)                                                             | [handle.ts](src/lib/services/handle.ts)                                                                                            | [dmsg_handle](../dmsg_handle/README.md)                                                       |
| 套餐与会员           | [CommerceSettings](src/lib/components/CommerceSettings.svelte)                                                         | [commerce.ts](src/lib/services/commerce.ts)、[wallet.ts](src/lib/services/wallet.ts)、[usage.ts](src/lib/services/usage.ts)        | [commerce_zh](../../docs/protocol/commerce_zh.md)                                             |
| 付费来信与资金恢复   | [InboxSettings](src/lib/components/InboxSettings.svelte)、[PaymentSettings](src/lib/components/PaymentSettings.svelte) | [inbox.ts](src/lib/services/inbox.ts)、[payment.ts](src/lib/services/payment.ts)                                                   | [dmsg_payment](../dmsg_payment/README.md)                                                     |
| Agent Delegation     | [AgentSettings](src/lib/components/AgentSettings.svelte)                                                               | [agent.ts](src/lib/services/agent.ts)                                                                                              | [agent_zh](../../docs/protocol/agent_zh.md)                                                   |
| 旧版个人迁移         | [LegacySettings](src/lib/components/LegacySettings.svelte)                                                             | [crypto/legacy.ts](src/lib/crypto/legacy.ts)、[dmsg_legacy](../dmsg_legacy)                                                        | [legacy_archive_zh](../../docs/protocol/legacy_archive_zh.md)                                 |
| 共享频道继承         | [SharedSettings](src/lib/components/SharedSettings.svelte)                                                             | [shared-migration.ts](src/lib/services/shared-migration.ts)                                                                        | [legacy_freeze_zh](../../docs/protocol/legacy_freeze_zh.md)                                   |

主要上限：单文件 100 MiB（1 MiB 分块）；条目正文与秘密合计 32 KiB；普通对象 200,000 字节、频道对象 512 KiB；一次云端快照最多 10,000 个对象、100,000 个版本和 8 MiB 证据；同时待确认的外部请求最多 20 个、每个 origin 5 个。

## 外部应用接入

网页通过 `chrome.runtime.connect(<扩展 ID>, { name: 'dmsg-extension/4' })` 连接。来源必须同时在构建配置 `externalOrigins` 和该应用在 `dmsg_commerce` 的认证登记中，且是活动的顶层文档；每个请求都要用应用的 P-256 会话密钥签署扩展下发的一次性 nonce。支持 `authenticate`、`signDocument`、`signAction`、`checkout` 以及 `getOperation`、`openOperation`、`cancelOperation`、`acknowledge`。

请求只在独立的 `approve.html` 窗口中审核，审核窗口自己解锁，结果只在该窗口已解锁时返回给原 origin。签名执行前会再次确认原页面仍然在线，执行后保存认证回执；结果未知时按原执行 ID 对账。精确帧、错误码与恢复规则见 [browser-v4](../../docs/protocol/browser-v4.md)，应用侧 SDK 在 [packages/dmsg-sdk](../../packages/dmsg-sdk)。

## 测试

| 命令                                                                                              | 内容                                                                                               |
| ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `pnpm --dir src/dmsg_app check`                                                                   | `svelte-check` 与工具脚本的 TypeScript 检查                                                        |
| `pnpm --dir src/dmsg_app test`                                                                    | Vitest 单元与集成测试（`fake-indexeddb`、协议向量、根包、同步、签名、支付与重试）                  |
| `pnpm --dir src/dmsg_app build`                                                                   | 构建并运行 [verify-build.mjs](scripts/verify-build.mjs)：入口齐全、无探针页、无宽泛权限、无 `eval` |
| `pnpm --dir src/dmsg_app test:inventory` / `test:cose-prune`                                      | 运维脚本的 `node --test`                                                                           |
| `pnpm --dir src/dmsg_app exec playwright test e2e/extension.spec.ts e2e/external-browser.spec.ts` | 加载实际 `dist`：建立工作台、Popup/侧栏、视口溢出、外部端口来源绑定（CI 运行）                     |
| `pnpm --dir src/dmsg_app test:cloud`                                                              | 真实扩展 → PocketIC `dmsg_user` → 本地 workerd 的签名 profile 与负面信任检查                       |
| `pnpm --dir src/dmsg_app test:account`                                                            | 账户、设备、根与恢复；按需打开探针（见下）                                                         |

Playwright 必须使用 Chromium / Chrome for Testing（`pnpm --dir src/dmsg_app exec playwright install chromium`），品牌版 Google Chrome 不会加载 `--load-extension`。`DMSG_TEST_CHROME` 可指定已有的测试浏览器。测试使用临时浏览器配置，只把回环权限和探针页加入临时副本；发布构建拒绝携带探针页。

`test:cloud` 与 `test:account` 需要私有云端 checkout（`DMSG_CLOUD_DIR`，未设置时跳过，不算通过）、`POCKET_IC_BIN` 指向的 PocketIC 16.0.0 和预先构建的 Wasm：

```sh
cargo build --locked --release --target wasm32-unknown-unknown \
  -p dmsg_user -p dmsg_cose -p dmsg_handle -p dmsg_payment -p dmsg_commerce \
  -p membership -p dmsg_test_ledger -p dmsg_test_sns
DMSG_CLOUD_DIR=/path/to/dmsg-cloud DMSG_CONTENT_PROBE=1 pnpm --dir src/dmsg_app test:account
```

`test:account` 的可选探针：`DMSG_CONTENT_PROBE`、`DMSG_SIGNING_PROBE`、`DMSG_CHANNEL_PROBE`、`DMSG_COMMERCE_PROBE`、`DMSG_DELIVERY_PROBE`、`DMSG_MIGRATION_PROBE`（共享迁移还需 `DMSG_LEGACY_RELEASES` 指向与盘点一致的旧发布工件）。`e2e/legacy-snapshot.spec.ts` 用同一组旧工件核对冻结快照证明。云端命令向量 [cloud-v1.json](tests/fixtures/cloud-v1.json) 只在 `DMSG_WRITE_CLOUD_VECTOR=1` 时由 `tests/cloud.test.ts` 重写，配套服务须独立核对后再接受。

## 工具脚本

| 命令                                                       | 用途                                                                          |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `pnpm --dir src/dmsg_app bindings`                         | 用 `didc` 从公开 `.did` 重新生成 `src/lib/canisters/generated`                |
| `node src/dmsg_app/scripts/cose-prune.mjs --canister <ID>` | 匿名分页调用 `dmsg_cose.prune_executions`，可用 `--after` 续跑                |
| `node src/dmsg_app/scripts/legacy-inventory.mjs`           | 盘点旧版部署实例，见 [legacy_inventory_zh](../../docs/legacy_inventory_zh.md) |
| `node src/dmsg_app/scripts/legacy-freeze-plan.mjs`         | 生成待审核的冻结 controller 调用，不签名、不提交                              |
| `node src/dmsg_app/scripts/legacy-name-plan.mjs`           | 生成旧名称导入的离线调用材料                                                  |

## 已知限制

- 同一浏览器配置只有一个工作台，同一时刻只有一个解锁页面；在侧栏或审核窗口解锁会锁定全页工作台。
- 自动同步只在登录解锁时进行，PRF 解锁和日常编辑后需要在“设置 → 云端同步”手动同步；公开资料只在明确发布时上传。锁定后后台只提交已签好的密文，不收取新内容。
- 每次同步读取完整云端快照，受上文快照上限约束。
- 客户端只连接一个 commerce、payment 与 COSE 实例，见 [技术架构 8.5 节](../../docs/dmsg_architecture_zh.md)。
- 没有可信时间戳；签名产物只证明设备批准了确定内容，认证回执证明 dMsg 记录了该次授权。

## 参考

- [技术架构](../../docs/dmsg_architecture_zh.md)、[canister 实现与验证边界](../../docs/dmsg_canisters_zh.md)、[加密设计](../../docs/dmsg_encryption_zh.md)、[公开协议](../../docs/protocol/README_zh.md)。
- 内部产品与实施设计按 [AGENTS.md](../../AGENTS.md) 定位，不在本仓库。
- Chrome 平台约束：[消息通信](https://developer.chrome.com/docs/extensions/develop/concepts/messaging)、[Service Worker 生命周期](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle)、[扩展存储](https://developer.chrome.com/docs/extensions/develop/concepts/storage-and-cookies)。
- 字体与 Remix 图标授权见 `public/assets/*-LICENSE.txt`；Private Gate 位图按原样复用。
