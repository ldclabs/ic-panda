# dmsg-cloud/1 客户端互操作合同

[English](cloud.md) | 简体中文

本文件描述客户端使用的 wire 合同与内容映射，不包含服务端实现。客户端实现在 `src/dmsg_client`（`cloud`、`upload`、`sync`、`library`、`connect`、`evidence`、`channel_log`、`channel`、`inbox`、`payment` 模块）。账户和执行证明继续使用现有公开 Rust 类型。

## 标识和编码

| 值 | wire 表示 |
| --- | --- |
| AccountId | Candid/认证路径为 12 字节，URL/JSON 为规范 20 字符小写 Xid |
| 设备、请求、对象和版本 ID | 各自 32 字节；云端 JSON 为非零小写 hex |
| 摘要 | SHA-256 的小写 hex；不是文件明文摘要的自动公开入口 |
| 签名、密文、证书和 witness | 无 padding 的规范 base64url |
| Principal | Candid Principal；认证叶 CBOR 为原始 bytes；JSON 为规范文本 |
| 时间 | 安全证据、HTTP 和业务期限为 Unix 毫秒；文档 CWT iat 仍为秒 |
| 金额 | JSON 十进制字符串；按公开商业/支付合同转换，不使用 JS Number |

请求 body 使用确定性 CBOR。浮点、重复键、尾随字节、过深结构和不支持的版本拒绝。HTTP PoP 与内容命令是不同 COSE profile，不是正式文档签名，也不能授权账户根变更或付款。

文本使用原始合法 UTF-8，不做 Unicode 归一化，也不剥离开头的 U+FEFF。Rust 字符串和 TypeScript 编码器都保留签名文本；TypeScript 入口拒绝没有合法 UTF-8 表示的孤立 UTF-16 代理项，见 SDK 的编码与往返测试。

## 设备命令和 HTTP PoP

线上命令只有 `{cose_sign1: base64url}`，不能另附一份可冲突的 account、payload 或 signature。使用 tagged COSE_Sign1、Ed25519 算法 -19、空非保护头和空 external AAD。

保护头为：`1=-19`、`2=[15,16]`、`3="application/cbor"`、`4=设备 ID bytes`、`15={1:账户 issuer URI}`、`16=profile`。签署完整 `Sig_structure` 原字节，不预先换成 SHA-256 摘要。

- 命令 profile：`application/vnd.dmsg.command+cose;v=1`。
- 命令 payload：`{protocol:"dmsg-cloud/1", action, security_epoch, request_id, deadline, payload}`。
- HTTP profile：`application/vnd.dmsg.http-pop+cose;v=1`。
- HTTP payload：`{protocol:"dmsg-cloud/1", security_epoch, request_id, deadline, audience, method, target, body_digest}`。
- `X-Dmsg-Pop` 直接携带 base64url 的 HTTP COSE。audience 是固定 API origin；body_digest 是实际发送 bytes 的 SHA-256。GET 的 body 是空字节，不是 CBOR null。
- target 为规范 path/query；拒绝重复 query key、编码斜杠和跨 origin URL。签名与网络请求使用同一组序列化字节。
- 提交期限满足 `now < deadline <= now+60000`，同时不能超过所使用安全证据的有效期。设备签名回调之后再次检查期限。
- 业务与 HTTP request_id 必须一致。重新签署 HTTP PoP 不改变业务命令和操作 ID；未知结果先查询原操作，不自动换 ID 重发。

dmsg-connect/1 与设备提示通道新增七个 action（路由与规则见 [connect-v1](connect-v1_zh.md) §7.3、§7.5）：`dmsg/connect/invite/v1`、`dmsg/connect/pairing/grant/v1`、`dmsg/connect/pairing/revoke/v1`、`dmsg/connect/authorize/v1`、`dmsg/connect/respond/v1`、`dmsg/hint/token/create/v1`、`dmsg/hint/token/revoke/v1`。其中配对授予、配对撤销、授权与回复要求签发设备在认证设备表中具备 `FormalApprove`，设备能力按具体 action 校验。`dmsg/connect/respond/v1` 的请求 body 是 `{cose_sign1, sealed}`：密封回复与命令并列（最多 512 KiB），命令 payload 只承诺它的 SHA-256，因此这是唯一允许在 `cose_sign1` 之外附带字段的命令 body。

