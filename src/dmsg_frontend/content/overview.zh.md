# dMsg — AI 时代的私密工作台

dMsg 是保管密码、笔记与文件的私密工作台：用端到端加密保护资料，在你授权的设备间同步保险库，通过链接分享选定文件，并为接入的 AI 助手设置明确的访问权限。dMsg 是 ICPanda DAO 的产品，其链上账户与设备规则的升级由 DAO 通过公开提案治理。

[English](https://dmsg.net/_/about.html) · [最新发布与校验值](https://github.com/ldclabs/ic-panda/releases/latest) · [公开源码](https://github.com/ldclabs/ic-panda) · [智能体阅读索引](https://dmsg.net/llms.txt)

## 要点

- **它是什么：** 端到端加密的保险库与文件库，带文件分享、私密频道和按权限接入的 AI Agent。
- **支持平台：** iPhone 与 iPad（App Store）、Apple Silicon Mac、Windows x64，命令行另支持 Linux。Android 正在开发。
- **价格：** <!-- prices -->
- **对方无需注册：** 接收者打开文件分享链接、客户通过文件收集链接上传，都只需要浏览器。
- **AI Agent：** 通过 MCP 接入，拥有独立的 Agent ID 和被授予的权限。Agent 可以请求正式签名，只有你能批准。
- **谁能修改规则：** 账户与设备授权运行在 Internet Computer（ICP）的 canister 中，升级需要 ICPanda DAO 的提案和投票。
- **开发者：** ICPanda DAO。客户端、协议与 canister 源码公开。

## 可以用它做什么

### 保管自己的私密资料

把密码、一次性验证码、私人笔记和密钥存入加密保险库，把文件存入设备上的加密文件库。一个人就能开始使用，不需要先组建团队或配置 Agent。一段时间未操作后 dMsg 会自动锁定，查看或复制敏感内容需要主动点击。

### 在多台设备上使用

保险库记录以密文形式在获准访问保险库的设备间同步。新设备需要经过你的批准，仅登录并不会获得解密权限。本地文件库的方式不同：你主动备份某个文件，之后可以在另一台已授权设备上下载这份加密备份。dMsg 不会自动镜像整个本地文件库。

### 收集与分享文件

- **收集：** 为客户或协作者创建文件收集链接。对方在浏览器中上传，无需注册 dMsg 账户；该链接不能用来打开你已收到的文件。文件收集需要 Plus 或以上套餐。
- **保管：** 查看收到的文件，选择需要的存入加密文件库。
- **交付：** 通过下载链接或频道分享单独加密的文件副本。接收者在浏览器中打开链接即可，无需账户。其他文件和保险库记录继续保持私密。

私密频道在受邀成员之间传递端到端加密的消息和文件，加入频道不会获得你的保险库访问权。公开资料与联系规则决定谁可以向你发送私密来信。

### 为 AI Agent 划定权限

通过 MCP 接入兼容的 AI 客户端。每个 Agent 有自己的 Agent ID，只能使用你授权的记录、文件夹、频道和服务。已授权的读写在该范围内执行；需要你正式签名的请求会等待你批准。Agent 还可以通过开放的 [Agent Protocols](https://github.com/ldclabs/agent-protocols) 交换加密的 Agent Mail、参与带签名的 Agent Discourse 讨论、发布 Agent Knowledge。公开内容与私密工作区数据的可见范围不同。

### 签名与验证

签名前，在审批窗口查看具体文档或受限的应用动作。网站通过 `connect.dmsg.net` 发起签名请求，无需浏览器扩展。导出的签名证据可在[浏览器验证器](https://dmsg.net/_/verify)核对，原文件只在本地检查。团队可以用多签钱包按审批门槛共同管理资产；Agent 权限和钱包成员身份都不能绕过这些规则。

## 谁能修改规则

端到端加密让内容保持私密。另一个问题是：决定哪些设备可以访问账户的规则，谁能修改？

- **内容**在已授权的客户端加密和解密，云端只保存和投递密文。
- **账户与设备授权**运行在 Internet Computer 的 canister 中。客户端依据 ICP 证据核验经认证的账户状态与设备记录，仅修改云端数据库既不能授权一台设备，也不能生成它所需的密钥。
- **这些 canister 的升级**需要 ICPanda DAO 的 Service Nervous System（SNS）提案和投票。2026-11-12 核对时，dMsg 各控制 canister（包括提供 dmsg.net 的那一个）的唯一 controller 都是 ICPanda SNS root。

任何人都可以查证。提案和投票记录在 [ICPanda SNS 仪表板](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai)，每个 canister 的页面会显示它的 controller，例如[账户 canister](https://dashboard.internetcomputer.org/canister/wqqa6-3aaaa-aaaaj-qsjvq-cai)。

治理本身也是你需要信任的一部分：通过的提案可以升级链上代码并改变规则。应用程序及其分发与更新、云服务，都在这项投票之外发布和运营。各信任域见[技术架构](https://dmsg.net/_/docs/architecture.md)。

## 安装与开始使用

dMsg 已正式上架 App Store，支持 iPhone 和 iPad，需要 iOS 18、iPadOS 18 或更新版本。桌面安装包面向 macOS Apple Silicon 和 Windows x64，Windows 的验证覆盖较少。命令行还支持 Linux x86_64 与 arm64。Android 正在开发。

- [在 App Store 下载 dMsg](https://apps.apple.com/app/id6827050680)
- [下载 Mac 版](https://github.com/ldclabs/ic-panda/releases/latest/download/dMsg-mac-arm64.dmg)
- [下载 Windows 版](https://github.com/ldclabs/ic-panda/releases/latest/download/dMsg-win-x64.exe)
- [全部下载、命令行与校验值](https://github.com/ldclabs/ic-panda/releases/latest)

使用 Homebrew 安装：

```sh
brew install --cask ldclabs/tap/dmsg-desktop
brew install ldclabs/tap/dmsg
```

安装并设置账户后，先保存一条私人笔记或一份文件。开始依赖它时，再添加另一台设备并检查恢复设置。安装、更新与诊断命令见 [CLI 参考](https://dmsg.net/_/docs/cli.md)。

## 外部智能体如何使用

阅读本站和公开文档不需要登录、凭据或 JavaScript，也不会因此取得任何人的工作区访问权。

需要调用工具时，由用户创建 Agent ID、授予所需权限，并向 MCP 客户端提供该智能体的凭据。MCP 配置使用命令 `dmsg`、参数 `mcp`，通过环境变量 `DMSG_AGENT_TOKEN` 传入凭据。凭据保存在客户端的受保护配置中，不应公开到提示词或文档。MCP 会话以该 Agent ID 行动。

以已安装服务的 `tools/list` 为准获取准确的工具和 schema。工具覆盖 Agent Protocols，以及获授权的频道、保险库记录、签名请求和钱包提案等操作。保险库读取会遮蔽秘密与 OTP 字段；经批准的凭据使用有独立策略。智能体只能发起敏感请求，不能自行批准，也不能管理用户的设备。你允许外部 AI 客户端读取的内容，也会受该客户端数据处理政策的影响。

- [连接器、工具与权限](https://dmsg.net/_/docs/agents.md)
- [MCP 配置与命令](https://dmsg.net/_/docs/cli.md)
- [PrincipalDocument 与 Agent Delegation](https://dmsg.net/_/docs/agent-protocol.md)
- [受控凭据使用](https://dmsg.net/_/docs/secrets.md)

## 网站集成

`dmsg-sdk` 通过 `dmsg-connect/1` 提供认证、文档签名、受限应用动作和结账能力。桥接页转述浏览器报告的来源，将请求交给用户设备；它不能批准、签名或读取保险库。应用应保留原 operation ID，结果未知时先核对，避免重复提交。

入口：[SDK 指南](https://dmsg.net/_/docs/sdk.md)、[连接协议](https://dmsg.net/_/docs/connect.md)、[集成合同](https://dmsg.net/_/docs/integration.md)。

## 隐私、恢复与信任边界

加密保护的是内容，不是全部服务信息。大小、时间、设备信息，以及协议要求公开的身份与授权记录仍可能可见。日常消息与文件分块不存储在链上。

云端可用性、客户端更新、恢复服务和外部 AI 客户端仍有各自的信任边界。撤销分享链接或 Agent 权限会停止新的授权，但不能删除已下载的副本、截图或已经获知的秘密。公开源码不能证明具体部署使用了哪份代码。[安全审查报告](https://github.com/ldclabs/ic-panda/blob/main/DMSG_SECURITY_AUDIT_REPORT.md)记录审查范围、发现、修复与验证结果。

**提前准备设备丢失。** 建议配对另一台授权设备。当前公开实现中新账户默认关闭链上恢复，需要在已解锁的管理员设备上主动开启。只有开启后，事先绑定的登录身份才能在全部设备丢失时申请恢复，经过可配置的一至七天等待期；任何仍获授权的设备都可以在此期间提出异议。已有账户保留原来的恢复设置。登录身份本身不等于内容解密权；未开启恢复且全部授权设备丢失时，内容无法恢复。恢复不能找回从未上传或备份的本机文件，目前也没有完整的离线账户恢复或导出工具。详见[账户与恢复合同](https://dmsg.net/_/docs/account.md)及其部署前提。

## 会员与费用

Free、Plus、Pro、Max 增加存储、设备和使用额度；付费套餐包含文件收集，不改变加密或审批要求。开始使用私密工作台不需要先购买 handle 或 PANDA。

<!-- membership -->

以上与交互首页共用同一份注明核对日期的公开数据。请在应用中选择套餐，结账前确认当前价格、支付方式与续期条款。当前桌面会员流程支持 ckUSDC 或 ckUSDT 付款，符合条件的 PANDA 神经元也可以抵扣年度会员；不同平台和版本的购买路径可能不同。钱包激活另行收费。频道额度适用于自己创建或赞助的频道；普通设备签名不消耗每月正式签名额度。单文件备份上限适用于云备份，文件分享和频道附件有各自的限制。[商业合同](https://dmsg.net/_/docs/commerce.md)描述权益和支付语义。

## 常见问题

### 我可以一个人使用 dMsg 吗？

可以。先保存私人笔记、登录信息和文件，再按需要邀请他人或接入 AI 助手。

### 哪些内容会在多台设备间同步？

加密保险库记录会在获准访问该保险库的设备间同步。新设备需要经过批准，仅登录并不会获得解密权限。本地文件库中的文件保留在当前设备，只有主动备份或分享副本时才会上传。你可以在另一台获准访问这些内容的设备上下载云备份。dMsg 不会自动镜像整个本地文件库。

### 客户收发文件也需要注册 dMsg 吗？

客户通过文件收集链接上传，或通过文件下载链接接收文件时，可以直接使用浏览器，无需注册 dMsg。创建文件收集链接需要 Plus 或以上套餐。加入 dMsg 频道属于另一种需要账户的协作方式。

### 接入的 AI Agent 可以访问什么？

通过 dMsg 访问受保护资料与操作时，你授予的权限决定 Agent 的使用范围。已授权的读取、写入可以在该范围内执行；需要你正式签名的请求仍须由你批准。隐藏的凭据值有单独的控制机制。传给外部 AI 客户端的内容，也会受到该客户端数据处理方式的影响。

### 端到端加密保护哪些内容？

它保护授权端点之间的私密内容，例如保险库记录、私密频道消息和加密文件副本。它不会隐藏全部服务信息；大小、时间、设备信息，以及部分公开身份或授权记录仍可能可见。

### 如果所有设备都丢失了怎么办？

请在仍可访问账户时提前准备：添加另一台已授权设备，并检查可选的链上恢复设置。新账户默认关闭链上恢复，必须事先在仍可解锁的已授权管理员设备上启用。若已提前启用恢复，且仍可使用已绑定的 Internet Identity 登录，就可以发起恢复，并须经过所设的等待期。等待期内，任何仍获授权的设备都可以对该请求提出争议。若所有授权设备都丢失、又未预先启用恢复，仅凭登录身份无法恢复加密内容。恢复也不能找回从未上传或备份的本机文件。

### 可以收回文件或撤销 Agent 权限吗？

撤销分享链接或 Agent 权限，会按对应规则停止新的访问或操作授权。已经获得授权的下载或操作仍可能完成。撤销无法删除对方已经下载的文件、保存的截图，或外部 Agent 已经收到的内容。

### dMsg 使用区块链做什么？

dMsg 使用 Internet Computer（ICP）记录账户与设备授权，并提供其他可验证的协议能力。修改这些链上代码需要 ICPanda DAO 通过公开提案，仅凭云服务无法做到。私密文件内容和私密消息正文不会作为公开区块链内容发布。

### 如何选择和支付套餐？

在 dMsg 中选择套餐，确认前查看当前价格、可用支付方式与续期条款。当前桌面会员流程支持 ckUSDC 或 ckUSDT，也可以通过符合条件的 PANDA 神经元取得年度会员。不同平台和版本的购买路径可能不同。

### dMsg 由谁开发？谁能修改它的规则？

dMsg 是 ICPanda DAO 的产品。DAO 同时治理 dMsg 的链上规则：升级这些规则需要公开提案和投票，任何人都可以在 ICP 仪表板上查看。应用程序和云服务的发布与运营另行负责。

### 支持哪些设备？

dMsg 提供 iPhone 与 iPad App、Apple Silicon Mac App、Windows x64 App，以及包括 Linux 在内的命令行版本。Android 仍在开发中。

## 公开文档与版本依据

本指南和链接的文档快照随公开仓库构建生成，描述产品和公开实现，不代表逐项核验过生产部署。构建日期不等于每份文档的审阅日期，参考文档保留各自的核对日期与限制。可在[构建清单](https://dmsg.net/_/knowledge.json)查看源码提交和内容哈希，在[发布页](https://github.com/ldclabs/ic-panda/releases/latest)查看已发布安装包。

- [技术架构与信任边界](https://dmsg.net/_/docs/architecture.md)
- [公开协议索引与签名格式](https://dmsg.net/_/docs/protocols.md)
- [加密存储与分享](https://dmsg.net/_/docs/cloud.md)
- [多签钱包合同](https://dmsg.net/_/docs/wallet.md)
- [隐私政策](https://dmsg.net/_/privacy)、[服务条款](https://dmsg.net/_/terms/v1.txt)、[支持](https://dmsg.net/_/support)

交互首页中的演示使用样例数据。旧网页版用户应在桌面应用的“设置 → 旧版迁移”开始迁移，完成前保留旧浏览器数据。[旧版读取页](https://dmsg.net/legacy)是兼容入口，不是新版工作台。
