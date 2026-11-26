# dmsg-passkey/1：Passkey 登录与按需续期

公开接口以 [dmsg_auth.did](../../src/dmsg_auth/dmsg_auth.did) 和 [Rust 类型](../../src/dmsg_types/src/auth.rs)为准。此登录得到独立的 dMsg 登录 Principal，通过现有账户绑定接入 AccountId；不会把 II Principal 或它的外部资产迁移到新身份。II 的 [dmsg-login/1](login-v1_zh.md) 继续提供。

## 认证接口

| 调用                                  | 说明                                                                                                                                          |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `prepare_challenge(ChallengeRequest)` | P-256 硬件钥匙直接作为 caller，公钥必须与 caller 一致；返回五分钟一次性 challenge，绑定 Register、Login 或 LogoutAll                          |
| `register(RegisterPasskey)`           | 验证注册和可选准入票据，建立单 Passkey 身份及本机会话；每个硬件 caller 只允许注册一个身份，退出后也不重用其准入票据；返回 delegation 截止纳秒 |
| `login(AuthenticatePasskey)`          | 验证该身份的 Passkey assertion，登记本机会话；每身份最多 16 个，不自动淘汰                                                                    |
| `renew()`                             | caller 必须已有本机会话；无 Passkey 交互和跨 canister 查询，签发新的 12 小时 delegation                                                       |
| `get_delegation(public_key)`          | query 取得最近签发的公钥、目标设备公钥、截止和认证签名；客户端立即获取并验证签名后缓存                                                        |
| `logout()`                            | 结束 caller 的续期资格；本机独立清理凭据，网络失败不排队                                                                                      |
| `logout_all(AuthenticatePasskey)`     | 新鲜 Passkey 验证后删除该身份全部会话，旧认证结果不可重建                                                                                     |

时间通常为 Unix 毫秒；delegation `expiration` 为 IC 纳秒。Candid 与浏览器 CBOR 操作名均为 `register/login/logout_all`。

新增会话触及每身份 16 个会话上限时返回 `SessionLimitExceeded`，客户端错误码为 `SESSION_LIMIT_EXCEEDED`。全局限流、续期间隔及其他容量限制继续返回 `QuotaExceeded` / `QUOTA_EXCEEDED`。桌面和 iOS 仅在 Passkey 登录收到会话上限错误时提示退出所有设备的登录；普通额度错误保留重试入口。

交互总预算同时计入准备与完成请求；准备阶段每分钟最多使用总预算的一半（向下取整，最少一次），为完成请求保留额度。不存在、过期或不匹配的 challenge 不消耗共享额度；匹配 challenge 后的认证失败仍计入预算。每个 caller 每分钟最多八次交互请求，替换 challenge 或退出不会重置计数；临时计数表每分钟最多跟踪十万个 caller，满时拒绝新 caller，不驱逐已跟踪者。总预算设为一时，准备和完成须分属不同分钟。

会话每分钟最多延长一次 delegation。升级清空签名缓存后，仍处于该间隔内的会话可通过 `renew` 补发原授权，保持原到期时间；补发仍计入全局续期预算，签名恢复后不能再次绕过间隔。已经退出的会话不能补发。

服务端仅接受 RP ID `dmsg.net`、ES256、UP 与 UV 标志。网页与 iOS origin 为精确 `https://dmsg.net`；原生 Android 仅接受下节精确证书准入。注册要求 none attestation，凭据 ID 与 attested data 一致；登录验证 authenticatorData 和原始 clientDataJSON 的签名。检查 crossOrigin/topOrigin，拒绝子域 origin。同步 Passkey 的签名计数不作为是否有效的判断依据。

## Android 原生 origin 与发布证书