`dmsg_client::cloud::sign_command` 构造设备命令信封，`sign_http` 构造 HTTP PoP。用户公开资料不使用此命令通道；action 的载荷、权限和状态转换由对应资源合同限定，通用编码器不授予额外能力。

Rust `Cloud` 提供证据提交、签名请求与密文块传输，限制响应大小，禁止重定向与 cookies。安全读取使用有界重试；非 GET 请求不由通用 transport 自动重发。上传和内容同步只在核对原操作后按各自幂等合同恢复。网络在 `Hold` 的同步 Client 步骤之外等待；具体预算见[客户端传输说明](../../src/dmsg_client/README.md#sync)。支付与订阅由对应客户端模块实现。

## 注册准入票据

home 配置了 `admission_key` 时，`create_account` 须附带云端签发的 `AdmissionTicket`。客户端在创建账户前请求：

```text
POST /v1/account-admission
content-type: application/cbor
body: { home: bstr(user home Principal), principal: bstr(创建账户的登录 Principal) }

200 { ok: true, data: { expires_at: <Unix 毫秒>, signature: <base64url> } }
```

签名是准入私钥对 `digest("dmsg/account-admission/v1", (home, principal, expires_at))` 的 Ed25519 签名，字节规则见协议向量 `account_admission_v1`。请求不带 PoP：票据绑定了 principal，只有该 principal 自己调用 `create_account` 才能使用，每个 principal 在一个 home 只能建一个账户。服务端只为配置的 user home 签发，按请求来源限流；限流耗尽返回 `QUOTA_EXCEEDED`。`expires_at` 不超过签发后 5 分钟，home 接受最长 10 分钟的剩余期限，余量覆盖时钟误差。票据不授予账户或设备权限，未使用的票据过期即作废。

## 账户证据

客户端账户模块查询 `security_snapshot_batch` 与 `get_device_bundle`。必须验证固定 user canister、ICP BLS 根、certificate、时间、单段 AccountId 路径和 witness，再检查 issuer/home/account 及完整设备根。

设备根是 `digest("dmsg/devices/v1", BTreeMap<Hash,Device>)`。保留所有公开字段、撤销记录和 next_sequence；key 是原始 bytes，不是 hex 字符串。两次查询之间发生设备变化时，根不匹配则拒绝，不拼接权限快照。

证据桥形状：

```text
{schema:1, canister, certificate,
 entries:[{account_id, value, witness, devices}]}
```

value/witness/certificate 使用原始认证字节，devices 是完整设备 map 的规范 CBOR。缓存截止是 certificate 时间加 60 秒；不能以读取或提交时间续命。调用者只可从验证后的设备记录选择未撤销且具有所需能力的设备，不能将返回的所有记录均当作有效授权。

用户公开资料改为由 `dmsg_user` 管理、`dmsg_directory` 发布的官方 `PrincipalDocument`，读取和认证见 [agent_zh.md](agent_zh.md#统一公开身份文档2026-10-22)。云端不再保存另一份用户 Profile，也不接受 `dmsg/profile/v1` 命令。头像上传仍经设备 PoP 与对象提交验证。

## 本地内容与云端映射

账户、设备与根见[账户与根合同](account_root_zh.md)。`content` 为每个本地版本建立持久任务，`sync` 上传密文并提交已签 revision；没有账户或设备授权的工作区不能写内容。以下是当前字段与校验关系。

| 本地来源 | 云端目标 | 转换与校验 |
| --- | --- | --- |
| `workspace::Meta.account` / `EncryptedObject.subjectId` | 真实 `account_id` | 使用已绑定账户的规范 Xid，不以本地随机标识冒充账户 |
| `EncryptedObject.id/revision/parent` | `item_id/revision_id/base_revision` | 保留对象/版本来源映射；父 revision 必须属于同一 item；本地复合 `key` 不作为 wire ID |
| `generation/deviceId` | 根 generation / 签名设备 | 使用已认证账户的已提交根与设备，不能按本机值伪造在线状态 |
| 本地内容 AAD | `['dmsg/content/1', environment, accountIdText, generation, id, revision, deviceId, kind, parent, tombstone]` | subject 值为真实 Xid；身份、版本等上下文参与认证，不能改 metadata 后复用旧密文 |
| 本地 item-key 包装 | `['dmsg/wrap/1', accountIdText, id, generation, revision, 'item-version']` | 使用对应代次的根保护每版本内容钥；旧版本按原 AAD 和历史根验证 |
| 加密对象 | `kind=vault` 上传，再签署 vault revision | 序列化记录为规范 CBOR 密文容器；上传块的摘要按实际 bytes 计算，不能拿本地 `record.digest` 冒充上传摘要 |
| vault revision | `{item_id,revision_id,base_revision,upload_id,tombstone,restore}` | 先完成密文上传再引用 upload_id；删除引用 null，恢复必须明确指定当前墓碑；冲突以服务器返回为准 |
| 文件 manifest 中的 `chunks[].size` | upload plan `chunks[].size` | 本地值是明文长度，wire 值是解码 base64url 后实际密文字节长度；保留两个独立口径 |
| 本地文件块 ID `file:version:index` | `upload_id + index` | object_id/version_id 对应文件来源；上传 ID 独立且幂等；原文件块 AAD 仍需保留并验证 |
| 文件 key/name/MIME/明文摘要 | 加密 manifest / vault 内容 | 不放公开 upload plan。manifest_digest/size 指加密 manifest 的实际 bytes；不明文上传 FileKey |
| 空文件 | 加密 manifest + 满足非空块计划的格式容器 | 云端要求至少一个非空块；空文件使用下文的占位容器，不伪造明文数据块 |
| 用户公开资料 | 官方 `PrincipalDocument`，经 user/directory 上链发布 | 云端仅保存公开头像二进制，不保存用户资料投影；inbox policy 独立 |
| 本地 channel/message 草稿 | 签名 genesis/control/epoch/message | 按当前成员资格、控制 head 和 epoch 校验，不把本地状态或序号当成在线授权 |
| 本地任务与 journal | 资源操作查询和实际服务回执 | 按原 request ID 对账；本地已排队或已发送不等于服务已接受 |

本地文件库（`dmsg_client::library`）默认上限为 4 GiB，密文块保存在 `blobs/`，不会自动同步。当前云端备份和分享入口限 100 MiB；`kind=backup` 需用户显式备份，频道附件使用独立加密的分享版本。保险库条目本身不携带文件块；导入或秘密快照的选定附件进入 Files，具体选择与权限见[Secrets 合同](secrets_zh.md)。账户 `kind=file` 上传用于旧版归档部件 `migration_part`。

明文文件块为 1 MiB，密文块预算为 `1 MiB + 64 bytes`，manifest 最多 64 KiB；上传大小按真实密文字节计算并受资源配额约束。超限明确失败，不静默截断。

若上传计划到期或账户换根，客户端先查询原修订操作和每个上传状态。已提交对象继续复用；无法继续的 staging 上传明确取消后，以当前内容根和新 `upload_id` 重新封装 manifest。原对象、修订 ID、请求 ID 和密文文件块保持不变；配额释放尚在处理中时保留任务供用户重试。

普通拉取仍使用完整导出清单核对版本与缺口，但密文逐块验证并缓存到本机工作区，重复拉取复用已验证块；全部清单和文件验证成功后才原子推进对象 head 与快照游标。单账户拉取不再受单备份包的 256 MiB 限制；单文件、块、清单数量与证据大小的限制继续有效。中断留下的缓存块不代表内容已提交。每次解锁后的同步完成时，客户端删除不再需要的本机块：保留刚应用的快照列出的全部块与 manifest（下一次拉取会核对它们，删掉会被重新下载）、尚未发送版本的任务、未完成导入，以及密封任务中仍未过期的上传计划（频道信封与附件、换根）所用的块；被替换的版本、过期或放弃的上传、频道附件下载留下的缓存因此被回收。已解决的冲突副本与历史版本仍在中继快照中，所以保留。

导出快照的签名依据：每个 `object` 与 `revision` 条目带签名命令的 `security_epoch`，每页带 `security: {"<epoch>": {evidence, stored_at}}`，即中继在该纪元下首次接受上传预留或 vault revision 时核对的认证证据。客户端每次拉取对每个纪元用 `stored_at` 核对一次证据（证书、账户叶与设备表，叶中的纪元须等于该纪元），再逐条在该纪元的设备表中找签名设备（当时未吊销、有 `content_sign`）验签，命令体的 `security_epoch` 须等于该纪元。不按当前设备集合验签：全设备丢失恢复会替换所有设备，设备表满 16 条时加设备会移除最早吊销的一台，它们签过的版本仍须可以拉取。缺少所属纪元证据的条目被拒绝。

本地对象载荷上限为 200,000 bytes（`content::MAX_OBJECT`），读写共用边界。频道索引从认证后的解密内容建立，仅保存在本地 head 元数据中，不改变内容 AAD 或上传记录字段。冲突解决关系放在加密 Item 的 `resolvedConflicts` 中，随后编辑保留该关系，备份与同步一起传递。

旧 dMsg 则先在 `ChannelMessages.svelte` 对完整文件执行一次 COSE_Encrypt0，再调用 IC OSS 的 `toFixedChunkSizeReadable` / `upload_chunks`，按 **256 KiB 密文传输片**上传；下载后由 `ChannelFileCard.svelte` 对完整 bytes 解密。IC OSS 的 Rust `file.rs` 与 TS `stream.ts` 均定义 `CHUNK_SIZE=256*1024`。这不是每个 256 KiB 分片独立 AEAD 的文件格式。旧文件迁移保留分片索引/长度与原始 bytes，按原顺序恢复完整 COSE 后验证；不能套用新版的 1 MiB 明文规则，或将 OSS 分片当作独立认证的新版加密块。此判断基于本地旧客户端与 IC OSS 源码，线上版本仍须按[只读盘点](../legacy_inventory_zh.md)核对。

本表约定了字段、AAD、权限和大小口径。对象块容器固定为规范 CBOR `{format:"dmsg-cloud-object/1", record:EncryptedObject}`；`record` 使用上表正式工作区的字段与既有内容加密规则，保留必要的原始加密字段，不包含明文 Item 或 FileKey。

上传 manifest 的明文固定为规范 CBOR `{format:"dmsg-cloud-manifest/1", account_id, upload_id, object_id, version_id, kind, root_generation, chunks:[{digest,size}], content}`。ID 使用上述 JSON 文本表示，chunks 与 upload plan 一致。vault 的 content 为 null；file 的 content 为现有 FileManifest，包括明文大小/摘要及文件 key，全部处于 manifest 加密层内。使用随机 nonce 的 COSE_Encrypt0 AES-256-GCM，由目标内容根封装；external AAD 为 `['dmsg/cloud-manifest/1', account_id, upload_id, object_id, version_id, kind, root_generation]`。manifest_size/digest 对最终加密 bytes 计算。manifest 不包含自己的摘要，避免循环。

内容根（`kind=root`）例外：manifest 是原始 RootBundle 字节（见[账户与根合同](account_root_zh.md)），`object_id` 取链上 `bundle_digest` 的 hex。`bundle_digest` 覆盖接收者与 body，不等于字节摘要；`GET /v1/accounts/:account/root` 按认证安全快照的 `content_root_generation` 与 `content_root_digest` 匹配 `root_generation` 与 `object_id` 返回对象，客户端下载后仍按 `bundle_digest` 验证。

公开头像（`kind=avatar`）也是例外，内容不加密：唯一的块是 PNG、JPEG 或 WebP 原图（12 字节至 512 KiB，中继按文件头核对），`root_generation` 为 0，`epoch` 为 0，`control_head` 为 null，`object_id`、`version_id` 是随机 ID；manifest 是明文规范 CBOR `{format:"dmsg-cloud-avatar/1", account_id, upload_id, mime}`，中继只保存不读取。上传提交后发布公开对象；finalize 成功响应和上传状态中的 `public_url` 是最终地址（production 为 `https://assets.dmsg.net/avatars/:account/:upload`），由 PrincipalDocument 的 `avatar_url` 原样引用。客户端不能从 API origin 拼接图片 URL。上传 `status=committed` 只表示上传已提交；缺少 `public_url` 表示公开发布尚未完成，应使用同一 upload ID 重试 finalize，不能重新预留或上传。发布失败可返回可重试的 503，后台仍会恢复；重复 finalize 与操作查询返回同一个已发布 URL。只有已提交的 avatar 类型可以公开读取；取消文档引用不会删除或关闭这个公开 URL，普通对象回收拒绝头像。图片按不可变对象缓存一年，更新头像生成新 URL；公开读取无凭据 CORS。旧 `/public/accounts/:account/avatar/:upload` 入口已移除。本地开发从本地公开桶的 `/avatars/:account/:upload` 读取，本地 origin 使用 directory 的 HTTPS delegation_service 别名，客户端核对后映射到回环 API 读取；生产直接读取响应 URL。客户端保存最后一次头像上传计划，相同图片重试复用原 upload ID，只有明确未提交且已结束的计划才重新预留。

空文件的 manifest 保留 `size=0`、空文件摘要及空文件块列表；上传层使用一个规范 CBOR 容器 `{format:"dmsg-cloud-empty-file/1"}` 作为非空占位块。它不计作明文文件块。恢复程序必须检查空文件标志、零长度和摘要一致性，不能把占位 bytes 返回为文件内容。

备份（`kind=backup`）：本机文件库的一个版本由用户显式备份时上传。块就是本机 `blobs/` 中该版本的密文块（不重新加密），空文件同样用占位块；`object_id`、`version_id` 是随机 ID，文件与版本身份只在 manifest 内；`root_generation` 为当前根代次，`epoch=0`、`control_head=null`。manifest 仍是 `dmsg-cloud-manifest/1`，`content` 为规范 CBOR `{format:"dmsg-backup/1", file:FileManifest（含 key）, path:[文件夹名…], tags, note, created_at, modified_at, device}`，由当前根封装，中继不读取。中继按账户分页列出已提交的 backup 上传与其回收 tombstone（`GET /v1/accounts/:account/backups?after=&limit=`，每条含签名的 upload plan、中继预留时核对的认证证据与接收时间 `evidence`、`stored_at`、上传设备与提交时间），客户端像核对频道上传的 descriptor 一样，在该证据里找到当时未吊销、有 `content_sign` 的签名设备并验签 `dmsg/upload/reserve/v1`，再下载 manifest、用对应代次的根打开，得到云端备份目录。验签不依赖当前设备集合：全设备丢失恢复会替换所有设备，设备表满 16 条时加设备也会移除最早吊销的一台，它们上传的备份仍能核对；backup 对象不进入 vault 的导出快照。删除云端副本走显式回收 `dmsg/object/collect/v1`；没有墓碑同步：云端删除不动本机文件，本机删除不动云端对象。

writer/reader、任务恢复与加密容器往返已在公开客户端实现；测试入口见下节。legacy 历史归档有独立格式，不能伪装成上述新版内容。

## 验证和发布边界

- 固定命令/HTTP 向量：`tests/vectors/client/cloud-v1.json`；由公开客户端生成，配套服务使用独立实现逐字节核验。
- 单元测试：`cargo test -p dmsg_client`（含上述向量）。
- 端到端测试：桌面客户端在 PocketIC 与本地 workerd 中继上运行（[`tests/dmsg_integration`](../../tests/dmsg_integration/README.md) 的 `desktop.rs`）。测试创建全新账户，使用局部测试根，不联系生产 canister。
- 错误 canister/账户、过期证据、错误设备 map、伪证书、篡改签名/HTTP body、错误资源和旧协议版本均有负例。
- 设备、签名、资源与运行状态的拒绝必须显式处理，readiness 不能替代业务权限检查。客户端 UI 已接入；生产配置、部署版本、真实账户与端到端验收仍需独立证据，本次文档更新未重跑这些测试。

## 后台密文与认证新鲜度

锁定的 daemon 只完成解锁时已经签好的提交：固定 vault revision 及查询同一操作的 GET，HTTP PoP 绑定 origin、路径、方法、body digest、账户、设备和最多 45 秒期限，不持有可续期会话。未知响应查原编号，签名期限过后暂停。开启“后台在线”时，daemon 锁定后仍持有设备签名私钥，可以发送不需要重新封装的版本并缓存密文，但不能读取内容。重新封装、重新授权和本机加密状态确认在解锁后完成。

公开 `Certification::batch` 读取一次 IC 时间，使认证查询依赖当前 batch time；仅加 transport nonce 无法绕过 replica 按 caller/method/args 建立的缓存。该行为与 [IC query cache 实现](https://github.com/dfinity/ic/blob/master/rs/execution_environment/src/query_handler/query_cache.rs) 一致，并有不写账户状态时证书更新的 PocketIC 回归。客户端和 Worker 的 60 秒新鲜度及防回退规则保持不变。

支付配置新增 `get_configuration_certified`：`configuration`、`signer/<u64be>` 和 `fee/<u64be>` 认证叶用于核对收款路由、报价/回执签署钥和费用政策。它不改变已有托管的固定条款或资金状态机。


## 收件终态管理

收件分页的 `entries[].value` 按 `state` 区分。`visible` 保留原签名、证据和密文；`aborted` 仅返回 `order_id/sender/recipient/state/created_at/read/archived/resolved/abort_reason` 管理字段，不含密文、签名包或证据。两类条目共用原 `next_cursor`，客户端按 order_id 投影最新值。未受理/在途条目仍不作为收件内容返回。

收件人的 status、reconcile 和 mark 返回同样的隐藏字段投影。发送人的原订单查询仍可用于本人对账。受理后退款不自动允许再次联系；收件人使用既有 `dmsg/inbox/mark/v1` 的 `reopen_contact=true` 显式重开。界面把 aborted 显示为未投递的管理状态，不尝试解密或显示为已认证消息。重复重开不会再次减少名额。

## MCP Events 中继端点

本机智能体没有公网 webhook 地址，中继为设备签发回调端点，代收上游 MCP 服务器按 MCP Events 草案（webhook 模式）与 Standard Webhooks 投递的事件。客户端实现在 `src/dmsg_client/src/mcp_events.rs`。

设备命令（`content_sign` 设备）与路由：

| action | 路由 | payload |
| --- | --- | --- |
| `dmsg/mcp-events/endpoint/create/v1` | `POST /v1/accounts/:account/mcp-events/endpoints` | `{callback_id: bstr(16), secret}` |
| `dmsg/mcp-events/endpoint/rotate/v1` | `POST /v1/accounts/:account/mcp-events/endpoints/:callback/rotate` | `{callback_id, secret}` |
| `dmsg/mcp-events/endpoint/delete/v1` | `POST /v1/accounts/:account/mcp-events/endpoints/:callback/delete` | `{callback_id}` |
| `dmsg/mcp-events/ack/v1` | `POST /v1/accounts/:account/mcp-events/ack` | `{callback_id, through_seq}` |

- `callback_id` 由设备随机生成，URL 与 JSON 中为 32 个小写 hex 字符；回调 URL 为 `<API origin>/mcp-events/<callback_id>`，与设备 API 同一个 origin，由创建结果的 `url` 给出。`secret` 为 `whsec_` 加 24–64 字节随机数的标准 base64，由设备生成；轮换时旧密钥再验证 24 小时。每账户最多 16 个活跃端点。
- 端点绑定创建它的设备与该设备的 HPKE 公钥。轮换、读取与确认只限该设备；删除限该设备或具备 `RootManage` 的设备。设备被撤销或失去 `ContentSign` 后，端点在之后的投递时被挂起。
- `GET /v1/accounts/:account/mcp-events/endpoints` 列出账户的端点（不含密钥）：`state`（`active`、`suspended`、`deleted`）、`subscription_id`（验证信封的 `X-MCP-Subscription-Id`）、`seq`、待取数量等。
- `GET /v1/accounts/:account/mcp-events?endpoint=&after=&limit=` 返回 `seq > after` 的记录 `{seq, kind, webhook_id, received_at, sealed?, dropped?}` 与 `cursor`，单页至多 100 条、约 1 MiB。`kind` 为 `event`、`verification`、`gap`、`terminated` 或 `relay_gap`；`relay_gap` 没有 `sealed`，`dropped` 是未被取回、保留期（7 天）满而丢弃的记录数。确认后记录删除，序号不重用。

封装：`sealed` 为 base64url 的 `CBOR({enc, ciphertext})`，HPKE base 模式（X25519、HKDF-SHA256、AES-256-GCM），`info = aad = CBOR(["dmsg/mcp-event/1", account, device_id_hex, callback_id, seq])`，明文为 `CBOR({headers, body, received_at})`：`headers` 是小写的 `webhook-id`、`webhook-timestamp`、`webhook-signature` 与可选的 `x-mcp-subscription-id`，`body` 是原始正文字节，`received_at` 是中继收到的 Unix 毫秒。中继在入口可见明文（验签、识别类型、回显挑战），存储只有密文。

webhook 入口只有 `POST /mcp-events/<callback_id>`（`application/json`，正文不超过 262,144 字节）与 `GET /.well-known/mcp-webhook-receiver.json`，不经设备 API 的 PoP 与 Origin 校验。时间戳超出 ±300 秒或 `v1` 签名不符返回 401，正文过大 413，非 JSON 415，未知、已删除或挂起的端点 410，单端点每分钟超过 120 次、或未确认记录超过 1,000 条或 16 MiB 返回 429；同一 `webhook-id` 的重复投递返回首次结果。`verification` 信封验签后回显 `{"challenge": …}`。每次新记录经提示通道发出 `mcp-event` 提示（connect-v1 §7.5）。

设备取回后自行复验：`v1` 用本机保存的密钥、按 `received_at` 判断 5 分钟容差；上游服务器在其 origin 的 `/.well-known/mcp-webhook-jwks.json` 发布 Ed25519 JWK 时再验 `v1a`（同样覆盖 `id.timestamp.body`），`v1a` 不符的投递丢弃（中继持有 `v1` 密钥，只有它能造出这种投递）。

## 公开资料网页

handle 查询和证明使用小写规范名；页面优先显示认证名称记录的 `display_handle`，旧记录无此字段时保留访问 URL 的大小写。简介只将显式 HTTP/HTTPS 地址转成可点击链接，其他内容仍按纯文本转义。

公开入口为 `/<account_id>` 和 `/<handle>`，永久链接使用前者。handle 经 `dmsg_handle.resolve_handle_certified` 验证，账户按注册表的分配器指纹选择 home，再从其配置的 directory 读取认证 `PrincipalDocument`。网页不再读取 Cloudflare 用户 profile，也不再展示“设备签名已验证”标签。

公开网页在专用 Web Worker 中并行读取配置、handle 与文档，并执行证书、签名和绑定校验；主线程先渲染页面外壳与加载状态，只展示验证成功的资料。导航取消会中止相应后台请求，Worker 异常重启后仍保留页面会话的防回退记录。部署中的 home/directory ID 仅作为提前发起请求的提示，仍以注册表及 home 的实际配置核对绑定，提示失效时回退。浏览器仅在内存中短暂缓存已验证配置和资料证明（30 秒）；命中后仍重新校验证明及其原始 60 秒新鲜度，过期即重新获取，未发布/未找到或失败结果不缓存。显式重试绕过缓存，已见文档的防回退检查保持不变。

JSON 与 HTML 的内容来源、字段和更新时间一致；浏览器独立验证 directory 的 ICP 证书。链接名称和 `extra["dmsg.link_images"]` 图片一起展示。头像与链接图片按文档中的 HTTPS 地址加载（`referrerpolicy="no-referrer"`），部署的 CSP `img-src` 必须允许这些来源。旧资料导入先生成包含名称、简介、具名链接、图片扩展和头像 URL 的草稿，用户发布后才上链。

旧公开资料保留在 `/legacy/users/<username>`，本人旧资料在 `/legacy/profile`。新版映射不存在时可查询冻结名称预留，提供旧资料入口；不能把旧 owner 当作新版账户。只有公开头像 GET/HEAD/OPTIONS 保留无凭据 CORS，设备 API 不对浏览器开放。详见 [PrincipalDocument 合同](agent_zh.md#统一公开身份文档2026-10-22)。

## 云端副本管理（2026-10-22）

UploadPlan 可带 `management:{generation,ciphertext}`（密文字节最多 16 KiB），由上传者用对应账户内容根加密 JSON 来源/恢复记录。COSE_Encrypt0 AES-256-GCM 的 external AAD 是规范 CBOR 数组 `['dmsg/storage-management/1',account_id,upload_id,object_id,version_id,generation]`。记录保存来源文件/版本、名称和大小，以及建立链接或发布附件所需的恢复信息；服务端不持有根密钥或明文。缺少旧根时只能显示 ID，不伪造名称或链接。可选字段缺省时不编码，既有签名向量不变。

| 接口 | 合同 |
| --- | --- |
| `GET /v1/{accounts,channels}/:id/storage?after=&limit=` | 每页至多 100 条上传记录；entries/next_cursor/end，按当前账户或频道 epoch 权限过滤。条目包含 kind、bytes、payer/uploader、generation、status、charged_bytes/reserved_bytes（未知为 null）、回收能力、失败信息和本人管理密文；空页仍按 end 判断结束 |
| `GET /v1/accounts/:id/storage/ledger` | 本人承担的分页账目及当前总量；consumer/charge/upload ID 和余额，不授予内容访问或删除权 |
| `GET /v1/{accounts,channels}/:id/storage/deletions` | 按记录序号读取可见删除墓碑；客户端保留原消息，显示附件已删除 |
| `POST /v1/{accounts,channels}/:id/storage/retry` | `dmsg/storage/retry/v1 {upload_id,generation}`，只重试既有清理，不创建新删除或提前释放额度 |

`dmsg/channel/message/v1` 增加可选 `attachment_upload`；当前客户端的附件消息携带该字段，绑定当前 epoch 下本人已提交的 file 对象。同一原消息重放保持回执；同一附件再次发布或删除后发布拒绝。频道附件可由当前仍有该 epoch 阅读权的发送者或 owner/admin 按 `dmsg/object/collect/v1` 删除；账单转移期间以及 envelopes 均拒绝，付款人身份不授予权限。删除后对象读取返回 410，原消息与删除证据保留。

删除命令成功仅表示已受理，物理 deleted 与额度 released 分别确认。只有 deleted 且 charged_bytes/reserved_bytes 均为 0 才显示空间已释放；未知余额或失败继续显示待处理。恢复沿用原 upload/share/message ID；客户端可以跨设备解密已上传管理记录并恢复未发布的链接或消息，已完成 publication 不重复发送。未确认预留的本地任务也保留；未过期时远端一次查无记录不能证明取消。

桌面提供云端空间、全局分享链接和备份三个视图，支持来源关联、筛选、费用预览和逐项反馈的批量操作（每次最多 50）。文件仅在本机删除不影响云端副本；删除云端副本也不能收回他人已经下载的文件。明细扫描不是跨资源原子快照，差额须刷新核对，不能直接改配额。新增 Rust 互操作向量可由 `print_storage_management_vector` 测试输出。

## 私有文件直读（2026-10-23）

普通 backup/file/share 的下载授权接口是 `GET /v1/{accounts|channels}/:resource/objects/:upload/download`，沿用 HTTP PoP；成功 data 为 `{upload_id,expires_at,manifest:{url,size,digest},chunks:[{url,size,digest},...]}`。URL 仅供精确对象 GET，最长 60 秒，期限不超过原请求和认证证据。客户端把上传者签名计划/已认证 manifest 的摘要作为预期值，核对响应对应关系、实际大小、SHA-256 与解密完整性。下载响应本身不替代上传者签名。

字节直接从私有 S3 端点读取，不携带设备证明、Cookie、Referer，不跟随重定向。只接受 HTTPS 的 R2 账户域名（包括 jurisdiction）；本地开发可接受当前 loopback 中继的 `/__r2/` 适配器。URL 仅驻留本次操作内存，不写入本地目录或普通分享链接。403/404/暂时网络错误进行一次有界重新授权并仅重试失败块；撤销/删除由授权响应终止，授权仍成功但缺块是完整性失败。根、vault、epoch envelopes 保留逐次设备鉴权读取。

分享 descriptor 仍是 `GET /public/shares/:share`。公开 `/manifest` 和 `/chunks/:index` 移除：页面生成非零随机 32 字节 session ID，`POST /public/shares/:share/downloads/:session` 开始或重试同一会话，`GET` 同一路径续签。data 增加 `session_expires_at`，会话固定最多一小时且不超过分享期限；续签不延长、不计新次数，不存在/过期拒绝。开始响应丢失仍用原 ID 重试，已知会话只能 GET 续签，显式新下载才生成新 ID。会话 ID 是 bearer secret，只保存在页面内存。

次数限制表示新下载会话数；已经开始的会话在次数用尽后仍可完成。撤销/删除停止新签发，但原 URL 在截止或物理删除前可重复读取密文；URL 不含解密密钥，已开始的传输和已保存副本不可收回。客户端完成状态晚于全文件校验。跨语言样本由 `downloads::tests::print_download_vector` 输出，固定测试密钥不证明真实部署。
