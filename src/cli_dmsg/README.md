# cli_dmsg（旧服务工具）

旧 ic_message/PANDA 服务的命令行工具，不是新版 dmsg_user/dmsg_cose 的 SDK。当前代码连接固定的公共 IC 网关和旧 canister ID。

```sh
cargo run -p cli_dmsg -- --help
cargo run -p cli_dmsg -- blocks -s 0 -l 10
```

`blocks` 读取旧名称区块。`send` 使用 --id-file 的身份执行实际 PANDA 转账，并把发送记录写入指定 CBOR 文件；应先核对参数及目标。默认身份为 Anonymous。本轮重构没有修改旧服务行为。新版公开协议见 [dmsg_protocol](../dmsg_protocol/README.md)。
