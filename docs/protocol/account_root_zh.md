# 账户控制与根封装合同

[English](account_root.md) | 简体中文

核对日期：2026-10-24，公开源码 `9fd70dca`。本文描述桌面客户端的认证绑定、按账户分开的工作区和全设备丢失恢复；发布工件与具体部署验收分别核对。
此文描述公开客户端实现，不代表生产 key 或生产部署已验收。

## 账户控制

`dmsg_client::account` 直接使用 `dmsg_types` 的命令类型，签名消息与 canister 用同一个 `dmsg_protocol` 编码为确定性 CBOR：Principal、AccountId 和固定字节字段为 byte string，unit variant 为文本。账户创建使用 `dmsg/create-account/v1`，home 配置了准入公钥时另附云端对 `dmsg/account-admission/v1`（home、caller、期限）签发的票据；账户变更使用 `dmsg/device-approval/v2`、`dmsg/account/v2` 和 `dmsg/account-operation/v2`；文档认证使用 `dmsg/attest/v1`，根派生使用 `dmsg/derive-root/v1`，恢复设备 PoP 使用 `dmsg/recovery-device/v1`，controller 注册 PoP 使用 `dmsg/controller-pop/v1`。签名覆盖实际 home/caller、账户、命令、版本、设备序号、请求 ID 和期限。

user home 不在客户端配置中固定。客户端读取 `dmsg_handle.get_handle_config()` 的 `user_homes`（权威列表；`config.toml` 的 `user_homes` 只作钉定，必须都在列表中）和 `registration_homes`（当前接收新账户的子集，随机选择一个注册）。登录后并行对列表中的每个 home 调用 `my_account`，全部成功后选取列表中首个命中的账户；任一查询失败视为未知，不落到注册。账户的 `home_user` 随账户固定。

客户端在联网前把完整请求写入工作区的加密日志。响应丢失时通过 `my_account` / `get_operation` 查询原操作；不会把认证成功或返回账户 ID 当成设备批准。当前账户安全叶（schema 4）、完整设备 map、版本及根摘要先经 IC 证书校验；本机已经看到的更高版本和已安装根不得被旧响应覆盖。

新设备请求绑定目标账户、home、设备公钥、角色、能力、预期版本、请求 ID；管理员核对后批准。默认成员为 ContentSign/VaultUnlock，默认管理员另含 RootManage，不默认开启 FormalApprove/PaymentOffer。根包只封装给持有 `VaultUnlock` 的活跃设备：新设备持有它时，管理员换根后它才能读到根；没有它的设备不在根包中，客户端也不尝试打开根。云端同样按 `VaultUnlock` 放行账户密文读取。账户的初始设备和登录恢复的替换设备必须同时持有 `RootManage` 与 `VaultUnlock`。认证绑定分三步：新登录在本机生成 nonce 并把请求（home、账户、Principal、nonce）交给管理员设备；管理员批准 `BindAuth { principal, nonce }`，账户内登记 10 分钟有效的待接受项；新登录用同一 nonce 调用 `accept_auth_binding` 后才绑定。登录身份不等同于设备授权。

客户端的认证绑定（`dmsg_client::security`）有两种走法，管理员一侧都经审批中心批准：

- **跨设备**：没有账户的新登录生成请求令牌（base64url 的确定性 CBOR `{format: "dmsg-auth-request/1", home, account, principal, nonce, expires_at}`，5 分钟有效），nonce 记在该登录的工作区；管理员设备核对令牌的 home、账户与期限，并由人输入对方显示的登录 Principal 后创建审批；批准后新登录调用 `accept_auth_binding`，随即进入该账户的工作区（本机已有该账户的已绑定工作区时直接解锁，否则等待配对或恢复）。
- **同一设备**：已解锁的设备发起一次“用于绑定”的登录，daemon 收到第二个登录的 delegation 后不切换账户，而是先确认它没有账户，再生成 nonce 并创建审批；批准即提交 `BindAuth`，随后用第二个登录的身份调用 `accept_auth_binding`。该身份只留在内存，至多等待 10 分钟。

`RemoveAuth`（不能移除当前使用的登录）、`SetPolicy`（冻结、每日执行次数、允许的用途）、`SetRecoveryDelay` 与 `DisputeRecovery` 同样经审批中心。审批记录里登录 Principal 以文本保存，签名前才转换为命令。

