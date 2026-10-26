# dMsg 凭据、文件引用与开发工作流教程

日期：2026-10-26。本文介绍当前桌面客户端和 CLI 的使用方法，以及可以自行复现的检查。构建成功不代表云端已部署或原生文件系统扩展已安装；精确权限、限制和失败状态见 [Secrets 合同](protocol/secrets_zh.md)。

建议先用合成数据完成本机交付，再测试链接、频道和外部服务。文中的 `<…>` 是需要替换的 ID，不要原样执行。

## 1. 准备环境

打开你刚构建的桌面应用，登录并解锁测试账户。CLI 与桌面应使用同一个 dMsg home、环境和账户；下列 `dmsg` 必要时替换为新二进制的绝对路径。

```sh
command -v dmsg
dmsg --version
dmsg status --json
dmsg doctor
```

同版本号重新构建后，后台 daemon 可能仍是旧进程。没有在途操作时，用新二进制执行 `dmsg restart`，再重新解锁。不要在真实请求结果不明时通过反复重启、重新提交来尝试恢复。

桌面主要入口为 **签名与授权 → 凭据使用 → 开发工作流**：策略、机密文件、SSH 与 Git、健康与发现、秘密快照、导入快照和本设备活动。文件与字段的引用直接从 **文件** 和 **保险库** 详情复制，无需新建策略。

## 2. 在桌面查看 ID、复制引用

### 2.1 文件与固定版本

1. 在文件库选择一个文件。
2. 详情中的 **文件 ID** 和 **当前版本 ID** 显示完整 ID；点击带复制图标的 ID 即可复制。
3. 点击 **复制文件版本引用**，取得当前版本的 `dmsg-file://` 引用。
4. 要引用历史版本，找到详情下方的 **版本** 列表。每一行都有完整 **版本 ID**、复制 ID 和 **复制文件版本引用** 按钮。

```text
dmsg-file://<file_id>/<version_id>
```

注意：

- 这是当前已解锁账户内的 Files 引用，不是网站链接，不含解密密钥，也不授予读取权限。
- URI 不含账户 ID；daemon 在当前账户工作区内查找。执行审批仍绑定账户、设备和解锁会话，不会自动切换或搜索其他账户。
- 文件 ID 标识文件，版本 ID 标识固定内容版本，`SHA-256` 是内容摘要，三者不能互换。
- 复制时固定版本；以后文件更新，旧 URI 不会自动变成新版本。预览历史版本也不会改变上方明确标为“当前版本 ID”的引用。
- 云端版本也可复制引用，但实际用于进程交付或挂载前，需要将该版本恢复到本机。复制引用成功不代表内容已下载或获批。
- 回收站中可查看和复制 ID；应先恢复文件，再使用其引用。

### 2.2 保险库条目与字段

1. 在保险库选择条目。
2. 详情顶部的 **保险库条目 ID** 显示完整 ID，点击即可复制。底部的 `revision` 是条目版本，不能当作条目 ID。
3. 在需要的字段下点击 **复制字段引用**。daemon 校验当前条目后返回规范 URI，界面将它复制到剪贴板。

```text
dmsg-secret://<account_id>/<item_id>/<field_id>
```

复制字段引用不显示或复制字段值，不触发“显示秘密”的操作，也不创建授权或策略。账户 ID 自动填入，无需手工拼接。

进程交付允许普通非 OTP 字段，包括隐藏字段与文本字段；GitHub 凭据策略另要求隐藏字段。OTP 种子、钱包条目和带 `dmsg-` 标签的托管条目不提供这种引用入口。回收站条目先恢复，未解决冲突先处理；冲突时按钮禁用。源状态在点击时变化，daemon 仍会拒绝无效请求。离开条目、锁定或源版本更新后到达的旧回复不会覆盖剪贴板。

字段引用没有固定 revision，申请使用时才固定当前条目版本。申请后条目有修改，旧计划失效，不能退回历史版本取值。

### 2.3 CLI 等价操作

桌面使用无需这些命令；脚本或排查时可使用：

```sh
dmsg files search "service.json" --json
dmsg files show <file_id> --json
dmsg files versions <file_id> --json
dmsg vault list --json
dmsg vault show <item_id> --json
dmsg secrets ref <item_id> <field_id>
```

