# dMsg canisters：实现与验证

[English](dmsg_canisters.md) | 简体中文

核对日期：2026-10-24，公开源码基线 `9fd70dca`。本文导航当前实现，不是部署清单或审计报告。整体架构与实测容量见 [dmsg_architecture_zh.md](dmsg_architecture_zh.md)；精确接口与字节以各 canister 的 Candid 和[公开协议](protocol/README_zh.md)为准。

## 组件

控制面有七类服务，另有团队钱包工厂与每个团队的钱包实例。客户端、桥接页与私有云端分别承担自己的职责。

| 包 | 职责 | 文档 |
| --- | --- | --- |
| dmsg_types | 公开数据合同，包含基础签名、ICP 接口和可选应用协议 | [README](../src/dmsg_types/README_zh.md) |
| dmsg_protocol | 确定性编码、批准构造、标准 COSE 验签、CTT 摘要 | [README](../src/dmsg_protocol/README_zh.md) |
| dmsg_runtime | 参考实现共用的稳定记录、认证树、账本和调用工具 | [README](../src/dmsg_runtime/README.md) |
| dmsg_user | 主体、认证、设备、登录恢复、本机解锁秘密、根承诺与设备签名认证 | [README](../src/dmsg_user/README.md) / [Candid](../src/dmsg_user/dmsg_user.did) |
| dmsg_cose | 内容根 vetKD 公钥与恢复派生 | [README](../src/dmsg_cose/README.md) / [Candid](../src/dmsg_cose/dmsg_cose.did) |
| dmsg_handle | 名称权属、导入、收费和转移 | [README](../src/dmsg_handle/README.md) / [Candid](../src/dmsg_handle/dmsg_handle.did) |
| membership | PANDA SNS 资格、跨产品占用与授益决定 | [README](../src/membership/README.md) / [Candid](../src/membership/membership.did) |
| dmsg_commerce | 套餐、现金订单、退款与认证资源权益 | [README](../src/dmsg_commerce/README.md) / [Candid](../src/dmsg_commerce/dmsg_commerce.did) |
| dmsg_payment | 固定条款托管、入金验证、互斥资金决定与出金 | [README](../src/dmsg_payment/README.md) / [Candid](../src/dmsg_payment/dmsg_payment.did) |
| dmsg_directory | Agent Delegation principal 文档的认证 HTTP 发布 | [README](../src/dmsg_directory/README.md) / [Candid](../src/dmsg_directory/dmsg_directory.did) |
| dmsg_wallet_hub | 团队钱包的激活、创建、版本登记、升级与锁定、成员索引 | [README](../src/dmsg_wallet_hub/README.md) / [Candid](../src/dmsg_wallet_hub/dmsg_wallet_hub.did) |
| dmsg_wallet | 团队钱包实例：成员、策略、提案、ICP/ICRC 转账、链密钥签名、审计 | [README](../src/dmsg_wallet/README.md) / [Candid](../src/dmsg_wallet/dmsg_wallet.did) |

## 当前合同

