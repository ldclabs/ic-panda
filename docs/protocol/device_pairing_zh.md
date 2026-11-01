# 二维码设备授权（`dmsg-device-invite/2`）

[English](device_pairing.md) | 简体中文

两种方向均进入原有设备审批流程：

- **新设备扫旧桌面**：桌面在“设置 → 设备 → 授权另一台设备”显示短期邀请。
  新 iOS 设备登录同一账户后扫码，通过已配置中继回传加密配对请求；桌面解密、
  核验并显示完整设备 ID、角色与能力。用户选择核对后进入审批窗口。两端可以使用不同网络。
- **旧设备扫新设备**：新设备显示已有的 `dmsg-device-request/2` 请求二维码，
  旧设备直接扫描并核对。桌面支持摄像头、所选图片、剪贴板图片及粘贴文本；iOS
  支持扫描和粘贴。此方向不需要邀请邮箱或请求回传。

扫码或中继收据不等于设备授权，也不绑定登录身份。原有 `AddDevice` 校验和内容
换根继续生效，新设备最终从账户认证状态检查加入结果。

## 邀请与加密

邀请文本为 `dmsg-device-invite/2:` 加无填充 base64url 编码的规范 CBOR map：

| 字段 | 编码 |
| --- | --- |
| `home` | user home Principal 字节 |
| `account` | 12 字节账户 ID |
| `relay` | 已配置中继 origin 文本，不含末尾斜杠 |
| `invite_id` | 随机 32 字节邮箱 ID |
| `expires_at` | Unix 毫秒，最多五分钟后过期 |
| `key` | 独立随机 32 字节 AES 密钥 |
| `submit_token` | 独立随机 32 字节提交凭据 |

生成端采用 270 秒期限，在五分钟校验上限内留出时钟偏差余量。手机必须核对邀请的
home/账户与自身认证账户一致，中继与本机配置一致；二维码不能让它改投其他服务。

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
读取或取消创建者的邮箱。提交时再次确认创建者仍有上述权限。每个邀请只接收一份
密文；相同字节重试返回相同 `{digest: 小写 hex SHA-256}`，不同字节返回冲突。
提交请求体上限 8192 字节，不发送解密密钥或原始配对明文。

创建返回 `{id, expires_at}`，读取另含 `sealed`（null 或 base64url）。相同创建重试
幂等，不延长期限；已取消邀请不能被重放创建复活。每账户最多四个活跃邀请、每分钟
十次创建。取消清除密文，并在原截止前阻止创建重放；到期后自动丢弃邮箱元数据。读取不刷新 TTL。

桌面解密密钥只留在内存，绑定原解锁会话。关闭对话框先使本地邀请失效，再通知中继。
离线或已锁定时远端取消可能失败，但原本地会话仍不能处理请求，中继原截止继续有效。

## 客户端接口与验证

owner daemon 提供 `device/pair_invite`、`device/pair_invite_status {id}`、
`device/pair_invite_cancel {id}`、`device/pair_inspect {token}` 和
`device/pair_scan {image}`（base64 图片字节，本地解码）。系统图片选择与剪贴板访问
仍由主进程负责，renderer 不传文件路径；摄像头在成功、取消、离页或锁定后停止。
两种方向都调用原有 `device/approve {token, device}` 创建审批，主 renderer 不能批准。
移动端提供 `join_device_invitation(invitation)`，同时保留直接请求/核对接口。

[固定向量](../../tests/vectors/client/device-invite-v2.json) 只含公开测试密钥、二维码、
加密载荷及规范创建/提交请求体。核心测试验证解密与请求证明、HTTP 传输、提交凭据
与密钥隔离、过期、会话变化及图片二维码读取。Electron 使用模拟 daemon 和合成
视频，iOS 使用模拟器；不能代替物理摄像头或生产登录/设备加入联调。应先部署兼容
中继，再发布生成此邀请的客户端；本地测试不包含部署。