`files show` 的 `file.id` 是文件 ID，`file.current` 是当前版本 ID；`files versions` 每项的 `id` 是版本 ID。`vault list` 每项的 `id` 是条目 ID，`vault show` 的 `item.fields[].id` 是字段 ID。不要加 `--reveal` 来获取引用。

安装了 `jq` 时，可直接生成当前文件版本引用：

```sh
dmsg files show <file_id> --json |
  jq -r '.file | "dmsg-file://\(.id)/\(.current)"'
```

## 3. 不启用 FSKit，先验证进程交付

### 3.1 环境变量

在保险库新建一个 API 凭据条目，给隐藏凭据字段填入测试值 `dmsg-test-only-v1`。点击字段旁的 **复制字段引用**。

创建临时项目，将复制的 URI 替换到下面的变量中：

```sh
DMSG_TEST_DIR=$(mktemp -d /tmp/dmsg-secrets-manual.XXXXXX)
cd "$DMSG_TEST_DIR"
DMSG_TEST_REF='粘贴复制的 dmsg-secret:// 引用'
printf 'TEST_TOKEN=%s\n' "$DMSG_TEST_REF" > .env.refs

dmsg run --env-file .env.refs -- python3 -c \
  'import os; assert os.environ["TEST_TOKEN"] == "dmsg-test-only-v1"; print("PASS: env")'
```

CLI 等待审批时，子进程尚未启动。核对审批中的程序、参数、工作目录和凭据来源，再批准并完成设备要求的在场确认。原命令继续，输出 `PASS: env`；活动记录应有交付结果。

`.env.refs` 只保存 `NAME=dmsg-secret://…`、空行和整行注释，不解释 shell。不要把明文密码或 `$(command)` 写进去。当前上限为 64 KiB、64 个环境变量。

### 3.2 文件管道

使用同一个测试字段：

```sh
dmsg run --file "TEST_FILE=$DMSG_TEST_REF" -- python3 -c \
  'import os; b=open(os.environ["TEST_FILE"],"rb").read(); assert b == b"dmsg-test-only-v1"; print("PASS: file")'
```

也可以从文件详情复制版本引用，将 `TEST_FILE` 的来源换为 `dmsg-file://…`，测试 JSON、PEM 或二进制文件。

`TEST_FILE` 是继承的 `/dev/fd/N` 顺序管道路径，不是普通磁盘文件。读完即 EOF，不能依靠重新打开来重复获取内容；需要 seek、反复打开或普通路径读取的程序，应使用后面的机密文件挂载。单次最多交付 8 个文件，每个不超过 8 MiB。

子进程和后代可以保留已交付内容；dMsg 不控制程序的日志、网络请求或环境变量转储。此处示例只输出合成数据的断言结果。

### 3.3 验证失败路径

每次重新运行命令，分别检查：

| 操作 | 应有结果 |
| --- | --- |
| 拒绝审批 | 子进程不启动 |
| 等待审批时修改 `.env.refs` | 原计划不能启动程序 |
| 等待审批时更改凭据值 | 原请求失效，需重新申请 |
| 等待审批时锁定 dMsg | 解锁后旧请求不能继续交付 |
| 获批程序返回非零退出码或按 Ctrl-C | CLI 正确传递退出/中断结果 |

## 4. 机密文件挂载：普通路径读取时审批

### 4.1 macOS 前置条件

普通本地构建不一定包含可用扩展。正式模块需要有效 FSKit 描述文件，daemon 和扩展需要匹配的团队签名和 App Group 配置，并由用户在系统设置中启用。详见 [原生 FSKit 构建与签名](../src/dmsg_desktop/native/fskit/README.md)。本教程不自动登记 Apple Developer App ID、配置签名或启用扩展。

系统设置中可能出现两项：

| 名称 | 用途 |
| --- | --- |
| dMsg secret files | 正式机密文件模块，连接 daemon，提供获批内容 |
| dMsg synthetic filesystem probe | 独立合成测试探针，不连接保险库或账户；正常使用不需要启用 |

如果保持正式扩展禁用，先验证启动失败提示，将实际读取测试记为未运行。缺失、禁用、签名不符和 daemon 未签名应给出不同提示，不会回退成明文文件。

Linux 使用原生 FUSE 后端，不需要 macFUSE；其句柄授权语义与 macOS 不同。Windows 原生挂载尚不在范围内。