## 本机工作区

每个账户在本机有自己的工作区，账户之间互不可见：`<home>/<environment>/accounts/<account>/` 存放该账户的 `workspace.sqlite` 与 Agent 连接器状态 `agents.sqlite`；还没有账户的登录用 `logins/<principal>/`，等它有了账户（创建、被绑定）后整个目录改名为 `accounts/<account>/`。登录时客户端用 `my_account` 找到该登录的账户并选择对应工作区，没有时选择该登录自己的工作区；同一账户的不同登录共用一个工作区。`current` 记录最近选择的账户，daemon 重启和快速解锁使用它；切换到另一个账户的工作区时先锁定，其驻留密钥一并丢弃。每个工作区有自己的设备 ID 与密钥，所以同一台电脑上的不同账户是不同的设备。

## 本机解锁

没有本地解锁口令、恢复码或服务外离线恢复包；文件导出不属于账户恢复。本机数据密钥 LDK 只在以下两种钥匙下封装：

```text
name        = "dmsg:" + environment + ":" + account + ":" + deviceId
LUK         = HKDF(unlock_secret, ["dmsg/local-unlock/2", name])
wrappedKey  = E(LUK, LDK, ["dmsg/local-key/2", name])
fastWrapped = E(K_fast, LDK, ["dmsg/local-key/2", "fast:" + name])   // 可选
```

- `unlock_secret(account_id, device_id)` 是 user home 的认证 query：caller 必须是账户的登录 Principal，设备必须存在且未撤销，返回 32 字节 `digest("dmsg/unlock-secret/v1", (master_secret, account_id, device_id))`，其中 `digest(domain, value) = SHA256(CBOR([1, domain, value]))`。`master_secret` 在 home 初始化的 timer 中由 `raw_rand` 生成一次，升级保留。撤销设备后该设备的解锁材料不再发放。
- 新工作台在绑定账户前只用一把明文保存的临时随机键封装 LDK；此时本机只有新生成的设备密钥，没有任何内容，写入被拒绝。账户创建或配对批准后，客户端取得 `unlock_secret`，用 LUK 重封 LDK 并删除临时键。
- 日常解锁：经 dmsg.net 登录页的 II 登录（[dmsg-login/1](login-v1_zh.md)，30 天 delegation，会话私钥由 daemon 持有）→ `unlock_secret` → LUK → LDK → Bundle，全部只在内存。`login_unlocked_at` 记录最近一次登录解锁。
- 快速解锁：登录解锁后默认开启，人关闭后保持关闭；`secure_storage = "file"` 时没有快速解锁。`K_fast` 只封装 LDK 的第二份副本：macOS 上由 Secure Enclave 的密钥协商恢复，每次解锁要求 Touch ID 或 Mac 登录密码，并恢复以 LDK 封存的 30 天登录；其他平台是存于凭据库的随机 32 字节，没有用户在场验证，也不保存登录。距上次登录解锁超过 30 天时拒绝；快速解锁后一旦联网就核对认证设备表，已撤销则清空本机数据（封存的登录随之删除）。

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
- 恢复信封：`ic-vetkeys` 的 `IbeCiphertext::encrypt(dpk, identity, VRK, seed)`，`dpk` 是 COSE `root_public_key(account_id, generation)` 返回的 96 字节派生公钥（所有账户共享同一 context 公钥），`identity = CBOR([AccountId bytes12, generation])`。客户端核对描述中的 home、环境、代次、派生版本 2、账户、`SHA256(public_key)` 指纹与 key 名（生产为 `key_1`）。
- `previous.envelope` 以新 VRK 包装上一代 VRK，AAD 为 `["dmsg/previous-root/1", account, generation, previous.generation, previous.digest]`。`previous.digest` 是上一份 bundle **字节**的 SHA-256（即云端 manifest digest），`uploadId` 定位它；读者验证摘要并要求代次严格递减，至多读取 256 代。
- 链上 `ContentRootRef { generation, suite: "dmsg-root-v2", recipients_digest, body_digest, bundle_digest }`：`recipients_digest = digest("dmsg/root-recipients/1", [sorted device_id bytes, generation])`，`body_digest = SHA256(CBOR(body))`，`bundle_digest = digest("dmsg/root-bundle-digest/2", [recipients_digest, body_digest])`。`CommitRoot` 用持有 `VaultUnlock` 的当前活跃设备集合重算 `recipients_digest`，并要求批准设备本身在其中，任一不符则拒绝，因此已撤销或没有 `VaultUnlock` 的设备收不到新根，未批准设备也不能被塞进根包。

