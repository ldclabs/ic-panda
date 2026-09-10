# dmsg_handle

名称权属的独立 ICP 注册表。handle 是到稳定主体的映射，转移名称不转移账户、内容、签名身份或外部应用权限。

## 接口与流程

完整接口见 [dmsg_handle.did](dmsg_handle.did)。先通过用户服务批准具体 `HandleIntent`，再调用 `reserve_handle` / `commit_handle`。转移由双方针对同一名称、版本和操作 ID 分别批准。

`resolve_handle_certified` 返回名称的 ICP 包含或不存在证明；验证者核对 canister ID、信任根、certificate、witness 和所需新鲜度。名称事件独立编号，可通过 `get_handle_event` 查询。

## 收费和导入

ASCII 规范化、PANDA 定价及旧名称导入属于此注册表的业务规则，不是基础签名协议。新注册在旧快照封存前关闭。导入分 begin/import/seal；未认领旧名保持预留，争议记录不自动释放。

扣款前固定付款账户、金额、fee、memo 和账本纳秒时间。未知扣款保持名称锁，使用原参数或真实账本块对账；不能因为本地超时而再次售卖名称。查询 `get_handle_operation` 可恢复原流程。

名称拥有者、转移目标和授权引用均为 12 字节 `AccountId`；handle 与 issuer URI 分别维护，名称转移不会改变 Xid。

## 实现

`api.rs` 保存入口与名称状态转换，`store.rs` 保存有类型的名称、操作、锁和事件表。schema 3 的私有 `stable_codec.rs` 为配置、名称、导入、操作、事件和转移回执递归使用 CBOR 整数 map key；公开名称摘要和认证叶编码不变。部署参数由 `HandleInit` 定义。

## 验证

从仓库根目录运行：

```sh
cargo test -p dmsg_handle
```

四个真实 Wasm 的调用、恢复、认证查询和资金异常测试：

```sh
POCKET_IC_BIN=/path/to/pocket-ic bash scripts/test-dmsg.sh
```

开发阶段使用新实例，不兼容之前的实验接口和稳定布局。生产部署、容量和真实外部服务仍需单独验收。