### 4.2 配置映射

在 **凭据使用 → 机密文件** 中：

1. 填写项目名称。
2. 添加 `.env` 映射：虚拟路径填 `.env`，来源选“仅含引用的环境”，按名称搜索保险库条目和字段，填写变量名并添加引用；也可粘贴已有 `.env.refs`。
3. 添加普通机密文件：例如 `service.json`，来源选“文件版本”，按名称搜索文件并选择固定版本。JSON、PEM、二进制内容不改写。
4. 如需从项目根目录读取，设置相应项目别名，例如 `.env`、`service.json`。
5. 点击“选择项目并预览”，选择空的测试项目目录，检查映射，再提交审批。
6. 批准保存配置后点击“启动”。配置批准不等于读取批准。

别名指向挂载中的文件，不覆盖已有 `.env` 或其他文件。将挂载目录与别名加入项目 Git、索引和备份排除规则；dMsg 不擅自修改这些配置。dotenv 生成拒绝引号、反斜线、美元符、换行和 NUL，不能转换的内容可作为原始 Files 文件交付。

### 4.3 验证读取

先将 macOS 文件窗口设为 5 秒，再在项目目录执行：

```sh
python3 -c 'from pathlib import Path; b=Path(".env").read_bytes(); assert b"dmsg-test-only-v1" in b; print("PASS: mounted env")'
```

读取先等待，批准后同一个调用继续。未决 IO 最长等待约 30 秒；无人批准、拒绝或取消应返回错误，不返回空文件成功。

macOS 授权覆盖指定文件、版本、UID 和固定短窗口（最长 60 秒）。窗口内同一用户的其他程序也可能读取同一文件，不能把它理解为只授权 Python 或 IDE。Linux 绑定实际打开句柄，新句柄需按其范围审批。

继续检查：

| 场景 | 预期 |
| --- | --- |
| macOS 窗口内第二进程读取同一文件 | 可读取，无需重复批准 |
| 读取另一文件 | 需要另一文件的审批 |
| 窗口结束后读取，包括旧句柄 | 需要重新批准；拒绝后失败，持续读取不会续期 |
| 点击撤销读取 | 后续读取不能沿用旧授权；撤销失败必须保留错误 |
| 更改来源条目 | 旧来源立即拒读，随后停卷；重新启动才使用新来源 |
| 锁定、退出或切换账户 | 授权失效并尝试卸载；解锁后不自动恢复挂载 |
| 写入或覆盖已有项目文件 | 拒绝，不生成明文替代文件 |

mmap 不在支持合同内；已经交付给程序的字节无法收回。IDE 热重载、休眠、跨 UID 描述符转交和发行包升级/卸载需要独立实测，不能由一次读成功推断。

## 5. SSH 与 Git：私钥留在 dMsg

先生成专用测试密钥：

```sh
dmsg vault add --type ssh --generate --title 'Secrets 验收 SSH'
```

在 **凭据使用 → SSH 与 Git** 找到该密钥，启用 `git_sign` 并批准绑定。页面提供公钥、指纹和实际 socket。绑定只允许申请，每次签名仍需审批。页面可复制公钥、socket 及终端/Git 配置；“检查连接状态”只检查本机 daemon 与绑定，不签名或连接远端主机。

创建新的临时 Git 仓库，把对应公钥保存为 `key.pub`，不导出私钥。在该临时目录运行：

```sh
git init -q
printf 'dmsg-test@example.invalid %s\n' "$(cat key.pub)" > allowed_signers

SSH_AUTH_SOCK="$(dmsg secrets ssh socket)" \
git -c user.name='dMsg Test' \
    -c user.email=dmsg-test@example.invalid \
    -c gpg.format=ssh \
    -c user.signingkey=key.pub \
    commit --allow-empty -S -m 'dMsg signing test'

git -c gpg.ssh.allowedSignersFile=allowed_signers verify-commit HEAD
```

提交应等待审批，批准后成功，验签通过。拒绝签名或禁用绑定后，新的签名应失败。Git 审批核对协议摘要，不代表已经核对完整提交原文。

SSH 登录需另启用 `ssh_auth`，将测试公钥安装到自己的测试服务器，使用该 socket 连接。只支持 Ed25519 与有效、非转发的 session-bind；不支持加密私钥、Agent forwarding 或通用任意消息签名。签名交付不等于登录成功。

