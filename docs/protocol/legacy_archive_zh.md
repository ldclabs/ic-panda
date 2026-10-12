# Legacy 档案与一次性迁移文件（v1）

2026-10-13。本合同覆盖已实现的只读收集、客户端加密交接、桌面端保管、离线阅读与冻结对照。它不证明旧服务已经冻结，也不授予名称、设备或共享频道所有权。真实 Local/ECDH/VetKey 账户恢复仍待授权样本验收。

## 读取边界

`@dmsg/legacy` 独立于旧业务 store。调用同时限制服务类别、方法和盘点过的 canister；每次 await 前后核对旧 Principal。允许读取，以及 `my_iv`、旧 ECDH/vetKD 和下载 token 等必要恢复 update。不存在创建、更新 setting、已读、交换确认、PoL 或初始化入口。

原站 IndexedDB 使用只读事务，既不创建缺失数据库也不升级旧库。归档保留选定 MK 包装、静态 ECDH 包装、频道 KEK、缓存消息和上传记录；不导出缓存口令派生值或下载 bearer token。无随机 Local KEK 时返回缺钥，不生成替代钥。

COSE 设置仅在明确 NotFound 时读取历史 namespace-owned 路径；权限/网络错误不触发该回退。保留原 keyId CBOR、Principal AAD、实际 ownership、namespace、版本、myIV 和已有包装。远端公钥校验与服务端派生上下文是不同证据；声明的历史 context/key 配置不能代替部署或真实样本核验。

旧文件格式为整文件 COSE_Encrypt0。OSS 传输片为 256 KiB，最后一片使用实际长度。读取验证索引、长度、缺口、可用的源哈希及复制前后元数据；重组后才进行整文件 AEAD。不得将这些传输片标为新格式的独立认证块。

## `dmsg-legacy-archive/1`

格式为有界 CBOR，字段以 `src/dmsg_legacy/src/archive.ts` 的严格 schema 为准：

- `inventory`：旧 Principal、message canister、显式模式、`pre_migration` 来源状态、对象、缺口及无参数/无 token 的调用记录。
- `objects[]`：稳定来源键、类型、原观察编码、SHA-256、观察时间及 `query_observation` / `local_cache` 等级。观察编码保留 Candid Principal、整数、bytes、数组和 map 的类型；不将普通 query 包装成 ICP 认证快照。
- `keys[]`：历史 COSE_Key 和原参数，仅允许同一个旧 Principal。该字段必须始终处于交接 HPKE 或本地内容加密之内。
- `checks[]`：固定对象摘要及解密/系统明文/删除/仅密文验证结果。导入者离线重新计算验证，不信任导出者提供的成功标签。
- `createdAt`：本次客户端收集时间，不是可信时间戳。

同一来源键不同 bytes 拒绝覆盖。删除洞、缺块、权限、网络、版本和缺钥分别记录，无法访问不等于不存在。旧作者和时间只是来源声明，不补造新设备作者签名。

## `dmsg-legacy-pair/2` 与 `dmsg-legacy-transfer/1`

接收设备（桌面端 `dmsg legacy pair`）生成随机一次性接收种子、X25519 公钥和 nonce。配对 offer 为 `{version: "dmsg-legacy-pair/2", nonce, publicKey, origin, target, expiresAt}`：`origin` 为 `https://dmsg.net` 或 `https://panda.fans`，`target` 为接收设备 ID（64 位小写十六进制），有效期十分钟。设备用系统浏览器打开 `<origin>/legacy#pair=<base64url(CBOR(offer))>`；fragment 不到达服务器，页面读取后从地址栏移除。页面只接受规范 CBOR、与自身 origin 一致且未过期的 offer。双方显示 offer CBOR 的 SHA-256 指纹、目标设备与旧身份，由用户逐项核对。种子只以设备本地数据密钥加密保存在设备工作区，不同步。

交接文件使用 RFC 9180 X25519/HKDF-SHA256/AES-256-GCM HPKE。每块至多 192 KiB 明文；HPKE info/AAD 为 CBOR `[header,index,previous]`，header 含格式、完整 offer、随机 transfer ID、块数和总长度，previous 为前一帧编码的 SHA-256（首帧为空）。文件内的 offer 必须与设备保存的 offer 完全一致；缺帧、乱序、重复、替换、错误目标、错误 origin、错误指纹或首次接收时已过期均拒绝。

设备在保存内容前持久化本次文件摘要和接收时间；该 offer 此后只能继续同一文件，同一文件再次导入返回既有档案。档案按内容标识（排除收集时间与传输信息）去重，同一内容的再次导出也返回既有档案。完成后删除接收种子。整个加密交接文件（包括 framing）上限 256 MiB，超限拒绝，不截断。

字节规则的跨语言向量见 `tests/vectors/client/legacy-v1.json`：页面库 `@dmsg/legacy` 生成并打开，桌面 Rust 客户端以同一文件核对。

## 桌面保管与恢复

导入者验证旧身份、对象摘要并离线重新打开本人历史内容（消息、附件、头像），导出者自带的检查结果只用于比对，不一致记为缺口。档案作为账户的加密内容保存：至多 32 MiB 一段的 `migration_part` 记录（各携带一个文件）和一条 `migration` 记录（档案摘要、内容标识、分段、旧身份、缺口数与冻结对照）。内部拆分是存储实现，不是多卷备份。历史钥只在档案明文内，不进入普通文件列表、云端明文或设备授权流程。

档案随云端同步到账户已批准的设备，其他设备同样可离线阅读；排除配对种子和设备私钥；没有离线恢复包，新设备仍需链上批准。档案的缺口与最终切换状态独立记录。

冻结后，原站按频道导出最终历史证明（CBOR：频道的 `ChannelAuthority` 页、全部 `Messages` 页与附件 manifest 页）。设备以已审核的冻结批次核验证书，逐频道列出变化、新增或缺失的消息、频道钥变化与附件差异，结果按频道记入 `migration` 记录；没有冻结证明的频道计为差异。共享频道继承见 [channel_migration_zh.md](channel_migration_zh.md)。
