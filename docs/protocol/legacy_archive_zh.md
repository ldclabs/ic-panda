# Legacy 档案与一次性迁移文件（v1）

2026-09-22。本合同覆盖已实现的只读收集、客户端加密交接、本地保管和离线阅读。它不证明旧服务已经冻结，也不授予名称、设备或共享频道所有权。真实 Local/ECDH/VetKey 账户恢复仍待授权样本验收。

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

## `dmsg-legacy-transfer/1`

接收扩展生成随机一次性接收种子、公钥和 nonce。配对请求绑定 `https://dmsg.net` 或 `https://panda.fans`、精确 `chrome-extension://<id>` 和十分钟截止时间；双方显式核对请求 CBOR 的 SHA-256 指纹及身份。种子只以 LocalDataKey 加密存入扩展本地库。

交接文件使用 RFC 9180 X25519/HKDF-SHA256/AES-256-GCM HPKE。每块至多 192 KiB 明文；HPKE info/AAD 为 CBOR `[header,index,previous]`，header 含格式、完整配对请求、随机 transfer ID、块数和总长度，previous 为前一帧编码的 SHA-256（首帧为空）。缺帧、乱序、重复、替换、错误目标、错误指纹或首次接收过期均拒绝。

接收者在保存内容前持久化本次已验证文件摘要；该 nonce 只能继续同一文件。完成后删除接收种子，重复提交同一文件返回既有结果，不新建档案。关闭窗口后可重新解锁，继续尚有效或已开始的本次交接。源站收集尚未提供持久分页断点，重新导出需重新配对/读取。

整个加密交接文件（包括 framing）上限 256 MiB；最终 `.dmsg` 恢复包独立按其完整编码检查同一上限。因此 256 MiB 的源内容不保证能装入一个 256 MiB 的恢复包。超限拒绝，不截断、不生成虚假的完整声明。

## 扩展保管与恢复

导入者验证旧身份、对象摘要和本人历史内容，将档案作为加密 `migration_part` 文件记录和 `migration` 清单保存。内部拆分是本地存储实现，不是多卷备份；恢复仍只有一个 `.dmsg` 文件。历史钥不会进入普通文件下载列表、云端明文或设备授权流程。

恢复包包含所有档案分段，排除配对种子和设备私钥。空白浏览器可在主站/ICP/旧服务不可达时恢复并重新验证已保留档案；新设备仍未获得链上批准。档案的部分缺口与最终切换状态独立于备份是否完整保存本机内容。云端迁移同步、最终冻结核对、真实样本及共享继承仍是独立工作。
