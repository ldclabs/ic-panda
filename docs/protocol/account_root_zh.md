# 扩展账户控制与根封装合同

日期：2026-10-08。适用于开发中的 local/staging 扩展；生产发布门禁保持关闭。
此文描述公开客户端实现，不代表正式扩展 origin、生产 key 或生产部署已验收。

## 账户控制

`protocol/account.ts` 将生成的 Candid 参数映射为公开 Rust 协议的确定性 CBOR：Principal、AccountId 和固定字节字段为 byte string，unit variant 为文本。账户创建使用 `dmsg/create-account/v1`，home 配置了准入公钥时另附云端对 `dmsg/account-admission/v1`（home、caller、期限）签发的票据；账户变更使用 `dmsg/device-approval/v2`、`dmsg/account/v2` 和 `dmsg/account-operation/v2`；文档认证使用 `dmsg/attest/v1`，根派生使用 `dmsg/derive-root/v1`，恢复设备 PoP 使用 `dmsg/recovery-device/v1`，controller 注册 PoP 使用 `dmsg/controller-pop/v1`。签名覆盖实际 home/caller、账户、命令、版本、设备序号、请求 ID 和期限。

user home 不再在构建配置中固定。客户端启动时读取 `dmsg_handle.get_handle_config()` 的 `user_homes`（权威列表，可与构建配置 `canisters.userHomes` 交叉核对）和 `registration_homes`（当前接收新账户的子集，随机选择一个注册）。登录后对列表中的每个 home 并行调用 `my_account`，命中即连接该 home；任一查询失败视为未知，不落到注册。账户的 `home_user` 随账户固定。

`services/account.ts` 在联网前把完整 Candid 请求加密保存到当前工作区。响应丢失时通过 `my_account` / `get_operation` 查询原操作；不会把认证成功或返回账户 ID 当成设备批准。当前账户安全叶（schema 4）、完整设备 map、版本及根摘要先经 IC 证书校验；本机已经看到的更高版本和已安装根不得被旧响应覆盖。

新设备请求绑定目标账户、home、设备公钥、角色、能力、预期版本、请求 ID；管理员核对后批准。默认成员为 ContentSign/VaultUnlock，默认管理员另含 RootManage，不默认开启 FormalApprove/PaymentOffer。根包只封装给持有 `VaultUnlock` 的活跃设备：新设备持有它时，管理员换根后它才能读到根；没有它的设备不在根包中，客户端也不尝试打开根。云端同样按 `VaultUnlock` 放行账户密文读取。账户的初始设备和登录恢复的替换设备必须同时持有 `RootManage` 与 `VaultUnlock`。认证绑定分三步：新登录在本机生成 nonce 并把请求（home、账户、Principal、nonce）交给管理员设备；管理员批准 `BindAuth { principal, nonce }`，账户内登记 10 分钟有效的待接受项；新登录用同一 nonce 调用 `accept_auth_binding` 后才绑定。登录身份不等同于设备授权。

## 本机解锁

没有口令、恢复码和离线导出。本机数据密钥 LDK 只在以下两种钥匙下封装：

```text
dbName      = "dmsg:" + environment + ":" + subjectId + ":" + deviceId
LUK         = HKDF(unlock_secret, ["dmsg/local-unlock/2", dbName])
wrappedKey  = E(LUK, LDK, ["dmsg/local-key/2", dbName])
K_prf       = HKDF(prf_output, ["dmsg/local-unlock-prf/1", dbName])
prfWrapped  = E(K_prf, LDK, ["dmsg/local-key-prf/1", dbName])        // 可选
```

