# dMsg canisters：公开接口与参考实现

本目录记录实际实现。公开协议及语言无关的字节规则见 [protocol/README.md](protocol/README.md)，声明的 CDDL 见 [statements.cddl](protocol/statements.cddl)。内部设计依据的定位方式见 [AGENTS.md](../AGENTS.md)。

## 子库

| 包 | 职责 | 文档 |
| --- | --- | --- |
| dmsg_types | 公开数据合同，包含基础签名、ICP 接口和可选应用协议 | [README](../src/dmsg_types/README.md) |
| dmsg_protocol | 确定性编码、批准构造、标准 COSE 验签、CTT 摘要 | [README](../src/dmsg_protocol/README.md) |
| dmsg_runtime | 参考实现共用的稳定记录、认证树、账本和调用工具 | [README](../src/dmsg_runtime/README.md) |
| dmsg_user | 主体、认证、设备、恢复、根承诺与执行批准 | [README](../src/dmsg_user/README.md) / [Candid](../src/dmsg_user/dmsg_user.did) |
| dmsg_cose | 标准签名产物、受限 vetKD、密钥来源和执行状态 | [README](../src/dmsg_cose/README.md) / [Candid](../src/dmsg_cose/dmsg_cose.did) |
| dmsg_handle | 名称权属、导入、收费和转移 | [README](../src/dmsg_handle/README.md) / [Candid](../src/dmsg_handle/dmsg_handle.did) |
| dmsg_payment | 固定条款托管、入金验证、互斥资金决定与出金 | [README](../src/dmsg_payment/README.md) / [Candid](../src/dmsg_payment/dmsg_payment.did) |

配套私有设计与服务中关于早期接口和编码的描述尚需同步；这里记录公开端这轮重构后的实际合同，不把两者视作已经一致。

## 当前合同

- Statement v3 设计已实现两个文档 profile v1，执行批准域为 `dmsg/execute/v3`；其他独立域的准确版本见协议说明和向量。与早期实验编码不兼容。
- `sign` 输出 `SignedArtifact { cose_sign1, cose_key }`：RFC 9052 COSE_Sign1 和 public-only COSE_Key。ICP 密钥来源另存于结果的 `key` 描述，不能把公钥查询本身当作身份/授权证明。
- 内部账户使用独立的 12 字节 Xid `AccountId`，用户服务同步原子发号；初始化增加固定 issuer_namespace，密码派生版本为 2，使用新开发实例。
- `get_execution_receipt` 提供认证执行叶，绑定请求 ID、待签字节、公钥和签名；请求元数据不再进入可移植 Statement。
- `get_account` 返回 `AccountInfo`，支付返回 `EscrowInfo`。内部预算、去重窗口、ID 分配器不进入这些视图。
- `dmsg_types` 不含稳定存储、认证树或网络调用。各 canister 使用自己的 `store.rs`、StableCell 和有类型的 StableBTreeMap；用户和 COSE 执行记录独立保存。
- 文本签署原始 UTF-8，摘要签署 RFC 9995 Hash Envelope；issuer/subject 使用标准 CWT 文本语义，kid 可变长，BIP340 入口已删除。浏览器消息合同为 `dmsg-extension/3`。
- 付费投递的 Quote/AdmissionReceipt 位于公开的 `profiles::delivery`，它们不是所有签名实现必须支持的基础类型。

## 构建与验证

需要 Rust stable、wasm32-unknown-unknown target、candid-extractor 0.1.6、didc 和 Node.js；PocketIC server 与测试 crate 固定为 16.0.0。依赖以 Cargo.lock 为准。

```sh
rustup target add wasm32-unknown-unknown
make build-dmsg
pnpm --dir src/dmsg_app bindings
POCKET_IC_BIN=/path/to/pocket-ic make test-dmsg
pnpm --dir src/dmsg_app check
pnpm --dir src/dmsg_app test
```

`test-dmsg` 检查 Rust 测试、Clippy、Wasm、Candid 一致性、独立 Rust/JavaScript 协议向量与 PocketIC 调用。测试账本支持故障注入和公开 mint，仅用于测试，不在 dfx.json 中。详见 [集成测试说明](../tests/dmsg_integration/README.md)。

## 状态与保证

四类 canister 保持各自权威：账户批准、名称权属、固定密钥执行、资金终态。账户批准先本地提交再跨 canister 执行；管理调用之前保存执行状态。未知结果查询原请求，不能自动新建请求重签或刷新未知转账的时间戳。设备撤销、恢复争议、根 CAS、结果清理后的重放保护、结算/退款互斥和资金守恒继续由本地状态机执行。

稳定布局版本由各 canister 的 store.rs 自己维护，使用新实例联调，不维护早期开发状态的升级兼容。相同代码升级后的执行恢复仍需测试。认证树由稳定记录重建，容量与升级指令成本尚需实测。

生产部署须先创建四个 canister ID，再用各自 Init 参数配置引用。user/cose 的 issuer_namespace 必须一致且固定；当前仅支持固定单 user home，未来多 home 发号须先登记并排除分配器指纹碰撞。COSE 由 controller 初始化并核对生产 key 与 fingerprint；公钥未就绪不接受执行，不能降级为测试根。生产 ledger/归档、扩展完整批准流程、私有服务协议、容量和审计仍需单独验收。

当前 TSA 接点为 RFC 9921 CTT message imprint 和明确不验证信任的 token 组装函数。没有 TSA 网络客户端、CMS/X.509 信任验证、完整证据包归档或 anchor_snapshot 入口；普通签名成功不表示已取得时间戳。频道/profile/普通 grant/消息/文件正文不在这些 canister 中存储，也没有周期 checkpoint 写入。
