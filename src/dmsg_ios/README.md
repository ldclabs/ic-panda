# dMsg for iOS

Native SwiftUI client with an in-process `dmsg_mobile` / UniFFI bridge to
`dmsg_client`. There is no daemon, loopback server or embedded web client.
The local implementation brief is `misc/dMsg_iOS_technical_architecture_zh.md`
v0.4 and the compact layouts in `misc/dmsg-ios-uiux` (these are not public
build dependencies).

## First sign-in

An authenticated login without an account opens a focused Get started screen.
It explains that the action creates the encrypted workspace, keeps Join an
existing account as a secondary path, and moves other sign-in choices behind
Switch account. A renewable passkey login does not by itself enable biometric
unlock: the account must exist and its initial setup must no longer be pending.
Interrupted creation resumes the saved operation, including when the account
already exists but its first content root has not been confirmed. Progress and
slow-network guidance remain visible while the action is running.

## Messaging

A channel hint received during an in-flight refresh schedules another read as
soon as that refresh finishes. Completed reads only coalesce requests that
predate their start, so a stale response cannot consume a newer refresh request.
Failed sends retain their original ID and show specific network, key and
permission failures; native diagnostics record fixed error codes without message
text, account IDs or URLs. Receiving the latest message is not a send prerequisite.

Messages groups conversations and letters. Letters have Inbox, Sent, Pending,
Drafts and Archived folders, local search and bounded list presentation. The iPad
split view initially shows the messages sidebar and routes selected letters to
their details. Drafts
and outgoing text/status remain encrypted in the account's local workspace;
these sender records are device-local, not replicated history. They also remain
on the source device during the current record-transfer flow. Older sender
journals without text or a timestamp remain visible without invented values.
Draft input is saved while editing and flushed on an explicit lock. Queueing
validates the same 6,000 UTF-8-byte limit and invitation format as submission,
then atomically moves the draft to the original send intent. Retries keep its ID
and bytes; only an intent with no prepared contact request can return to editing.
Service acceptance is distinct from delivery or reading.

On iOS, archiving only changes organization. Allow another letter is a separate
confirmed action and does not add a contact or channel member. Existing desktop
and CLI mark behavior is preserved. Letter details can reply to the sender or
create an account-bound invitation to an existing/new conversation. Incoming
channel links are retained through login and reviewed before the core verifies
and accepts them; links never authorize joining by themselves.

Conversation rows show recent activity, unread, mute, draft and pending-send
states. The verified ledger controls publishing and invitation affordances.
Search includes attachment names and pages beyond 100 results. Timelines show
dates, unread boundaries and reply previews with navigation to the original.
Attachments remain in the composer until Send, including the reply reference.
System files and photos can be imported there; received attachments show progress
and open the verified local library copy. Settings preserve edits during refresh,
and reminder routes display available local messages before awaiting the network.

## Vault browsing and details

The vault uses a single list with All, Favorites, Passwords, Notes and a complete
type picker. Favorites have no four-item limit. Search, exact tags, type and
favorite filters are applied before sorting and pagination; counts describe the
whole query. Sorting retains only non-hidden summaries while releasing the client
between decryptions. The old duplicate Favorites grid and its separate read are
removed. Empty, loading and failed reads have distinct actions.
List previews follow the desktop: login, API, server and database entries may show
a username or HTTP(S) host, while other types show their type name. Arbitrary field
values, note bodies and URL credentials, paths or parameters do not enter previews.
Search stays visible above the category bar. Switching categories retains their
query counts and only scrolls the bar when the selected category is offscreen.
Vault, Files and Messages share one capsule tab and keep 8 pt between their bar
and the list content, with search always visible above it. The tab bar names the
workspace, so a workspace root shows no title; screens pushed from it use compact
titles. Inside a folder, Files shows the folder's path above its list.

Recovery and synchronization feedback use overlays and a persistent toolbar
entry, so background status changes do not move list rows. That entry retains
progress, pending counts, failures, retry deadlines and the last successful sync.
Critical notices stay available after dismissal; copy/save feedback also appears
inside sheets. Sync failures remain in the toolbar's status details, without an
automatic "Sync paused" overlay or VoiceOver announcement. Messages likewise
keeps the sync panel out of its conversation list. Displayed OTP expiry is
independent of other field operations,
and an OTP clipboard copy expires no later than its displayed validity. Login rows
copy their username or password by swipe or context menu.

Details keep the title, type, tags and sync state above compact, ordered field
sections, with creation/modification dates in Item information at the bottom.
Empty fields and empty sections are hidden only while reading; editors retain them.
Tap a field value to copy it in one step. The field menu (also available by long
press) provides reveal/hide, large type, opening HTTP(S) websites and field references.
Choosing reveal or large type is the explicit mobile action: there is no additional
confirmation or thirty-second confirmation grace period. Large type supports long,
multiline values, Dynamic Type and a direct copy button. It clears on dismissal,
backgrounding, locking, account changes or a changed item revision. Current one-time
codes refresh in both views; historical codes disappear at expiry. Copies stay on
this device and expire within thirty seconds, or sooner when the code expires.
Hidden values are never loaded by list browsing. Managed keys remain read-only, tombstones have an explicit trash
state, and current details refresh on content/version changes without replacing an
open editor's draft. Historical versions show their timestamp and can be compared
with the current version. Conflict resolution supports individual metadata and
field choices and checks the expected current revision before saving.

Editors keep dirty drafts within the unlocked account session, confirm discard
from Cancel or a swipe, preserve edits when changing type, and disable mutation
while saving. Failures retain the draft with a visible retryable error. Fields can
be reordered in a compact list. Each field puts its editable name before its value,
with optional grouping below; adding a field focuses its name, and Next moves to
its value. Validation appears after leaving an input or attempting to save. Date,
month, email, website and OTP inputs have suitable input and validation.
Password generation is local, and QR scanning only fills an OTP draft
for review. AutoFill exports only an explicit selection: choosing it asks for
system authentication, and while dMsg is open and unlocked the snapshot then
follows those items without asking again. System authentication opens it, and
left alone it expires after 24 hours. Import summaries expose selection,
duplicates and incomplete records, then link to the imported items; trash and
history lists distinguish an unavailable read from an empty result.

## Account and settings

Hardware quick unlock remains available after the II login expires: Face ID,
Touch ID or the device passcode opens already cached local passwords. The account
page offers a non-blocking login action when II authorization is absent or expired;
copying a local secret does not start a login. Device revocation checks, automatic
locking and clipboard expiry remain independent controls.

Membership execution usage restores the saved Passkey authorization through the
shared client before reading its private ledger. An account switch clears the
in-memory identity but preserves its hardware-backed grant; a parent delegation
with more than ten minutes remaining is reused locally, otherwise renewed on
demand. Local content unlock remains independent of this online operation.

Account setup shows a spinner and disables competing access actions while it
runs. After 15 seconds, the screen explains that it is still waiting for network
confirmation. An uncertain result refreshes the native session handle and makes
one bounded attempt to reconcile the saved account/root operation. Continued
failure restores the controls and explains how to resume after checking the
connection; completed steps are retained.

Account groups identity and security, synchronization and storage, and app settings.
Advanced contains agents, wallets, browser connections, hardware approval and local
activity. Pending approvals and unsynced changes have direct entries. Device names
and public account thumbnails are kept in this device's protected preferences.
The account switcher reads public names for every local account through the same
certified-name cache used by desktop, then refreshes them without switching or
unlocking the other workspaces. Names appear above the full account ID; an unnamed
account keeps its ID as the label. Failed refreshes preserve known names.