- `unlock_secret(account_id, device_id)` 是 user home 的认证 query：caller 必须是账户的登录 Principal，设备必须存在且未撤销，返回 32 字节 `HKDF(master_secret, account_id, device_id)`。`master_secret` 在 home 初始化的 timer 中由 `raw_rand` 生成一次，升级保留。撤销设备后该设备的解锁材料不再发放。
- 新工作台在绑定账户前只用一把明文保存的临时随机键封装 LDK（`LocalEnvelope.provisional`）；此时本机只有新生成的设备密钥，没有任何内容，写入被拒绝。账户创建或配对批准后，客户端取得 `unlock_secret`，用 LUK 重封 LDK 并删除临时键。
- 日常解锁：II 登录（15 分钟 delegation，会话私钥由 Worker 持有、解锁前即可签名）→ `unlock_secret` → LUK → LDK → Bundle，全部只在内存。`loginUnlockedAt` 记录最近一次登录解锁。
- 生物识别快速解锁：平台 passkey 的 WebAuthn PRF 扩展，salt 为 `HKDF(0, ["dmsg/prf-salt/1", dbName])`。启用动作在登录解锁后进行，凭据 ID 与 `prfWrapped` 存 IDB，PRF 输出不落盘。距上次登录解锁超过 7 天时拒绝 PRF 解锁；PRF 解锁后一旦联网就核对认证设备表，已撤销则清空本机数据。扩展 origin 下的真实 WebAuthn 调用尚未在 Chrome 上实测。

## 不可变 RootBundle v2

字节为确定性 CBOR：

```text
{ body: {
    format: "dmsg-root-bundle/2",
    context: { account, environment, generation, opId, securityEpoch },
    commitment: hex(SHA256(HKDF(VRK, ["dmsg/root-commitment/1"]))),
    envelopes: [ { device, enc, ciphertext } ... ],      // 按 device 升序，每台持有 VaultUnlock 的活跃设备一项
    recoveryKey: { homeCose, keyName, publicKey },        // COSE 内容根 vetKD 公钥
    recovery: base64url(IbeCiphertext),                   // IBE 到身份 CBOR([AccountId, generation])
    previous: null | { digest, uploadId, generation, envelope },
    device, signingPublic
  }, signature: bytes64 }
```

公钥、COSE_Encrypt0 与 HPKE envelope 字节使用无填充 base64url；摘要、设备/操作/上传 ID 使用小写 hex。account 为规范的 20 字符 Xid，代次为安全正整数，`securityEpoch` 为预留槽的安全版本。

- `signature` 是设备 Ed25519 对 `digest("dmsg/root-bundle/2", body)` 的签名。签名数学正确不单独证明设备权利；读取当前根还须核对 user 的认证根承诺。
- 设备信封：`HPKE.Seal(device.hpke_pub, VRK, info = aad = CBOR(["dmsg/device-root/1", environment, account, generation, device_id]))`，复用频道 epoch 密钥的 RFC 9180 X25519/HKDF-SHA256/AES-256-GCM 原语。读取方解开后核对 `commitment`，保证所有设备拿到同一个根。
- 恢复信封：`@icp-sdk/vetkeys` 的 `IbeCiphertext.encrypt(dpk, identity, VRK, seed)`，`dpk` 是 COSE `root_public_key(account_id, generation)` 返回的 96 字节派生公钥（所有账户共享同一 context 公钥），`identity = CBOR([AccountId bytes12, generation])`。客户端核对描述中的 home、环境、代次、账户、`SHA256(public_key)` 指纹、生产 key 名，以及可选的构建 pin `coseRootPublicKey`。
- `previous.envelope` 以新 VRK 包装上一代 VRK，AAD 为 `["dmsg/previous-root/1", account, generation, previous.generation, previous.digest]`。`previous.digest` 是上一份 bundle **字节**的 SHA-256（即云端 manifest digest），`uploadId` 定位它；读者验证摘要并要求代次严格递减，至多读取 256 代。
- 链上 `ContentRootRef { generation, suite: "dmsg-root-v2", recipients_digest, body_digest, bundle_digest }`：`recipients_digest = digest("dmsg/root-recipients/1", [sorted device_id bytes, generation])`，`body_digest = SHA256(CBOR(body))`，`bundle_digest = digest("dmsg/root-bundle-digest/2", [recipients_digest, body_digest])`。`CommitRoot` 用持有 `VaultUnlock` 的当前活跃设备集合重算 `recipients_digest`，并要求批准设备本身在其中，任一不符则拒绝，因此已撤销或没有 `VaultUnlock` 的设备收不到新根，未批准设备也不能被塞进根包。

