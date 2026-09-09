# dMsg Chrome extension

Svelte 5 + TypeScript + Vite 的 Manifest V3 客户端。视觉遵循同仓库 `dmsg_frontend/DESIGN.md` 的 Paper / Ink / Forest Green，复用本仓库字体、品牌素材与图标。

当前交付为 **R0 本地工作台**，可实际加密保存、解锁、导出及离线恢复内容。它不是已经接通生产服务的 R1a / R1b / R1c 发布包。默认无外部网站接入，没有演示联系人、虚构消息、模拟投递或模拟阈值签名。

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
- 生成了四类 canister 的 Candid/TS 绑定；IC agent 保持 query 验签，提供实际 Rust 证书路径验证器。II 连接使用 Worker 持有的 transport 私钥和短期、限制目标的 delegation；不使用旧 auth-client-db。
- 候选中继传输适配器、就绪检查及可恢复 outbox 状态机，网络未知结果先查原操作，不自动重新加密或盲目重发。

## 服务配置与未开放能力

`dmsg.config.json` 是构建配置，只包含公开参数。配置后必须重新构建。

- `canisters.user/handle/cose/payment`：新版服务 ID，默认均为空。
- `icHost`：固定网关；只有 local 环境的回环地址允许取本地 root key。
- `relayOrigin`：精确中继 origin；加入构建时 `host_permissions`。
- `externalOrigins`：精确 HTTPS origin 白名单；为空时不产生 `externally_connectable`，外部网页不能连接。
- `derivationOrigins`：明确区分 `https://dmsg.net` 与旧 `https://panda.fans`。需在原派生站点配置实际扩展 ID 的 II alternative-origins，并实测同一 Principal。不会继承原网页会话。

**仅填写 ID/地址不会自动开放生产能力。** 尚需完成以下接口和发布验证：

| 能力                                                    | 当前状态 / 依赖                                                                                     |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| 链上主体创建、认证绑定、设备批准和根 CAS                | 公开类型与连接边界已准备；完整初始化与恢复授权流程尚未接入 UI                                       |
| 云端同步、正式频道、epoch/HPKE 分发、邀请接受、历史授权 | 需要公开冻结的安全证据与内容合同；当前候选云端叶和 Rust 叶不同，不能仅改标签互通                    |
| 正式文件/声明阈值签名                                   | 审核/拒绝可用；批准按钮关闭。类型明确的执行 SDK、批准向量及 canister 联调已有测试；账户初始化和批准 UI 接线仍待完成 |
| handle 认领、购买和转移                                 | 未开放；本地主体不能被名称查询结果替换                                                              |
| 付费来信、资金和 provider controller                    | 未开放；需 R1c 资金/协议门禁，无通用 signHash 或钱包入口                                            |
| 旧 Local / ECDH / VetKey 内容解码                       | 当前可保管用户自行取得的旧档案文件；不自动解码旧 MK/KEK、不写旧 canister、不推断共享频道所有权      |
| 锁定后收取云端密文                                      | 未接入短期限定读取凭证。后台仅处理本地请求元数据，不持有可刷新的全能会话                            |

本地加密格式是 **`dmsg-backup/1` / `dmsg/content/1` 的客户端候选格式**，不是已发布的云端互操作标准。开发中的本地主体不能通过修改 ID 原地变成另一链上主体；正式迁入必须显式重新封装并验证。

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
// 明确批准后才调用；完整初始化/批准 UI 仍待接线。
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

- 同仓库原前端 `utils/crypto.ts`、`stores/message_agent.ts` 的原语和 IndexedDB 经验；没有复制旧共享 MK→KEK→DEK 业务模型或关闭 query 验签的配置。
- Anda Bot 扩展的 Svelte/Vite 多入口和 MV3 组织方式；不继承其浏览器自动化权限。
- [Chrome 消息通信](https://developer.chrome.com/docs/extensions/develop/concepts/messaging)、[Service Worker 生命周期](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle)、[扩展存储](https://developer.chrome.com/docs/extensions/develop/concepts/storage-and-cookies)。
- 字体和现有 Remix SVG 授权保存在 `public/assets/*-LICENSE.txt`。现有 Private Gate raster 素材按原样复用，不宣称为新绘制的官方矢量母版。