## 6. Agent 受控使用：GitHub issue

这一步会实际创建 issue。使用测试仓库以及仅授权该仓库所需 Issues 写权限的凭据。

在 **凭据使用 → 策略** 新建策略，选择隐藏凭据字段、GitHub owner/repo 和测试 Agent，核对配置审批。此批准只授予 `secret.use:<policy_id>` 的申请权限。

通过已配置的 MCP 客户端调用 `dmsg_secret_use_request`：

```json
{
  "policy_id": "<策略ID>",
  "operation": "issue.create",
  "arguments": {
    "title": "dMsg acceptance test",
    "body": "Synthetic acceptance test"
  },
  "idempotency_key": "secrets-manual-001",
  "reason": "验证一次审批对应一次创建"
}
```

先拒绝一次，确认没有创建；再用新幂等键申请并批准。审批应完整展示目标仓库、标题和正文。成功结果只有 issue 编号和 URL，MCP 不获得 token。

同键同输入返回原请求，同键不同输入冲突。禁用策略或撤销授权后，新申请失败。普通 `dmsg_vault_read` 仍遮罩隐藏字段；`vault.read/write` 不包含受控使用权限。

`lost` 表示可能已创建而结果不明。先人工核查 GitHub，标记已核查不会改变外部执行事实，也不会重发。不要把新幂等键当作自动恢复手段。

## 7. 链接与频道秘密快照

### 7.1 链接发送、领取和导入

准备一个有两个测试字段、备注与小附件的条目。在 **秘密快照** 中选择条目，只勾选一个字段和一个附件，预览、核对并批准。

- 所有内容默认未选；未选字段和备注不能进入快照。
- 发布成功后才能复制链接；链接本身含访问材料，不放进截图、日志或 issue。
- 默认一天有效、一个下载会话。如果同一链接既要测浏览器领取又要测客户端导入，提前设为至少两个会话，或使用不同测试链接。
- 浏览器完整验证后逐字段显示、逐附件保存，不提供整包明文下载。
- 在 **导入快照** 粘贴链接，明确选择字段、备注、附件和目标 Files 文件夹，再批准导入。

导入生成新的 Vault/Files ID，不覆盖发送者条目。相同接收记录与选择重复导入返回原收据；不能把另一组选择冒充原导入。来源更新不会更新已交付副本。

附件继承目标 Files 文件夹已有的 Agent 授权。“仅本地”仅表示未备份，不表示已授权 Agent 无法读取。

撤销后用新的领取会话验证拒绝。已签发下载 URL、已开始传输和已经保存的副本不能即时收回。会话次数不是阅读次数，同一会话的块重试不应重复扣次数。

### 7.2 频道交付

在 **秘密快照** 选择“发送到频道”和测试频道，明确选择内容并批准。接收者在频道消息中点击 **导入秘密快照**，完整校验后选择导入；建议用第二个测试账户检验。

频道快照没有链接有效期、下载会话次数或收回副本的语义。能够读取对应纪元的成员，包括后续获得历史权限的人，可以保存副本。它不是共享保险库，也不自动同步来源更新。

准备后、执行前变更来源或频道成员/权限，旧计划应拒绝。发布响应不明时，用原任务“对账”，只查询原 operation ID；不能自动发送第二条消息。

## 8. 健康、扫描、引用索引和轮换

在 **健康与发现** 使用页内入口切换到检查结果、本机发现或轮换。结果按级别排序，可按名称筛选、显示已忽略项，并打开对应来源处理；完整 ID、版本及内部规则位于技术详情。忽略只针对当前版本，条目更新后需重新评估；读取失败、未完成与云端未知状态不能显示为安全。

选择专用测试目录扫描，在其中放置合成 `.env`：

```dotenv
TEST_API_TOKEN=synthetic-discovery-only
```

报告应只显示路径、变量名等元数据。明确选中候选并批准后导入，不修改或删除源文件。预览后改文件应拒绝旧导入；目录外符号链接不跟随；达到上限显示未完成，扫描期间锁定停止处理。