单份 bundle 最多 64,000 字节。通过 cloud upload 的 `kind=root` 保存：manifest 是原始 bundle 字节；必须存在的一块为确定性 CBOR 空数组 `0x80`。云端 ObjectRef 的 digest 等于 manifest SHA-256，客户端据此下载并按链上 `bundle_digest` 验证。

## 初始化与换根顺序

1. 验证账户与设备能力；用单独 op_id 预留根代次（`ReserveRoot`）。丢弃/过期预留可能产生代次间隙。
2. 本地生成随机 VRK（候选记录用 LDK 加密保存在 `local_private`，重试复用），按认证设备表封装给每台持有 `VaultUnlock` 的活跃设备，查询并核对 COSE 恢复公钥后生成 IBE 恢复信封，生成唯一 bundle 字节。
3. 上传计划与 upload_id 持久化；重试查询同一上传，复用计划和密文。上传并 finalize 后，下载 manifest 再核对 SHA-256。
4. 执行唯一的 user `CommitRoot` CAS。上传成功不能替代链上提交成功。
5. 读回认证承诺后启用工作区（`activateAccountRoot`）：首次绑定把 `subjectId` 改为账户 Xid（此前没有内容），换根时把旧根并入加密的历史根索引。

全程没有链上密钥调用。只有根包接收者变化（新增或撤销持有 `VaultUnlock` 的设备，或为设备增减这项能力）和登录恢复会让 `vault_write_state` 变为 `RekeyRequired`，管理员换根即可；认证绑定和其他能力的变化只递增 security epoch。其他接收者下载当前根包、解开自己的信封即可读取（`openCurrent`）。

## 全设备丢失恢复

1. 新设备用绑定过的 II 账户登录，提交 `request_recovery(account_id, RecoveryRequest { op_id, new_auth = caller, device, expires_at }, device_proof)`，设备 PoP 为 `dmsg/recovery-device/v1`。II 也丢了就不能恢复。
2. 等待 `recovery_delay_ms`（默认 3 天，`SetRecoveryDelay` 可设 1–7 天）。等待期间已有设备从认证叶 `pending_recovery_digest` 看到申请，任意有效设备提交 `DisputeRecovery { op_id }` 即取消；用 `RemoveAuth` 解除发起申请的登录同样作废申请。没有再确认流程。
3. 到期后 `complete_recovery(account_id, op_id)`：替换全部设备与登录绑定，`security_epoch` +1，已有根置 `RekeyRequired`，记录 `recovered_device = (device_id, generation)`。
4. 恢复设备取得 `unlock_secret` 完成本机绑定，然后 `derive_root(DeriveRootRequest { generation, transport_public_key, max_cycles, approval })`：只允许 `recovered_device` 对当前代次派生（同一 `request_id` 重试返回原结果，换根前也可凭新批准再次派生），COSE 派生该身份的 vetKey 并加密给传输公钥，客户端 `decryptAndVerify` 后用它解开根包的 IBE 恢复信封（`recoverCurrent`），随即换根到下一代（只封装给自己）。换根后派生权消失。

恢复派生的批准域为 `dmsg/derive-root/v1`，命令为 `[generation, transport_public_key, max_cycles]`；`rootDerivationMaxCycles` 是构建时固定的批准上限，默认 70,000,000,000。派生与正式认证共用账户政策的每日执行次数（默认 20，`SetPolicy` 最高 64）和 64 条执行保留窗口。II 被盗且延迟期内所有设备都没响应等于内容泄露，这是取消恢复码的代价。