Settings navigation rows and field content use leading alignment. Standalone
actions center their labels, with native prominent buttons for primary actions
such as publishing or saving; destructive actions retain their red text and
confirmation steps. The Account header opens the public-profile editor, where
the account ID is displayed with a direct copy button.
Login identities also have direct copy buttons, including on devices without
account-management permission.

Password AutoFill and Files app copies are separate pages under Account → System
integrations. Security keeps the frequent settings on its first page, with login
identities and account recovery on their own pages. Account recovery shows recovery
status and actions, and opens the on-chain recovery policy; it does not link back
into account security.
Pending recovery details and cancellation live only on Account recovery, including
when opened from a security reminder. The waiting-period end and request expiry
show the local date and time. Cancelling creates an approval request; any authorized
device may review it, while changing the policy still requires an administrator.
As on the desktop, cancelling a recovery, the recovery policy, the hardware policy,
registering or retiring an approval key, revoking a device and approving a pairing
request each keep the request they created: until it is done, rejected, cancelled,
expired, failed or decided elsewhere, that change stays unavailable and the page
offers View approval instead of creating another request. An unreadable request
stays kept, and locking or switching accounts clears these receipts.

The public offline tour has been removed. Blocking and report history live under
Messages → Message settings. Privacy remains available offline before sign-in;
contact information is accessible there and under Account → About dMsg. Removing
local account data is separate from deleting the service account and first shows
unsynced work and file versions without a backup.

Profile links and avatar changes remain drafts until publication. Profile edits
keep their expected version across navigation; a conflict requires reloading and
reviewing the preserved draft. Settings drafts stay only in the unlocked session
and are cleared on lock or account transitions. Usage reads cloud and execution
counts independently; unavailable values are never displayed as zero.

AutoFill selection survives searches and reopening its picker. Export status shows
counts and expiry without opening secrets or bypassing system authentication.
Wallet transfer forms use existing asset metadata and fee preview, exact decimal
conversion and the original proposal journal for uncertain outcomes. A preview
creates no approval, transaction or automatic execution.

## Multisig wallets

Manage personal or shared assets across chains. Set approval thresholds and
device co-signing requirements. The account entry, wallet list and device
handover use the same name and all 23 locales as desktop and the website.
Member approvals and device co-signing remain separate; one person can use
multiple cosigner keys without counting as multiple members.

## Device authorization

To add this iPhone, sign in to the existing account and choose **Scan desktop
invitation**. The authorized desktop shows its QR code under Settings → Devices
→ Authorize another device. The encrypted request returns through the configured
relay, so the devices may use different networks.
Scanning delivers a device request; compare the full device ID and approve in
the desktop approval window. The phone notices the decision by itself. The existing
phone-displayed request QR and copied tokens remain available and renew when they
expire: an authorized iPhone or desktop can scan the new device’s request directly
without an invitation mailbox, and goes straight to reviewing it. This is separate
from binding another login identity. See the
[public invitation contract](../../docs/protocol/device_pairing.md).

## On-chain recovery

Account security → Account recovery → On-chain recovery offers one recovery toggle and an optional 1–7 day waiting
period. Changes go through the existing approval view; the shared core rotates
the root automatically. New accounts default off and older accounts retain their
policy. The recovery screen disables request/completion while the policy is off.
Cloud backups remain available, but losing all authorized devices while recovery
is off makes their content unrecoverable even with the login.

## Account synchronization

After device approval and root unlock, a new iPhone verifies a complete account
snapshot before publishing its contents. Its inventory and ciphertext progress
survive interruption; later passes receive only changes. Ordinary navigation and
editing do not cancel the session's automatic pass. A manual sync waits for an
existing pass, retaining explicit upload retries and server backpressure.

Automatic sync is enabled by default while the app is foreground and unlocked.
Failures use the core's retry deadline without adding a separate one-minute wait.
The vault, channel list and account screen show initial synchronization, progress,
failure, next retry and actual last success separately from an empty account.
Backgrounding, locking and account changes cancel old work; returning resumes it.

**Sync now** checks account content and the cloud backup inventory independently.
Conversation bodies and inbox letters refresh in their own views; files are
restored explicitly. Inbox key recovery uses the same content lane and cooldown,
receives incrementally after initialization, and never uploads local drafts.
Content-sync console records contain only fixed trigger/stage/error categories,
counts, byte totals and timings, including cancellation and queue time.

## Notifications

The inbox shows verified letters saved on this device while refreshing. If the
receiving key has not reached a newly authorized device, it pulls encrypted
account content and retries the uncommitted letter page without uploading local
work. A key that is still missing needs synchronization on the device that
enabled receiving. Loading, an empty inbox and a failed refresh have distinct
states; read, archive and restore actions update the detail after confirmation.

Conversation views read local history while the core refreshes the channel.
Verified pages appear before the remaining history and attachment metadata
finish downloading. Loading, an empty conversation and a failed refresh have
distinct states; retry keeps the available messages visible.

App Store purchases first check verified subscription-group status and each
current transaction's account token. Historical original tokens can differ and
do not establish the service's durable binding. A matching transaction takes
priority over other records in the same group; active records take priority over
expired history. An active foreign subscription offers account switching, and missing
ownership information blocks purchasing. A fully expired group permits a new
purchase with the selected account's token; billing retry, grace and revocation
do not qualify as full expiry. The check runs again immediately before
Apple's purchase sheet; late purchase and restore results cannot update a
replacement session. Restore checks each transaction's token independently, even
when product/status lookup is unavailable. It reports another-account,
unavailable, applied or empty results and does not transfer a subscription.
Only authoritative service application permits finishing a matching transaction.
Startup reconciles both current entitlements and unfinished transactions, deduplicated
by transaction ID. An already-finished StoreKit transaction may still need a fresh
server verification after a missed renewal or an account-token repair.
The service keeps each transaction's beneficiary immutable and permits a new
account on the same original subscription only for a new, non-overlapping term.
Deploy that commerce policy before distributing the iOS repurchase change;
older servers reject a new account when Apple reuses the original subscription ID.

The product catalogue and verified subscription-group status determine purchase
readiness. An empty group permits a first purchase; a failed status read remains
unknown, keeps offers visible and provides Retry. An unavailable or slow
`AppTransaction.shared` read does not block first purchase or show an account
verification error: it describes the app download and provides only an optional
environment hint. Each subscription transaction is verified independently.
Without a hint, preflight applies the stricter Production rules; it never grants
Sandbox exceptions to an unknown context. The transaction's verified environment
and the authoritative service still decide delivery. Storage packs remain hidden
until the context is known to be Production.

Automatic page loads never request Apple authentication. Explicit Retry of a
failed subscription-status read, or Restore purchases, uses `AppStore.sync()`;
the app no longer refreshes app-download credentials as a prerequisite to buying.
Late reads and synchronization results from a previous dMsg account are discarded.

The plan page keeps the current entitlement separate from the product scheduled
for the next renewal. Returning from Apple subscription management or foregrounding
the page refreshes that schedule. While a purchase or restore is in progress, the
inactive return from Apple’s sheet does not start a second reload; the flow reloads
after delivery. These local checks do not change the commerce
service's per-transaction ownership rule, grant extra entitlements, or
cancel an already scheduled change in Apple’s system.

