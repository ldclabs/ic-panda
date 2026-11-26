# dmsg-nearby-share/1：面对面近场分享

[English](nearby_share.md) | 简体中文

协议核心实现于 `dmsg_client::nearby`，字节规则由
[nearby-v1 向量](../../tests/vectors/nearby/nearby-v1.json) 固定。

两个近在咫尺的 dMsg 客户端（不要求同一账户，亦支持纯离线 Standalone 工作区）之间，经端到端加密通道直接分享文件库中的文件或经过显式筛选的保险库快照（`dmsg-secret-package/1`）。

## 1. 传输与发现

- **默认「仅扫码」**：接收方打开「近场接收」时仅监听本机端口并展示 `dmsg-nearby/1:` 二维码（及可粘贴文本），**不发送 mDNS 广播**。
- **可选局域网发现**：用户显式开启「让附近设备看到我」后，接收方在 `_dmsg-share._tcp` 下以 16 位随机 hex 实例名广播，TXT 仅含 `v=1`、`n=<32B以内显示名>` 与 `p=<4字节临时公钥短提示>`，不暴露 `AccountId`、handle 或永久设备 ID。
- **并行竞速与盲中继兜底**：接收方若为在线已登录设备，会异步向已配置中继申请短命盲中继房间（不落 R2、加入截止 5 分钟、配对后限 100 MiB / 60 秒空闲 / 15 分钟绝对上限）。发送方扫码后先拨号私网候选地址与 mDNS/AWDL，延迟 1.8 秒后并行拉起盲中继，先完成握手者胜出。
- **地址安全约束**：二维码内 `addrs` 仅允许私网（RFC 1918 / RFC 4193 ULA）与链路本地地址，拒绝公网 IP、多播与域名；`relay.env` 必须匹配本机部署环境，绝不盲连二维码中的任意 URL。

## 2. 抗 MITM 承诺-揭示握手（Commitment-then-Reveal）

```text
1. Sender -> Receiver: ClientHello  = { v: 1, commit }
     commit = SHA-256(CBOR(["dmsg/nearby-share/commit/v1", s_pk, s_nonce]))
2. Receiver -> Sender: ServerHello  = { v: 1, sid, r_pk, r_nonce }
     (扫码模式下发送方在此强校验 r_pk == offer.pk)
3. Sender -> Receiver: ClientReveal = { s_pk, s_nonce, ? qr_proof }
     transcript = SHA-256(CBOR(["dmsg/nearby-share/handshake/v1", 1, sid, commit, s_pk, s_nonce, r_pk, r_nonce]))
     qr_proof   = SHA-256(CBOR(["dmsg/nearby-share/qr-proof/v1", qrs, transcript]))
```

- 接收方先验证 `commit`；在默认「仅扫码」模式下，缺少或无效的 `qr_proof` 直接拒绝且不弹窗。
- 双方执行 X25519（拒绝非贡献零点）并用 HKDF-SHA-256 派生：
  - `K_tx = HKDF(shared, CBOR(["dmsg/nearby-share/key/v1", transcript, qr_bound, qrs_or_empty]))`
  - `sas = u32_be(HKDF(shared, CBOR(["dmsg/nearby-share/sas/v1", transcript]))[0..4]) % 1_000_000`
- **核对门控**：所有模式下接收方都核对 6 位码；列表模式下发送方须先确认 6 位码一致后才发送 `OfferMeta`。核对前不发送任何文件名、标题或身份信息；可选 `Identity` 帧默认不发，仅在双方确认后按需交换。
- **防槽位抢占**：接收页仅在用户点击「接受」时锁定单活跃会话槽位并关闭竞争连接。

## 3. 帧加密、信用窗口与载荷交付

- 握手后每帧为 AES-256-GCM 密文，nonce 为 `direction (1B) || 0x000000 || seq (8B BE)`，AAD 为 `CBOR(["dmsg/nearby-share/frame/v1", sid, direction, seq])`，明文上限 `1 MiB + 64 KiB`。
- **按块信用窗口**：接收方在 `Accept` 授予初始信用 `W = 4` 块，每消费 2 块回发 `Credit { granted }`，LAN 与 WebSocket 中继共用同一背压流控。
- **文件分享**：普通文件不走高风险审批；接收方复用 `library::import`（`free_name` 不并入同名文件、独立 `FileKey`、按 `files_max_bytes` 验收，默认 4 GiB，单次会话 ≤ 10 GiB / 64 文件），**逐文件独立提交并回执 `FileAck`**。
- **保险库条目**：复用 `dmsg-secret-package/1`（上限 100 MiB、≤ 64 字段、≤ 20 附件、默认全不选、拦截 `dmsg-` 保留条目）。发送方经高风险 `Action::SecretShare`（绑定 `transcript` 与 6 位码）批准后流式发出；接收方整包校验后进入现有 `secrets/receiving` 真实无值预览与 `Action::SecretPackageImport` 原子导入。

## 4. 中继合同（`dmsg-nearby-relay/1`）

| 路由 | 认证与说明 |
| --- | --- |
| `POST /v1/accounts/:account/nearby-relays` | 设备 HTTP PoP + `dmsg/nearby-relay/create/v1` 签名命令，payload `{rid, recv_token_hash, send_token_hash, join_expires_at}`；每账户最多 2 活跃房间、每分钟最多 6 次创建。 |
| `POST /v1/accounts/:account/nearby-relays/:rid/cancel` | 创建设备的 HTTP PoP + `dmsg/nearby-relay/cancel/v1` 签名命令。 |
| `GET /v1/nearby-relays/:rid/ws` | 拒绝 `Origin` 头，收发双端统一经 `Authorization: Bearer <base64url(token)>` 升级 WebSocket，仅做不透明二进制密文帧转发，零 R2 持久化。 |
