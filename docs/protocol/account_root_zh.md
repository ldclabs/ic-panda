# 扩展账户控制与根封装合同

日期：2026-09-22。适用于开发中的 local/staging 扩展；生产发布门禁保持关闭。
此文描述公开客户端实现，不代表正式扩展 origin、生产 key 或生产部署已验收。

## 账户控制

`protocol/account.ts` 将生成的 Candid 参数映射为公开 Rust 协议的确定性 CBOR：Principal、AccountId 和固定字节字段为 byte string，unit variant 为文本。账户创建使用 `dmsg/create-account/v1`；账户变更使用 `dmsg/device-approval/v2`、`dmsg/account/v2` 和 `dmsg/account-operation/v2`。签名覆盖实际 home/caller、账户、命令、版本、设备序号、请求 ID 和期限。

`services/account.ts` 在联网前把完整 Candid 请求加密保存到当前工作区。响应丢失时通过 `my_account` / `get_operation` 查询原操作；不会把认证成功或返回账户 ID 当成设备批准。当前账户安全叶、完整设备 map、版本及根摘要先经 IC 证书校验；本机已经看到的更高版本和已安装根不得被旧响应覆盖。争议状态仅由账户恢复界面显式允许读取，云端内容认证仍要求 Active。

新设备请求绑定目标账户、home、设备公钥、角色、能力、预期版本、请求 ID；管理员核对后批准。默认成员为 ContentSign/VaultUnlock，默认管理员另含 RootManage，不默认开启 FormalApprove/PaymentOffer。认证绑定必须由新 Principal 先登记 nonce，再由现有管理员批准；两者不等同于设备授权。

恢复码按 `recoverySeeds(R, environment, Xid, recovery_generation)` 分域产生 Ed25519/X25519 种子。账户恢复码与 R0 本地恢复码独立。首次设置中断时仅在 LocalDataKey 保护下暂存恢复码；登记与验证完成后清除。恢复请求、争议再确认和完成分别调用公开 user 接口，等待期由 canister 执行。离线恢复始终生成新设备私钥且 `registered=false`。

## 不可变 RootBundle

字节为确定性 CBOR：

```text
{ payload: {
    format: "dmsg-root-bundle/1",
    context: { account, environment, homeCose, generation, opId,
               recoveryGeneration, recoveryPublic, recoverySigningPublic },
    key: { publicKey, fingerprint, keyId, keyName },
    online, recovery: { enc, ciphertext },
    previous: null | { digest, uploadId, generation, envelope },
    device, signingPublic
  }, signature: bytes64 }
```

公钥、COSE_Encrypt0 与 HPKE envelope 字节使用无填充 base64url；摘要、设备/操作/上传 ID 使用小写 hex。account 为规范的 20 字符 Xid，homeCose 为规范 Principal 文本，代次为安全正整数。

- `signature` 是设备 Ed25519 对 `digest("dmsg/root-bundle/1", payload)` 的签名；链上 `bundle_digest` 是整个上述 CBOR 的 SHA-256。签名数学正确不单独证明设备权利；读取当前根还须核对 user 的认证根承诺。
- 内容根为客户端随机 32 字节。vetKD transport secret 仅在密码 Worker 和本机加密候选记录中存在。执行结果须绑定原请求、目标 canister、account、环境、generation、derivation_version=2、算法和 purpose。
- `key.fingerprint = SHA256(raw vetKD public key)`；`keyId` 使用公开 `dmsg/key-id/v3` 域。encrypted VetKey 必须经 `decryptAndVerify`，input 为 `CBOR([AccountId bytes12, generation])`。
- 在线域 `C = ["dmsg/online-root/1", environment, AccountId bytes12, homeCose bytes, generation, 2]`。`K_online = VetKey.deriveSymmetricKey(CBOR(C), 32)`；`online = COSE_Encrypt0(K_online, VRK, C)`。
- 恢复 envelope 使用既有 RFC 9180 X25519/HKDF-SHA256/AES-256-GCM，info/AAD 均为 `CBOR(["dmsg/recovery-root/1", environment, account Xid text, generation])`。仅需可信恢复公钥即可为新 VRK 封装，不需重新输入 R。
- `previous.envelope` 以新 VRK 包装上一代 VRK，AAD 为 `["dmsg/previous-root/1", account, generation, previous.generation, previous.digest]`。digest/uploadId 定位上一份不可变 bundle；读者验证摘要并要求代次严格递减。当前实现至多读取 256 代，不静默截断历史。

单份 bundle 最多 64,000 字节。通过 cloud upload 的 `kind=root` 保存：manifest 是原始 bundle 字节；必须存在的一块为确定性 CBOR 空数组 `0x80`。其摘要/大小进入不可变上传计划。云端 ObjectRef 的 digest 等于 manifest SHA-256，因而与链上承诺一致。

## 初始化、恢复与换根顺序

1. 验证账户、设备能力与恢复公钥；用单独 op_id 预留根代次。丢弃/过期预留可能产生代次间隙。
2. 保存候选 VRK 和 transport secret；保存签过名的 `derive_root` 原请求再发送。未知结果查询/对账同一 request_id，保留原 transport key。
3. 验证 VetKey，生成并保存唯一 bundle 字节。上传计划与 upload_id 持久化；重试查询同一上传，复用计划和密文。
4. 上传并 finalize 后，下载 manifest 再核对 SHA-256；随后执行唯一的 user CommitRoot CAS。上传成功不能替代链上提交成功。
5. 读回认证承诺后才能启用工作区。中断后按原请求、原 upload_id 和摘要继续；预留过期时显式核对链上状态，再保留旧候选并申请新代次。

云端可暂存大于当前代次的 root 候选，以兼容过期预留造成的间隙；这不使候选成为当前根。当前根读取和 vault 写入仍同时核对认证代次与摘要，既有配额和上传槽上限继续适用。

`rootDerivationMaxCycles` 是构建时固定的批准上限，当前默认 70,000,000,000。PocketIC 实测一次约 68,256,433,153 cycles；这不是生产费率承诺。user 安全派生预算单独限制为每天 20 次 / 300,000,000,000 cycles，单次不得超过 100,000,000,000；未知操作不退还预留。默认单次预算允许每日四次申请，第五次明确拒绝。部署时须核对实际费用；客户端不自动提高预算。

## R0 副本转换和备份

R0 的 random32 内容身份保留在原数据库。显式转换先完整验证源对象/附件，在新的 Xid 数据库内为内容版本生成新身份、密钥、AAD 和封装，并验证目标密文；已验证的文件块可保留原 bytes，其 FileKey 只存在于重新加密的条目载荷内。本地频道保持草稿。完成后通过 registry CAS 激活新工作区，将原库标为 retained，不删除源库。

换根保留加密的历史根索引。`dmsg-backup/1` 随元数据保存根 bundle 与受当前 VRK 保护的历史根索引，完整包与账户恢复码可离线打开各代本地内容。控制操作日志、LocalDataKey、设备/认证私钥、vetKD transport secret 和恢复私钥不导出。单包上限仍为 256 MiB。

当前导出范围仍是本机可验证内容。云端完整内容同步、服务端固定导出清单和远端未缓存对象归入后续 A2；本地导出不能声称包含未下载的其他设备内容。
