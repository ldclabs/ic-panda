# dmsg_cose

受限 ICP 阈值签名和 vetKD 服务。正式签名输出可独立解析的 COSE_Sign1；原始任意 signHash、任意派生路径和 namespace 权限接口不开放。

## 接口

见 [dmsg_cose.did](dmsg_cose.did)。

| 入口 | 调用者及结果 |
| --- | --- |
| `initialize_keys` | controller；核对配置与真实公钥后启用 |
| `key_state` | 查询当前初始化与根描述 |
| `public_key(account_id, selector)` | 公开查询，离线派生公钥；不证明主体存在或具备执行权 |
| `execute(grant)` | 仅配置中的 user home；生成签名或 encrypted VetKey |
| `get_execution` | 仅配置中的 user home；查询原请求 |

应用通过 `dmsg_user.sign` / `derive_root` 提交设备批准。`Signature` 结果内的 `artifact` 含标准 COSE_Sign1 和 COSE_Key，`key` 保存 ICP 派生来源。`EncryptedRootKey` 保留独立的 vetKD 结果。

## 密码实现

`cose2` 负责 RFC 9052 待签结构和封装，`ic_cose_chain_key` 负责管理调用、公钥派生和费用。Ed25519 签 Sig_structure 原字节；ES256K 签其 SHA-256 摘要；dMsg 不再提供 BIP340 入口。文件摘要签署不受云端文件上传上限约束。

`CoseInit` 指定与 user home 相同的固定 issuer_namespace；account_id 为 12 字节 Xid。签署端同时检查 issuer、完整待签结构和实际派生公钥指纹。签名 kid 使用 RFC 9679 SHA-256 指纹，通用文档 profile 的 kid 仍为可变长字节。

所有用途保留固定 key home 和 `dmsg/formal/v2` 派生域，derivation_version=2。Production 配置拒绝测试根并核对 fingerprint；升级不能暗中换根。执行在管理调用前落盘，未知结果保留原请求，重试不再次签名。

## TSA

`dmsg_protocol::timestamp_imprint` 提供 RFC 9921 CTT 接点。当前没有 TSA 网络客户端、TSA 信任验证或 `anchor_snapshot` 入口；这些能力不能由普通签名成功推断。

## 代码

`api.rs` 负责入口和密码调用，`model.rs` 管理有界执行状态，`store.rs` 保存配置、公钥缓存和内部记录。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_cose
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