Android Credential Manager 的 origin 为 `android:apk-key-hash:<base64url-no-padding(SHA-256(installed-signing-certificate-DER))>`。客户端从当前安装包读取证书，仅原样转交返回的 `clientDataJSON`；服务端用其配置的证书哈希生成规范 origin 并精确比较，不接受通配符、任意 `android:` 前缀、追加路径或带填充的编码别名。[Android 官方说明](https://developer.android.com/identity/passkeys/create-passkeys)

`dmsg_auth.android_signers()` 返回当前允许的 SHA-256 列表；治理/controller 使用 `admin_set_android_signers(vec blob32)` 原子替换，配套 `validate_admin_set_android_signers` 可在执行前审阅。最多 8 项，不接受重复或全零值；空列表为默认值，关闭原生 Android 注册/登录/退出全部的断言准入。开发/测试证书只加入隔离测试 issuer，生产列表只登记实际发行证书。服务端不能仅由哈希自动判断其是否是 debug 证书，发布配置审查必须拒绝开发证书。

Android 在申请系统凭据前，使用系统签名轮换链的最后一枚证书（当前 signer）查询签发方准入，注册继续完成 readiness、admission 与 targets 预检查。系统返回凭据后，客户端要求原始 origin 精确匹配此次请求开始时从 `SigningInfo.signingCertificateHistory` 取得的某一枚证书；这兼容凭据提供方使用轮换链中较早证书的情况。缺少签名链或存在多个独立 signer 时拒绝请求。最终认证仍以 canister 当前政策和原始签名为准，预检查不代替权威校验。包名、证书和 `dmsg.net` 的 Digital Asset Links 还需分别配置 `delegate_permission/common.handle_all_urls` 与 `delegate_permission/common.get_login_creds`；这份准入列表不是 DAL 的替代。

证书轮换按安装包实际 signer 同时登记新旧正式证书，完成发行过渡后移除旧项。移除项会拒绝新的 WebAuthn 证明，不自动撤销已登记硬件会话或已经签发的最长 12 小时 delegation；现有 `logout/logout_all` 与账户设备撤销保持独立。生产不使用测试向量中的合成证书。

新增稳定内存 ID 4 独立保存列表；ID 0–3 的配置、salt、身份、会话与注册 caller 索引编码不变。旧 Wasm→新 Wasm 使用原地升级，不重装。测试向量为 [passkey-android-v1.json](../../tests/vectors/client/passkey-android-v1.json)。本地升级测试不表示线上 issuer 已升级或任何证书已准入。

## 浏览器与 iOS

桌面系统浏览器打开 `https://dmsg.net/_/login#<base64url(canonical-CBOR)>`。请求字段固定为：

```text
{ protocol: "dmsg-passkey/1", state: bstr32,
  challenge: { challenge: bstr32, expires_at: uint, action: text },
  next_url: text, credential_id: bstr / null }
```

`next_url` 复用 dmsg-login/1 的回环回调或固定 iOS HTTPS 回调白名单。`state` 仅用于本机关联；服务端 challenge 独立随机且一次性。已记住的登录在 `logout_all` 中限制当前 credential_id；新设备遇到会话上限时可通过可发现的 Passkey 认证退出全部登录，再重新登录。普通登录同样使用 discoverable credential。网页只收集 WebAuthn 结果，不持有设备私钥、登录 delegation 或内容密钥。

成功回调：`{protocol, state, response: {credential_id, client_data_json, authenticator_data, signature}, error: null}`，四个响应字段均为 bytes。注册的 `authenticator_data` 是完整 attestationObject，signature 为空；登录为原始 authenticatorData 与 DER ES256 签名。失败时 response 为 null，error 仅为 `user_cancelled/passkey_error/timeout`。回调通过顶层导航携带 fragment。固定向量见 [passkey-v1.json](../../tests/vectors/client/passkey-v1.json)。

网页检查安全上下文、`PublicKeyCredential` 及 credentials 的 create/get 能力，不以本机是否有平台认证器判断外接安全钥匙能否使用。不支持时显示浏览器提示，并允许返回应用，回调为 `passkey_error`。注册同时设置 `residentKey: required` 和 `requireResidentKey: true`。`NotAllowedError` 的原因可能是取消、超时、无可用凭据或权限限制，统一返回 `passkey_error`；只有 `AbortError` 映射为 `user_cancelled`，明确的 `TimeoutError` 或请求已过期返回 `timeout`。回调不携带浏览器原始错误文本。

iOS 用 AuthenticationServices 原生生成相同数据，经 UniFFI 提交共享 Rust 流程；Associated Domains 使用已有的 `webcredentials:dmsg.net` 和 AASA。原生 origin 须在真实签名设备验证，不能用模拟器结果替代。

注册客户端先检查 issuer 是否就绪并取得所需准入票据，再申请 challenge 和调用系统创建凭据；本次交互的截止时间不超过票据与 challenge 的较早到期时间。准入接口只签发短期票据，不创建身份，网络失败可有界重试。系统保存凭据不等于服务端完成注册；尚未注册的凭据登录返回 `NotFound`，不能据此自动创建另一个身份。每次主动创建新凭据会建立独立的登录身份，关联已有账户仍须走账户认证绑定。

委托的部署 targets 也在系统创建凭据前读取；全部退出不派生委托，不读取 targets。持有原交互会话的客户端只在原挑战有效期内重试：先取回同一硬件 caller 的 delegation，签名已被签发方按一分钟期限修剪时先 `renew` 同一会话再读取；仅在尚未确认成功且服务端报告该会话不存在时，重送相同证明。凭据、硬件钥匙和挑战不变；服务端一次性挑战及注册 caller 索引保证重复投递不会产生第二个身份，也不能恢复已退出的会话。

iOS 的当前绑定流程在临时网络、硬件或存储失败时保留原生响应和已验证的身份，继续完成原账户的绑定审批，不再次调用系统创建凭据。“继续”只续用已保存的流程，不会打开创建凭据的系统界面。取消、锁定、离开页面，或后台暂停使会话代际变化时丢弃待完成流程，失效会话不接纳迟到结果；之后可主动选择已有 Passkey 重新认证并绑定。iOS 与桌面的绑定入口都区分已有凭据与新建身份。

桌面登录网页、iOS 和 Android 在新建凭据时，按设备本地日期和 24 小时时间生成显示名称 `dMsg-MM-DD-HHmm`。例如 11 月 21 日 10:37 创建的凭据显示为 `dMsg-11-21-1037`。同一分钟创建的凭据可以同名，各自的 WebAuthn 用户标识仍然独立。

## 本机会话与内容解锁

会话满额的恢复必须固定到这次认证返回的 `credential_id`，不能回退到当前工作区已记住的其他凭据。桌面 `session/login_result` 在满额失败时返回该凭据的 base64url ID；`session/login` 的 `reset_sessions` 动作要求显式 `credential_id`，向签发方提交的仍是新鲜 `logout_all` challenge 与系统确认。iOS 使用对应的 `ResetSessions` 操作。完成后保留当前工作区，再用同一凭据重新登录；原来发起的是绑定时仍进入原账户的绑定审批。普通 `logout_all` 继续针对当前登录，并退出本机会话。两种操作不新增 canister 方法，不改变已签发委托的最长有效期。

每个登录设备单独创建硬件 P-256 钥匙（Apple Secure Enclave 或 Android Keystore），与内容解锁/批准钥匙分开。句柄和父 delegation 保存在系统凭据库独立槽位，只有已批准且完成本机硬件绑定的工作区才保存。硬件不可用的平台不提供此入口，继续使用 II。

父 delegation 不限 targets。客户端为内存 Ed25519 钥匙追加一级限定 targets 的 delegation，范围沿用 dmsg-login/1，每次在线续期按当前部署刷新，截止不超过父级。共享 `Hold::ensure_login` 默认保留十分钟余量：先用内存身份，再使用本机缓存父委托，最后才在线续期。并发续期合并；暂时网络或 Keychain 故障保留凭据；明确失效则要求重新登录。返回时核对工作区与会话代数，锁定/切换/退出后不接纳迟到结果。

锁定清除内容密钥和内存会话钥，保留硬件登录凭据。本机读取、搜索和复制秘密不调用续期。关闭快速解锁保留硬件绑定与登录凭据；解除硬件绑定时按退出本机处理，并尽力结束服务端会话。退出所有登录只停止后续签发，已签发的委托最多仍有效 12 小时；内容设备撤销继续使用账户现有入口。

## 启用与发布

1. 构建、安装一个 `dmsg_auth`，指定治理 Principal、准入 Ed25519 公钥及实际预算；检查 `auth_stats.ready`。正常升级保留 canister ID 和 salt，不使用重装。
2. 给准入票据服务配置该 auth canister。请求仍为 `/v1/account-admission` 的 `{home, principal}`，此处 home 为 auth ID，principal 为硬件 caller；签名字节不变。auth 不加入账户 user-home 信任列表。
3. 发布支持 dmsg-passkey/1 的 `/_/login`；核对已有 AASA 与实际 iOS 签名身份。
4. 桌面/CLI 和 iOS 共用 `src/dmsg_client/production.toml` 中的生产 auth 默认值 `mpld4-maaaa-aaaas-qhepa-cai`。旧的生产 `config.toml` 缺少 `auth` 时自动使用该默认值，无需用户修改文件；显式 `auth` 优先。local/staging 不继承生产 issuer，仍需配置各自的 `auth`，iOS staging 部署文件也支持该字段。桌面和 iOS 都重建发布，iOS 同步重建 Rust/UniFFI。
5. 对真实硬件验收创建、登录、重启后续期、离线复制、绑定原账户和退出，再开放注册。容量预算依据部署测量设置；此实现不承诺千万级容量或主网吞吐。

生产 auth ID 已内置；源码配置不代表服务端已部署，发布客户端前仍须完成上述服务端准备与真机验收。此前验证记录见 [passkey-validation_zh.md](../passkey-validation_zh.md)。
