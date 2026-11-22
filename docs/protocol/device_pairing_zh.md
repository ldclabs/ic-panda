# 二维码设备授权

[English](device_pairing.md) | 简体中文

桌面、iOS 和 Android 设备都可以申请加入，也可以在具有管理员权限时批准其他设备。

- **管理员出码。** 管理员在「设备 → 授权另一台设备」显示短期邀请。新设备选择「打开已有账号」，扫描或粘贴邀请。两台设备可以使用不同网络。管理员核对两端的核对码，审阅登录方式和设备权限。
- **新设备出码。** 管理员用手机直接扫描新设备显示的请求。已绑定的登录仅显示设备请求。使用新登录时，管理员先处理登录绑定，再批准设备。桌面也支持用摄像头、二维码图片、剪贴板图片和粘贴文本读取请求。

扫码或中继收据不等于设备授权，也不绑定登录身份。原有 `AddDevice` 校验和内容
换根继续生效，新设备最终从账户认证状态检查加入结果。

扫码入口也接受已授权设备生成的 `dmsg-approval-key/1` 登记请求。客户端验证账号、设备及钥匙持有证明，显示完整指纹，再创建本地审批。手机缺少账号要求的批准钥匙时，可把新设备的原请求交给有钥匙的管理员，或单独为本机生成登记请求。登记钥匙会改变账号版本，新设备须刷新其签名请求。直接扫码和转交请求均不创建服务端待办。

设备授权先于新内容根分发提交。新设备可在同一已验证的本机会话中继续读取认证设备状态和内容根；等待内容根不会重复要求本人解锁。

## 用另一种登录方式加入

桌面、iOS 和 Android 的管理员设备可显示 `dmsg-device-invite/3:` 邀请。加入设备在「打开已有账号」中扫码或粘贴，确认目标账号。属于该账号的登录直接申请设备授权。尚无账号的登录经本人确认后先请求成为该账号的登录方式。属于其他账号的登录需选择其他登录方式，原工作区保留。

手机作为管理员时，在设备页选择「授权另一台设备」。新桌面可扫手机的邀请、选择二维码图片或粘贴邀请文本。需要用手机摄像头时，新桌面选择「显示二维码，让另一台设备扫描」并填写手机账号 ID。手机先扫登录绑定码并核对登录身份，桌面接受绑定后自动显示按新账号版本签出的设备码，手机再扫此码批准设备。已绑定的登录直接显示设备码。这个方向使用本地展示的请求，无需邀请邮箱。

v3 在 v2 的 CBOR map 中增加 `binding: {invite_id, key, submit_token}`，三个字段均为独立随机的 32 字节值。两组邮箱使用同一 home、账号、中继和截止时间，各自沿用下文 v2 的创建、加密、读取、取消和到期约定。设备邮箱仍只收原始设备 token。登录邮箱收 base64url 编码的规范 CBOR `{token, device, proof}`。其中 token 是原有 `dmsg-auth-request/1`，device 是申请的完整 DeviceInput，proof 是设备对 `digest("dmsg/invited-login/v1", (设备邮箱 ID, token, device))` 的 Ed25519 签名。核对码取 `digest("dmsg/join-comparison/v1", (home, account, 设备邮箱 ID, 新登录 Principal 或 null, device))` 的前四字节，以两组四位十六进制显示。

管理员在审批中心分别批准登录和设备。新登录以自己的 IC 身份接受绑定后，加入设备按新的账号版本签出设备请求。邀请端核对两份请求中的设备公钥、角色和权限，并确认新登录已绑定。加入端在本机加密工作区暂存本人已确认的邀请及原密文，可续办和幂等重投。等待内容根期间复用已验证的本机会话。旧版 v2 邀请和直接出示设备请求仍可使用。账户 canister 的限时待接受绑定及长期授权见[账户控制协议](account_root_zh.md)。

## 邀请与加密

设备邀请 v2 的前缀为 `dmsg-device-invite/2:`，账号邀请 v3 的前缀为 `dmsg-device-invite/3:`。两者都使用无填充 base64url 编码的规范 CBOR map。公共字段如下，v3 另含前述 `binding` 字段。

| 字段 | 编码 |
| --- | --- |
| `home` | user home Principal 字节 |
| `account` | 12 字节账户 ID |
| `relay` | 已配置中继 origin 文本，不含末尾斜杠 |
| `invite_id` | 随机 32 字节邮箱 ID |
| `expires_at` | Unix 毫秒，最多五分钟后过期 |
| `key` | 独立随机 32 字节 AES 密钥 |
| `submit_token` | 独立随机 32 字节提交凭据 |

