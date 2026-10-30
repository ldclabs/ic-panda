# dmsg-device-transfer/1：设备间直传

[English](device_transfer.md) | 简体中文

2026-10-30 起草。协议核心实现于 `dmsg_client::transfer`，字节规则由
[transfer-v1 向量](../../tests/vectors/transfer/transfer-v1.json)固定。宿主的传输
（macOS 与 iOS 上的 TCP 监听和 Bonjour）只负责承载，不改变本文描述的字节。

同一账户的两台已验证设备之间，经本地连接一次性、端到端加密地传送本机内容。它传送
账户云端没有的内容，例如 dMsg Files 文件库，用于新设备接替旧设备（继任）或与之并
存时。两台设备必须是同一环境中同一账户的活跃设备。不产生可以保存的导出文件：内容
从一台设备的工作区直接进入另一台设备的工作区。

## 1. 角色与会话

| 角色 | 职责 |
| --- | --- |
| 接收方 | 开启会话（随机 16 字节 `session_id`），监听、广播，并请求自己缺少的内容；把记录提交到自己的工作区。 |
| 发送方 | 连接会话，把新的会话密钥封装给接收方，按清单提供记录与块；不修改本机任何数据。 |

- 接收方以 Bonjour 服务 `_dmsg-xfer._tcp` 广播会话，实例名为
  `hex(digest("dmsg/device-transfer/advert/v1", session_id)[0..8])`，TXT 只含
  `v=1`。只有经继任令牌、二维码或短码得知会话 ID 的设备才能找到它。Bonjour 不可用
  时，接收方显示 `地址:端口` 供手动输入。
- `digest(domain, value) = SHA-256(CBOR([1, domain, value]))`，采用核心确定性
  编码，与其他 dMsg 合同相同。
- 双方在会话开始时和结束前各读取一次账户的已认证状态，核对对方是活跃设备、其设备
  ID、签名公钥和 HPKE 公钥与认证状态一致，且双方都未被撤销。

## 2. Hello

发送前，宿主创建本机 `device_transfer` 审批，绑定接收设备、会话、文件库清单摘要及字节数。
复用高风险审批流程，在平台支持时要求系统身份确认。发送在有效期内一次性领取授权，
并在 Hello 前再次核对清单；内容或解锁会话改变后不能继续使用已领取的授权，
中断后的重发也需要重新审批。该审批不修改 user home。

首帧为明文：4 字节大端长度，后接以下结构的规范 CBOR：

```text
Hello = {
  header: {
    format: 1,
    environment: "local" | "staging" | "production",
    account: AccountId,             // 12 字节 Xid
    from: bytes .size 32,           // 发送方设备 ID
    to: bytes .size 32,             // 接收方设备 ID
    session_id: bytes .size 16,
    enc: bytes,                     // HPKE 封装密钥
    key: bytes,                     // 封装后的 32 字节会话密钥 K
    manifest: bytes .size 32,       // 清单摘要（第 4 节）
  },
  signature: bytes .size 64,        // 发送方的 Ed25519 签名
}
```

- `K` 为 32 字节随机数。`(enc, key) = HPKE-Seal(接收方 hpke_pub, info, K)`，base
  模式（RFC 9180：DHKEM X25519 HKDF-SHA256、HKDF-SHA256、AES-256-GCM），`info`
  与 AAD 均为 `CBOR(["dmsg/device-transfer/v1", account, from, to, session_id])`。
- `signature = Ed25519(发送方签名私钥, digest("dmsg/device-transfer/v1", header))`。
- 接收方最多等待 hello 10 秒。`format`、`environment`、`account`、`to` 或
  `session_id` 与本机不符、`from` 是本机或不是活跃设备、签名不通过或密钥打不开时
  拒绝。hello 至多 4,096 字节。

## 3. 帧

之后每一帧为 4 字节大端长度，后接 `AES-256-GCM(K, nonce, aad, CBOR(body))`：

- `nonce = direction ‖ 0x000000 ‖ sequence`（1 + 3 + 8 字节，序号为大端）。
  `direction` 发送方到接收方为 0，反向为 1；`sequence` 在每个方向上从 0 计数。
- `aad = CBOR(["dmsg/device-transfer/v1", session_id, direction, sequence])`。
- body 至多 1,114,112 字节（1 MiB + 64 KiB），恰好容纳一个 1 MiB 的已封存块及
  其编码。重放、重排、反射到另一方向或挪到其他会话的帧都无法打开。任一方 120 秒
  内收不到下一帧即放弃。

body 为只含一个键的 CBOR map，键名即类型：

