# dMsg Chrome extension

Svelte 5 + TypeScript + Vite 的 Manifest V3 客户端。视觉遵循同仓库 `dmsg_frontend/DESIGN.md` 的 Paper / Ink / Forest Green，复用本仓库字体、品牌素材与图标。

当前交付为 **本地 / staging 集成开发版本**：账户与根、云端内容、正式签名、个人迁移、名称认领、正式频道、共享继承和商业客户端已接线。生产门禁、真实旧账户样本、正式 II origin 与真实资金验收仍独立保留；不是生产发布包。默认无外部网站接入，没有演示联系人、虚构消息、模拟投递或模拟阈值签名。

## 构建与加载

在仓库根目录执行：

```sh
pnpm install --frozen-lockfile
pnpm --dir src/dmsg_app check
pnpm --dir src/dmsg_app test
pnpm --dir src/dmsg_app build
```

打开 Chrome 的 `chrome://extensions`，启用开发者模式，选择“加载已解压的扩展程序”，加载 **`src/dmsg_app/dist`**。Chrome 120+；建议使用当前稳定版。工具栏图标打开 Popup，Popup 可打开全页和 Side Panel。

开发预览：`pnpm --dir src/dmsg_app dev`，访问 `http://127.0.0.1:5176`。网页预览使用自己的 IndexedDB；它不会读取或共享扩展的本地数据。确认扩展 CSP、后台和窗口行为应使用实际扩展构建。

| 入口                | 职责                                         |
| ------------------- | -------------------------------------------- |
| `index.html`        | 秘密库、消息草稿、签名请求、身份资料、设置   |
| `sidepanel.html`    | 窄屏工作台，默认显示签名请求；不读取当前网页 |
| `popup.html`        | 锁定状态、待确认计数、打开全页和侧栏         |
| `approve.html?id=…` | 独立请求审核窗口；只读取内部请求库           |
| `recovery.html`     | 离线备份恢复；全部代码随扩展打包             |

## 已实现的路径

- 不依赖名称或代币建立本机工作台；设置独立口令、下载初始恢复包、验证恢复码。验证完成前，恢复码由 LocalDataKey 加密暂存，刷新或自动锁定后可重新解锁继续；验证成功即删除该临时副本。
- 笔记、登录凭据、API 凭据、原始密钥材料；标题、类型、标签、正文和私密字段均在加密载荷内。本地搜索在解锁后执行。
- 不可变条目版本、乐观并发检查、可见冲突副本和显式采用版本；删除墓碑和回收站，不自动清理历史。
- 显示或复制私密字段须明确确认；复制时按需请求 `clipboardWrite`。
- 单文件上限 100 MiB、1 MiB 分块加密。中断后重选原文件继续，检查已处理明文块摘要并复用原密文，避免重复创建最终条目。下载必须通过块顺序、长度、密文和完整文件摘要检查。
- 两人、协作、分发会话的本地草稿；拟邀请成员状态、加入后的历史范围提示、加密消息及编辑器草稿。明确显示尚未投递。
- 身份公开字段选择与访客预览，关闭/邀请来信规则的本地加密草稿。没有默认公开发布。
- 恢复包覆盖本机不可变内容历史、所需文件块和编辑器草稿；导出前逐块解密并核对完整文件，HMAC 认证完整清单。未完成导入或未归档请求列为缺口，导出明确标为部分。备份不包含设备、认证私钥或阈值私钥。
- 新设备离线恢复会逐项解密、校验文件并生成全新设备密钥；保留内容主体但不自动授予链上设备权限。恢复只接受空白浏览器配置，避免把已有工作区静默隐藏。
- 固定 origin 白名单、顶层文档绑定、载荷摘要和半开期限检查、HPKE 加密的持久请求、拒绝/取消/过期状态、独立审核窗口。
- 生成了六类 canister 的 Candid/TS 绑定；IC agent 保持 query 验签，提供实际 Rust 证书路径验证器。II 连接使用 Worker 持有的 transport 私钥和短期、限制目标的 delegation；不使用旧 auth-client-db。设置页包含 commerce/membership actor、精确商业意图、独立付款身份与来信托管流程。
- `dmsg-cloud/1` 命令/HTTP PoP、完整设备证据桥、GET/POST/密文块传输、profile 验签及就绪检查。账户内容与正式频道有持久化重试；后台只能提交已固定的密文版本及最多 45 秒的精确 HTTP 批准。未知结果按原操作对账，过期后重新解锁。

