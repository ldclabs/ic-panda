# dmsg_integration

基于 PocketIC 16.0.0 的真实 Wasm 联调，覆盖四个 dMsg canister 和故障注入账本。测试只使用生成的测试身份、固定测试 seed、临时本地实例，不调用生产服务。

```sh
make build-dmsg
cargo build --release --target wasm32-unknown-unknown -p dmsg_test_ledger
POCKET_IC_BIN=/path/to/pocket-ic cargo test -p dmsg_integration --features pocketic-tests --test control_plane -- --test-threads=1
```

可设置 DMSG_WASM_DIR 指向 Wasm 目录，默认读取 target/wasm32-unknown-unknown/release。完整一致性检查使用 `bash scripts/test-dmsg.sh`。

覆盖：设备/恢复授权、根 CAS、标准 COSE 的 Ed25519/ES256K、Xid 原子发号/重试/升级、认证执行回执与签名绑定、vetKD、独立认证树检查、名称认领/转移/收费、入金/结算/退款、丢失响应、BadFee、迟到资金与升级恢复。测试通过公开 AccountInfo/EscrowInfo 查询，不读取 canister 内部状态。

这些测试不代替真实账本部署、扩展 UI、私有 Worker、TSA 信任验证和容量验收。
