# dmsg-cloud/1 客户端互操作合同

本文件描述公开扩展 SDK 使用的 wire 合同与内容映射，不包含服务端实现。SDK 位于 `src/dmsg_app/src/lib/protocol/cloud.ts`、`services/cloud-security.ts` 和 `services/relay.ts`。账户和执行证明继续使用现有公开 Rust 类型；本次没有修改 Candid 或正式签名域。

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

客户端在签名前检查 payload 能按相同 bytes 往返；拒绝孤立 UTF-16 代理项。当前 CBOR 依赖会处理字符串开头的 U+FEFF，因此该输入也在签名前明确拒绝，不能删掉字符后代替用户签名。普通中文、emoji 等合法 Unicode 保持原文。

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

允许的 action 由公开 SDK 的 `cloudActions` 固定。`signCloudCommand` 构造通用信封；`profileSchema` 对 profile 提供严格字段校验。其它 action 仍必须使用对应资源合同，通用信封不是任意字段都能被服务器接受的保证。

`CloudClient` 提供证据提交、签名 GET/POST、密文块 PUT/GET、readiness 和 profile 写入。它限制响应大小、禁止重定向/携带 cookies，保留错误 code/retryable/request_id，不自动重试。资金和会员刷新等非命令 POST 的专用适配由后续客户端工作包接入，不能用 profile 方法替代。

## 账户证据

`readCloudSecurity` 查询真实 `security_snapshot_batch` 与 `get_device_bundle`。必须验证固定 user canister、ICP BLS 根、certificate、时间、单段 AccountId 路径和 witness，再检查 issuer/home/account 及完整设备根。

设备根是 `digest("dmsg/devices/v1", BTreeMap<Hash,Device>)`。保留所有公开字段、撤销记录和 next_sequence；key 是原始 bytes，不是 hex 字符串。两次查询之间发生设备变化时，根不匹配则拒绝，不拼接权限快照。

证据桥形状：

```text
{schema:1, canister, certificate,
 entries:[{account_id, value, witness, devices}]}
```

value/witness/certificate 使用原始认证字节，devices 是完整设备 map 的规范 CBOR。缓存截止是 certificate 时间加 60 秒；不能以读取或提交时间续命。调用者只可从验证后的设备记录选择未撤销且具有所需能力的设备，不能将返回的所有记录均当作有效授权。

profile 响应包含签名和 JSON 投影。`verifyCloudProfile` 校验签名、issuer/device/action、原 COSE 摘要、version 与投影一致性。调用者必须提供已认证的设备公钥；数学验证不证明中继给出了最新版本。历史命令的签名验证不因提交 deadline 已过而失效，当前写授权独立判断。

## R0 本地内容到云端的映射

这张表冻结 A1/A2/C1 的转换边界。A1 的账户、根封装与本地副本转换见 [账户与根合同](account_root_zh.md)；普通内容同步仍归 A2，本地 outbox 尚不自动作为云端命令发送。

| 本地来源 | 云端目标 | 转换与校验 |
| --- | --- | --- |
| `WorkspaceMeta.subjectId` | 真实 `account_id` | 本地随机标识不能截短成 Xid；先取得账户与设备批准，显式转换到新工作区 |
| `EncryptedObject.id/revision/parent` | `item_id/revision_id/base_revision` | 保留对象/版本来源映射；父 revision 必须属于同一 item；本地复合 `key` 不作为 wire ID |
| `generation/deviceId` | 根 generation / 签名设备 | 使用已认证账户的已提交根与设备，不能按本机值伪造在线状态 |
| 本地内容 AAD | `['dmsg/content/1', environment, accountIdText, generation, id, revision, deviceId, kind, parent, tombstone]` | 复用版本化内容算法，但正式工作区 subject 值为真实 Xid；转换时重新加密并验证，不能改 metadata 后复用旧密文 |
| 本地 item-key 包装 | `['dmsg/wrap/1', accountIdText, id, generation, revision, 'item-version']` | 用目标根保护每版本内容钥；旧工作区和旧备份保持原 AAD，不覆盖原副本 |
| 加密对象 | `kind=vault` 上传，再签署 vault revision | 序列化记录为规范 CBOR 密文容器；上传块的摘要按实际 bytes 计算，不能拿本地 `record.digest` 冒充上传摘要 |
| vault revision | `{item_id,revision_id,base_revision,upload_id,tombstone,restore}` | 先完成密文上传再引用 upload_id；删除引用 null，恢复必须明确指定当前墓碑；冲突以服务器返回为准 |
| 文件 manifest 中的 `chunks[].size` | upload plan `chunks[].size` | 本地值是明文长度，wire 值是解码 base64url 后实际密文字节长度；保留两个独立口径 |
| 本地文件块 ID `file:version:index` | `upload_id + index` | object_id/version_id 对应文件来源；上传 ID 独立且幂等；原文件块 AAD 仍需保留并验证 |
| 文件 key/name/MIME/明文摘要 | 加密 manifest / vault 内容 | 不放公开 upload plan。manifest_digest/size 指加密 manifest 的实际 bytes；不明文上传 FileKey |
| 空文件 | 加密 manifest + 满足非空块计划的格式容器 | 当前云端要求至少一个非空块；A2 显式表示零长度文件，不能伪造一个明文数据块 |
| 本地 `Profile` | `{version,prev_hash,display_name,bio,avatar_upload,links}` | 仅发布明确选中的公开字段；name→display_name，link→links；contact 另走 inbox policy；头像必须是明确公开的 avatar 对象 |
| 本地 channel/message 草稿 | 签名 genesis/control/epoch/message | 先创建正式频道、接受成员并取得新 epoch；不把本地 `active`、签名或序号直接当在线授权 |
| 本地 outbox receipt | 资源操作查询和实际服务回执 | `sequence/digest` 通用占位不覆盖全部资源；按 request ID 对账并验证资源级返回值 |
| `.dmsg` 备份 | 仍为 `dmsg-backup/1`，单包 256 MiB | 同时保存需要的格式/AAD/源映射、历史封装与缺口；离线恢复不授予链上设备权利 |

