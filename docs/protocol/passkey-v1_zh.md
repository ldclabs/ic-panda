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

服务端仅接受 RP ID `dmsg.net`、精确 origin `https://dmsg.net`、ES256、UP 与 UV 标志。注册要求 none attestation，凭据 ID 与 attested data 一致；登录验证 authenticatorData 和原始 clientDataJSON 的签名。检查 crossOrigin/topOrigin，拒绝子域 origin。同步 Passkey 的签名计数不作为是否有效的判断依据。

## 浏览器与 iOS

桌面系统浏览器打开 `https://dmsg.net/_/login#<base64url(canonical-CBOR)>`。请求字段固定为：

```text
{ protocol: "dmsg-passkey/1", state: bstr32,
  challenge: { challenge: bstr32, expires_at: uint, action: text },
  next_url: text, credential_id: bstr / null }
```

`next_url` 复用 dmsg-login/1 的回环回调或固定 iOS HTTPS 回调白名单。`state` 仅用于本机关联；服务端 challenge 独立随机且一次性。已记住的登录在 `logout_all` 中限制当前 credential_id；新设备遇到会话上限时可通过可发现的 Passkey 认证退出全部登录，再重新登录。普通登录同样使用 discoverable credential。网页只收集 WebAuthn 结果，不持有设备私钥、登录 delegation 或内容密钥。

成功回调：`{protocol, state, response: {credential_id, client_data_json, authenticator_data, signature}, error: null}`，四个响应字段均为 bytes。注册的 `authenticator_data` 是完整 attestationObject，signature 为空；登录为原始 authenticatorData 与 DER ES256 签名。失败时 response 为 null，error 仅为 `user_cancelled/passkey_error/timeout`。回调通过顶层导航携带 fragment。固定向量见 [passkey-v1.json](../../tests/vectors/client/passkey-v1.json)。

iOS 用 AuthenticationServices 原生生成相同数据，经 UniFFI 提交共享 Rust 流程；Associated Domains 使用已有的 `webcredentials:dmsg.net` 和 AASA。原生 origin 须在真实签名设备验证，不能用模拟器结果替代。

## 本机会话与内容解锁

每个登录设备单独创建 Secure Enclave P-256 钥匙，与内容解锁/批准钥匙分开。句柄和父 delegation 保存在系统凭据库独立槽位，只有已批准且完成本机硬件绑定的工作区才保存。硬件不可用的平台不提供此入口，继续使用 II。

父 delegation 不限 targets。客户端为内存 Ed25519 钥匙追加一级限定 targets 的 delegation，范围沿用 dmsg-login/1，每次在线续期按当前部署刷新，截止不超过父级。共享 `Hold::ensure_login` 默认保留十分钟余量：先用内存身份，再使用本机缓存父委托，最后才在线续期。并发续期合并；暂时网络或 Keychain 故障保留凭据；明确失效则要求重新登录。返回时核对工作区与会话代数，锁定/切换/退出后不接纳迟到结果。

锁定清除内容密钥和内存会话钥，保留硬件登录凭据。本机读取、搜索和复制秘密不调用续期。关闭快速解锁保留硬件绑定与登录凭据；解除硬件绑定时按退出本机处理，并尽力结束服务端会话。退出所有登录只停止后续签发，已签发的委托最多仍有效 12 小时；内容设备撤销继续使用账户现有入口。

## 启用与发布

1. 构建、安装一个 `dmsg_auth`，指定治理 Principal、准入 Ed25519 公钥及实际预算；检查 `auth_stats.ready`。正常升级保留 canister ID 和 salt，不使用重装。
2. 给准入票据服务配置该 auth canister。请求仍为 `/v1/account-admission` 的 `{home, principal}`，此处 home 为 auth ID，principal 为硬件 caller；签名字节不变。auth 不加入账户 user-home 信任列表。
3. 发布支持 dmsg-passkey/1 的 `/_/login`；核对已有 AASA 与实际 iOS 签名身份。
4. 桌面/CLI 和 iOS 共用 `src/dmsg_client/production.toml` 中的生产 auth 默认值 `mpld4-maaaa-aaaas-qhepa-cai`。旧的生产 `config.toml` 缺少 `auth` 时自动使用该默认值，无需用户修改文件；显式 `auth` 优先。local/staging 不继承生产 issuer，仍需配置各自的 `auth`，iOS staging 部署文件也支持该字段。桌面和 iOS 都重建发布，iOS 同步重建 Rust/UniFFI。
5. 对真实硬件验收创建、登录、重启后续期、离线复制、绑定原账户和退出，再开放注册。容量预算依据部署测量设置；此实现不承诺千万级容量或主网吞吐。

生产 auth ID 已内置；源码配置不代表服务端已部署，发布客户端前仍须完成上述服务端准备与真机验收。此前验证记录见 [passkey-validation_zh.md](../passkey-validation_zh.md)。
