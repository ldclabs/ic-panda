# dMsg 文档

[English](README.md) | 简体中文

dMsg 是 ICPanda DAO 的产品之一，为人与智能体提供私密协作。先看[产品介绍与下载](../README.md#-dmsg--private-collaboration)，再选择下列实现或协议说明。

本索引于 2026-10-24 按公开源码 `9fd70dca` 核对。除非注明具体发布、部署或测量，文档描述的是源码行为。源码变更、可安装版本、生产部署和独立审计是不同事实。私有设计按 [AGENTS.md](../AGENTS.md) 定位，不在此公开其正文。

## 使用与开发

| 主题 | 说明 |
| --- | --- |
| 安装与更新 | [下载](https://github.com/ldclabs/ic-panda/releases/latest) · [发布脚本](../scripts/dmsg/README.md) |
| 桌面工作台 | [桌面应用](../src/dmsg_desktop/README.md) · [当前设计系统](../src/dmsg_desktop/DESIGN.md) |
| 命令行与 MCP | [CLI 与 daemon](../src/dmsg_cli/README.md) · [Agent 连接器](../src/dmsg_agent/README.md) |
| 可复用 Rust 核心 | [dmsg_client](../src/dmsg_client/README.md) |
| 网页与应用接入 | [SDK](../packages/dmsg-sdk/README.md) · [账户适配示例](../examples/dmsg-account-product/README.md) · [桥接页](../src/dmsg_connect/README.md) |
| 本地验证 | [集成测试](../tests/dmsg_integration/README.md) · [客户端向量](../tests/vectors/client/README.md) · [Agent 向量](../tests/vectors/agent-protocols/README.md) |

## 架构与合同

| 主题 | 维护中的文档 |
| --- | --- |
| 架构、容量与多实例 | [English](dmsg_architecture.md) · [简体中文](dmsg_architecture_zh.md) |
| Canister 职责、升级与验证 | [English](dmsg_canisters.md) · [简体中文](dmsg_canisters_zh.md) |
| 协议索引、签名与回执 | [English](protocol/README.md) · [简体中文](protocol/README_zh.md) |
| 加密与恢复边界 | [加密实现](dmsg_encryption_zh.md) · [账户与根](protocol/account_root_zh.md) |
| 凭据受控使用与秘密分享 | [Secrets 合同与验证入口](protocol/secrets_zh.md) |
| 团队钱包 | [产品范围](dMsg_wallet_zh.md) · [公开合同](protocol/wallet_zh.md) · [桌面实现](../src/dmsg_desktop/README.md) |
| 旧版数据与迁移 | [只读盘点](legacy_inventory_zh.md) · [个人档案](protocol/legacy_archive_zh.md) · [冻结证据](protocol/legacy_freeze_zh.md) |

[协议索引](protocol/README_zh.md#协议导航)还覆盖云端投递、频道、浏览器登录、dmsg-connect、commerce 2、Agent Delegation 和 Mail 事件。各 canister README 负责具体限额、测量与运维；Candid 和共享向量定义可执行接口。

## 当前边界

- 主客户端是共用 Rust daemon 的桌面应用与 CLI；网页应用使用 dmsg-connect/1，原浏览器扩展已退役，尚无移动端应用。
- `dmsg_user` 与 `dmsg_directory` 已有线上用户数据。稳定布局变化必须提供保留数据的升级与旧 Wasm 验证，不得重装代替迁移。
- 桌面端暂停新建团队钱包，已有钱包仍可使用；这不改变 CLI、daemon 或 hub 的创建接口。原生机密文件挂载有独立的 OS、打包与验证要求。
- 旧版个人档案继续支持阅读与导入。共享旧频道继承流程已从公开客户端删除；当前频道协议与旧版证据格式仍分别承担不同职责。
- dMsg 尚未经过独立审计。既有本地测试与容量样本仅证明注明的范围，不是完整生产验收。

## 历史记录

已完成的设计迭代和单次调查保存在 `archived/docs/`：

- Statement v3 设计依据：[English](../archived/docs/statement-v3-design.md) · [简体中文](../archived/docs/statement-v3-design_zh.md)。实现以现行协议和 CDDL 为准。
- [桌面设计与实施记录](../archived/docs/dmsg_desktop_design_zh.md)。当前组件规则放在桌面源码旁。
- [2026-10-22 解锁性能调查](../archived/docs/dmsg_desktop_performance_zh.md)。样本与未运行项保留原适用范围。

被后续实现覆盖的 canister 每日增量保留在 Git 历史。新增行为应写入对应主题，必要时注明版本证据，不追加相互冲突的第二份合同。中英文架构章节编号保持一致，移动文档时更新入链；`protocol/integration.cddl` 由脚本生成，不手工编辑。

本目录每个 Markdown 主题都有英文 `name.md` 和简体中文 `name_zh.md`，章节对应，可通过页首切换语言。CDDL 是共享的机器可读合同，两种语言都引用同一份定义。