这里的“本地文件”指新版 `src/dmsg_app` 的 R0 格式：100 MiB 是明文上限，按 1 MiB 明文独立加密。2026-09-22 A2 实施修订为保留已存在的 R0 密文，单块密文预算统一为 `1 MiB + 64 bytes`，整次上传预算为 `100 MiB + 128×64 bytes + 64 KiB manifest`。这些仅是有界格式开销，资源配额仍按实际密文字节计算，Free 配额不会因本次修订扩大。上限以 writer 真实编码校验；超限明确失败，不静默截断。

2026-09-23：若上传计划到期或账户换根，客户端先查询原修订操作和每个上传状态。已提交对象继续复用；无法继续的 staging 上传明确取消后，以当前内容根和新 `upload_id` 重新封装 manifest。原对象、修订 ID、请求 ID 和密文文件块保持不变；配额释放尚在处理中时保留任务供用户重试。

旧 dMsg 则先在 `ChannelMessages.svelte` 对完整文件执行一次 COSE_Encrypt0，再调用 IC OSS 的 `toFixedChunkSizeReadable` / `upload_chunks`，按 **256 KiB 密文传输片**上传；下载后由 `ChannelFileCard.svelte` 对完整 bytes 解密。IC OSS 的 Rust `file.rs` 与 TS `stream.ts` 均定义 `CHUNK_SIZE=256*1024`。这不是每个 256 KiB 分片独立 AEAD 的文件格式。旧文件迁移保留分片索引/长度与原始 bytes，按原顺序恢复完整 COSE 后验证；不能套用新版 R0 的 1 MiB 明文规则，或将 OSS 分片当作独立认证的新版加密块。此判断基于本地旧客户端与 IC OSS 源码，线上版本仍需 I0 核对。

本表约定了字段、AAD、权限和大小口径。对象块容器固定为规范 CBOR `{format:"dmsg-cloud-object/1", record:EncryptedObject}`；`record` 使用上表正式工作区的字段与既有内容加密规则，保留必要的原始加密字段，不包含明文 Item 或 FileKey。

上传 manifest 的明文固定为规范 CBOR `{format:"dmsg-cloud-manifest/1", account_id, upload_id, object_id, version_id, kind, root_generation, chunks:[{digest,size}], content}`。ID 使用上述 JSON 文本表示，chunks 与 upload plan 一致。vault 的 content 为 null；file 的 content 为现有 FileManifest，包括明文大小/摘要及文件 key，全部处于 manifest 加密层内。使用随机 nonce 的 COSE_Encrypt0 AES-256-GCM，由目标内容根封装；external AAD 为 `['dmsg/cloud-manifest/1', account_id, upload_id, object_id, version_id, kind, root_generation]`。manifest_size/digest 对最终加密 bytes 计算。manifest 不包含自己的摘要，避免循环。

空文件的 manifest 保留 `size=0`、空文件摘要及空文件块列表；上传层使用一个规范 CBOR 容器 `{format:"dmsg-cloud-empty-file/1"}` 作为非空占位块。它不计作明文文件块。恢复程序必须检查空文件标志、零长度和摘要一致性，不能把占位 bytes 返回为文件内容。

A2 需按此映射实现 writer/reader，并在开放 UI 前交付加密容器和 manifest 的往返向量。P0 的测试范围是签名/证据/HTTP/profile 链路，不声称已支持端到端文件同步。legacy 历史归档有独立格式，不能伪装成上述新版内容。

## 验证和发布边界

- 固定命令/HTTP 向量：`src/dmsg_app/tests/fixtures/cloud-v1.json`；由公开客户端生成，配套服务使用独立实现逐字节核验。
- 单元测试：`pnpm --dir src/dmsg_app test`。
- 真实 MV3 / PocketIC / workerd 测试：见扩展 README 的 P0 互操作说明。测试创建全新账户，使用局部测试根；不读取用户浏览器配置，不联系生产 canister。
- 错误 canister/账户、过期证据、错误设备 map、伪证书、篡改签名/HTTP body、错误资源和旧协议版本均有负例。
- `/ready=false` 和生产禁止写入继续有效。真实账户初始化 UI、内容同步 UI、商业 UI 和生产验收分别交付。

## 后台密文与认证新鲜度增量

扩展后台只提交预先签署的固定 vault revision 和查询同一操作的 GET。批准绑定 origin、路径、方法、body digest、账户、设备和最多 45 秒期限；SW 不持有设备/认证私钥或可续期会话。未知响应查原编号，授权过期后暂停。上传、重新授权和本机加密状态确认在解锁后完成。

公开 `Certification::batch` 读取一次 IC 时间，使认证查询依赖当前 batch time；仅加 transport nonce 无法绕过 replica 按 caller/method/args 建立的缓存。该行为与 [IC query cache 实现](https://github.com/dfinity/ic/blob/master/rs/execution_environment/src/query_handler/query_cache.rs) 一致，并有不写账户状态时证书更新的 PocketIC 回归。客户端和 Worker 的 60 秒新鲜度及防回退规则保持不变。

支付配置新增 `get_configuration_certified`：`configuration`、`signer/<u64be>` 和 `fee/<u64be>` 认证叶用于核对收款路由、报价/回执签署钥和费用政策。它不改变已有托管的固定条款或资金状态机。
