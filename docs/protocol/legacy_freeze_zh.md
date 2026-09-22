# Legacy 服务冻结与快照合同

2026-09-22。以下为本地候选实现及测试边界，不表示已对生产服务停写或升级。

## 消息、资料、频道和名称身份

四个旧 canister 使用独立稳定内存 200 保存冻结状态，原业务存储布局保持不变。`admin_legacy_drain(cutover_id)` 进入 Draining；所有普通和管理员业务入口拒绝新写。已开始的异步操作有持久 ticket，跨 canister 返回后检查当前阶段、boot 和时间。原业务参数用于未知结果对账，只有 controller 能读取。

传输或响应解码失败不会被当作“肯定没有发生”：父请求返回后仍保留未知 ticket。活跃调用不能强制清掉；已结束的未知调用、升级中断的调用需 controller 用实际核对材料调用 `admin_legacy_resolve(ticket,evidence_digest)`。该摘要是管理核对记录，不是自动生成的账本证明。清点和账本证据仍须独立保留。

`admin_legacy_seal(cutover_id)` 只在 Draining、ticket 全部收敛且基线核对完成后进入 ReadOnly。首次从未登记 ticket 的旧模块升级，需要 `admin_legacy_acknowledge_baseline(evidence_digest)`；新跟踪机制不能追溯证明升级前没有在途请求。没有重开旧业务的接口；冻结后也不能通过同模块的普通升级配置参数改业务配置。

保留现有读取、原 `my_iv`、频道下载 token、名称身份 `sign_in/get_delegation`，以及必要的名称委托撤销。名称身份冻结角色在稳定内存 201 单独保存，后续登录时间或委托收窄不会重写冻结角色。旧认证与名称事件的 certified_data 树不被快照功能覆盖。

## 可独立验证的分页快照

`legacy_snapshot(scope,after)` 是有界 update，调用者保留 `request_status` 的 IC 证书及原请求标识。`@dmsg/legacy` 的 `collectSnapshot` / `verifySnapshotProof` 验证可信 IC 根、目标 canister、method/参数/调用者对应的 request ID、认证 reply、ReadOnly 状态、范围、游标、总数和 rolling digest。历史证书作为冻结时证据验证，不转换成当前操作权限。

范围包括公开名称/冻结角色，以及按原 caller 权限读取的 profile、频道、消息。每页最多 64 条、128 KiB 记录；缺失消息槽位显式编码为 None。名称和其他有序集合固定顺序，profile 中原 HashMap 先转有序表示。每行保留原 Candid bytes；源版本、cutover 和范围属于承诺。名称事件 tip 作为附加上下文，不能代替完整名称映射摘要。

## COSE 与 OSS 的作用域

COSE 候选基于实际使用的 v0.9.3，新增 `admin_legacy_drain_namespace` / `admin_legacy_seal_namespace`。只关闭指定 namespace 的数据/权限写入和新业务签名；异步业务签名在返回前再检查冻结状态。原 setting、keyId、ownership、派生路径、根与 context 不替换，ECDH/vetKD 恢复继续可用。冻结时保存 setting/历史 payload 摘要。仍被 legacy 使用的全局根配置不能静默更换。

OSS 候选基于 v1.2.3，新增 `admin_legacy_drain_folder` / `admin_legacy_seal_folder`。根目录 0 不可作为隐含“冻结全 bucket”的参数。目录及后代的数据写入、移动、删除，在存储入口统一拒绝；移动/删除包含冻结目录的祖先也被阻止。既有写 token 不绕过该限制；其他目录仍按原权限读写。

`legacy_folder_manifest` 按原读取权限返回已封存文件的 metadata、每个实际 256 KiB 传输片的摘要与长度、缺片、实际 ciphertext 摘要和完整性标记；它不会把 OSS 片标成独立 AEAD 块。文件整体解密仍由授权客户端完成。带 token 的原始请求证据应留在加密材料中，不作为公开附件或日志发布。

两个依赖仓库的候选分支为 `codex/dmsg-legacy-freeze-2026-09-22`，从相应 release tag 创建；不覆盖主 checkout。具体本机位置只写入被忽略的开发记录。

## 操作材料与验证

`src/dmsg_app/scripts/legacy-freeze-plan.mjs` 从目标 inventory、已完成基线核对文件和 cutover ID 生成分阶段、绑定模块哈希的 Candid controller payload。工具不签名、不提交、不升级、不停写；调用前仍须核对部署、控制权限、在途操作和实际证据。生产执行不属于本轮开发授权。

本地测试使用 I0 已匹配公开 release 的 Wasm：四服务升级后旧 IV、profile、消息保持；真实跨 canister 等待阻止提前 seal；Chrome 验证多页快照及篡改负例。另有 COSE 的 ECDH/VetKey/包装前后对照、OSS 旧写 token 阻断及其它目录继续写入测试。模拟账本只用于建立测试数据，无真实资金。

真实用户三模式恢复、生产 controller/在途请求盘点、独立发布与退出验收仍不能由这些合成测试替代。旧 minter 未整体停用；来源业务冻结阻止新 PoL 事件，历史权益、结算和是否停发仍按独立治理决定处理。

`Attestation { digest, expires_at }` 是显式迁移批准范围：仅在 ReadOnly 且调用者非匿名时，返回该 caller、32 字节意图摘要和最多七天的期限；不修改旧账户、频道、ACL 或密钥。客户端必须展示摘要对应的完整方案并取得用户点击批准；服务端还须独立验证冻结 ACL，不能把任意 caller 的证明当作管理员权利。调用与认证仍使用 `legacy_snapshot` 的 ingress 证明。
