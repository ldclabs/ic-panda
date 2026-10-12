# dmsg-login/1：本机客户端的 Internet Identity 登录中转

2026-10-12 设计草案，尚未实现。dMsg 桌面端（`dmsg daemon`）与以后的移动端不内嵌 Internet Identity 登录；它们用系统浏览器打开 `https://dmsg.net/_/login`，由这个页面请 II 直接为客户端的会话公钥签发 delegation，再把结果交回客户端。页面本身没有登录态：不读取、不保存 dmsg.net 站点的会话，也不持有任何可用的私钥。

## 1. 为什么放在 dmsg.net

dmsg.net 是 II 的派生 origin（`derivationOrigin`），它的代码本来就能在用户经过 II 确认后为用户的 Principal 申请 delegation。登录页放在这里不新增信任点，而且两个派生站点的 `ii-alternative-origins` 都不需要改动：`https://panda.fans/.well-known/ii-alternative-origins` 已经包含 `https://dmsg.net`，所以同一页面也能以 `https://panda.fans` 为派生 origin 服务旧 Principal。网页应用的桥接页 `connect.dmsg.net` 不承载登录，也永远不列入任何 `ii-alternative-origins`。

现有的 `/_/oauth/deeplink` 复用站点自身的登录状态再追加 delegation，继续服务旧客户端，不改。

## 2. 流程

```mermaid
sequenceDiagram
  participant C as dmsg daemon
  participant B as 系统浏览器
  participant L as https://dmsg.net/_/login
  participant II as Internet Identity
  C->>C: 生成会话 Ed25519 密钥（内存）、state；开始监听回环端口
  C->>B: 打开 https://dmsg.net/_/login#<请求>
  B->>L: 加载页面，读取 fragment 后立即清除
  L->>L: 校验参数；显示"将连接到本机 dMsg（端口 n）"与派生 origin
  L->>II: icrc34_delegation { publicKey: session_pubkey, targets, maxTimeToLive, derivationOrigin }
  II-->>L: delegation chain
  L->>B: location.assign(next_url#<结果>)
  B->>C: GET /auth/callback（fragment 留在浏览器）
  C-->>B: 回调页（极小 HTML）
  B->>C: 回调页读取 location.hash，同源 POST /auth/callback
  C->>C: 校验 state、公钥、targets、期限；关闭监听
  C-->>B: 显示"可以关闭此页"
```

fragment 不会随 HTTP 请求发送，所以 daemon 的 `GET /auth/callback` 收不到结果；它返回一个只做一件事的 HTML：读取 `location.hash`，以同源 `POST` 交给 daemon。Chrome 的 Local Network Access 限制 HTTPS 页面向回环地址发起 `fetch`，但不限制顶层导航，所以登录页必须用导航（`location.assign`）而不是 `fetch` 或表单提交把结果带回。

## 3. 请求

URL 为 `https://dmsg.net/_/login#<base64url(规范 CBOR 请求)>`，没有 query。请求：

```text
LoginRequest = {
  v: 1,
  state: bstr .size 32,            ; daemon 生成的一次性随机值
  session_pubkey: bstr,            ; 会话公钥的 SPKI DER（Ed25519 为 44 字节）
  targets: [* tstr],               ; canister Principal 文本，1 到 32 个
  ttl_ms: uint,                    ; 申请的 delegation 时长，毫秒
  derivation_origin: tstr,         ; "https://dmsg.net" 或 "https://panda.fans"
  next_url: tstr                   ; 回调地址，见第 5 节
}
```

页面校验：键集合闭合；`state` 非零；`session_pubkey` 是合法 SPKI DER；`targets` 每项都是合法 Principal 文本且不重复；`ttl_ms` 在 1 分钟到 8 小时之间；`derivation_origin` 在允许列表内；`next_url` 符合第 5 节。任一项不符时页面不联系 II，只显示错误，不跳转（因为 `next_url` 本身可能就是不合法的那一项）。

页面以 `@icp-sdk/signer` 向 II 调用 `icrc34_delegation`，`publicKey` 为 `session_pubkey`，`targets` 与 `maxTimeToLive` 取请求值，`derivationOrigin` 取 `derivation_origin`。页面不生成也不持有会话私钥。

## 4. 回调