## 服务配置与未开放能力

`dmsg.config.json` 是构建配置，只包含公开参数。配置后必须重新构建。

- `canisters.user/handle/cose/payment/commerce/membership`：新版服务 ID，默认均为空。
- `icHost`：固定网关；只有 local 环境的回环地址允许取本地 root key。
- `relayOrigin`：精确中继 origin；加入构建时 `host_permissions`。
- `externalOrigins`：精确 HTTPS origin 白名单；为空时不产生 `externally_connectable`，外部网页不能连接。
- `derivationOrigins`：明确区分 `https://dmsg.net` 与旧 `https://panda.fans`。需在原派生站点配置实际扩展 ID 的 II alternative-origins，并实测同一 Principal。不会继承原网页会话。

**仅填写 ID/地址不会自动开放生产能力。** 尚需完成以下接口和发布验证：

| 能力                                                    | 当前状态 / 依赖                                                                                     |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| 链上主体创建、认证绑定、设备批准和根 CAS                | 设置页已接入创建/绑定/设备/延迟恢复/根上传与 CAS，以及 R0 副本转换；正式 II origin 连续性仍待实测                                       |
| 云端同步、正式频道、epoch/HPKE 分发、邀请接受、历史授权 | 账户证据与云端 wire 已对齐，实际扩展 profile 互操作已验证；内容与正式频道已接通并完成真实本地互操作；生产部署仍关闭 |
| 正式文件/声明阈值签名                                   | 已接通精确批准、正式签名、认证执行回执、原请求对账与结果归档；可信时间戳另行验收 |
| handle 认领、购买和转移                                 | 旧名称认证快照导入和免费认领已接通；新名称购买/转移 UI 不在此次接线中                                                              |
| 付费来信、资金和 provider controller                    | 付费来信、现金/SNS 会员在本地合成资金环境已联调；真实资金及 provider controller 不在本轮放行范围                                            |
| 旧 Local / ECDH / VetKey 内容解码                       | 已按明确模式解锁旧 MK/KEK/DEK 并阅读消息和附件；真实授权样本尚待验收，共享继承需冻结证明与全部 managers 同意      |
| 锁定后的限定后台同步                                      | 可提交已固定的密文版本并读取对应原操作状态，批准最多 45 秒；自动收取新内容仍需解锁                            |

本地加密格式是 **`dmsg-backup/1` / `dmsg/content/1`**。到云端的字段、AAD、文件大小与导出映射见 [cloud 合同](../../docs/protocol/cloud_zh.md)；A2 已接通普通内容 writer/reader、冲突保留、实际密文合并导出。开发中的本地主体不能通过修改 ID 原地变成另一链上主体；正式迁入必须显式重新封装并验证。单个备份包继续限制为 256 MiB。

## COSE 执行 SDK

采用两个文档 profile v1，浏览器桥为 `dmsg-extension/3`。文本直接签署 UTF-8，摘要为 RFC 9995 SHA-256；issuer 使用 URI，subject 表示声明对象，issuedAt 是可选的 Unix 秒。账户为 12 字节 Xid，设备/请求编号仍为 32 字节。具体格式见 [公开协议](../../docs/protocol/README.md)。

