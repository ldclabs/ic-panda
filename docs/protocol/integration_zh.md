# 第三方接入合同

[English](integration.md) | 简体中文

源码合同核对日期：2026-10-24。专用认证、类型化应用签署与结账通过 [dmsg-connect/1](connect-v1_zh.md) 使用桌面审批。现金结算和 PANDA 承诺采用 commerce 2 / membership 2。[账户适配器](../../examples/dmsg-account-product/README.md) 与协议 fixture 展示接入方式；各外部产品的部署与真实资金验收须独立核对。旧商业端点和会员变更/买断路径已删除，不提供 v2 到 v1 转换；运行方法见 [commerce 2](commerce_zh.md)。

## 版本与职责

| 接口面 | 版本 |
| --- | --- |
| 外部登记与应用批准 | `dmsg-integration/1` |
| 认证叶 | `application/vnd.dmsg.authentication+cbor;v=1` |
| 类型化动作 profile | `application/vnd.dmsg.app-action+cose;v=1` |
| 通用结账与不可撤回会员 | `dmsg-commerce/2`、`membership/2` |
| 网页连接 | `dmsg-connect/1` |

既有三个文档 profile 保持 v1 字节。认证是经批准的独立 IC 认证叶，不是签名声明、当前安全快照、JWT、钱包 delegation 或产品角色。调用 `match_authentication_result` 前，必须验证 IC 根、预期 user home、证书时间、witness 路径与确切字节。

`dmsg_types::integration` 定义公开 Rust 类型，`dmsg_protocol::integration` 负责校验与承诺，`packages/dmsg-sdk` 导出独立 TypeScript 编码器。[integration.cddl](integration.cddl) 指定规范记录结构；CDDL 与 TypeScript 形状从同一公开类型生成，独立编码测试防止两端共享同一序列化错误。

## 身份与准入

应用固定精确规范 origin、认证接收者、动作权威、能力、封闭签名 profile、产品 ID，且仅在有 `SignAction` 时登记[自身命令 schema](app-action_zh.md)；服务哪些 user home 由登记它的 commerce 决定。产品固定报价权威、受益主体权威、适配器、subject schema/大小、商户账户、账本和条款。subject 指定其中一个受益主体权威，由其核验产品授权；权威列表只增不减，因此 dMsg 账户产品需列出全部 user home。暂停登记只阻止**新**批准与承诺，不停止对账、退款或到期处理。

应用和产品由治理登记。更新递增配置版本；已有 ID 不能改变环境或主体身份语义。更新应用 origin 不追溯改写已接受订单。canister Principal 不来自网页建议的回调地址。

批准的 dMsg AccountId、受益主体、经济操作者与 ICRC 付款人是不同字段。只有登记产品核对自身权限后，dMsg 账户才能批准项目订阅。原始报价/批准值和类型校验不是权限证据。

初始受益主体 schema：`tokenlist-project-v1`（8 字节大端 ProjectId）、`dmsg-account-v1`（12 字节 Xid）、`sample-account-v1`（12 个不透明 fixture 字节）。长度不决定 schema。示例 Principal 与 ID 只是 fixture，不是已部署身份。

## 规范字节

记录使用确定长度 CBOR map，字段名与 CDDL 文本完全一致。每个字段都出现，可选账户 subaccount 为 null 或 32 字节。Serde externally tagged enum 的 unit variant 为文本，有载荷 variant 为单项 map。Principal、AccountId、hash 都是原始 byte string。数组保留声明顺序，允许列表拒绝重复；map key 按 RFC 8949 core deterministic 字节序排列。拒绝未知字段/版本/variant、非规范编码和越界整数，不进行 Unicode 或文本归一化。

不超过 u64 的非负整数使用最短普通 CBOR 整数；更大的 u128 使用 tag 2 与最短大端 byte string。没有浮点或负整数。SDK 使用 bigint，不使用 JS Number。JSON 桥接须通过已检查的十进制字符串和明确的字节编码转换；普通 Rust Serde JSON 不是该桥接。

承诺统一为 `SHA256(CBOR([1, domain, complete_value]))`：

| 值 | 精确 domain |
| --- | --- |
| BillingOffer | `dmsg/commerce/offer/v2` |
| AuthenticationRequest | `dmsg/authentication/request/v1` |
| ApplicationApproval | `dmsg/application/approval/v1` |
| PandaQuote | `dmsg/commerce/panda-quote/v2` |
| CashQuote | `dmsg/commerce/cash-quote/v2` |
| ProductDecision | `dmsg/commerce/decision/v2` |

认证路径为单段原始字节：`b"authentication/v1/" || account_id[12] || operation_id[32]`。设备签名继续使用现有批准结构、用途专属命令域与当前 security epoch/sequence。这些 hash 本身都不是签名或授权。