| body | 发出方 | 内容 |
| --- | --- | --- |
| `have` | 接收方，首帧 | `{manifest: 摘要 / null}`：本会话之前连接时保存的清单；发送方据此不再重发。 |
| `manifest` | 发送方 | `{entries: [Entry], last: bool}`：清单的一部分，按名称排序，每部分至多 2,048 条。 |
| `want` | 接收方 | `{records: [名称], blobs: [摘要]}`：合计至多 256 项；回复按此顺序，先块后记录。 |
| `blob` | 发送方 | `{digest, bytes}`：一个已封存的块。 |
| `record` | 发送方 | `{key, value}`：记录的规范 CBOR；记录已不存在时为空。 |
| `done` | 接收方 | `{categories, committed}`：已处理所选类别的全部条目，`committed` 为这些条目的清单摘要。 |
| `ack` | 发送方 | `{pending_sync: uint}`：已核对 `done`，并报告发送方仍待云同步的改动数。 |
| `abort` | 任一方 | `{code}`，例如 `INTEGRITY_FAILED`、`FORBIDDEN`。 |

发送方只提供清单中列出的名称和摘要，其他请求一律以 `FORBIDDEN` 拒绝。

## 4. 清单

```text
Entry = {
  key: text,                 // 记录名，例如 "lib-file:<id>"
  sha256: bytes .size 32,    // 记录规范 CBOR 的 SHA-256
  size: uint,                // 记录长度
  blobs: bytes,              // 所引用各已封存块的 SHA-256，每个 32 字节
  blob_bytes: uint,          // 这些块的总长度
}
清单摘要 = digest("dmsg/device-transfer/manifest/v1", [Entry, ...])
```

条目按 `key` 排序且不重复。接收方据此显示总体积，允许用户不选某些类别，并把清单
与会话一起以本机数据密钥封存，因此断线后可以续传：下一次连接使用新的 `K` 和同一个
`session_id`，`have` 指出已保存的清单，接收方只请求尚缺的部分。

### 类别与不传的内容

只有下表中的名称会被传送，其他名称一律留在发送方。

| 类别 | 记录 | 块 |
| --- | --- | --- |
| `library` | `lib-folder:`、`lib-file:`、`lib-version:` | 本机版本的已封存块与可选缩略图。仅云端版本必须有备份；本机版本缺块时停止传输。 |

不传：设备的密钥与身份、钱包调用密钥、软件协签私钥与 Secure Enclave 句柄、本机数据
密钥、登录、daemon 的 owner 凭据、应用与 MCP 授权、本机的批准与证明、打开后的明文
副本与导入暂存、云端副本（可从账户重新读取）、分享链接、备份作业、钱包作业、报价与
草稿、付款与结账日志、频道与收件箱状态（经 relay 同步），以及云端已有的已同步对象。
钱包作业与付款日志在撤销旧设备前由旧设备处理完毕。

## 5. 提交

- 接收方先请求记录引用的块，只有全部块都已存储且 SHA-256 一致后才提交该记录。顺序
  为文件夹、版本、文件。
- 只追加：本机已有的记录保持不变。文件获得新到达的版本，其名称、位置、标签和回收站
  状态保留接收方的值。不恢复已移入回收站的条目，也不删除任何内容。字节与清单不符
  （发送方在此期间修改过）的记录不提交；一个版本都没有到达的新文件也不提交。此时
  返回 `RECOVERY_INCOMPLETE`，保留清单供重试。已有版本元数据仍会补取缺少的块，
  块齐全后更新本机可用状态。
- 最终核对双方设备后，接收方发送 `done`，带上所选类别及其条目的清单摘要；发送方按
  自己的清单重新计算，再次核对双方设备，然后回复 `ack`。接收方随后把会话标记为完
  成，并删除保存的清单。
- 继任的本机内容步骤仅在没有跳过记录、且发送方没有待云同步改动时完成。本协议只传
  文件库；未同步的秘密库改动须另外同步，或由用户明确选择留在旧设备。
- 写入失败（例如磁盘已满）时，传输在任何块不全的记录之前停止；腾出空间后可继续。

## 6. 向量

[transfer-v1.json](../../tests/vectors/transfer/transfer-v1.json)：使用仅供测试
的密钥，包含广播名、两条清单条目及其摘要、HPKE info、hello（header CBOR、签名摘
要、签名和带长度的帧字节），以及每种 body 各一帧及其 AAD 与密文。HPKE 临时密钥来
自记录的 `ekm`，因此文件可以复现。读者应重建每一项编码和摘要、验证签名、用接收方的
HPKE 种子打开 `K`，并打开每一帧。