The named UniFFI methods in `dmsg_mobile::notifications` share the desktop's
settings, durable reminder records, version checks, budgets and snooze policy.
The activity menu in each workspace's toolbar opens pending/recent reminders and notification settings. The
account switcher shows each local account’s actionable plus unread count, with a
bell and a visual cap of 99+; zero counts stay hidden. Counts follow the shared
notification status, independently of the system badge preference. The
old per-account push toggle is imported once; core preferences then own it.
The reminder list offers an explicit read action per row and one action for all
local accounts, including unloaded pages. Opening a row also marks that reminder
version read. These markers persist locally and clear reminder counters and
local presentations, while unfinished work stays in Needs action and business
message read receipts remain unchanged. Newer versions become unread again.
Network service reminders open Account synchronization; receiver-registration
issues open Notifications with a retry action even if the Apple lease is cached.
Fixed notification copy and APNs localization keys follow all 23 desktop catalogs;
`node scripts/ios/sync-notifications.mjs --check` checks their parity.

APNs owns background presentation of shared account events. Foreground callbacks
wake reconciliation; they do not create a duplicate local notification. Native
local notifications are reserved for device-local work. Real, loaded business
views report their visible version; opening the reminder list never marks a
message read or completes an approval. Channel reads advance only when the
latest loaded message is actually visible. Network reconciliation runs separately
from local presentation so a slow source does not delay an existing reminder.

Push registration reacts only to a new APNs token; receiving the same token does
not enqueue another registration. A valid Apple registration is retained when
the following notification registration or settings sync fails. Explicit retry
rechecks that notification registration without repeating App Attest unnecessarily.
The settings page shows one current status, places progress in the relevant
action, and follows remote acknowledgements without overwriting unsaved edits.

Clicks resolve an opaque original-account route. Account switching and unlock
remain explicit; an invalid route shows an unavailable state and network failures
keep a retry action. The core provides only metadata for inactive accounts.
Approval decisions and signing out use centered system alerts with explicit Cancel
actions. The approval footer keeps its request-only explanation above the action
row, including when the text wraps.
Timed channel mutes retain their expiry when editing the channel name. Quiet
hours use the saved IANA time zone, including DST transitions.

New installations start with notifications off; an upgrade keeps the earlier
default through the core migration. Once an account is set up, Reminders and
Login identities show a non-modal prompt to turn on security alerts. It only
opens notification settings; the system permission request still comes from
changing the settings, which save themselves shortly after a change. "Not now" is kept per account in the private preferences
file, and binding another login shows the prompt again. Turning on an account
while another local account already receives notifications asks for native
confirmation: the notification service and Apple can tell that these accounts
share the device.

Once enabled, the notification service processes opaque resource IDs, channel
mute and hidden state, quiet hours and time zone, view receipt times and
watermarks, which accounts share the installation, and recovery records uploaded
for deadline reminders (with the new login identity and device public key). The
offline privacy policy states this and that revoking the registration, removing
the account or deleting the service account clears it. App Store Connect labels
must match `PrivacyInfo.xcprivacy`: User ID and Device ID (identifiers, recovery
records, accounts per installation) and Product Interaction (view receipts,
mutes and quiet hours), all linked to the person, used for app functionality,
and not used for tracking.

See [validation](VALIDATION.md) for the tested build and simulator scope. APNs,
App Attest, signed installation, physical-device lock/Focus/cold-start delivery
are separate acceptance gates. Wallet proposals currently require authenticated
member reads; no independent remote wallet notification source is claimed.

## Build

Validated toolchain: Xcode 27.1, Rust 1.98.1, UniFFI 0.32.2, XcodeGen 2.46.0.
Deployment target: iOS / iPadOS 18. The SDK version does not raise that minimum.
The current XCFramework and project support ARM64 devices and Apple Silicon
simulators; an Intel Mac simulator slice is not included.

Business icons use the desktop's Remix Icon paths, including navigation,
account destinations and all eleven vault item types. `Features/AppGlyph.swift`
selects the shipped template assets; system actions keep SF Symbols.
Run `python3 scripts/ios/sync-icons.py` after changing the shared desktop paths
or the selected icons. `python3 scripts/ios/sync-icons.py --check` verifies the
assets and bundled license against their source and is part of `verify.sh`.

Object lists use shared compact row insets and 36 pt icon tiles. Standard
two-line vault, channel and file rows target 56 pt, with a 44 pt minimum content
hit area; Dynamic Type can increase their height. Inbox dates share the sender
line at ordinary text sizes and move below it at accessibility sizes.

From the repository root:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
bash scripts/ios/build-core.sh
open src/dmsg_ios/dMsg.xcodeproj
```

Build the `dMsg` scheme. `CoreBridge/Package.swift` consumes the generated
Swift module and the device/simulator XCFramework. Build artifacts and the
generated Xcode project are ignored; do not copy a library from a different
commit. The build records the transitive local source fingerprint, toolchain, profile, Swift binding and both native-library hashes. A source change during the build prevents recording a misleading manifest.
Use `DMSG_IOS_PROFILE=release` for optimized Rust slices. App code uses Swift 6;
generated UniFFI code is isolated in its Swift 5 language-mode package.

```sh
bash scripts/ios/verify.sh SIMULATOR_UUID
```

Tests run in parallel, one test class per simulator clone. The script allows one
worker per 5 GB of memory and two cores, at most four; `DMSG_IOS_TEST_WORKERS`
overrides that. To run a class while working on it, pass `-only-testing` to
`xcodebuild` rather than running everything.

The manual `dmsg-ios.yml` workflow runs the same checks on a runner with the
reviewed SDK. A missing SDK is an explicit failure, not a lower-version pass.
The workflow has not been run on GitHub as part of local development.

## 一键生成 TestFlight IPA

在已配置 Apple 开发者账号和签名的 Mac 上，从仓库根目录执行：

```sh
python3 scripts/ios/testflight.py
```

需要完整 Xcode、Python 3、Rust 和 XcodeGen。自动签名可以使用 Xcode
Settings → Accounts 中具有本团队 App Store Connect 和分发签名权限的账号，
也可以使用下面的 App Store Connect 团队 API Key。首次配置时安装 XcodeGen 和两个 Rust target：

```sh
brew install xcodegen
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
```

脚本取 `project.yml` 中主 App 和所有扩展的最大 `CURRENT_PROJECT_VERSION`
加一，统一写回所有目标，保留其余配置和已有改动；`MARKETING_VERSION` 不变。
例如主 App 为 `4`、扩展为 `3` 时，下次会统一生成 `0.1.0 (5)`。
需要更改产品版本号时，先把所有目标的 `MARKETING_VERSION` 改为同一版本。
构建号依据本地文件递增，不查询 App Store Connect；在其他机器上传过构建时，
先同步已提交的构建号，或把本地最大构建号更新到已使用的编号。

可以先预览，不修改文件或执行构建：

```sh
python3 scripts/ios/testflight.py --dry-run
```

未在 Xcode 登录的构建环境，可以显式提供 App Store Connect **团队 API Key**：

```sh
python3 scripts/ios/testflight.py \
  --authentication-key-path "$HOME/.appstoreconnect/private_keys/AuthKey_KEY_ID.p8" \
  --authentication-key-id KEY_ID \
  --authentication-key-issuer-id ISSUER_ID