成功时导航到 `next_url#<base64url(规范 CBOR 结果)>`：

```text
LoginResult = {
  v: 1,
  state: bstr .size 32,
  delegation: bstr                 ; DelegationChain.toJSON() 的 UTF-8 JSON（@icp-sdk/core 格式）
}
```

失败时：

```text
LoginError = { v: 1, state: bstr .size 32, error: tstr }
```

`error` 为 `user_cancelled`、`ii_error`、`derivation_origin_not_allowed`、`timeout` 之一。参数本身不合法时不回调（第 3 节）。

daemon 的 `GET /auth/callback` 返回回调页；回调页把 `location.hash` 原样 `POST /auth/callback`（`Content-Type: application/cbor`，body 为解码后的 CBOR）。daemon 在 POST 上校验：

1. 监听器只在登录期间存在（默认 10 分钟），只接受一次 POST，成功或失败后关闭；`GET /auth/callback` 在监听器关闭后返回 404。
2. 请求的 `Origin` 头等于回环 origin（`http://127.0.0.1:<port>` 或 `http://[::1]:<port>`）；不设置 CORS 头，拒绝带其他 `Origin` 的请求。
3. `state` 等于本次登录生成的值。
4. `delegation` 解析为 DelegationChain：`publicKey` 等于会话公钥 DER；链中每一级 delegation 的 `targets` 都是请求 `targets` 的子集；最终 `expiration` 不晚于 `now + ttl_ms`；链结构完整。签名的有效性由 IC 在首次调用时裁决：daemon 用该身份先做一次只读的 `my_account` 查询，失败即丢弃。
5. 校验通过后把 delegation 保存在内存，不写盘；监听器关闭。

## 5. `next_url` 白名单

`next_url` 只接受：

- `http://127.0.0.1:<port>/auth/callback`
- `http://[::1]:<port>/auth/callback`

不接受 `localhost`（解析结果不受页面控制）、其他路径、query 或 fragment。移动端阶段再增加 app link 形式，在本文件中登记。

## 6. 页面要求

- 路由 `/_/login` 使用独立布局，不加载站点的登录状态、认证 store 或其他业务模块。
- 页面只显示：连接目标"本机 dMsg（端口 n）"、所选派生 origin、"继续"按钮；按钮点击后才联系 II。不提供其他动作。
- 读取 fragment 后立即用 `history.replaceState` 清除，避免留在历史记录。
- 不写 cookie、localStorage、IndexedDB。
- 响应头沿用 dmsg.net 资产 canister 的策略：`frame-ancestors 'none'`；`upgrade-insecure-requests` 不影响到回环地址的跨主机导航；`form-action 'self'` 意味着不能用表单提交回调，必须用导航。

## 7. 安全边界

- 页面没有登录态，拿不到任何可用密钥；用户在 II 中完成认证，页面只是把 II 签给 `session_pubkey` 的 delegation 转交给 `next_url`。
- `next_url` 只能是回环地址，所以这个页面不能被用来把 delegation 送到远端。
- 同一 OS 用户的恶意进程可以自己监听回环端口并打开登录页，用户看到的"将连接到本机 dMsg（端口 n）"无法区分请求方。这是回环回调流程（RFC 8252）的固有局限，与 OS 用户边界一致；dMsg 客户端的本机凭证边界见桌面方案。
- dmsg.net 主站代码的完整性是 II delegation 的信任前提，这一点与登录页放在哪里无关。

## 8. 验证要求

- 向量：`LoginRequest`、`LoginResult`、`LoginError` 的规范 CBOR 与 base64url 编码，由 Rust 实现生成（`tests/vectors/client/login-v1.json`），页面与 daemon 逐字节核验。
- 页面（Playwright）：参数缺失、键集合不闭合、非法 Principal、`ttl_ms` 越界、不允许的 `derivation_origin`、`localhost` 与非回环的 `next_url` 都不联系 II 且不跳转；fragment 被清除；成功与失败回调的 fragment 格式。
- daemon：`state` 不符、公钥不符、`targets` 超集、过期、重复 POST、带外部 `Origin` 的 POST 都被拒绝；监听器关闭后 `GET /auth/callback` 为 404。
- 端到端：Local 环境用测试身份替身代替 II；真实 II 在 staging 手工验收。