生成端采用 270 秒期限，在五分钟校验上限内留出时钟偏差余量。加入端核对本机配置的中继和目标账号的 home。发送设备请求前，当前登录必须属于该账号。

请求明文是原配对 token 的 UTF-8 字节，最多 4096 字节。密封结果由随机 12 字节
nonce、AES-256-GCM 密文及 16 字节标签组成，总长最多 4124 字节。AAD 为规范
CBOR `["dmsg-device-invite/2:", home, account, relay, invite_id, expires_at]`。
加密密钥只经过二维码/复制传递；创建时中继只收到独立提交凭据的 SHA-256，提交时
收到该凭据和密文，均不能据此解密配对内容。

## 中继合同

接口使用已配置的 HTTPS origin（仅 local 模式允许回环 HTTP）、规范 CBOR 请求体，
返回标准 `{ok,data}` 或 `{ok:false,error}` JSON。拒绝浏览器 Origin 头，响应均为 no-store。

| 路由 | 认证和请求体 |
| --- | --- |
| `POST /v1/accounts/:account/device-invites` | 设备 HTTP PoP 与 `dmsg/device-invite/create/v2` 签名命令，payload 为 `{invite_id: bstr32, submit_hash: bstr32, expires_at: uint}` |
| `GET /v1/accounts/:account/device-invites/:invite` | 创建设备的 HTTP PoP |
| `POST /v1/accounts/:account/device-invites/:invite/cancel` | 创建设备的 HTTP PoP 与 `dmsg/device-invite/cancel/v2` 签名命令，payload 为 `{invite_id: bstr32}` |
| `POST /v1/device-invites/:account/:invite` | 一次性凭据，body 为 `{submit_token: bstr32, sealed: bstr}`；无需已加入账户的设备身份 |

创建和读取要求当前有效、持 `RootManage` 的管理员设备；同账户其他管理员也不能
读取或取消创建者的邮箱。提交时再次确认创建者仍有上述权限。每个邮箱只接收一份
密文；相同字节重试返回相同 `{digest: 小写 hex SHA-256}`，不同字节返回冲突。
提交请求体上限 8192 字节，不发送解密密钥或原始配对明文。

创建返回 `{id, expires_at}`，读取另含 `sealed`（null 或 base64url）。相同创建重试
幂等，不延长期限；已取消的邮箱不能被重放创建复活。每账户最多四个活跃邮箱、每分钟十次创建。v3 邀请占两个邮箱。取消清除密文，并在原截止前阻止创建重放；到期后自动丢弃邮箱元数据。读取不刷新 TTL。

发出邀请的设备只在原解锁会话的内存中保存解密密钥。关闭邀请页先使本地邀请失效，再通知中继。
离线或已锁定时远端取消可能失败，但原本地会话仍不能处理请求，中继原截止继续有效。

## 客户端接口与验证

owner daemon 提供 `device/pair_invite`、`device/pair_invite_status {id}`、
`device/pair_invite_cancel {id}`、`device/pair_inspect {token}` 和
`device/pair_scan {image}`（base64 图片字节，本地解码）。系统图片选择与剪贴板访问
仍由主进程负责，renderer 不传文件路径；摄像头在成功、取消、离页或锁定后停止。
桌面通过 `device/pair_invite {with_login: true}` 发出账号邀请，用 `device/join_invitation` 加入，用 `device/saved_invitation` 续办。登录绑定由 `account/bind_approve` 创建审批，设备授权由 `device/approve {token, device}` 创建审批。决定仍在审批窗口完成。
移动端提供 `start_account_invitation`、`account_invitation` 和 `saved_account_invitation`，并保留直接扫码接口；`account_invitation` 也接受 v2 邀请。

[固定向量](../../tests/vectors/client/device-invite-v2.json) 只含公开测试密钥、二维码、
加密载荷及规范创建/提交请求体。核心测试验证解密与请求证明、HTTP 传输、提交凭据
与密钥隔离、过期、会话变化及图片二维码读取。Electron 使用模拟 daemon 和合成
视频，iOS 使用模拟器；不能代替物理摄像头或生产登录/设备加入联调。应先部署兼容
中继，再发布生成此邀请的客户端；本地测试不包含部署。