```

三个参数必须一起提供，并会同时传给归档和导出阶段的 `xcodebuild`。
密钥需要本团队的开发者资源及分发签名权限；APNs 或 Sign in with Apple 的 `.p8`
不能替代 App Store Connect API Key。私钥放在仓库之外，脚本只传入文件路径，
不会复制私钥到产物目录。参数不完整或私钥文件不可读时，在递增构建号前报错。

使用 Xcode 账号时，脚本先检查已保存的账号登记。如果 Xcode 的已知账号列表明确为空，
且本地没有本团队带私钥的 Apple Distribution / iPhone Distribution 身份，脚本在递增
构建号、编译和创建导出目录之前停止。已打开的 Xcode 仍可能显示内存中的登录状态，
而新的 `xcodebuild` 进程读不到它。检查只读取账号登记和签名身份元数据，不读取密钥、
修改 Xcode 偏好或注销账号。API Key 跳过该检查；已有本地分发身份、未知账号存储格式或
无法读取诊断信息时，仍交给 Xcode 实际校验。此检查不能保证账号未过期、团队权限或描述文件有效。

脚本复用 `archive.sh`，重新构建优化后的 Rust 核心、生成 Xcode 工程并归档
Swift Release，再使用自动签名导出 App Store Connect IPA。导出时使用系统工具
PATH，避免 Homebrew `rsync` 与 Xcode 导出的参数冲突。完成后会检查 IPA 内主 App
及全部扩展的版本/构建号，并输出 IPA 路径及 SHA-256 校验文件。

以 `0.1.0 (5)` 为例，输出目录是：

```text
src/dmsg_ios/.build/TestFlight-0.1.0-5/
├── dMsg.xcarchive
├── ExportOptions.plist
├── archive.log
├── export.log
└── Distribution/
    ├── dMsg.ipa
    └── dMsg.ipa.sha256
```

输出目录被 Git 忽略且不会覆盖已有同名目录。开始构建后即保留递增后的编号；
如果归档失败，查看 `archive.log`，修复后重新执行会使用下一个编号。
如果归档成功但导出失败，修复签名配置后可直接重试已有归档：

```sh
python3 scripts/ios/testflight.py \
  --export-archive src/dmsg_ios/.build/TestFlight-0.1.0-5/dMsg.xcarchive