```ts
const prepared = prepareSign(accountContext, {
  origin: browserSource.origin,
  key: { algorithm: 'Ed25519', kid: descriptor.key_id,
    publicKeyFingerprint: descriptor.public_key_fingerprint },
  statement: { issuer: accountContext.issuer,
    content: { kind: 'text', text: 'Approve this release' } }
})
// accountContext 来自已认证账户，含 accountId、issuer、设备、epoch、序号、
// homeUser、毫秒期限与费用上限；descriptor 来自选定用途的密钥查询。
// 明确批准后才调用；正式签名批准 UI 仍待接线。
const result = await prepared.approveAndExecute(userCanister, deviceSigner,
  async signedOperation => encryptedOutbox.save(signedOperation))
```

[services/cose.ts](src/lib/services/cose.ts) 固定请求副本，批准同时绑定最终 COSE 待签字节、公钥指纹、origin 和执行上下文。同一 prepared 对象只提交一次；review、toBeSigned、approvalMessage 返回副本，完成结果还须匹配被批准的签名内容和公钥。未知结果用 `getExecution` / `reconcileExecution` 查询同一 ID。

[protocol/statements.ts](src/lib/protocol/statements.ts) 提供独立的 Ed25519/ES256K 验证，不访问 issuer URI。结果区分数学签名、内容、身份、授权、时间戳和当前状态。`verifyExecutionReceipt` 先认证 ICP 证书/路径/witness，再把回执与签名匹配，才能确认服务记录的身份和执行授权。它不验证外部 TSA 或当前项目权限。

网页请求使用 `accountId`（规范 Xid）、`statement: {issuer, subject?, issuedAt?, content}`；content 为 `{kind:'text', text}` 或 `{kind:'digest', sha256, contentType?, location?}`，摘要为小写 hex，时间为十进制字符串。requestId/nonce/expiresAt 仅用于浏览器及执行流程，不进入签署正文。

R0 `WorkspaceMeta.subjectId` 是现有本地加密 AAD 的随机标识，**不是链上账户 ID**。可选 `account: {id, issuer, homeUser}` 记录明确绑定；外部请求须匹配已注册账户，不能把本地标识截短成 Xid。链上创建/设备批准及完整签署 UI 尚未开放，填写配置不会绕过这些条件。

## 锁定与恢复

口令采用固定版本的 Argon2id（64 MiB、t=3、p=1）+ HKDF，包装随机 LocalDataKey；设备私钥包再由 LocalDataKey 加密。内容根随机生成，每个条目版本独立内容密钥，使用确定性 CBOR 和 COSE_Encrypt0 AES-256-GCM，AAD 绑定主体、代、对象、版本、设备、种类和墓碑状态。解密失败不会自动创建替代根。

恢复码为随机 256 位材料，分域派生签名与 HPKE 种子。RFC 9180 X25519/HKDF-SHA256/AES-256-GCM 封装恢复根；初始化完成恢复检查前，本机会用 LocalDataKey 加密暂存恢复码，以便中断后继续。验证成功后，日常工作台只保留恢复公钥与封装，不保留恢复码/恢复私钥。恢复包清单使用内容根派生的 MAC，避免攻击者删除内容后重新计算公开摘要冒充完整备份。

密码能力属于解锁页面的 dedicated Worker。IDB 租约、fencing token 和 RPC generation 阻止旧 owner 的写入与迟到响应；15 分钟无操作、锁定、页面关闭和重启均需重新解锁。锁定终止 Worker、移除明文组件并撤销 Blob URL。JavaScript 不承诺法证级内存擦除，剪贴板或外部下载副本也不会随锁定消失。

离线恢复：先保存 `dist` 的副本和 `.dmsg` 文件，把恢复码另存；在空白 Chrome 配置中离线加载同一扩展包，打开工作台选择“从加密备份恢复”。无需主站、ICP 或中继。丢失的密文不能仅凭恢复码重建。

导出当前有 256 MiB 包大小上限，导入/导出及最终 Blob 下载会占用内存。流式文件处理限制加密工作集，但不是任意总库容量的流式归档器。超限会停止并提示，不输出假完整包。

## 验证

P0 云端互操作使用真实 MV3 测试包、PocketIC 16.0.0 的 `dmsg_user` Wasm 和配套本地 workerd。先从公开仓库根目录构建：