## 时间与资源边界

业务时间为 u64 Unix 毫秒。COSE iat 仍为秒；ICRC created_at_time 和 IC 证书时间仍为纳秒。区间为左闭右开，等于截止时间即过期。

| 边界 | 上限 / 规则 |
| --- | --- |
| 认证请求 / 证明 | 5 分钟，证明不超过请求期限 |
| 派生产品会话 | 24 小时后须重新认证 |
| 新鲜认证证书 | 60 秒，不接受未来证书 |
| 未接受报价 | 签发后 15 分钟 |
| 现金入金 | 接受后 30 分钟 |
| 首次现金激活 | 24 小时，且不超过合同终点 |
| PANDA 申请 | 24 小时，且不超过合同终点 |
| PANDA 资格租约 | 最多 1 小时，并在合同终点截断 |
| 现金、到期与 Free 资源租约 | 最多 30 天，在下次已知额度变更处截止 |
| 费率政策通知 | 至少 15 天；首份政策可发布即生效 |
| 每账户待处理操作 | 32 |
| 成功外部批准 | 每账户每 UTC 小时 60 次 |
| 应用 origin / 产品 | 各最多 16 个 |
| 通用编码输入 | 最多 65,536 字节 |

user issuer 强制每账户待处理与每小时批准限额，要求账户活跃、未冻结且设备未撤销并持有 FormalApprove，不授予 RootManage 或 vault 读取权。commerce 配置调用返回后再次核对设备序号与 security epoch。`prune_external_approvals` 每次有界处理 64 个账户的过期记录。已接受操作超时不能授权释放结果未知的 Apply。

## 现金与 PANDA

权威 BillingOffer 以 USD 微单位为**精确完整区间**报价。免费报价不能消耗 PANDA 申请。服务从登记产品权威获取/核验报价，不使用浏览器的金额或截止时间。接受后冻结报价；Apply 前的延迟不会延长或重新定价，合格申请立即 Apply。

PANDA 质押需求严格为 `ceil(amount_usd_micros * r_num * 100_000_000 / (1_000_000 * r_den))`。中间值任意精度，只有最终结果须能放入 u128；使用当前生效的不可变费率政策。截止时间是锁期覆盖与承诺边界，不是再次按时长折算的因子。抵扣账单不是现金收入或商户现金。只支持全额抵扣，没有折扣、部分付款或买断。

产品 Apply 成功后，原合同终点不可修改。用户、产品、暂停、解绑或普通治理操作都不能缩短该记录中的承诺终点。已知失格会终止权益，在此前已签发资格租约到期后释放神经元占用；原合同终点本身不继续占用神经元。Apply 前可取消；未知决定保留原身份和预留，直到回执对账。续期创建独立批准的连续期间。SNS 所有者保留原生控制权，本合同不声称托管本金或阻止 SNS Split；有状态执行由 membership 工作包负责。

生产现金资产固定为 ckUSDT `cngnf-vqaaa-aaaar-qag4q-cai` 与 ckUSDC `xevnm-gaaaa-aaaar-qafnq-cai`，部署前仍须验证元数据、费用与 archive 行为。Local fixture 使用不同且明确的 Principal。每单承诺一个 ledger、payer、deposit、金额、换算依据、手续费上限/准备及期限。账务与区块去重按 ledger 隔离，商户准备金还按商户隔离。

## 交付与恢复

调用产品适配器前先持久化完整 ProductDecision。Apply 验证登记服务，原子写业务状态与完整终态回执。重复 ID 返回原回执；同 ID 不同字节失败。已知成功和已知拒绝均持久保存。Unknown 表示缺少经过认证的终态回执，不是允许重发或退款的新决定结果。

回执承诺 decision hash 与 adapter；Applied 回执保留原报价终点。产品授权与当前角色/版本核对独立于资金验证、dMsg 账户批准及签名数学有效性。

## 复现合同检查

```sh
cargo test --locked -p dmsg_types --test integration_contract
cargo run --locked -p dmsg_types --example integration_vectors > /tmp/integration-vectors.json
cmp /tmp/integration-vectors.json src/dmsg_types/tests/integration_vectors.json
python3 scripts/generate-integration-sdk.py --check
node --test packages/dmsg-sdk/tests/contracts.test.ts
```

向量覆盖项目与账户主体、两个不同 fixture 账本、认证、精确批准、现金/PANDA 报价、持久决定和 u128 溢出边界。这些测试不含真实外部身份、真实转账或生产默认费率。

浏览器传输与恢复见 [dmsg-connect/1](connect-v1_zh.md)。