```

使用 API Key 时，在此命令后追加相同的三个认证参数。
`--export-archive` 从归档读取团队、版本和主 App/扩展信息，不读取或修改
`project.yml`，不递增构建号，不重新编译，也不要求安装 Rust 或 XcodeGen。
每次重试在归档旁创建独立的 `Export-*/` 目录，包含本次 `ExportOptions.plist`、
`export.log` 和 `Distribution/`；原来的归档、日志和 IPA 保留。
可以追加 `--dry-run` 只检查归档，不导出或创建目录。

将 `project.yml` 的构建号变更随代码提交，便于下次更新和其他机器接续。
脚本打包当前工作区，未提交改动也会包含在内；打包期间不要同时修改源码或运行另一套
原生构建。脚本不自动执行完整单元/UI 测试，发布前仍按上面的 `verify.sh` 验证。

生成后，在 Apple **Transporter** 中添加 `Distribution/dMsg.ipa` 并点击“交付”。
也可以用以下入口打开归档，再在新打开的 Xcode Organizer 窗口选择
**Distribute App → App Store Connect → Upload**；取消勾选
**Manage version and build numbers** 可保留脚本分配的构建号。

```sh
bash scripts/ios/open-organizer.sh src/dmsg_ios/.build/TestFlight-0.1.0-5/dMsg.xcarchive
```

这个入口沿用 `DEVELOPER_DIR` / `xcode-select` 选择的 Xcode，显式启动一个使用系统工具
PATH 的新进程；已打开的 Xcode 窗口保持原状。入口只打开归档，不会自动验证、上传或
递增构建号。请在**新窗口**中执行 **Validate App** 或 **Distribute App**；若难以区分
窗口，可先正常退出已有 Xcode，再运行入口。

直接双击归档可能复用带有 Homebrew PATH 的 Xcode，绕过打包脚本的环境隔离。
如果界面只显示 `Copy failed`，检查分发日志中的 `IDEDistributionPipeline.log`：
`IDEDistributionCreateIPAStep` 的 `rsync: ... --extended-attributes: unknown option`
表示系统 `rsync` 按 PATH 启动了不兼容的子进程，本机 IPA 打包已失败；
此时 `ContentDelivery.log` 的 HTTP 200 不能代表上传或验证完成。
使用上面的新进程入口，或 `--export-archive` 重新导出同一归档即可，不需要重编译、
更改签名配置或卸载 Homebrew `rsync`；仅修改终端 PATH 不会改变已运行的 Xcode。

Apple 处理完成后，在 App Store Connect → TestFlight → 测试组中添加新构建。
打包脚本不会自动上传、提交审核或邀请测试员。

如果 `export.log` 出现 `No Accounts`，说明导出进程无法取得可用的团队认证。
先用 `xcode-select -p` 核对命令行使用的 Xcode；`DEVELOPER_DIR` 如果设置了会覆盖该选择。
同时核对是否由同一 macOS 用户运行。**界面显示已登录，不代表命令行能读取到账号**：
如果脚本报告已保存账号列表为空，在对应 Xcode → Settings → Apple Accounts 中用
**Sign In** 重新认证同一账号，再重试已有归档；无需为此先删除证书或退出原账号。
密码和验证码只在 Xcode 输入，也可以使用上述 API Key 参数。账号可读取后，再检查团队、
App Store Connect 及云端分发证书权限。若仅受限终端失败，而同一 Xcode 的界面可导出，
可在 macOS Terminal 中重试已有归档；沙箱外也报同样错误时，不能归因于沙箱。

归档可使用已有的 Apple Development 证书和开发描述文件，因此归档成功不代表已具备
TestFlight 分发签名条件。`security find-identity -v -p codesigning` 可以检查本地带私钥的
签名身份；Apple Development 和用于 macOS 的 Developer ID Application 都不能替代
Apple Distribution。本地分发签名需要证书及其私钥；自动签名也可使用
[Apple 云端管理的分发证书](https://developer.apple.com/help/account/certificates/cloud-managed-certificates)，
因此本地没有 Distribution 身份不一定是错误，仍需核对账号和云签名权限。
归档成功但导出失败时不会显示“IPA ready”，也不能把 `.xcarchive` 当作 IPA 上传。

当前 Release/TestFlight 连接线上服务，内购开关为关闭；脚本沿用 `project.yml`
和应用代码中的部署设置，不额外切换环境。

## Implemented paths

- System authentication with the fixed HTTPS callback, account initialization,
  device pairing by text/share/QR, second-login binding, account switching,
  device revocation and delayed recovery through the existing core.
- Vault search, full favorites and type filters, field-level reveal/copy, TOTP,
  draft editing, version comparison and field-level conflict choices, field
  references, attachments, trash and local/sync status. App-managed keys remain
  read-only.
- Channels, invitations, messages, encrypted durable send intents and immutable
  attachments. Retries retain the original message ID after restart. Incoming
  attachments are verified and re-encrypted a chunk at a time. The inbox has
  cached/refreshable lists, native reading, read marks and archive/restore.
- Private document/photo import, folder browsing, verified Quick Look/share
  copies, explicit cloud backup, public profile publication and approval review.
- Hardware approval-key registration, administrator enrollment tokens,
  replacement/retirement and account-purpose policies. Independent Secure Enclave
  signing and agreement keys support user presence or current-biometry-only ACLs.
- Optional device-bound workspace unlock, recovery warnings from public certified
  security, and distinct missing-key, unavailable-hardware and invalidated-key UI.
- Selected secret snapshots by link or channel, verified previews and selective
  imports, explicit sharing/revocation/reconciliation, local health, dependency
  lists, rotation review and paginated activity including terminal outcomes.
- Four system tabs and adaptive `NavigationSplitView`, resources for 23 locales,
  Dynamic Type, dark appearance and system accessibility controls.

Production trust parameters come from `dmsg_client/production.toml`, also used
by the desktop. No deep link can replace endpoints, roots or canister pins.
The file library accepts document imports up to 1000 MiB. New cloud backups
allow 1000 MiB for eligible Max memberships and 100 MiB for other tiers; existing
backups remain restorable after a downgrade. Photo/video library staging accepts
1000 MiB too; conversation attachments, vault imports and explicit File Provider
exports retain their separate 100 MiB limits. These are
engineering bounds, not measured minimum-device performance guarantees.
Secret packages have a separate 8 MiB payload ceiling, checked before downloads,
against cached previews, before approval and again on import. This is not a peak
RSS budget: chunk/manifest buffers and password derivation also consume memory.
The limits API reports those bounds. Normal attachment saving refuses secret
packages so their contents use the verified preview and explicit-selection flow.

## Session and privacy behavior

When the workspace is locked and quick unlock is enabled, cold launch or a
return from the background automatically requests the existing system
authentication once. Cancelling or failing leaves the manual unlock button
available; an authentication dialog's inactive/active transition does not
retrigger it. Explicit and idle locks stay locked during the current visit.
The existing Secure Enclave, presence and account checks still perform unlock.

The scene has explicit inactive, background, resuming, active and protected-data
unavailable phases. Visibility remains independent of login and content unlock;
only the active phase uncovers the workspace. A single operation token and kind
own busy/unlock/setup/sync feedback, so an older completion cannot clear its
replacement. Existing session and view generations still fence asynchronous work.

After a transient system-authentication interruption, the app synchronously
validates the existing scoped core handle before uncovering the same workspace.
It does not enqueue a session read behind an approval’s account update or root
distribution. Background returns, protected-data loss and identity transitions
still require full session recovery. While a decision is in flight, its page
shows Waiting for confirmation and keeps both decision buttons disabled.

Device transfers keep their plan in the phase that owns it: waiting for export
approval, transferring or suspended. Returning to the foreground can resume the
same receiving session; sending again requires another reviewed export. System
ciphertext transfers distinguish transferring, paused, failed and awaiting
foreground verification. They retain the existing on-disk format and attempt
numbers, including across retries and restarts.

Wallet transfer, control-change, cosigner registration and replacement proposals
share editing, submitting, unconfirmed and created stages. An interrupted or
unreadable result retains the original request ID for reconciliation. Only a
confirmed absent/rejected journal permits a new ID; a completed journal is read
back before showing the proposal as created.

The app follows iOS language preferences unless a per-app language is selected.
The **Language** link on the access screen and in **Account** opens iOS
Settings, where the dMsg language option is always shown. Supported languages
are English, Simplified and Traditional Chinese, French, Spanish, Russian, Arabic,
Hindi, Indonesian, Brazilian Portuguese, Japanese, German, Korean, Italian, Turkish,
Vietnamese, Thai, Filipino, Malay, Bengali, Urdu, Polish and Dutch.
Traditional Chinese uses one `zh-Hant` catalog for Taiwan, Hong Kong and Macau. This uses
the system's per-app setting for native UI and Foundation strings; no in-memory
locale override is installed.

Application text is maintained per language in
`Resources/Localizations/<language>.lproj/Localizable.strings`, with plural
rules in the adjacent `Localizable.stringsdict`. These are UTF-8 XML property
lists, the native format emitted by Apple's String Catalog compiler. The app
and all five extensions include the same resource directory; default
`Text` and `String(localized:)` lookups continue to use the `Localizable` table.
No catalog merge step or custom runtime loader is needed.

Add new keys to English and the other language resources. The localization
check compares keys, format arguments and plural rules, and checks compiler
extraction with `--derived-data`. Unlike String Catalogs, these files are not
automatically populated during a build. The small `InfoPlist.xcstrings` and
`AppShortcuts.xcstrings` tables retain their existing platform-specific roles.

The access screen explains the next step for the current account state: first
sign-in, workspace creation, approving this iPhone for an existing account, or
unlocking a saved workspace. Language and privacy links remain accessible before
sign-in and adapt to large text. The privacy page starts with a contact link.

Mobile login and login binding always use the `dmsg.net` identity domain. The
mobile UI and bridge no longer offer the `panda.fans` selector; old-account and
legacy-data migration remain desktop workflows. Existing account data is not
recreated or merged just because a login domain changes.

System web authentication requests an ephemeral browser session for each login.
This avoids reusing persisted website data after an Internet Identity IndexedDB
cache failure in the system authentication window. II still owns authentication
and passkey access; users may need to select their identity again. The fixed HTTPS
callback and core delegation validation are unchanged. This is a client-side
isolation measure, not a repair of II's cache implementation; successful Safari
login alone does not establish that the in-app flow works on a physical device.

Newly approved devices still need relay connectivity to retrieve their encrypted
root bundle before entering the workspace. A failed initial download is not a
reason to revoke the device or reset its local data. The native console records
only the core's allowlisted diagnostic source, reason and HTTP status, prefixed
with `dmsg:`; it does not record request URLs, identities, tokens or content.

Vault, file and message lists use `NavigationStack` at compact widths and
`NavigationSplitView` at regular widths, with one shared selection for rows
and favorites. Compact lists do not also drive split-view selection or column
changes during a push. Secret tasks follow the selected item and session.
Secret reads restart after foreground recovery and offer Retry after a failed
read. Cancelled or old-session results cannot overwrite the current detail.
The navigation UI tests include delayed reads, managed keys, repeated back/tab
navigation and background interruption; these are presentation fixtures, not
claims about production network availability.

One application owns one engine. UI snapshots carry a generation, and Swift
also invalidates view work on lock/account changes. The core's bounded host
owns the client without holding a mutex across network waits. Lock cancels the
current future before clearing keys; existing core journals remain authoritative.
Heavy operations have explicit cancellation tokens, linked to Swift task
cancellation. Cooperative holds yield between streaming file steps.

Identity changes have a shared transition gate and fences at both entry and
completion. Old or intermediate snapshots cannot mint a new content handle,
including during binding acceptance, creation or recovery of the same account.
Foreground resume is conditional on the lifecycle generation observed before
the async call; a late foreground task cannot undo a newer background pause.
Background suspension and optional immediate lock are requested synchronously.

Legacy `&mut Client` account/control operations are serialized on this
cancellable owner. This is the architecture's temporary dedicated-worker path;
it is **not a claim that every legacy operation has been converted to shared
`Hold` steps**. Local reads can wait behind an explicit network operation;
lock/pause do not wait for the remote response. The desktop shares the split
fast-unlock preparation/presence/completion path and no longer holds Client
while waiting for its system prompt.

Fast unlock requires Secure Enclave user presence. It never creates a Plain
fast key on iOS and removes an old Plain record. Keychain entries are scoped by
bundle, environment and installation, non-synchronizable, and use
`WhenPasscodeSetThisDeviceOnly`. Hardware denial, cancellation and unavailable
hardware are distinct. Simulator presentation fixtures do not unlock Rust and
are compiled out of Release builds.

The adapter supports device-passcode fallback and an explicit current-biometry-only
policy. A missing biometric-state observation is unknown, not a changed enrollment.

Each workspace also has a hardware anchor (`createAnchor`/`signAnchor`): a
Secure Enclave signing key with `AfterFirstUnlockThisDeviceOnly` access that
never asks. A workspace a restored backup or device transfer copied from
another iPhone cannot sign with it and opens moved (`state: "moved"`): it never
unlocks, and the app offers only to set it aside (`setAsideMoved`) and start a
new device of the account. Copies set aside stay for import until deleted
(`movedCopies`, `discardMoved`). Before this iPhone joins, a copy of this
account's administrator device can approve it in its place
(`movedApproveSuccessor`, decided in the approval view); once it has joined,
Devices imports a copy's file library (`movedImport`), which settles the content
step of an open handover from the copy's device.

Replacing a device hands an administrator device over to a new one. The new
iPhone, before it joins, asks from the access screen (`successionRequest`: its
approval and cosigner keys are made here) and shows the request as a QR code to
scan or share. The old device reviews it in Devices → Replace a device
(`successionReview`, with what only that device holds) and approves it once the
person enters the new device's ID (`successionApprove`). Each side then runs
its steps in order (`successions`, `successionRun`): steps that ask open the
approval view, wallet bindings wait in Approvals, native addresses link to
moving them, and local content moves with a device transfer or a copy import, or
is left behind (`successionSkip`), which the revocation lists. Stopping
(`successionCancel`) keeps what was done. What only this device holds
(`localOnly`: versions without a cloud backup, unsynced changes, the last
successful sync) is available during device replacement and warns in Files and
before binding the workspace to this device. The Devices overview does not show
a backup reminder: file-library cloud backups are optional.

Device transfers move the file library between devices of the account over
the local network (dmsg-device-transfer/1). The core runs the TCP listener or
connection (`transferReceiveStart`, `transferSendStart`, `transferStatus`,
`transferCancel`); the app advertises the receiving session with Bonjour
(`_dmsg-xfer._tcp`, under the name `transferAdvert` gives) or finds it through
DNS Service Discovery, and otherwise takes the address and port the receiver
shows. The first use asks for local network access
(`NSLocalNetworkUsageDescription`, `NSBonjourServices`); when it is off, the app
says where to turn it on. While a transfer runs the screen stays on, and
moving bytes keep the app from locking for idleness. Leaving the foreground
stops it and returning starts it again with the same session, which resumes;
locking forgets it. Devices → Transfer between devices receives, sends to a
session code and device ID, and resumes a stopped transfer with its session.
Hardware approval uses the core's purpose selection and canonical commitments;
Swift signs Data(H), while Rust verifies and normalizes the signature. A private
authorization token belongs to one decision and is released on completion or
cancellation. Neither handles nor an arbitrary-digest signing API are exposed to UI.

Login, binding acceptance, strict device binding, re-enabling bound fast unlock
and enrollment-token proof use preparation, native authentication outside the
client owner, and completion against the same account, generation and key slot.
The mobile host rejects an unprepared fallback authentication if requirements
change. The original bound slot survives logout, disabling fast unlock and errors.
Explicitly removing binding rewraps the same local data key; reset is a separate
operation. Binding protects copies made after activation, not earlier copies.

Inactive scenes cover content, including native sheets. Background stops new
core work; the default grace is three minutes, measured with `ContinuousClock`.
An immediate-lock preference persists. Protected-data loss closes the engine.
Foreground idle lock is fifteen minutes. Clipboard copies are local-only and
expire after thirty seconds; explicit lock clears only a still-owned copy.
The grace period preserves keys in memory; if iOS suspends execution, an expired
interval is enforced before foreground access resumes. It is not a promise of
running a timer while suspended, or strict hardware binding of every login path.

Imports and temporary exports have Complete Data Protection and are excluded
from backup. Staged imports are account-bound and are removed only after
successful import. Interrupted staging is preserved. Shared copies outside the
app cannot be recalled. Swift strings do not promise forensic memory erasure.

Automatic synchronization is **on by default** for every account, as the
product requires after login unlock, binding and recovery; the first automatic
pass of an account says so once, and settings turn it off for that account
(`manualSyncAccounts` in the local preferences). It runs only while the app is
open and unlocked. Interaction does not cancel it, and leaving the app gives
what the person just did a short window to finish: on the way to the background,
unless immediate locking is on, sends left open and already signed vault
submissions finish for at most 20 seconds under `beginBackgroundTask`, then the
engine stops. Unprepared vault versions wait for the next foreground pass.
Nothing fetches content in the background, and the next foreground catches up.
It does not select files for backup. Files require their own explicit selection
and confirmation.

`MobileEngine.run_content_sync` owns the schedule (`dmsg_client::sync::Schedule`,
shared with the daemon) and reports status and passes through
`ContentSyncObserver`; Swift only keeps the views current. While the hint channel
is connected, a periodic pass only backs up a lost hint. A send without a verdict
leaves again under its own ID when the network returns or the app comes back,
for 24 hours. Conversations with news show a dot before their messages arrive,
and unmuted ones have at most their latest page read ahead every 30 seconds while
on screen. The read position and delivery cursor follow to the account's other
devices, and an authorized device rotates a channel that waits for a new epoch
when it reads it.

Manual synchronization shows an activity indicator until completion or failure.
Vault synchronization publishes completion before independent maintenance work. It does not fetch channel/Inbox bodies or cloud-backup inventory. Files reads its saved cloud inventory first and refreshes it while that screen is in use; restoring
file bytes onto this device remains an explicit action. The Account header reads
the official public name and avatar, with anonymous, bounded avatar downloads and
an explicit retry when loading fails.

Recovery checks use public certified metadata while the app is active, including
when the content workspace is locked. They do not depend on automatic sync,
unlock keys or upload content. A previous pending warning survives a failed
refresh and is marked stale; unknown never becomes a verified safe state. Checks
stop on background/protected-data loss. This is not an always-on notification
service. Recovery details and disputes remain in the account-security flow.

Approval capability is an explicit allowlist shared with the UI through the
fresh request view. Secret use, CLI execution, SSH and mounts are not mobile
execution paths; local desktop requests/grants/history do not synchronize here.
Connect checkout and other financial approvals remain disabled. Share and import
execution uses cancellation tokens and existing receipts: an unknown delivery
is reconciled by its original operation, never automatically repeated as new.
Link limits apply to download sessions; channel snapshots have neither link
expiry nor recall. Files imported with a snapshot stay local and inherit the
selected folder's grants. OTP selection includes the reusable seed explicitly.

## Apple 服务接入

原生端继续通过 Core 的 `/v1/accounts/:account/mobile/*` 调用登记、配置和购买接口，不需要新增
Apple 服务地址或凭据。App Store Server Notifications V2 配置到 Core 的 `/storekit/notifications`；
生产与 Sandbox 地址、请求格式和确认语义见[公开移动合同](../../docs/protocol/mobile_zh.md#31-app-store-服务端通知)，
发布检查见[审核说明](Review/README_zh.md)。这项服务端入口调整不启用当前关闭的内购，也不改变
TestFlight 固定连接生产的策略。

## Deployment gates

This tree is development code, not an App Store release assertion. In particular:

1. The app is signed by LDC Labs Limited (`S9Z2WAMAV8`) as `net.dmsg.ios`;
   `project.yml` pins the team. The frontend ships the matching
   `static/.well-known/apple-app-site-association`, generated with
   `node scripts/ios/aasa.mjs S9Z2WAMAV8 net.dmsg.ios`; change both together.
   Release the frontend with the login page, fixed callback page and AASA, check
   that `https://dmsg.net/.well-known/apple-app-site-association` answers 200
   JSON without redirects, then verify association, the II popup/passkey and
   fragment callback on a physical phone. The fixed callback page clears
   fragments and offers no forwarding target.
2. Verify Secure Enclave, device-password fallback, lock-screen file protection,
   current-biometry invalidation, hardware anchors after a restore to another
   iPhone, real copy-to-another-app behavior and the
   cross-device login/edit/revoke slice. Confirm user home supports the current
   AccountInfo/hardware-proof contract before testing real protected accounts.
   Simulator/software-vector checks do not establish native ACL or presence behavior.
3. APNs, App Attest and StoreKit now have native APIs, provider/verification
   services and a governed commerce adapter. Apple products and deployment
   credentials must be provisioned; no private key belongs in this repository
   or app. Production can explicitly admit Sandbox annual subscriptions through
   matching gateway and commerce opt-ins (both default off). Formal memberships
   take priority; sandbox renewals/refunds cannot replace them. TestFlight and App Store
   Release builds both use the fixed production services; a sandbox receipt never
   changes business endpoints. Testers register with their own Internet Identity
   passkey. No shared review account or StagingDeployment.json is required.
   The current project sets DMSG_IN_APP_PURCHASES_ENABLED=YES. StoreKit product
   loading, purchase, restore and transaction delivery are enabled. App approval
   does not approve subscription products: the first subscriptions and their
   group must be submitted together with a new app version. Check the actual
   product approval and storefront availability in App Store Connect; successful
   TestFlight purchases do not establish production availability. See
   [the release checklist](Review/README_zh.md). Empty product results offer Retry
   and log missing public product IDs without account tokens or receipts.
   Staging remains an explicit Debug-only option with separate storage, Keychain
   namespaces and a visible banner; it is not selected by TestFlight.
4. Duo closed, open and partially open presentations have been exercised in
   Apple's iOS 27.1 Duo simulator. The remaining device matrix includes side
   Touch ID, system reserved regions with live activities/camera, real keyboard
   and fold transitions, and App Store screenshots. Simulator layout evidence
   is not Duo hardware evidence.
5. Wallet execution/management, cosign packets shared with the desktop (sign,
   share, import, execute once enough are collected; signatures that no longer
   match a cosigner's current key are set aside and their cosigners shown as
   needing to sign again), this iPhone's keys per wallet (the native Taproot
   key root and its protectors, moving native addresses to this device through
   L1 or L2, replacing a key this device does not hold with one it does, and
   unbinding revoked devices), agent management,
   Share/AutoFill/Files extensions,
   widgets, App Intents, browser app-hop navigation and bounded background ciphertext
   transfers are implemented. Existing product-level wallet creation pause and the
   paid-letter sending gate remain. iOS is not a persistent agent daemon.
6. Service-account deletion, report/block and offline privacy are implemented.
   The public tour has been removed; App Review still needs full account access. Real
   review account access, signing, merchant/provider configuration and device
   acceptance remain release tasks, detailed in [Review/README_zh.md](Review/README_zh.md).

The exact local checks and remaining gaps are recorded in [VALIDATION.md](VALIDATION.md).

Implementation: Codex (OpenAI), 2026-10-25; hardware-security and Secrets
integration updated 2026-10-26.

## Native capability contracts and checklist

The public [mobile contract](../../docs/protocol/mobile_zh.md) describes account
binding, assertions, signed routes, StoreKit authority, deletion and narrow
ciphertext grants. [The executable checklist](IMPLEMENTATION_CHECKLIST_zh.md)
separates implemented code from external deployment and device acceptance.

The main app is the only process holding a Client workspace. Share stages explicit
inputs, while AutoFill and Files get separate selected encrypted snapshots that
expire after 24 hours. File Provider is read-only; its independently authenticated
access session lasts five minutes and ends on app lock. Recipients may retain
already exported copies. No extension receives account roots or device signing keys.

Background URLSession handles one selected immutable backup at a time with at most
three tasks. Its separate directory contains only ciphertext and scoped capabilities.
Native task descriptions and the local progress record survive app reconstruction;
the original unlocked account verifies and finalizes. A completed download is not
an imported or committed file, and force quitting can postpone delivery.

Use the archive helper for optimized Rust plus Swift Release:

    bash scripts/ios/archive.sh --archive-path /absolute/output/dMsg.xcarchive

The 2026-10-27 App Store Connect encryption questionnaire returned no document
upload requirement for the published cryptography and selected storefronts
(United States, Hong Kong, Taiwan and Singapore; excluding France). The answers,
implementation evidence and assessment limits are recorded in
[ExportCompliance.json](Review/ExportCompliance.json). Accordingly,
`ITSAppUsesNonExemptEncryption` is false; this describes documentation exemption,
not absence of encryption. Reassess before changing algorithms or storefronts.

Set signing/provisioning for a real archive; local unsigned builds do not prove
distribution eligibility. Check compiler extraction and offline policy parity with:

    python3 scripts/ios/localizations.py --derived-data src/dmsg_ios/.build/ReviewDerivedData

Implementation and review: Codex (OpenAI), 2026-10-27.

### Cache and offline reading (2026-11-01)

`DisplayLoad` paints the LDK-encrypted Rust snapshot before refreshing a visible
profile, device/recovery/security, Inbox-rule, plan or wallet page. Failure preserves
saved values and drafts; a missing observation is distinct from a known empty list.
Account → Cached data reports and clears reconstructible snapshots, attachment
copies and cloud inventory. Vault/message records, file-library copies and unsynced
work are not cache-clear targets. Unlocked memory and temporary OS preview files
have their existing lifecycle; the entire SQLite file is not claimed to be encrypted.

Vault synchronization detects local edits during the foreground loop, coalesces
manual/automatic passes and polls idle accounts less often. Channel and Inbox body
increments run only in their visible views, with a 30-second fallback; notification
refresh never pulls bodies. File backup and restore remain explicit. A cloud-only
file has a restore action and cannot be previewed as a complete local file.

Automatic vault sync honors persisted retry deadlines and pauses after failures
requiring user action. An explicit Sync now retries repaired failures and resumes
quota-paused uploads, while retaining the server's busy deadline. Upload errors
remain visible in the session even when incoming changes were received successfully.

Conversation history uses local pagination and stops when no earlier page remains.
An uncached conversation starts with a bounded recent interval, with older history
fetched on demand. This window never advances the complete-history delivery cursor
over a gap. Empty display pages retain their next cursor. Existing conversations
continue incrementally, and explicitly granted history still reopens normally.
Scrolling past the beginning does not trigger a latest-message refresh, and explicit
history pages survive later refreshes. Attachment deletion metadata refreshes in a
separate visible-view task, so it cannot delay text completion. A send retains its
original ID; after two seconds without an ACK, the shared core starts bounded queries
for that operation's durable receipt while the single POST is still in flight. A
lost connection also triggers reconciliation. Only a matching receipt confirms the
send, and the message plus removal from both send queues commit atomically. Safe
reads rejected with `POLICY_STALE` refresh account evidence and re-sign once; writes
are never blindly replayed by this recovery path.

`MessageSending` belongs to the account session. Every text, reply, edit, withdrawal
or attachment is persisted before delivery; messages of a channel run in order.
The composer is released after local queuing, and navigating away does not cancel
a send. ACKs carry the verified display message so the bubble updates before a
later local refresh. Failures stay beside the original message with its retry ID.
Lock, account changes and background suspension cancel session tasks and clear
plaintext presentation; the encrypted core journals remain available for recovery.
This is not an iOS background-execution guarantee.

Message timing diagnostics contain fixed phase names and elapsed milliseconds:
queue, preparation, submission/reconciliation, local settlement, recent/history
reads and local page decoding. Swift additionally records local-page availability,
queued-row presentation and ACK presentation. They contain no message content,
resource IDs, URLs or credentials, and are not production latency targets.

Rebuild the Rust bridge with `scripts/ios/build-core.sh` before simulator validation.
Rust workspace/host tests cover offline attachment reopening, cache-clear retention,
late-write fencing and local reads alongside a stalled network request. Swift tests
cover wire decoding, lifecycle and local/cloud file availability; UI fixtures cover
presentation only and do not prove production-network or physical-device behavior.

Validation on 2026-11-01 rebuilt both native slices and checked their source
fingerprint. On iPhone 17e / iOS 27.0 Simulator, all 47 Swift unit tests and seven
selected NavigationTests passed (54 total). The selected UI tests cover the cache
explanation in English, Chinese, French and Arabic, dark/light appearance, long
profile names, manual-sync presentation, file detail/cloud inventory and read-only
plans. These are synthetic UI fixtures; physical hardware unlock, prolonged
background execution and live two-device cloud tests were not run.

### File library and backup workflows (2026-11-03)

Files separates the device's library from the account's cloud backup inventory.
Each scope has its own search results; names, dates and sizes can be sorted.
Refreshing the cloud list never backs up local files. Its saved observation time,
refresh failure and retry action share one status row. Local risk information
counts file versions without backups, separately from account synchronization.

Cloud rows show version dates and source devices. Details identify the original
location and the restore destination; existing file versions stay with their local
file record. Historical versions remain selectable, and opening waits for an
explicit restore and full verification. Background transfer is a secondary,
explicitly confirmed choice and still needs final verification in the original
account. The task page prioritizes active transfers, preserves committing and
verifying stages, and supplies task-specific retries and cancellation.

**Share by link** creates a link that anyone can open in a browser, with no account
or app. It is offered on a file row, in file details and on a cloud backup, with the
same terms as the desktop: expiry, download limit and an optional password. A backup
that is not on this device is fetched only to encrypt the new copy, which the sheet
explains; the library does not change. Files over 100 MiB explain why they cannot be
shared. **Share links** in the tools menu and in file details list links with their
state and counts, keep the saved list when a refresh fails, and revoke after a
confirmation. Secret snapshots stay under their own screen.

**Collect files** in the tools menu makes upload-only links for people with no account, one
per name if asked, on Plus and above; a Free account sees what it does and the way to the
plans. A link's detail lists what arrived, with the sender's name marked as unverified. A
file comes down only when saved, re-encrypted into the library under `Received files`, in
the foreground: unlike a restore it has no background transfer, so the screen asks to keep
dMsg open and holds off the idle lock meanwhile.

Folders and files are pushed onto a real navigation stack; a folder's last listing
shows at once on the way back. Opening a local file of a previewable type up to
200 MiB goes straight to its preview, with details one step away; a cloud file still
downloads only when asked. Clean imports and trash report briefly, and a results page
appears only when something failed.

The library supports folder naming/moving/trashing, multiple file imports,
photo/video selection, and per-file outcomes for batch backup/move/trash actions.
In the move workflow, the destination picker's **Move here** button executes the
move directly and returns to per-file results. There is no second submit step;
failed items can retry the chosen destination without repeating completed moves.
Interrupted imports retain their original staging record for continuation. Backup
admission distinguishes existing backups, running jobs, size limits and missing
local copies instead of always claiming a queued transfer. A local copy can be
removed only after the core refreshes the inventory and checks its verified backup;
its file record and restore action remain available. Trash displays the configured
retention period and removal dates. Cloud deletion is separate and warns when this
device has no copy.

The **+** menu also offers **Take photo or video** on devices with a camera. The
system camera returns the capture only to the app, so it never enters the Photos
library and no Photos permission is requested. Photos are stored as JPEG, keeping
orientation but no location or other capture metadata, so every client can view
them; recordings stop at seven minutes to stay within the 1000 MiB file limit. The
capture is staged and encrypted in the open folder like any import, and cloud backup
stays a separate action. Its temporary plaintext copy is removed after staging, or
at the next start if the app was interrupted.

The existing theme and compact file rows are reused. Maximum Dynamic Type uses a
wrapping scope menu; the local warning keeps its count and a labelled help control.
All added copy is included in the 23 supported languages. The implementation and
actual simulator/device coverage are recorded in `VALIDATION.md`.

## Passkey login and renewable sessions

A configured `auth` issuer enables the primary native Passkey entry, with Internet Identity as the alternate. AuthenticationServices uses RP `dmsg.net`, user verification and none attestation; the existing Associated Domains/AASA must match the signed app. UniFFI passes the authenticator's exact bytes to the shared Rust core. Independent Secure Enclave signing keys enable demand-only renewal; local content still requires hardware presence. The UI keeps local password copying available when the online delegation expires, and offers Passkey-confirmed sign-out of all login grants. There is no software-key fallback, session inventory or credential-management subsystem.

The production issuer defaults to the shared production configuration. The welcome screen has one Passkey entry; its next screen distinguishes signing in from creating a separate login identity and points existing accounts to the security binding flow. Signing in to another account hides the current workspace's local unlock action until cancelled. Registration checks readiness and obtains its admission ticket before opening AuthenticationServices, so admission failures do not leave a saved but unregistered credential. A saved credential with unconfirmed registration and a credential absent from the issuer have separate, actionable messages.

The security binding flow offers an existing Passkey before creating another identity. Its progress indicator stays inside the active action. Deployment discovery also finishes before credential creation. A temporary failure keeps the same response and any verified identity while this screen remains open in the same session, so Continue never opens another creation prompt; within the original challenge window it renews an issuer signature that has already expired. Leaving, locking or a background pause that starts a new session generation clears that attempt; Use an existing passkey then links the saved Passkey. Newly created credentials have a short distinguishing label (`dMsg · <8 hex digits>`); existing password-manager entries are unchanged and must be removed there if unused.

Simulator UI fixtures cannot validate a real Passkey origin or Secure Enclave persistence. Build, protocol, simulator and outstanding real-device checks are recorded in [Passkey validation](../../docs/passkey-validation_zh.md).

If a selected Passkey has reached its session limit, recovery pins both sign-out and the following login to that credential. It preserves the current workspace and the original binding intent, including when the current account uses Internet Identity. Ordinary sign-out-all remains a separate operation for the current login. The native `beginPasskey` bridge now carries an optional credential restriction and a `ResetSessions` operation; regenerate the Swift bindings and both Rust slices together before building the app.