```sh
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_user
cargo test --locked -p dmsg_integration --features pocketic-tests --test cloud_fixture --no-run
pnpm --dir src/dmsg_app build
# 本地设置 DMSG_CLOUD_DIR 指向配套仓库；需要 scripts/extension-probe-server.mjs。
pnpm --dir src/dmsg_app test:cloud
```

`DMSG_CLOUD_DIR` 必须指向实际私有 checkout；未设置时常规 E2E 会跳过云端用例，不视为通过。`DMSG_TEST_CHROME` 可指定已有测试浏览器；`POCKET_IC_BIN` 可指定匹配的本地服务。测试只向临时扩展副本添加 loopback 权限和探针页，正式构建拒绝包含探针页。它创建测试账户/设备，查询并验证新鲜真实证据，写入/重试/读回签名 profile，再验证错误 canister/账户、过期证据、设备 map 篡改、伪证书、错误资源、签名和 HTTP 正文篡改。生产门禁保持关闭。

`test-results/*/cloud-result.json` 保存结果，`cloud-processes.log` 保存本地进程输出，均被忽略。它是协议基线测试，不是初始化 UI、完整内容同步或生产部署验收。固定云端命令向量由 `DMSG_WRITE_CLOUD_VECTOR=1 pnpm --dir src/dmsg_app exec vitest run tests/cloud.test.ts` 显式生成，配套服务需独立核验后才能接受变更。

```sh
pnpm --dir src/dmsg_app check
pnpm --dir src/dmsg_app test
pnpm --dir src/dmsg_app build
pnpm --dir src/dmsg_app exec playwright install chromium
pnpm --dir src/dmsg_app test:e2e
```

已有兼容 Chromium / Chrome for Testing 时，可用 `DMSG_TEST_CHROME=/absolute/path/to/browser` 指定可执行文件。E2E 使用临时全新配置，实际加载 `dist`，不会使用个人浏览器数据。测试检查初始化、私密字段确认、文件往返、锁定后明文消失、IDB 密文、新浏览器恢复、Popup 和 320/390px 横向溢出；截图保存在忽略的 `test-results` 中。

单元/集成测试覆盖本仓库 Rust 编码与 Ed25519 向量、拒绝重复键/非规范编码/过深 CBOR、COSE AAD、恢复 HPKE、恶意 KDF 参数、加密落盘、冲突/墓碑、口令更换、文件完整性、断点恢复、恢复清单 MAC、租约 fencing、来源伪造和 outbox 未知结果。

更新公开 Candid 后运行 `pnpm --dir src/dmsg_app bindings`（需要 `didc`）。生成绑定只读取同仓库公开接口，不依赖私有仓库。

## 参考与素材

