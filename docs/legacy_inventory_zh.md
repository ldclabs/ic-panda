# 旧部署只读盘点工具

`src/dmsg_app/scripts/legacy-inventory.mjs` 读取现有 canister ID 配置，沿消息、频道、资料和 OSS cluster 的公开引用发现部署实例。工具固定使用匿名身份、主网证书根与 query 签名校验，只发 `read_state` 和白名单 query，不读取本机身份密钥，不发送 update。

从公开仓库根目录执行（依赖现有工作区 Node 模块）：

```sh
node src/dmsg_app/scripts/legacy-inventory.mjs \
  --ids canister_ids.json \
  --output misc/legacy-inventory/new-observation
node --test src/dmsg_app/scripts/legacy-inventory.test.mjs
```

必须使用新的输出目录。证据文件以独占写入方式保存，避免覆盖已有记录；公共 checkout 内只允许输出到 Git 忽略的目录。`--host` 默认 `https://icp-api.io`，只接受 HTTPS，不拉取自定义 root key。`--max-canisters` 默认 64、最多 256，用于限制动态发现规模；工具不递归扫描所有 controller 的其它业务。

输出包括：

- 每个实例的 module hash、controllers、已发布 Candid 和 BLS read_state 证书；证据文件另有 SHA-256。
- 经过节点签名验证的服务计数、路由、角色和配置投影；不保存 get_state 中的 latest_usernames 等无关字段。
- COSE 的无派生输入缓存公钥查询及原始公钥 bytes 的 SHA-256 指纹。该指纹不是 RFC 9679 COSE_Key 指纹，也不证明配置 key 名与管理 canister 当前派生根一致。
- Channel/Profile 指向的 cluster/bucket、cluster 公布的 bucket 列表与部署摘要。部署参数 bytes 不写入报告。
- 调用失败、字段缺失和采集时间范围。可选字段 null 表示不可得或被省略，不能当作链上没有该配置/密钥。

认证 module hash 不等于已完成源码复现构建。若要关联发布版本，另行下载候选 release 工件核对其实际 bytes 哈希，记录 tag commit 和对应 Candid；不能直接以 dfx.json 的下载版本或当前 checkout 推断现网版本。

服务状态 query 是节点签名观察，不是所有用户数据的认证冻结快照；多实例采集也不是跨 canister 的原子快照。运行内存、cycles、稳定状态导出、完整 ACL、在途账务、未完成上传和用户解密能力仍需要各自的权限及验证。工具不会申请 token、执行 vetKD/ECDH、登录用户或导出用户内容。

真实实例清单、治理角色、运行计数、样本授权登记和恢复材料应保存在私有运维或受控本地目录，不加入公开仓库。此工具不执行冻结、升级、转账或服务退出。