选择一个凭据字段，点击 **凭据引用**，查看本设备已保存的策略及 grants、挂载、SSH 绑定、选中该字段的快照和保留的进程记录。仅写了但从未观察过的 `.env.refs`、其他设备和已清除历史不在清单内；没有依赖不等于上游无人使用。

轮换界面根据当前凭据和已核查依赖开放下一步。只修改条目标题不能确认已更换凭据；依赖变化后需重新核查。轮换流程：

1. 在旧值仍存在时创建轮换任务。
2. 在上游创建新凭据并更新保险库中的值；合成测试可用 `dmsg-test-only-v2`。
3. 报告“已创建”，实际验证新值后报告“已测试”。
4. 点击 **查看依赖清单**，核对后报告“引用已检查”。确认绑定刚查看的清单摘要。
5. 处理完上游旧凭据后报告“上游已撤销”。dMsg 会再次检查依赖清单是否变化。

反例：步骤 4 后禁用一项相关策略，步骤 5 应拒绝旧确认，必须刷新并重新检查。若再次更改凭据值，应重新报告创建、测试和引用检查。

上游创建、测试和撤销是用户报告，dMsg 不声称独立验证上游状态，也不会自动轮换上游凭据。

## 9. 1Password 导入

使用仅含测试数据的 `.1pux`，从保险库导入入口预览。检查隐藏值不外露，Passkey、历史或不支持字段逐项报告遗漏；只导入明确选择的附件，不宣传完整迁移。

相同源文件、选择和目标重复提交应返回原收据；预览后替换源文件应失败。导入不应出现可见的半份条目/附件，源导出文件不会自动删除。附件仍继承目标 Files 文件夹权限。

## 10. 自动化回归与记录

以下命令均从仓库根目录开始。硬件测试需活动的 macOS 登录会话，屏幕锁定可能使 Secure Enclave 返回 `OSStatus -25308`；不能把这类失败算作通过。原生 FSKit/FUSE 等 ignored 测试需各自环境，不能一概用 `--ignored` 强行执行。

```sh
cargo test -p dmsg_client -p dmsg_cli -p dmsg_agent
cargo clippy -p dmsg_client -p dmsg_cli -p dmsg_agent --all-targets -- -D warnings
pnpm --dir src/dmsg_desktop check
pnpm --dir src/dmsg_desktop test
pnpm --dir src/dmsg_desktop build
```

进入桌面目录，运行真实 Electron 与本地合成 daemon 的测试：

```sh
cd src/dmsg_desktop
node scripts/references-smoke.mjs
node scripts/secrets-smoke.mjs
```

凭据工作流使用统一选择器、复选框和字段组件。`secrets-smoke.mjs` 以隔离账户验证逐次审批、来源搜索、历史游标保持、挂载/快照旧预览失效、局部读取失败与重试、导入输入恢复、离页迟到响应以及中文深色窄窗口；截图位于 `test-results/secrets/`。这些测试不访问真实凭据，不创建真实 GitHub issue，不验证操作系统在场确认或真实 FSKit/FUSE。

引用专项验证完整 ID、当前与历史版本引用、无值字段引用、受限条目、冲突、失败/迟到响应以及窄窗口与 RTL 布局。剪贴板写入被测试拦截，不读写用户系统剪贴板；截图保存在 `test-results/references/`。这不是对真实云端、系统在场确认或原生扩展的替代。

回到仓库根目录，单独检查不确定结果不重放：

```sh
cargo test -p dmsg_client --lib uncertain_post_is_lost_and_is_never_replayed
cargo test -p dmsg_client --lib lost_publication_response_queries_original_id_without_create_replay
cargo test -p dmsg_client --lib storage_faults_before_dispatch_and_after_success_never_resend
```

网站分享页另有检查；本地桌面新版本不意味着线上网站已更新：

```sh
pnpm --dir packages/dmsg-sdk build
pnpm --dir src/dmsg_frontend check
pnpm --dir src/dmsg_frontend exec vitest run src/lib/share src/lib/i18n
pnpm --dir src/dmsg_frontend build
```

每项验收记录构建提交、OS/运行时、测试步骤、预期、实际结果与请求 ID。不要记录真实 token、私钥、完整分享链接或包含秘密的终端输出。单独列出未运行项：真实云端计费/限额/撤销、真实 GitHub、系统在场确认、跨 UID、休眠、发行签名公证与升级卸载等；本地通过不能推断这些已通过。