- 同仓库原前端 `utils/crypto.ts`、`stores/message_agent.ts` 的原语和 IndexedDB 经验；新内容使用独立根与 epoch；legacy reader 仅为旧档案兼容显式解锁旧 MK→KEK→DEK。正式新账户 agent 保持 query 验签，旧普通查询观察和 BLS 冻结快照分级显示。
- Anda Bot 扩展的 Svelte/Vite 多入口和 MV3 组织方式；不继承其浏览器自动化权限。
- [Chrome 消息通信](https://developer.chrome.com/docs/extensions/develop/concepts/messaging)、[Service Worker 生命周期](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle)、[扩展存储](https://developer.chrome.com/docs/extensions/develop/concepts/storage-and-cookies)。
- 字体和现有 Remix SVG 授权保存在 `public/assets/*-LICENSE.txt`。现有 Private Gate raster 素材按原样复用，不宣称为新绘制的官方矢量母版。


## A1 账户与根流程

在设置 → 设备与认证中依次连接 II、创建账户、保存并复验账户恢复码、提交内容根、验证并启用正式工作区。已有本地工作区会保留为独立副本。新设备需要已有管理员批准或延迟恢复；恢复码不会作为日常在线解锁凭据。具体字节与断点语义见 [账户与根合同](../../docs/protocol/account_root_zh.md)。

`rootDerivationMaxCycles` 为固定单次预算，默认 700 亿 cycles；实际费用和账户日预算可能导致明确拒绝，客户端不自动加价。正式扩展 ID、II alternative-origins 和生产派生根尚未验收；不能以本地测试登录身份宣称 II Principal 连续性通过。

实际浏览器联调需要下列候选 Wasm、可读取的私有本地 relay checkout，以及 Playwright Chromium：

```sh
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_user -p dmsg_cose -p dmsg_handle -p dmsg_payment -p dmsg_commerce -p membership -p dmsg_test_ledger -p dmsg_test_sns
DMSG_CLOUD_DIR=/path/to/dmsg-cloud pnpm --dir src/dmsg_app test:account
```

测试使用临时 Chrome 配置、真实 user/COSE Wasm 和本地 workerd；认证 caller 为公开测试 identity，不访问 II 账户或生产服务。所有集成 probe 页面只构建到临时测试包，发布构建拒绝携带它们。

## Legacy 迁移增量（2026-09-22）

设置页现已接入一次性配对、原站加密档案导入、幂等保管和离线历史验证。共享 reader 位于 `../dmsg_legacy`，合同见 `../../docs/protocol/legacy_archive_zh.md`。原站有独立加密分页缓存，支持三种旧根模式、256 KiB 密文分片、未完成上传保全、PANDA/DMSG/PoL 记录、头像选择和冻结增量对照。真实三模式授权样本、未公开的未决权益与生产切换仍待验证。Chrome `e2e/legacy.spec.ts` 使用两个空白浏览器验证密码 Worker 重启和断网恢复；合成记录不能替代真实旧数据验收。


## 本轮集成范围与验证入口

- 云端同步合并固定服务器导出、实际密文块、根对象和本机未同步版本。冲突保留双方；单恢复包仍为 256 MiB，缺块或超限明确失败。
- 正式频道验证签名控制链、当前设备及恢复接收集合、换代 fencing、消息签名/AEAD、历史授权、附件、双方 owner 转移和费用承接。本机草稿单列，不显示为已投递。
- 共享旧频道按完整来源键固定唯一继承，全部冻结 managers 同意同一 genesis；共享名称只接受冻结管理员。成员先证明旧身份并批准新设备，再接受新频道和独立旧历史 grant。Worker 不持有旧 MK/KEK/DEK 或 epoch 私钥。
- 商业客户端核对认证目录、权益与月度用量，保存原订单/claim/转账参数。SNS actor 与 dMsg 账户分别显示，冷却后继续仍批准同一个意图。来信明确区分报价、入账、存储受理、B 点和实际转出。
- `legacy.cutover` 必须由已审核的冻结批次填入；为空时官方共享继承和最终来源确认不可用。个人档案导出不依赖共享经理在线。

本地复现可按工作包选择 `DMSG_CONTENT_PROBE=1`、`DMSG_SIGNING_PROBE=1`、`DMSG_CHANNEL_PROBE=1`、`DMSG_MIGRATION_PROBE=1`、`DMSG_COMMERCE_PROBE=1` 或 `DMSG_DELIVERY_PROBE=1`，运行 `test:account`。共享迁移还需设置 `DMSG_LEGACY_RELEASES` 指向盘点匹配的旧发布工件。测试只使用独立临时浏览器和合成账户/资金，不证明生产部署或真实旧用户恢复成功。

公开接口与边界见 [云端](../../docs/protocol/cloud_zh.md)、[历史档案](../../docs/protocol/legacy_archive_zh.md)、[冻结](../../docs/protocol/legacy_freeze_zh.md)、[频道与共享迁移](../../docs/protocol/channel_migration_zh.md)。

未打开的旧历史 grant 同时保留指定设备与当时恢复代的 HPKE 封装；原设备丢失时，可在已恢复/重新批准的目标账户中显式提供相应代恢复码接收。恢复私钥不会常驻日常内容根。已接收的档案直接进入普通离线恢复范围。