- **账户与设备**：`dmsg_user` 发放 12 字节 Xid 账户，管理登录绑定、设备能力、内容根承诺与延迟恢复。账户可登记每台设备的硬件批准钥匙（P-256），并按用途要求设备批准附带其签名；核验集中在 user home 的设备批准检查，消费方 canister 不变，登录恢复会退役全部钥匙并关闭要求，见[账户合同](protocol/account_root_zh.md)。客户端通过 `dmsg_handle` 发现 user homes，并行查询账户。按既定设计，不强制跨 home 的登录 Principal 唯一性。
- **内容与恢复**：设备生成内容根并为授权设备和恢复封装。`dmsg_cose` 只在全设备丢失恢复时执行 vetKD，不签署普通文档或 Agent Delegation 事件。
- **签名**：设备在本机签署 COSE 声明和应用动作；`attest` / `attest_app_action` 核验签名与设备批准、扣减执行额度、发布认证回执。公钥本身不证明账户授权，见[声明与回执](protocol/README_zh.md)。
- **公开身份**：user home 保存官方 PrincipalDocument 展示字段及 controller 记录，`dmsg_directory` 发布认证 JSON。展示资料更新写链，公开头像二进制留在云端。Agent controller 是客户端 vault 中自持的 Ed25519 key，见 [Agent Delegation](protocol/agent_zh.md)。
- **名称**：注册表保存规范名称及可选展示大小写，处理注册、旧名称认领和账户批准的转移；注册费直接进入配置的收入账户。
- **支付与订阅**：`dmsg_payment` 结算付费来信托管；`dmsg_commerce` 实现 commerce 2 现金结账、资源权益与自动归集，并接收登记验证方的 Apple 交易事实；共享 `membership` 实现 membership 2 的 PANDA 资格与跨产品占用。见[商业合同](protocol/commerce_zh.md)。这些职责独立于日常加密投递。
- **团队钱包**：hub 激活实例并管理已登记的代码版本；各钱包管理自己的提案、批准、资金、链上密钥签名与审计。升级先由钱包接受。桌面端当前暂停新建，已有钱包操作继续提供，见[钱包合同](protocol/wallet_zh.md)与[桌面范围](dMsg_wallet_zh.md)。

普通消息、文件、保险库编辑和频道控制不增加逐条 ICP update。桌面应用、CLI 和 MCP 共用 Rust daemon；[iOS](../src/dmsg_ios/README.md) 通过 UniFFI 直接复用客户端核心，Apple 与系统能力见 [mobile 合同](protocol/mobile_zh.md)；网页应用经 [dmsg-connect/1](protocol/connect-v1_zh.md) 接入。云端互操作见 [cloud_zh.md](protocol/cloud_zh.md)，不在此复制私有实现。

## 持久化与升级

公开 Candid/CBOR 与私有稳定编码分开。`dmsg_runtime::stable_types` 和各 canister 的 `stable_codec.rs` 使用整数键紧凑表示，不改变签名字节或认证值。当前 schema 常量与可接受的旧布局以存储及升级代码为准；版本号相同或递增本身不能证明兼容。

**`dmsg_user` 和 `dmsg_directory` 已有线上用户数据。** 布局变化必须提供保留数据的原地升级，并验证旧版 Wasm 到新版 Wasm。现有 user 14 → 15 → 16、directory 6 → 7 升级保留原数据；directory 首次转换会构建新增的文档认证树；user 16 新增的批准钥匙与硬件批准政策对旧账户读为空，认证值不变。不得以重装代替迁移。[Principal 升级检查](../scripts/test-dmsg-principal-upgrade.sh)覆盖 14 → 当前、directory 转换及相关 handle 变化，[批准钥匙升级检查](../scripts/test-dmsg-hardware-upgrade.sh)从 schema 15 的实际模块升级并核对账户、认证叶、解锁秘密与操作回放。

其他 canister 遵循各自支持的布局与恢复流程。含账户、托管资金、账务或不可重建记录的实例，不得通过重装修复。跨 canister 结果未知时保留原请求与账本参数对账。目标部署的配置、模块哈希、controllers 和运行就绪状态须另外核验。

## 构建与验证

使用仓库锁定的依赖及[集成测试指南](../tests/dmsg_integration/README.md)中的 PocketIC 环境：

```sh
rustup target add wasm32-unknown-unknown
make build-dmsg
POCKET_IC_BIN=/path/to/pocket-ic make test-dmsg
python3 scripts/generate-integration-sdk.py --check
```

测试目标包含 Rust 测试、Clippy、Wasm 构建、Candid 一致性、独立协议向量与 PocketIC 调用。客户端、桌面、平台凭据库、原生挂载与真实应用接入各有独立测试路径。本次文档更新未重跑这些完整套件。

各组件 README 保存具体测量与测试边界。历史测试数量和 Wasm 大小不是当前验收标准；容量上限属于防护值，只有注明的实测负载支持容量结论。模拟账本、CMC 替身、本地中继和浏览器 fixture 不代表真实资金、主网、签名分发或独立审计验收。
