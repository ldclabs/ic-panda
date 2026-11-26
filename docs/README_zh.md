# dMsg 文档

[English](README.md) | 简体中文

dMsg 是 ICPanda DAO 的产品之一，为人与智能体提供私密协作。先看[产品介绍与下载](../README.md#-dmsg--private-collaboration)，再选择下列实现或协议说明。

审查日期：2026-11-26；公开源码基线 `2899c99e`。本目录描述该基线的实现，历史测量保留原日期和范围；未核验线上部署。私有设计按 [AGENTS.md](../AGENTS.md) 定位，只用于核对目标与实现差异，不在这里公开正文。

## 使用与开发

| 主题 | 说明 |
| --- | --- |
| 安装与更新 | [下载](https://github.com/ldclabs/ic-panda/releases/latest) · [发布脚本](../scripts/dmsg/README.md) |
| 桌面工作台 | [桌面应用](../src/dmsg_desktop/README.md) · [当前设计系统](../src/dmsg_desktop/DESIGN.md) |
| 凭据与文件引用 | [桌面复制 ID/引用、进程交付、挂载、SSH、分享与验证教程](dmsg_secrets_tutorial_zh.md) |
| 命令行与 MCP | [CLI 与 daemon](../src/dmsg_cli/README.md) · [Agent 连接器](../src/dmsg_agent/README.md) |
| 移动端 | [iOS](../src/dmsg_ios/README.md) · [Android](../src/dmsg_android/README.md) · [移动合同](protocol/mobile_zh.md) · [Android 服务](protocol/android_zh.md) |
| 可复用 Rust 核心 | [dmsg_client](../src/dmsg_client/README.md) · [dmsg_mobile](../src/dmsg_mobile/README.md) |
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
| 多签钱包 | [产品范围](dMsg_wallet_zh.md) · [公开合同](protocol/wallet_zh.md) · [桌面实现](../src/dmsg_desktop/README.md) |
| 保险库离线备份 | [`.dmsgvault` 格式与导入边界](protocol/vault_backup.md) |
| 登录与验证 | [Passkey 合同](protocol/passkey-v1_zh.md) · [验证指南](passkey-validation_zh.md) |
| 旧版数据与迁移 | [只读盘点](legacy_inventory_zh.md) · [个人档案](protocol/legacy_archive_zh.md) · [冻结证据](protocol/legacy_freeze_zh.md) |

[协议索引](protocol/README_zh.md#协议导航)还覆盖云端投递、频道、浏览器登录、dmsg-connect、commerce 2、Agent Delegation 和 Mail 事件。各 canister README 负责具体限额、测量与运维；Candid 和共享向量定义可执行接口。

## 当前边界

- 桌面应用、CLI 与 MCP 共用 Rust daemon；iOS 和 Android 通过 UniFFI 直接复用 Rust 核心。网页应用使用 dmsg-connect/1，原浏览器扩展已退役。各平台发布与真机验证见源码目录中的 README 和验证记录。
- `dmsg_user` 与 `dmsg_directory` 已有线上用户数据。稳定布局变化必须提供保留数据的升级与旧 Wasm 验证，不得重装代替迁移。
- 桌面端暂停新建多签钱包，已有钱包仍可使用；这不改变 CLI、daemon 或 hub 的创建接口。原生机密文件挂载有独立的 OS、打包与验证要求。
- 旧版个人档案继续支持阅读与导入。共享旧频道继承流程已从公开客户端删除；当前频道协议与旧版证据格式仍分别承担不同职责。
- [安全审查报告](../DMSG_SECURITY_AUDIT_REPORT.md)记录审查范围、发现、修复与验证结果。既有本地测试与容量样本仅证明注明的范围，不是完整生产验收。

## 历史记录

已完成的设计迭代和单次调查保存在 `archived/docs/`：

- Statement v3 设计依据：[English](../archived/docs/statement-v3-design.md) · [简体中文](../archived/docs/statement-v3-design_zh.md)。实现以现行协议和 CDDL 为准。
- [桌面设计与实施记录](../archived/docs/dmsg_desktop_design_zh.md)。当前组件规则放在桌面源码旁。
- [2026-10-22 解锁性能调查](../archived/docs/dmsg_desktop_performance_zh.md)。样本与未运行项保留原适用范围。

被后续实现覆盖的 canister 每日增量保留在 Git 历史。新增行为应写入对应主题，必要时注明版本证据，不追加相互冲突的第二份合同。中英文架构章节编号保持一致，移动文档时更新入链；`protocol/integration.cddl` 由脚本生成，不手工编辑。

多数主题提供英文 `name.md` 与简体中文 `name_zh.md`，通过页首切换；教程、部分移动端合同和验证记录目前仅有一种语言。CDDL 是两种语言共用的机器可读合同。
