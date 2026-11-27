# Passkey 验证指南

Passkey 的会话、硬件与认证规则见[协议合同](protocol/passkey-v1_zh.md)。本文列出当前仓库的验证入口，历史测试结果保存在[2026 年 11 月记录](../archived/docs/passkey-validation_2026-11_zh.md)。

## 源码检查

从仓库根目录运行；大型构建前按 [AGENTS.md](../AGENTS.md) 核对磁盘空间和 Cargo 缓存位置。

```sh
cargo test --locked -p dmsg_auth --lib
cargo test --locked -p dmsg_protocol --lib passkey
cargo test --locked -p dmsg_client --lib passkey
cargo test --locked -p dmsg_integration --features pocketic-tests --test passkey -- --nocapture
```

PocketIC 使用的 Wasm 必须与待验收源码一致，构建和测试工件路径见[集成测试指南](../tests/dmsg_integration/README.md)。共享 Rust 核心改动后，应重新生成原生库并核对指纹，再运行平台测试。

## 验收范围

| 范围 | 应核对的行为 |
| --- | --- |
| Canister 与协议 | 注册、登录、续期、限流、会话上限、退出全部、升级后的 delegation 补发；旧会话和未知结果不得复活 |
| 客户端核心 | 硬件身份、父/子 delegation、准入失败重试、持久化与锁定、账户绑定和设备授权分离 |
| 桌面与网页 | 系统浏览器往返、取消、超时、重试、未注册身份；未成功创建身份不能显示注册完成 |
| iOS / Android | 真机 Passkey 和硬件凭据、App Link/Universal Link、切后台与返回、重装或硬件失效后的明确错误 |
| Windows 桌面 | TPM 2.0 与 Windows Hello 真机：入口显示条件、注册与登录、首次绑定、前台提示、取消与超时、提示期间锁定、分段凭据中断后重试、跨 daemon 重启续期、清除 TPM 或重置并重建同名 Hello 钥匙后的失效提示；停用 Hello 后仍检查已有锚，无 TPM 或未设置 Hello 时仅提供 II 新登录 |
| 部署 | issuer 与准入配置、目标 canister、网页及原生工件版本、当前会话可用性 |

UI、硬件、真实浏览器和部署检查各自留证，不能由 Rust 单测、模拟器或一次只读健康查询代替。恢复整个账户还依赖账户的设备批准或事先启用的链上恢复策略，Passkey 登录成功不等于内容可解密。

## 性能记录

历史记录包含 Wasm 哈希、指令数、cycles 与 stable 页数。它们只用于同环境回归对照：MemoryManager 的初始分配不能当作每用户成本，单次续期不能推出生产吞吐或年成本。

复测时记录提交、Wasm 哈希、工具链、fixture、实际结果与未运行项。本次文档审查未重跑上述构建、PocketIC、真机或生产验收。