单份 bundle 最多 64,000 字节。通过 cloud upload 的 `kind=root` 保存：manifest 是原始 bundle 字节；必须存在的一块为确定性 CBOR 空数组 `0x80`。云端 ObjectRef 的 digest 等于 manifest SHA-256，客户端据此下载并按链上 `bundle_digest` 验证。

## 初始化与换根顺序

1. 验证账户与设备能力；用单独 op_id 预留根代次（`ReserveRoot`）。丢弃/过期预留可能产生代次间隙。
2. 本地生成随机 VRK（换根任务连同候选根用 LDK 封存为本机记录 `root-job`，重试复用），按认证设备表封装给每台持有 `VaultUnlock` 的活跃设备，查询并核对 COSE 恢复公钥后生成 IBE 恢复信封，生成唯一 bundle 字节。
3. 上传计划与 upload_id 持久化；重试查询同一上传，复用计划和密文。上传并 finalize 后，下载 manifest 再核对 SHA-256。
4. 执行唯一的 user `CommitRoot` CAS。上传成功不能替代链上提交成功。
5. 读回认证承诺后启用新根：首次绑定时工作区才归属账户（此前没有内容）；换根后，旧根经新根包的 `previous` 链读取。

全程没有链上密钥调用。只有根包接收者变化（新增或撤销持有 `VaultUnlock` 的设备，或为设备增减这项能力）和登录恢复会让 `vault_write_state` 变为 `RekeyRequired`，管理员换根即可；认证绑定和其他能力的变化只递增 security epoch。其他接收者下载当前根包、解开自己的信封即可读取。

## 全设备丢失恢复

客户端实现见 `dmsg_client::recovery`：申请封存在新设备的工作区，回复丢失时原样重发（同一申请不会重新开始等待）；派生的批准与传输密钥种子按承诺的根摘要封存为 `recover-root:<digest>`，结果先按执行 ID 读取，原批准过期且认证时间与设备序号证明它不可能再执行时才重新批准；`max_cycles` 固定为 700 亿。完成后客户端解开恢复信封、沿 `previous` 链取得历史根，再换根。

1. 新设备用绑定过的 II 账户登录，提交 `request_recovery(account_id, RecoveryRequest { op_id, new_auth = caller, device, expires_at }, device_proof)`，设备 PoP 为 `dmsg/recovery-device/v1`。II 也丢了就不能恢复。
2. 等待 `recovery_delay_ms`（默认 3 天，`SetRecoveryDelay` 可设 1–7 天）。等待期间已有设备从认证叶 `pending_recovery_digest` 看到申请，任意有效设备提交 `DisputeRecovery { op_id }` 即取消；用 `RemoveAuth` 解除发起申请的登录同样作废申请。没有再确认流程。
3. 到期后 `complete_recovery(account_id, op_id)`：替换全部设备与登录绑定，`security_epoch` +1，已有根置 `RekeyRequired`，记录 `recovered_device = (device_id, generation)`。
4. 恢复设备取得 `unlock_secret` 完成本机绑定，然后 `derive_root(DeriveRootRequest { generation, transport_public_key, max_cycles, approval })`：只允许 `recovered_device` 对当前代次派生（同一 `request_id` 重试返回原结果，换根前也可凭新批准再次派生），COSE 派生该身份的 vetKey 并加密给传输公钥，客户端解密并核对 vetKey 后用它解开根包的 IBE 恢复信封，随即换根到下一代（只封装给自己）。换根后派生权消失。

恢复派生的批准域为 `dmsg/derive-root/v1`，命令为 `[generation, transport_public_key, max_cycles]`；`max_cycles` 是客户端在批准中写定的上限。派生与正式认证共用账户政策的每日执行次数（默认 20，`SetPolicy` 最高 64）和 64 条执行保留窗口。II 被盗且延迟期内所有设备都没响应等于内容泄露，这是取消恢复码的代价。
