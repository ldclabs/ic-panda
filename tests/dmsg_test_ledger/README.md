# dmsg_test_ledger

仅供 PocketIC 测试的 ICRC 账本替身。含公开 mint、手续费修改、拒绝转账及提交后丢失响应的故障注入；不可部署为真实资产账本。

实现测试所需的 icrc1_transfer、icrc2_transfer_from、icrc1_balance_of 和 icrc3_get_blocks 子集。它不实现完整 ICRC-2 allowance 语义，不能作为完整标准符合性证据。

```sh
cargo build --release --target wasm32-unknown-unknown -p dmsg_test_ledger
```

余额、去重索引、区块和配置使用各自固定类型的稳定存储。由 [dmsg_integration](../dmsg_integration/README.md) 安装并控制，未加入 dfx.json 的部署列表。
