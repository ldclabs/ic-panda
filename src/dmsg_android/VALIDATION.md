# Android implementation validation

## Conversations, bubbles and composer — 2026-11-23

Conversation and letter rows lead with an initial avatar on a tone chosen by
the account ID; participant names in a thread use the same avatar until a
published image loads. Unread rows are bold with a primary time and a badge,
list times are relative (clock, weekday, short date), drafts read in italics
and a missing time no longer shows "—". The conversation bar shows the avatar,
name and "Encrypted conversation" with Search and ⋮; the manual refresh is
gone and a sync error shows once as a banner that can be closed, instead of
repeating under every unsent message. Messages from one sender group with 2 dp
gaps and tucked corners, the time sits at the bubble's tail, a long press opens
an actions sheet, and quotes carry an accent bar. The composer is a filled
capsule with the attachment menu inside, and the send button grows in when
there is text. The letter folder menu is a menu chip; wide windows give the
conversation its own bar. The duplicate "In Trash" string added with the vault
batch now reuses the existing one, and two strings the old dialogs used were
removed.

On the API 36 emulator the conversation list, a thread with unsent messages,
the composer with text, attachments, a published avatar and the letter drafts
were checked in light, the list and a thread in dark, and a thread in Arabic
(RTL, mirrored bubbles and send arrow). The fixture has no received messages
from another sender, so multi-sender grouping and the long-press sheet were
checked only through the code path, not on screen. TalkBack was not rerun.
The attachment button sat 4dp below the text line; it now shares the centre
line with the text and the send button, checked on screen after the suite.
Debug: 138 JVM tests pass; lint reports 89 warnings, no errors and no newly
unused resources. QA: JVM tests pass and the device suite ran 140 tests, 133
passed and 7 skipped.

## Vault list, item, editor and filters — 2026-11-23

Pages can now declare their bar title and actions with `ScreenBar`; on wide
windows the detail pane draws its own compact bar for them, and the activity
bell stays on workspace roots and their scopes. A vault item shows its title as
the large page title, favourite and edit in the bar, and version history,
sharing and Move to trash under ⋮. Fields are grouped rows that copy on tap,
with inline reveal, open and copy icons and a long-press menu for large type
and the field reference; one-time codes show a countdown ring. Item
information uses relative times and shows the full time on a tap. One banner
at most replaces the stacked notices. The editor is a full-screen dialog with
Close and Save in its bar and Show and Generate as trailing icons. The list
gained a Type chip that opens the other types, a subdued sync icon instead of
the black cloud, and a filter sheet with a sort radio list, tag chips, and
Clear filters and Done. Four strings (More, Synced, In Trash, Item information)
reuse the iOS translations; three strings the old layout used were removed.

On the API 36 emulator the item page was checked in light and dark, Chinese,
Arabic (RTL) and 200% text, together with the ⋮ menu, the field long-press menu,
the Type menu, the filter sheet, the editor in light and dark, and a 2560×1600
window. That window first showed two back arrows, no bell once an item was
selected, and the list's FAB over the last detail rows; all three were fixed
and rechecked. The first light capture of the item lacked its bar actions,
because the bar did not follow the page's recomposition; `ScreenBar` now hands
the bar a new wrapper on every recomposition. TalkBack was not rerun.

Debug: 138 JVM tests pass; lint reports no errors and 87 warnings. QA: JVM
tests pass; the device suite ran 140 tests with 7 skipped and 2 failures, both
from the new layout: the Arabic check looked for the year, which Item
information now shows after a tap, and the snapshot test looked for Share
secret snapshot outside the ⋮ menu. Both tests were updated and pass.

## Navigation, page bars and motion — 2026-11-23

Phones show the Material 3 short navigation bar, flat and full width, on
workspace roots; wide windows show the collapsed wide navigation rail. Every
page now brings its own bar inside the page transition: a workspace root shows
its search field as the top bar with the activity bell and ⋮ inside it, an open
item and wide roots keep a compact bar, and every other page has a large title
that collapses as it scrolls. The vault, messages, letters and files declare an
extended FAB (New item, New conversation, Write a letter, Import files) that
collapses after scrolling;
its icon carries the label because the extended FAB hides its text from
accessibility. Pages slide 30 dp along a shared axis and fade through between
workspaces, and the back gesture scrubs the back transition. The busy bar no
longer reserves a strip on every page. The unused "New channel" string was
removed from all catalogs.

On the API 36 emulator, light and dark screenshots of the vault, an item,
Account, Security, a conversation, Files, the filter sheet, a dialog and the
locked page were compared with the previous build. Injected edge swipes showed
the parent page and its bar mid-gesture; cancelling returned to Security and
releasing opened Account. With the animator duration scale at 0, forward, back
and workspace switches completed at once. A screen recording on the swapping
host held about 8 frames per second and could not show the transitions, so
`AccountNavigationFlowTest` now steps them with the test clock: forward enters
from the end, the gesture reveals the parent, cancelling restores the child and
releasing completes. TalkBack order, Chinese, Arabic and 200% text were not
rechecked for this change.

Pages now read their own route: during a transition the leaving Files page
browsed the new route's root folder and reset the library, which the device
suite caught. Letters had four actions inside its search field, so a tap on the
field's centre opened the activity menu; Write a letter moved to the FAB. Debug:
138 JVM tests pass; lint reports no errors and 87 warnings. QA: JVM tests pass
and the device suite ran 140 tests, 133 passed and 7 skipped.

## Component kit and shared icons — 2026-11-23

Shared components moved to `ui/kit/` and pages now compose them: sections with
2 dp gaps and 4 dp inner corners, rows without chevrons, inline banners with
actions at their end, centred empty states, skeleton rows for a first read, and
Material filter and sort chips whose row fades at a scrolling edge.
`scripts/ios/sync-icons.py` now generates all 35 business icons for Android as
well as iOS and checks the Android vault type mapping against the desktop; the
navigation bar, vault types, account rows, approvals, empty states and file
actions use them. Overflow menus use ⋮. `UiConventionsTest` now also rejects row
chevrons, the horizontal ellipsis and shared components defined outside the kit.

`KitGalleryTest` rendered the kit in light, dark and Arabic right to left at
200% text on the API 36 emulator; every visible click target was at least 48 dp,
and the screenshots were reviewed. Chips grow with their text at 200%. Light and
dark screenshots of the vault, Account and Files were compared with the previous
build.

The debug variant passed 137 JVM tests, and lint reported no errors and the same
87 warnings as before. Of 140 QA device tests, 132 passed and 7 were skipped.
`cloudInventoryAndVerificationProgressRemainDistinctFromCompletedTransfers`
missed its progress label once while the host was swapping heavily; it passed
both times it ran alone.

## Design system foundation — 2026-11-23

Colour roles come from HCT tonal palettes seeded with the brand forest and gem
(material-color-utilities 0.4.0). Pages use `background` = `surface` (tone 95
light, 6 dark) and cards `surfaceContainerLow` (99, 10). `ThemeColorsTest` keeps
body text and text on containers at 7:1, accents, links and the snackbar at
4.5:1 and outlines at 3:1 on the page and the card in both modes, checks the
surface ladder, and compares the window, splash and launcher colours with the
Compose theme. `UiConventionsTest` rejects literal colours, corners, sp sizes,
monospace, nested themes and literal page padding outside the theme files.

On the API 36 ARM64 emulator the platform splash stayed until the local engine
had opened and then faded; `am start -W` reported 2.1 s (light) and 1.9 s (dark)
for a cold start, with no spinner frame. `values-night` set the window and bar
appearance before the first frame, and Pixel Launcher drew the new monochrome
layer as a themed icon in light and dark. Light and dark screenshots of the
vault, an item, a dialog, the editor sheet, Account, Security, a conversation,
Files and the locked page were compared before and after. Dark cards now sit
closer to the page (1.08:1 instead of 1.25:1), as the Material tone mapping
gives. Chinese, Arabic and 200% text were not rechecked for this change.

The QA and debug variants each passed 134 JVM tests and lint reported no errors.
All 139 QA device tests ran: 132 passed and 7 skipped. An earlier run stopped at
`deniedCameraRetainsPasteAndDoesNotCreateAnApproval` because the emulator still
held a camera grant from an earlier install, and Android kills the process when
the test revokes it; after a clean install the test passed.

## Workspace search, compact lists and navigation — 2026-11-23

On a 1080 × 2400 API 36 ARM64 emulator at 420 dpi, the first vault row began at
568 px in browse mode and 442 px after tapping search. Its default height was
147 px (56 dp). After selecting 你好 in Gboard Pinyin, the Search action
dismissed the keyboard and kept the query. Hardware Tab and Enter also opened
the editor and submitted a confirmed candidate.
System Back first dismissed the visible keyboard, then closed search on the same
page. Cancel closed an empty search in one tap. At 320 dp and 200% text size,
the Arabic search prompt occupied one line, Cancel stayed reachable, and all
four right-to-left navigation labels fit.

The four Android navigation drawables and iOS template assets are checked
against the same Remix paths by `scripts/ios/sync-icons.py --check`. The Account
label and the navigation instructions that refer to it were compared in all
23 packaged languages. All 123 QA JVM tests and 25 focused API 36 device tests
passed. The latter include actual locale changes for all 23 languages, fold and
large-text layouts, conversation and letter navigation, batch selection of
matching files and the 1,000-record encrypted-vault probe (125 favorites).
Checks used offline fixtures.

## Account switching and screenshots — 2026-11-21

The main workspace allows screenshots for feedback while its window has focus. Its Recent
apps card is black, including when the launcher renders a live window. The separate system-export
authentication window uses `FLAG_SECURE`. Earlier secure-window checks below describe previous builds.

In split screen, dMsg covers its pane when the other app takes focus. Tap the dMsg pane to
reveal it. On the Android 16 emulator, Recent apps can display that same live split-screen
window while the activity remains resumed, so the mask also follows window focus.

Android 12 and 16 ARM64 emulators each passed ten focused instrumentation tests. The system
screenshot action saved a readable welcome page on both. Visual checks covered an open language
dialog, direct entry into Recent apps and entry through Home. Turning the Android 12 screen off
also produced a theme-only task snapshot. The QA variant passed 121 JVM tests and the resource
check covered all 23 languages.

Account flow tests exercised locking and reopening a local workspace, choosing another login,
cancelling, and selecting the saved workspace again. The cloud fixture also retained its
selected account after cancellation. The system Back action waits during an alternate sign-in
and remains available on the join-account page. On Android 16, the alternate-account Internet Identity
action opened the dMsg sign-in page in Chrome. Physical devices and a completed sign-in with
real credentials remain unverified.

## Encrypted vault drafts and lifecycle recovery — 2026-11-17

Implemented on the `a9b371e9` worktree. Vault input now belongs to the account
session and the shared Rust LDK-encrypted checkpoint store. Background request
invalidation no longer closes the editor. The final pending snapshot participates
in the lock barrier; unlocking or reopening the same workspace restores the
active draft, its original revision, hidden-field replacement intent and raw
input. Save consumes the checkpoint in the same transaction as the item version;
discard/commit fences prevent delayed writes from reviving it. Drafts remain local
and are explicitly excluded from device-transfer records.

Validation:

- Shared Rust library suites passed: `dmsg_client` 679 passed / 5 existing ignored,
  `dmsg_mobile` 46 passed. Clippy with `-D warnings`, including the `ui-fixtures`
  feature, passed. Tests cover encrypted SQLite/WAL bytes, restart, account
  isolation, pause/lock barriers, old handles and revisions, atomic rollback,
  idempotent commit, unchanged saves and concurrent item revisions.
- Production-none QA Kotlin compilation and all 111 JVM tests passed. On an
  isolated API 36 ARM64 emulator, all three targeted instrumentation tests passed:
  background plus last-keystroke lock flushing; timeout followed by native-core
  and controller reconstruction from the same encrypted database, then saving
  through the UI; and Activity recreation. Hidden values that were not replaced
  remain intact, and consumed drafts cannot reopen after another background cycle.
- The QA-only restart/unlock helpers require the fixed offline fixture account
  and are absent from ordinary builds. These tests do not authenticate a real
  user. The reconstruction test closes and reopens the native database and
  controller inside instrumentation; it is not a real OS process-kill test.
- Generated native/binding fingerprints were verified by Gradle. Source changes
  during the initial native build were correctly rejected; that build was rerun
  after the core stabilized. Restored-editor and saved-item screenshots were
  inspected. Final results are under `app/build/test-results/testProductionNoneQaUnitTest`
  and `app/build/outputs/androidTest-results/connected/qa/flavors/productionNone`;
  screenshots are under `app/build/reports/vault-drafts`.

Physical devices, live services, process-kill instrumentation, API 31, x86_64,
GMS, Release/R8 and the full Android UI suite were not rerun for this change.
Existing Kotlin/Compose deprecation and redundant-nullability warnings remain.


## Branch review and fixes — 2026-11-17

Reviewed `codex/android-app` at `e92c5a24` against merge base `9488d02e` from local `main`. The checkout was clean before review. Remote refs were not refreshed; this is not a remote-latest claim. Earlier checkpoints below are historical evidence, not checks rerun for these fixes.

- Fixed five defects: retained inactive Google Play receipts bypassing plan admission on restoration; coalesced channel refreshes attempting to reuse their still-owned operation slot; cancelled profile publication leaving a stale in-flight flag; explicit unread choices retriggering automatic read; and the CI Pixel 4 viewport never reaching the adaptive test's 840 dp two-pane threshold.
- Rust library suites: auth 9 passed, client 671 passed / 5 ignored, mobile 45 passed, protocol 95 passed; commerce rerun after the fix: 69 passed / 2 ignored. The new commerce unit regression failed before the fix and passed afterward, covering both cash and App Store conflicts. Two Google Play PocketIC cases passed with the newly built commerce Wasm, including preservation of the existing cash plan and receipt after rejected restoration. The historical-Wasm upgrade case remained ignored. Extracted commerce Candid matches the checked-in interface.
- Strict-dependency ARM64 QA app/test APK builds, production none/GMS Debug Kotlin compilation and 110 JVM tests passed. Both final Lint tasks passed with zero errors (none: 66 warnings / 1 hint; GMS: 65 warnings / 1 hint). Frontend login protocol tests: 10 passed. The resource check passed for all 23 languages with 2070 strings each and complete notification resources.
- On a new, isolated API31 ARM64 AOSP emulator, the full QA XML reports 88 tests: 83 passed, 1 failed, 4 skipped. The failure was the adaptive test waiting for a two-pane layout on the default Pixel 4 landscape width of about 829 dp. After applying the CI script's new 1080×2400 / 420 dpi viewport, that one test passed, including rotation, injected folding, Arabic RTL and system 200% font size. This is a full run plus a focused rerun, not an all-green single invocation. The skipped cases require opt-in hardware, live network, 1000 MiB import or 1000-item scale probing.
- Both new Android regressions failed against the original behavior and passed after repair: coalesced refresh recovery and an open letter's explicit unread choice. The latter injects an acknowledged unread projection into the real client UI; it does not exercise a live inbox service. Existing local profile/avatar QA passed; publication cancellation recovery was verified from the operation lifecycle, not by a live publication test. All-23-language recreation QA passed. Screenshots were visually inspected for local profile/letters, dark vault details, Chinese, long German labels, Arabic RTL and the adaptive large-font layout.
- Final modified-Rust formatting, shell syntax and `git diff --check` passed. This review did not rerun Release/R8/APK/AAB packaging, x86_64 or API36 runtime, the GMS device suite, iOS runtime, physical hardware, live Passkey/FCM/Play/profile operations, complete manual accessibility checks or remote CI. Those release gates remain separate from this review's offline evidence.

Local run logs use the `dmsg-android-review-` prefix in the temporary directory; generated reports remain under `app/build/`. No credentials or private design text are included in this record.

## Pre-review delivery checkpoint — 2026-11-17

Work is now in the primary checkout on `codex/android-app`, HEAD `d39965d9e7c8965ee82f010bf3a6629a179464e6` plus uncommitted changes. The Android linked worktree was removed after source/artifact preservation and a 474-file hash comparison. Earlier entries below are historical checkpoints, not the current gate status. No remote-latest claim.

- Four Debug variants, dual-ABI none/GMS Release R8 APK+AAB, strict dependencies and 110 JVM cases passed. Both Lint variants have zero errors (66/65 warnings plus one hint each). All 23 languages contain 2070 strings, six plural groups and notifications; strict resource checks pass.
- API31 QA: none 82 passed / 2 skipped of 84; GMS 83 / 3 of 86. API36/16KiB QA: none 83 / 3 of 86; GMS 83 / 4 of 87. These are offline synthetic-account flows with real native storage, not live service or hardware acceptance.
- After migration: ordinary Debug 7 cases = 6 passed, one hardware opt-in skipped, including real JNI and public platform/Rust TLS; the current GMS QA APK passed both all-23-language recreation and cross-UID drag on API31. Product source did not change for this focused check.
- Final none/GMS APKs and AABs have ten native libraries each; actual ABI and ELF 16KiB passed, APK ZIP alignment passed. Ordinary fixture exclusion and intentional QA rejection passed. Non-debuggable/backup-disabled manifests and local development APK signatures were inspected. Actual ARM64 AAB split sets were verified and installed on 16KiB.
- Both APKs passed three cold starts on API31 and API36; both actual AAB split sets passed three on API36. API36 black-box probes completed real native read-only public deployment/TLS preflight and opened Chrome. No credentials were entered; no application crash was observed. Startup observations include warm OS caches and are not benchmarks.
- 1000MiB real native local import/verification and a 1000-item/125-favorite pagination probe passed. Standalone 1002-item QA scrolling on SwiftShader measured 16.19% jank, P95 46ms; this is not a passing physical-device performance gate. Perfetto and memory evidence retain that limitation.
- Current shared regression: client 671 passed / 5 ignored, mobile 45; protocol 95 unit + 20 vectors + 3 doctests, commerce 68 passed / 2 ignored and two Play PocketIC upgrade cases. iOS normal simulator signing: 213 unit cases and two screenshot flows passed. No physical iOS evidence implied.
- Explicit unresolved checks: AOSP hardware probe failed with HARDWARE_UNAVAILABLE; R8 in-process instrumentation failed before running any case because the AndroidX tracing runner interface was not preserved. No broad production keep workaround was added. External black-box R8 runtime and separate Debug JNI tests supply narrower, explicit evidence. Formal signing/DAL/Passkey, physical hardware, real cross-platform operations, FCM/Play and full manual accessibility/state/language review remain open. Remote CI was not run.

Ordinary native source fingerprint: `118728e554b2e18dd62b158172f9c0acc4f8791afeaf3fc9dd129b6c76ecf55d`. Developer-signed Release APK SHA256: none `56d828f3aea47b6c6d9643dd5d8ef1799598e8097ee5d1cff2d02b3666cc44b7`; GMS `23d2cba9d9dea1661397f3e2b27b4e20049d5c263fca0527fe78e39c952f62d3`. They are `net.dmsg.android`, version0.1.0, min31/target36, not formal App Signing identities. Local delivery/evidence is in `misc/dmsg-android-delivery/` and `misc/dmsg-android-validation/2026-11-17/main-final/`. The local Chinese summary and external acceptance checklist in misc contain exact artifacts, steps and limits; misc remains ignored.


## 2026-11-15 — P0 and native login work in progress

Base: ic-panda 9488d02e; iOS comparison: 1.2.1 (32). Changes are local and uncommitted. The full 121-task parity scope is not complete.

### Verified

- B01: the published Maven POM uses org.rustls:rustls-platform-verifier:0.2.0. Its version is read from Cargo.lock. Gradle verification-metadata.xml records SHA-256 artifacts; both noneDebug and gmsDebug Kotlin compilation passed before the subsequent Passkey changes.
- Eight JVM tests passed in the first build: exact integer decoding, P-256 DER/low-S verification and rejection, session A→B→A fences, authored draft revision/account ownership. New Passkey/contract cases are being run separately below.
- Shared Rust first round: client::tests 34/34, approval_key 11/11, dmsg_mobile 36/36. Explicit biometric-only policy without an enrollment digest is covered.
- First real noneDebug ARM64 APK built (106,792,712 bytes), installed and launched on the dedicated API 36 Google APIs ARM64 emulator (emulator-5556, 1080×2400, 420 dpi, 4 KiB pages). Activity launch reported 1294 ms; this single debug observation is not a performance benchmark. UI tree reached the sign-in screen through real MobileEngine.open and the embedded build-identity check. Crash log was empty.
- FLAG_SECURE produced a black adb screenshot while the UI tree contained the expected screen. Production privacy is retained; visual QA will use an instrumentation-only window override.
- All three ARM64 shared libraries in that APK (dmsg_mobile, jnidispatch, androidx.graphics.path) have 0x4000 LOAD alignment. zipalign -c -P 16 4 passed. This is static package verification, not a 16 KiB-device runtime test.
- Native Passkey policy: 6/6 protocol, 9/9 dmsg_auth, 11/11 dmsg_client Passkey tests passed; dmsg_mobile cargo check passed.
- PocketIC Passkey suite: 5/5 passed, including a pre-policy 9488d02e Wasm → current Wasm in-place upgrade. Existing salt-derived identity, sessions and delegation survived. Empty-by-default admission, unknown certificate denial, governance enforcement, accepted certificate, policy persistence across another upgrade, removal and separate session renewal were exercised. No reinstall.

### Current implementation boundaries

- HardwareSecret/context replaces ECDH-specific FFI names; iOS record bytes remain unchanged. Android uses versioned RSA-OAEP public wrapping and auth-per-use private decryption. Public wrapping needs no prompt during Client creation; the real private operation is authenticated outside Client. Main digest SHA-256 and MGF1 SHA-1 are explicit, matching the [Android Keystore OAEP guidance](https://developer.android.com/privacy-and-security/cryptography).
- P-256 signing operations are prepared for exact eligible handles before Client execution. Rust rechecks the reviewed request digest and generation. Each operation is consumed once with no prompt inside signAuthorized. Android wallet batches authenticate one operation at a time. Exact final-message precomputation and hardware cancellation behavior are still under review; S04 is not closed.
- Kotlin and native outputs embed a source fingerprint. Generated Java/JNI directories use AGP's variant API. ARM64 startup was verified; x86_64 and intentional stale-artifact refusal remain to test.
- Credential Manager adapter uses the installed APK signing certificate, ES256/discoverable credentials/required UV, exact challenge/origin checks and unchanged signed bytes. Original proof is kept only for an in-memory bounded completion retry. Native provider/device tests are not yet completed.
- Android certificate allowlist is an independent dmsg_auth stable memory ID 4; layouts 0–3 are unchanged. No online canister, DAL or production certificate policy has been modified.
- Strict domain parsing now rejects missing/malformed file, letter and approval fields rather than producing empty/zero values. UI read coordinators and full feature parity remain in progress.

### Reproduction and evidence

Use JDK 21, Android SDK 36 / NDK 28.2.13676358 and CARGO_TARGET_DIR pointing to a local build cache. Logs for this local run are under /private/tmp: dmsg-android-apk.log, dmsg-android-rust-tests.log, dmsg-android-passkey-protocol.log, dmsg-android-auth-tests.log, dmsg-android-passkey-client.log, dmsg-android-passkey-pocketic.log, dmsg-android-passkey-kotlin.log. Temporary logs and generated build products are not committed.

The upgrade test requires DMSG_PRE_ANDROID_AUTH_WASM (built from git archive 9488d02e) and DMSG_WASM_DIR, then cargo test --locked -p dmsg_integration --features pocketic-tests --test passkey -- --include-ignored --nocapture. The normal suite explicitly ignores that test without the baseline artifact; an ignored check is not an upgrade pass.

### Not yet verified

Subsequent Passkey Kotlin changes, complete feature/UI parity, iOS regenerated bindings regression, final release/R8, both ABIs, 16 KiB runtime, physical hardware, real identity/device pairing, backend Android notification delivery, Play billing and publication configuration. All remain open in the parity checklist.

The owner has no existing Android publication configuration or physical test device. Development uses net.dmsg.android.dev. No production certificate, Firebase service or Play entitlement is fabricated. See FIRST_RELEASE_zh.md for setup steps.

Pre-policy auth Wasm SHA-256: 763ad0706b50e37722c162b8ad4f5bff734cb71f8e25a3d4ab7fd7fdea3e28a2.

Current auth Wasm SHA-256: e1c234f91e691ffc6213c5c1aa276341e65edc406fce1a91e8612447b635c180.

## 2026-11-15 — second build and device pass

- Build variants now use environment × push dimensions: production/staging × none/gms. All four debug Kotlin variants compiled. Staging requires a build-supplied deployment JSON; no Intent can replace it. The latest four-way rebuild after subsequent UI changes remains pending.
- 20 JVM tests passed (8 suites): the previous cases plus native Passkey transport, strict list/type/deadline contracts, resource replacement, four independent navigation stacks/process metadata, draft value preservation, template changes and input validation/password generation.
- Both arm64-v8a and x86_64 native libraries built with fingerprint b78551c4ab2648966a4658f782d3f6745381a104b0041b28910a8719e6680a6c. verify-package.py checks every ELF and APK ZIP entry, including JNA and graphics-path. All 6 libraries passed for debug and release APKs.
- A copied, deliberately stale BuildIdentity.kt was rejected by the verifier. The actual generated bindings were not edited during this negative test.
- Instrumentation 2/2 passed on API 36 / ARM64 / 4 KiB: real JNI startup/build identity, Activity recreation sharing one controller, old scoped accounts() rejected after lock while a fresh handle succeeds, pre-cancelled native work rejected, FLAG_SECURE present and welcome-screen capture under an instrumentation-only override.
- The first instrumentation attempt incorrectly expected sessionState() to be scoped. It is intentionally controller metadata; the test was corrected to use accounts(), verifying ordinary account access. The failed attempt was not counted as a pass.
- ProductionNone Release APK built with R8/resource shrinking and native symbols; ProductionGms Debug APK built; ProductionNone Release AAB built. Release source APK/AAB are unsigned. A temporary APK signed with the existing Android Debug certificate was installed solely for local Release runtime checks. This is not production signing or a published build.
- That R8 APK reached the welcome screen through real native startup; crash log empty. Single debug-signed Release launch observation: 1583 ms (not a benchmark). Its no-account screen and UI tree were inspected. Runtime x86_64 remains untested on this ARM host.
- Actual II preflight returned UNAVAILABLE in this environment. Browser registration/authentication did not complete. A redacted native logcat adapter and a same-core host preflight probe are being used to distinguish network/TLS/deployment failures. TLS/real login are not marked passed.
- Official API 36 16 KiB ARM64 image download failed once with a ZIP-read error and is retrying. Static 16 KiB alignment is verified; actual 16 KiB runtime is not yet verified.

### iOS regression

The Swift bridge and both Apple XCFramework slices regenerated, and Xcode simulator build passed. The unit run with code signing disabled had 210 passed / 3 failed; all failures were NotificationKeychainTests with missing-entitlement OSStatus -34018. A targeted rerun of those 3 tests with simulator ad-hoc signing enabled passed 3/3. These are two recorded runs, not a claim that one unsigned run passed all 213. UI tests and physical Apple hardware were not rerun.

Later changes to the privacy-safe Android logger and hardware-error categories passed 3 diagnostic Rust tests and 36 mobile tests; their rebuilt Android artifacts are being checked. The latest logger-only change is after the recorded iOS build/test and needs a final shared regression.

### UI and state work still under construction

The vault now uses browseVault full-query counts/types/favorites/tags/sorting/paging, and vaultItem for details; the two-card favorites limit and summary-as-detail path are removed. Memory-only editor drafts preserve hidden values versus explicit clearing, include all templates/field kinds/tags/files and local generation, and clear on lock/account changes. Four stacks retain opaque routes through rotation. Per-resource operations/read stores reject stale replacements and show loading/empty/stale/error separately. Foreground content-sync and hints drivers use the Rust scheduler; manual requests no longer announce unverified SYNC_COMPLETE. These changes still require the broader feature, device and UI fixture matrix before their parity tasks close.

Package hashes and the no-account screenshot are retained in misc/dmsg-android-validation/2026-11-15 (local ignored artifacts). Newer source changes invalidate these package hashes; rerun packaging for the final code.

## 2026-11-15 — native HTTPS blocker resolved

The host core successfully verified 10 production deployment targets, and Android's HttpsURLConnection accepted the same public endpoint. The published verifier returned Revoked with a missing-OCSP-responder error. Root cause: the app's own res/xml/network_security_config.xml shadowed the matching AAR resource, removing the upstream CCADB revocation-host rules. Removing that duplicate resource restored signed CRL retrieval while retaining system trust, hostname, date and revocation verification. The manifest still uses the dependency's HTTPS-default network configuration. This matches [upstream's documented Android migration](https://github.com/rustls/rustls-platform-verifier/issues/221).

Four live instrumentation cases then passed on API 36 / ARM64 / 4 KiB: ordinary account scopes invalidated by lock, cancellation/startup/rotation, secure-window screenshot policy, and the explicit live probe (Android HTTPS → published verifier → Rust beginLogin with 10 verified public deployment targets). NetworkSecurityPolicy assertions deny HTTP for dmsg.net, api.dmsg.net, icp-api.io and an arbitrary host, and allow only sampled CA revocation hosts from the dependency's list. No account was authenticated, no browser approval or business operation was submitted. The native core fingerprint for this pass is 35826fe16d09ce56dd998c435ce31a5f5d9caa8be7991e7a35d766538a5c259b.

The live probe is opt-in via -Pandroid.testInstrumentationRunnerArguments.networkProbe=true. The regular offline device suite skips it and must not report a skipped probe as a TLS pass. Full II callback/Passkey device authentication and final R8 retest with this manifest fix remain separate gates.

The API 36 16 KiB SDK download failed twice with a truncated/invalid ZIP. A direct official archive download is in progress; its published length and SHA-1 must match before installation. No 16 KiB runtime pass is claimed yet.

## 2026-11-15 — stamped composer, iOS regression and 16 KiB runtime

- Native fingerprint 7db266f884c37e9bad98ad1f06a435484603b4039c9a10d58f2557e9558e8a4e includes writer/revision MessageDraftStamp and atomic composer checkpoints. Three new core cases cover stale saves after send, original-intent retries and a new process writer; all seven outbox-filtered tests and 36 mobile tests passed. Kotlin freezes text plus immutable attachment version on Send and serializes local draft writes. Letters/feedback and attachment-selection restoration remain unfinished.
- Latest matrix (dmsg-android-stamped-qa.log): 20 JVM tests, four debug Kotlin variants, ProductionNone Release/R8 and 4 live instrumentation checks on the 4 KiB emulator passed. Native FFI smoke includes ULong.MAX_VALUE revision marshaling while locked.
- Regenerated iOS bindings and all dMsgTests passed: 213 passed, 0 failed, 0 skipped on iPhone 18 Pro / iOS 27 simulator, with ad-hoc simulator signing. This supersedes the earlier unsigned Keychain-entitlement failures. No iOS UI or physical-device test is implied.
- Installed official Google API 36 16 KiB ARM64 image revision 7 after verifying 1,875,087,037-byte archive and official SHA-1 dda632745023567113d9954101709c6526a3c250. Dedicated emulator reports PAGE_SIZE=16384, model sdk_gphone16k_arm64. Four instrumentation checks passed, including opt-in live platform TLS and Rust deployment preflight (dmsg-android-16k-tests.log, 10.511 seconds).
- Current unsigned ProductionNone Release APK was signed only with the local Android Debug certificate for runtime QA, installed on the 16 KiB emulator and launched (single observed TotalTime 1819 ms, not a benchmark). UI-tree-selected Internet Identity action completed actual Rust preflight and opened Chrome. Chrome's first-run terms were not accepted; no identity login occurred. JNI/JNA/R8 startup passed, no crash was observed.
- Subsequent signing cancellation changes require another native build and relevant regression before their validation can be claimed. These phase artifacts do not represent final feature completeness or a production-signed release.

## 2026-11-15 — domain workspaces and queued-letter lock race

- Extracted MessageCoordinator, LetterCoordinator, FileCoordinator, ApprovalCoordinator and FeedbackCoordinator; removed aggregate content arrays and obsolete preview branch from UiState. Reads retain explicit stale/error states; message form state survives navigation. Five letter folders, original-ID delivery state/actions, file folder navigation/search/details/versions and feedback editor now call actual native operations.
- Before subsequent fixture-build changes, Kotlin application/instrumentation compilation and existing 20 JVM tests passed (dmsg-android-workspaces-compile.log). This is compile evidence, not full feature acceptance.
- Full dmsg_mobile unit suite: 38 passed, 0 failed, 0 skipped (dmsg-android-workspaces-rust.log). New regression cancels a batch during a blocked signature and checks that no second authorization opens. Another test queues a letter, locks with a late editor plus another unfinished draft, unlocks only through the test fixture helper, and verifies the original queued text plus the other draft survive without revival.
- Background/explicit lock now includes letters and feedback. Lock treats an existing sealed letter intent as superseding a stale editor; a pending/sent feedback report likewise supersedes its editor. Final messages, attachments, all lifecycle races and physical authorization still require broader evidence.
- Dedicated qa build type uses ui-fixtures Cargo feature and isolated generated/native directories/cache. It is currently under validation; ordinary Debug/Release must not contain the fixture constructor. Fixture UI tests are not account login, delivery or production safety proof.

## 2026-11-15 — isolated native UI fixtures and settings

- Separate ProductionNoneQa build passed 27 JVM tests and 6 device tests on API36 ARM64/16 KiB; the opt-in network probe was explicitly skipped (7 discovered, 1 skipped; Gradle's extra finish counter is not an additional pass). Latest log: dmsg-android-transfer-qa.log. Three workspace cases exercise real native offline storage: list→vault detail→lock concealment, UTF-8 letter editing→sealed native draft and navigation restoration, and nested folder/conversation composer navigation. They do not authenticate accounts, simulate server success or verify real hardware.
- QA native fingerprint: aa556193744922e240d92a6f2307e62042929f966932b2b896c104e3bf2d0002. Feature ui-fixtures is compiled only for build type qa, with its own generated bindings/native/cache directory. Native constructor refuses an existing database directory and uses unreachable test endpoints. Normal Debug/Release do not enable it; updated production package isolation proof is running separately.
- Domain ownership now includes FileTransferCoordinator with operation progress/cancellation and unconditional native preview-close attempts, plus AccountCoordinator and ApprovalCoordinator. B04/B07 implementation gates are closed on the native/fence/operation/Compose evidence above; physical and full cross-account scenarios remain open in Q03/Q04.
- Account identity/hardware/recovery/devices have typed models and dedicated controls; administrator permissions and recovery deadline/expiry boundaries have JVM tests. Approval receipts preserve a pending original request, only dropping it after a read confirms a final state. Binding now routes the returned approval instead of discarding it. Browser binding retries retain the exact callback only in memory, scoped to native generation and a bounded deadline. Real binding/hardware/recovery was not performed.
- Screenshot QA caught default Material purple surface roles and insufficient system-bar contrast. Both themes now supply the complete surface roles and set status/navigation icon appearance. Verified screenshots: misc/dmsg-android-validation/2026-11-15/workspaces/dmsg-qa/{qa-vault-light,qa-vault-detail-light,qa-vault-detail-dark,qa-letter-drafts,qa-conversation}.png. SystemUI appearance is captured after its asynchronous frame; no production screenshot privacy flag was disabled. The Compose fixture Activity belongs only to test code.
- Tablet message/file split panes have been implemented, but tablet/foldable and font-scale evidence is still pending. These pages and settings are not full parity acceptance; 23-language migration, richer flows and remaining advanced/platform domains are still open.

## 2026-11-15 — ordinary package isolation recheck

- Ordinary production/staging × none/gms Debug compilation, 27 JVM tests, ProductionNone Release/R8 and 4 live instrumentation tests on16KiB passed (dmsg-android-production-isolation.log). Production native fingerprint de494ca391c6ae64f0c3ac84d1847c4218e126111f8ca649cb16e45d0378ac08 differs from QA; both ARM64/x86_64 libraries present.
- Inspected both ordinary Debug and Release APKs: no open_ui_fixture native symbol, no openUiFixture or WorkspaceFlowTest DEX entry. Evidence: misc/dmsg-android-validation/2026-11-15/fixture-isolation.json. Release all6libraries passed ELF LOAD/ZIP16KiB verification in that phase (historical summary: release-pre-qr-summary.json; release-current.json now names the newer build); this phase APK is96,460,012bytes, SHA256a5082b25dbe11ac13c8836b0f15b882551de3e88c8a0e2ff388987fbe735a849.
- Locally debug-signed Release installed on16KiB, cold launcher start observed923ms (single observation, not benchmark), real native account-free screen loaded and crash buffer empty. UI-tree-selected II action completed Rust deployment/TLS preflight and opened Chrome first-run. No terms accepted or real login performed. The earlier am-start attempt merely foregrounded an existing Chrome child task; the actual cold-start observation used MAIN/LAUNCHER with a fresh app task.
- Subsequent vault-history/OTP/conflict UI work began after this checkpoint. Final APK/AAB and full feature acceptance still require current-source checks.


## 2026-11-15 — QR access flows and local secret tools

- Shared QR encode/decode tests: 2 passed (dmsg-android-qr-tests.log). CameraX 1.6.2, bounded image import and native luma decoding are wired to QR target/session ownership. Permission-denial initially failed because the system permission Activity paused Compose; the test now inspects and dismisses the actual system permission node, then waits for Compose. Recheck passed (dmsg-android-qr-qa-recheck.log: 33 JVM, 9 device passed + 1 opt-in network test skipped).
- Latest dmsg-android-secret-tools-qa.log: 36 JVM tests passed; qa/flavors/productionNone XML reports 11 discovered, 10 passed, 1 network probe skipped, 0 failures. The separate Debug XML contains an earlier 4-test run and is not added to this result. Native QA fingerprint remains a2c1f69ce878ac623a8f360d55235f36b2225e7367d35a41ab06027d7888f9e6. Native source/bindings did not change for these Kotlin secret-tool pages.
- New device flow uses the real encrypted offline database: field reference contains no plaintext, starting a rotation persists the original revision, unchanged credential refuses the created step, changing the actual field permits created/tested statements, and the local report retains the checklist. No provider request was made; these are synthetic owner statements, not provider evidence.
- Added strict models and dedicated pages for local attachments/references/dependencies, health observations, ordered rotation steps and paginated secret activity. Missing attachments remain visible; malformed reports fail instead of appearing empty; unsigned 64-bit observations remain exact. The current local dependency digest is sent for the references_checked step; core rechecks version/digest for later completion.
- Manually inspected qa-secret-reference, qa-secret-health, qa-secret-rotation and qa-camera-denied screenshots under the local misc validation folder. Current reference/health/rotation pages conceal secret values. Camera denial keeps text and image fallback. Real camera recognition, cross-device pairing, physical secure hardware, full localization/accessibility and real deliveries were not tested.
- Checked approval cancellation ownership: Host drops cancelled work and calls abandon_host_work, clearing the pending Authorized object before accepting another operation. No extra asynchronous cleanup was needed. Final-message preparation for S04 remains unfinished.
- The last ordinary Debug/Release and iOS regression runs predate the QR FFI additions. Rebuild/regression is required before treating current sources as release validated. No production signing, Firebase, Play or physical device is configured.


## 2026-11-15 — selected snapshots and bounded approval ownership

- dmsg-android-secret-sharing-qa-recheck.log: 39 JVM tests passed; 12 QA device tests discovered, 11 passed, 1 network probe skipped. Initial run had one return-navigation failure: an approval opened an extra list route. ApprovalCoordinator now pushes the original approval directly and returns to its initiating task. Full rerun passed.
- Real offline snapshot flow: all fields initially unselected; selecting only password creates a local prepared snapshot without plaintext in the preview. The original request is reused, no publication is attempted before approval, and returning to the composer opens the same prepared ID. Manually inspected qa-secret-compose and qa-secret-approval screenshots. Link publication/revocation and receiving real channel/link packages remain untested.
- Secret snapshot forms retain fixed item revision and file versions; cancellation before a result leaves the form requiring inspection of saved jobs. Import retries retain the original selection and package digest. Secret-package channel attachments open the dedicated preview/import path instead of ordinary plaintext attachment export. Approval details validate and show only selected fields/files, destination, expiry and retrieval limits; malformed or mismatched selections fail closed.
- Timeline/outbox snapshots now carry channel/query ownership; Files carries folder/query/trash ownership. Pages exclude a different resource before LaunchedEffect refresh, avoiding stale content flashes during navigation.
- Native key lifecycle regression: 13 approval-key tests passed (dmsg-android-key-lifecycle-tests.log). New unpersisted keys have an ownership guard; cancelled proof/session-invalidated completion attempts deletion of its unreferenced provider object. After committed metadata pruning, old unregistered aliases are deleted; the registered key remains. Deletion failure is best-effort and crash-time orphan inventory is not yet implemented.
- Native attestation regression: 19 tests passed (dmsg-android-authorization-attest-tests.log). Mobile suite: 39 passed (dmsg-android-authorization-mobile-tests.log), including original-request expiry bounded by monotonic time and the 90-second maximum. The original deadline applies before/after hardware signing; an expired post-network attestation cannot begin signing. Exact final-message staging across every approval type remains unfinished (S04).
- Platform callback deleteSigning was added for Android named approval keys. The iOS wrapped-key provider has no independently named object; its adapter/test double were updated, but bridge generation and full iOS regression are pending. Android QA native regeneration and the foreground-picker ownership regression are running in dmsg-android-authorization-qa.log; do not count them as passed yet.


## 2026-11-15 — current native QA and password import

- dmsg-android-authorization-qa.log completed successfully: 39 JVM tests, 12 device tests passed and 1 network probe skipped on the dedicated API36 ARM64 16KiB emulator. New QA native fingerprint: 9f3e925d4f9e1f69d51641e9fbb5c22e8d5a4a43e386d4e29a2d955b4bd51137. This includes deleteSigning and original-request expiry changes. The external-result regression proves a picker callback waits for foreground and an old callback cannot cross a subsequent lock.
- dmsg-android-vault-import-qa.log completed: 41 JVM tests, 13 device tests passed, 1 network probe skipped. Native fingerprint unchanged for this Kotlin increment. Password export staging is capped at 64MiB; preview exposes metadata only, selections start empty and are capped at1,000. A device test changes the staged CSV and verifies digest rejection, restores it, imports only the selected row, checks the real native vault and confirms staging cleanup. qa-password-import.png was manually inspected. No AutoFill export is enabled by import.
- Uncertain vault-import commits retain the original staged handle, digest and selection; confirmed receipts alone release the source. Import staging now attaches coroutine cancellation to CancellationSignal and stream closure; lock/account invalidation retains its existing synchronous I/O fence. Individual nonresponsive-provider interruption and full supported export-format matrix still require further validation.
- Actual QA APK was rejected by the default package gate because of fixture entries in classes6.dex and lib/arm64-v8a/libdmsg_mobile.so. Explicit --allow-ui-fixtures accepted its ELF/ZIP alignment and recorded those entries. Ordinary packages must pass the default gate. Changing this verification script updates the native build fingerprint.
- dmsg-android-complete-native-matrix.log is running: ordinary Debug JVM/device network checks, both ABI Release APK/AAB/R8 and all four debug variants. Results are not yet claimed. The previous Release artifacts and the prior iOS213 test run are historical, not current-source verification.


## 2026-11-15 — locked ordinary matrix, installed AAB splits and current iOS regression

- First ordinary matrix stopped at dependency-lock conflicts after CameraX changed app runtime constraints. Regenerated affected ordinary locks with verification still enabled (dmsg-android-native-matrix-lock-refresh.log), then reran WITHOUT --write-locks and with Gradle --offline. dmsg-android-native-matrix-locked.log passed: 41 JVM, 5 device tests including live network preflight, all4Debug variants, bothABI Release/R8 APK and AAB. No skipped device tests in this ordinary run.
- Current ordinary native fingerprint c1d04decc0a002d33b082f79efd3d84d23790419b53304dd8a77180889e9e65a. ReleaseAPK SHA25680a7da04e0f29e5997260a15987522849f396c5a9fc49bc5c78e8ec5a3b894d5,98,285,260bytes. AAB SHA2568a201fbdc64a9033d1687a9f1cd6c9ec782cc08b90826d680faf8a5f041c9ff8,155,320,980bytes,unsigned. Actual Debug/Release10native libraries (including CameraX image/surface libraries) pass16KiB ELF/ZIP checks; default package gate finds no fixture API/class/native marker. Immutable phase reports: {release,debug,bundle}-snapshots-imports.json.
- AGP's already verified bundletool1.18.3 validated AAB. Its library JAR does not embed aapt2; initial build-apks failed, rerun with installed SDK36 aapt2 succeeded. Device-spec generation and real AAB split build produced base-master/base-arm64_v8a/base-en/base-xxhdpi APKs. All5ARM64libraries in those splits pass16KiB ELF/ZIP, no fixture entries. Actual install-multiple and cold launcher run passed on PAGE_SIZE16384; welcome screen and four installed package paths were observed. bundle-splits-current.json records the split hashes. These were signed only with the existing local Android Debug certificate, not Play/store signing.
- Ordinary ReleaseAPK cold-process launch1495ms, AAB-split cold-process launch1675ms: single observations, not benchmarks or a startup performance gate. The APK's native II deployment preflight opened Chrome's first-run screen; no Chrome terms accepted and no real identity authenticated. No app crash was observed during these steps.
- Regenerated Swift binding and both Apple XCFramework slices (dmsg-android-ios-bridge-current.log). XcodeBuildMCP test_sim with -only-testing:dMsgTests,CODE_SIGNING_ALLOWED=YES,CODE_SIGN_IDENTITY=- passed213/213,0failed,0skipped on iPhone18Pro/iOS27.0 simulator. Current build log: test_sim_2026-11-15T13-28-31-120Z_pid45641_9a3dee7f.log; result bundle test_sim_2026-11-15T13-28-31-123Z_pid45641_8a284c90.xcresult. Covers current QR/deleteSigning/shared changes; no iOS UI/physical hardware test implied.
- Full121-task parity, S04 final-message staging, provider/hardware,23languages and complete UI/performance gates remain unfinished. Later source changes require their applicable checks again.


## 2026-11-15 — formal attestation message preparation (S04 partial)

- Added an opaque prepare/sign/commit path for mobile type=attest approvals that require a hardware-key answer. It keeps the request awaiting_user while reading certified state and freezing the actual Ed25519/P-256 protocol messages, original deadline and sequence. Authentication runs without a Client borrow. Commit rereads account state, rejects changed version/security epoch/device sequence/request expiry, and checks that no other local journal owns the execution ID. The exact prepared journal and executing approval are committed atomically before sending.
- Shared PreparedHardwareProof authenticates a named key only after its final hardware commitment is frozen; verification and existing lost-key/anchor classification run on completion. Existing desktop paths reuse the same unsigned-attestation constructor. No protocol or canister layout changed.
- Mobile RunningApproval now retains the owned decision mutex until cancellation cleanup finishes; an old cleanup cannot remove a later attempt's live marker. Reconciliation's wait for that mutex now falls inside OperationToken cancellation. Approval key-policy preflight moved to Hold network steps, so that network wait releases Client before the request is checked again.
- dmsg-android-prepared-attest-regression.log:20attestation tests passed, including frozen bytes/deadline, no journal before authentication and stale account/epoch/sequence/local-journal rejection. dmsg-android-prepared-proof-tests.log:15approval-key tests passed, including exact-message proof verification and expired-plan refusal before prompting. dmsg-android-prepared-attest-mobile.log:40mobile tests passed, including cancellation cleanup retaining serialization. First test compilation caught two slice arguments and three test calls needing the new explicit time argument; fixed before the passing runs.
- Final Hold-preflight adjustment passed40mobile tests in dmsg-android-prepared-preflight-mobile.log. This increment postdates the successful Android41JVM/5live/R8/AAB and iOS213 phase above; those artifacts/regressions do not yet cover it. Account/device/principal/connect final-message preparation and real hardware/cancellation integration remain open; S04 is not complete.


## 2026-11-15 — prepared account changes and operation ownership (S04 partial)

- Prepared account approvals now cover device capabilities, revocation, normal/succession pairing, account security/login/recovery changes, controller changes and approval-key maintenance. New local keys prove the fixed registration context first; the replacement mutation is then constructed with that proof before its separate authority signature. Compound operations keep the approval unknown until their existing root rotation/login acceptance follow-up completes. Moved-workspace and nonhardware decisions retain their existing paths.
- Each prepared approval keeps its own encrypted original account mutation and receipt state. Reconciliation queries its exact ID and validates ID, digest and resulting account version; unknown delivery never generates new bytes or prompts again. A later operation in the global account journal cannot satisfy or overwrite this receipt. Confirmed results cannot be downgraded by a late transport failure.
- The first added persistence regression exposed nested SQLite transactions in the new account/attestation commit paths. Replaced save_approval with the existing write_approval transaction participant and added an actual attestation-journal commit test. The full client run then exposed a missing composer-checkpoint transfer classification; composer checkpoints and approved-account journals now explicitly stay on their originating device.
- dmsg-android-prepared-account-regression.log:648passed,0failed,5ignored. This supersedes the preceding run with647passed/1failed/5ignored. No ignored test is claimed passed. This passing run predates the next platform callback changes.
- In progress: prepareSignature/signPrepared/releasePrepared callbacks bind one fixed message to a native operation before prompting; a host cancellation guard revokes only that operation even while a blocking worker still owns it. Android uses a single-use Signature CryptoObject; iOS retains the same fixed message with its LAContext. Android wallet batches use the same operation ownership per message. dmsg-android-fixed-signature-mobile.log is running; generated bindings/Android ABI/iOS tests are not yet updated for this interface.
- Platform reference checked: [Android BiometricPrompt CryptoObject](https://developer.android.com/reference/android/hardware/biometrics/BiometricPrompt.CryptoObject) and [KeyGenParameterSpec authentication parameters](https://developer.android.com/reference/android/security/keystore/KeyGenParameterSpec.Builder#setUserAuthenticationParameters(int,%20int)). Physical TEE/StrongBox, PIN/biometric and cancellation evidence remains required.


## 2026-11-15 — complete prepared signing paths and cloud workbench increment

- Browser authenticate/signDocument/signAction now freeze their final messages before hardware authentication. The proved execution waits in the encrypted connect operation until the original relay request is pinned to this device; only then is the executable journal activated atomically. Pending/foreign/cancelled pins discard the preparation with the corresponding review state. Original-ID reconciliation can resume after cancellation or lost replies without a new signature. Active foreground decisions are excluded from mailbox pin reconciliation.
- New-key request and successor proofs also prepare a single native signature before their blocking worker starts. Async cancellation guards release only their operation. Android refuses legacy authorizeKeys/signAuthorized calls carrying unspecified future messages; no supported normal hardware approval falls through that route. Moved-copy requests with an incompatible current hardware answer fail closed. Physical key policy, biometric/PIN, queued-cancellation behavior and full browser/cloud integration remain device/service gates.
- Full client regression:652passed,0failed,5ignored in dmsg-android-prepared-all-core.log. Latest mobile regression:41passed,0failed in dmsg-android-prepared-all-mobile.log. Initial connect compilation caught private-method visibility and a tuple-style StatementContent test constructor; corrected before the full passing run.
- Both Android ABIs and Kotlin bindings regenerated: b4deaf39f291bf2807661b77dd7242d40fb2b6e7957d8e3c460ad5b0f82198eb (dmsg-android-s04-bridge.log). Both Apple bridge slices regenerated. iOS dMsgTests:213passed,0failed,0skipped,138.5s on the existing iPhone18Pro/iOS27 simulator; log test_sim_2026-11-15T15-28-40-091Z_pid45641_923bb687.log, result test_sim_2026-11-15T15-28-40-092Z_pid45641_54261fb2.xcresult. The bridge packaging subprocess printed sandbox CoreSimulator connection warnings, but returned success; the plugin's actual simulator test passed.
- Android now has strict CloudInventory/BackupCapacity/BackupAdmission/DurableFileJob models, a separate cloud catalog with missing-key/unknown-observation states, explicit destination and cloud deletion, verified backup admission, and a foreground driver for already-requested uploads/downloads. restoreBackup only queues a persistent job; the new runDownloads driver now advances it. Transfer display preserves verifying/committing vs done and original upload IDs. Provider/cloud network scenarios remain unverified.
- Removed automatic activity refresh from generic perform calls, so read polling and hints cannot postpone idle locking. MainActivity user interactions and editor changes still count as activity. A clock-driven QA test was added. No plaintext enters navigation state. JSON u64 decoding now rejects quoted numbers; monetary strings remain their separate string contract.
- Ordinary matrix currently running in dmsg-android-s04-cloud-matrix-fixed.log with locked dependencies/offline Gradle. Initial run caught a missing forEach closing brace in the new cloud screen; fixed. JVM/device/release results and QA screenshots are not yet claimed.

- Ordinary matrix finished successfully:44JVM tests and5live16KiB instrumentation tests passed, fourDebugvariants compiled, bothABIReleaseAPK/AAB/R8 built (dmsg-android-s04-cloud-matrix-fixed.log). This phase has the new prepared-signature FFI and cloud UI.
- Subsequent Kotlin-only adjustment adds visible per-direction pass failures and monotonic15s–5min backoff when a transfer pass fails before it can record a per-job retry. Manual retry/network recovery permits another attempt; background clears cloud/job metadata. QA run is in progress in dmsg-android-s04-cloud-qa.log. This last adjustment is not yet covered by an ordinary Release build.


## 2026-11-16 — S04/cloud package closure and next feature work

- Isolated QA:44JVM passed; device XML reports16tests/0failures/1skip, hence15passed. The skipped test is opt-in network probing (already passed in the separate ordinary5-case live run). The runner's “Finished17” line is not used as the test count. Full XML was overwritten by a focused run before preservation; cloud-workbench/qa-full-summary.json is explicitly a summary of observed counts.
- Focused QA after cloud display changes:2passed. The extended version-navigation case first exposed missing test setup (the offline fixture has no library file); after importing a real sample via FileProvider and the normal staging/encryption flow, the case passed. A missing requested version offers no share action until the person explicitly selects the current version. Saved XML:cloud-workbench/qa-version-navigation-results.xml. Four final synthetic screenshots were pulled and visually inspected:cloud inventory, unavailable content key, verifying after all bytes arrive, and missing requested version. No remote cloud data path was exercised.
- Current ordinary package closure:44JVM,4DebugKotlin variants,ReleaseR8/APK/AAB passed in dmsg-android-s04-cloud-final-package.log. Native fingerprint remains b4deaf39f291bf2807661b77dd7242d40fb2b6e7957d8e3c460ad5b0f82198eb. APK99,415,756bytes SHA256e44268a92e38ed98886e6817b23323708db128782ea7f5072b51831b8a02c61a; AAB156,586,466bytes SHA2563c4df9f3a9521e663c9288850b902e1a7dca8f866b505b3e534342b2d5c636f1. All10APKnative libraries pass16KiB ELF/ZIP alignment; ordinary package contains no UI-fixture markers. Immutable reports:release-s04-cloud.json,bundle-s04-cloud.json,bundle-splits-s04-cloud.json.
- Built four device APKs from the actual AAB with the already verified AGP bundletool; all5ARM64 libraries and split packages passed validation. Installed monolithic APK and then four AABsplits on emulator-5558, PAGE_SIZE16384. Both reached the real welcome/InternetIdentity sign-in screen; no account was authenticated. Cold-process launch observations:APK1403ms,splits975ms, neither a distribution nor a performance benchmark. Only the existing local Android Debug certificate was used; no Play/production signing or deployment.
- Subsequent feature work will again invalidate its affected artifact evidence. Remaining checks include real hardware/PIN/biometric, browser and cloud business flows, plus the still-unfinished121-task parity scope.


## 2026-11-16 — durable file share and collection links

- File share creation now freezes the selected version and reviewed terms locally; retries resume the original edition/upload/share rather than calling a fresh create. Request-link batches prepare all recipients atomically, then persist each original publication command, key envelope and terms before network submission. Lost responses are reconciled against the original request and immutable fields; a changed root cannot silently republish an old-key request. Original creation records are device-local and sealed.
- Full shared regression in dmsg-android-file-links-requests-core-tests.log:659clientpassed/5ignored,41mobilepassed. Seven new core tests cover review freezing, discard races, original-command retries, mismatched receipts and root-generation changes. Relay doubles exercise command preparation and persistence; they do not verify production certificates or service behavior.
- dmsg-android-file-links-requests-qa.log:49JVMpassed; device XML19tests/0failures/1skip =>18passed. The skipped test is the opt-in network probe, not an implicitly successful test. Native QA fingerprint efa9e08d7f4323e24974c6b6d7cd84785fae0db1d1d8df9cecbb04420d79ce60, ARM64. Complete XML and summary are preserved under misc/dmsg-android-validation/2026-11-16/file-links-requests.
- Three new device scenarios use normal FileProvider staging/encryption, real sealed creation intents and repeated offline original IDs; request display fixtures exercise unreadable/withdrawn states, manual save selection and a downgraded creation allowance without automatically importing any content. No real cloud request, publication, file download or deletion was performed.
- Screenshot review caught the Compose QA Activity using different IME window behavior from MainActivity. Aligned resize policy and Scaffold inset consumption; dmsg-android-file-links-requests-ime-qa.log passed3focusedtests. The focused-input bring-into-view adjustment passed2tests in dmsg-android-file-links-ime-focus-qa.log; six final screenshots were visually inspected, including the password field above the open IME. Production FLAG_SECURE remains enabled.
- Previous ordinary dual-ABIRelease/AAB and iOS213 results predate the new file-link/request FFI. They are phase evidence, not validation of these later source changes.


## 2026-11-16 — complete composer snapshots and multi-attachment queue

- Full authored snapshots now include text, fixed library versions and reply references. Queueing commits all selected messages and the consumed composer in one transaction. A separate sealed writer/revision receipt retains original ordered IDs even after a newer draft or partial delivery. Core tests exercise actual disk restart, historical versions, missing-second-file rollback, changed selections, queue capacity and a newer editor following a lost queue reply.
- dmsg-android-full-composer-core-tests.log:662clientpassed/5ignored and41mobilepassed. MobileMessageDraft now carries attachment/reply data; save/lock flush uses that complete record. sendMessage uses a per-channel lane inside cancellation. iOS's three legacy text-only MessageDraft call sites were updated; generated Swift bridge and iOS tests still need rebuilding for this interface.
- dmsg-android-full-composer-qa.log:52JVMpassed. Initial device XML21tests:19passed/1failed/1skipped. Failure was the new test locating the absent placeholder while the field already held a saved draft; no product failure was inferred. Fixed the selector and dmsg-android-full-composer-focused-qa.log passed52JVM+2focuseddevice tests. Initial and focused XML retained in misc/dmsg-android-validation/2026-11-16/message-composer. Counts are not added as if all cases were newly independent.
- QA ARM64 native fingerprint0e30d593a2225969d9ea06b442e65d6479d033062cc45b8acf5af7a71c2c44d8. Two screenshots were pulled and visually inspected: two attachments stay above the keyboard before Send; both original outbox messages remain while new typing is preserved. Normal FileProvider import/encryption and real native queue were used; remote delivery was unavailable.
- Capture cleanup test writes a synthetic JPEG through the app's scoped URI, locks, verifies the file is no longer readable, and discards a late result. It is not a physical-camera test. Imported sources now require content:// URIs; raw file paths cannot enter the provider intake. Physical camera, photo picker/provider behavior, cross-route/rotation external results, real network ACK recovery and full release/iOS artifacts remain separate gates.


## 2026-11-16 — verified file previews and root-owned picker results

- File previews validate the complete native version before reading plaintext, constrain source paths to the native workspace, and bound text/image memory. Explicit external viewing uses a verified read-granted copy; PDFs/other unsupported inline formats use the selected external app. No in-app PDF/video renderer or image zoom is claimed. Export validates the source before writing a selected content URI, checks the owner throughout, closes streams off Main on cancellation, and attempts removal of incomplete new document targets.
- Found a real lifecycle flaw in the newly added screen-owned ActivityResult launchers: the privacy cover removes those screens. FilePickerHost now registers document/photo/camera/permission/export callbacks unconditionally at the DMsgApp root, while results remain bound to the original account session. The actual DocumentsUI cancellation-through-cover test passes.
- The QA native workspace is now nested under the normal app workspace boundary; no production-path validation exception was added. The password-import test's former relative-parent staging assumption was corrected to use paths.staging. An import-FAB screenshot was visible but its text was absent from merged semantics; added an explicit accessibility description and tested through it.
- Initial run had24devicecases with2failures and1skip; focused attempts and reasons are retained. Final dmsg-android-file-preview-full-qa.log passed52JVM and actualXML24devicecases/23passed/1skipped. Final XML and summary are preserved under misc/dmsg-android-validation/2026-11-16/file-preview; this supersedes the initial selector/path failures.
- Native text preview preserves UTF-8/emoji; export to a test FileProvider yields identical bytes; tampering with an actual encrypted blob prevents plaintext and the verified label; PNG sampling remains within1600px. Three screenshots were pulled and visually inspected. A subsequent small UI adjustment clears the previous success notice before a fresh preview; next feature regression will include it.
- Third-party providers, blocked IO/cancellation, external PDF apps, physical camera and200MiB per-type matrix remain unverified. Native QA fingerprint remains0e30d593a2225969d9ea06b442e65d6479d033062cc45b8acf5af7a71c2c44d8 for this phase.


## 2026-11-16 — file management, historical MIME and received attachments

- Added directory breadcrumbs/cache/preferences, typed metadata editing with field-value CAS, selected version/current-head checks, per-item batch results and guarded local trash/purge. Breadcrumbs return to an existing ancestor route instead of pushing another root. Shared version records now preserve their own MIME; unknown historical MIME does not borrow the newest file's type. Android and iOS display models were adapted.
- `dmsg-android-file-history-final-core-tests.log`:663 client passed/5 ignored and42 mobile passed. Product changes are covered; the later QA-only saved-message fixture also passed its native feature check.
- Initial `dmsg-android-file-management-qa.log`:52 JVM passed;27 device cases,25 passed/1 failed/1 skipped. The old folder test still addressed the removed Parent folder button. Replaced it with the accessible All files breadcrumb and asserted the actual navigation stack. Final `dmsg-android-file-management-final-qa.log`:52 JVM passed;27 device cases,26 passed/0 failed/1 skipped. Gradle's console prints an inflated finishing count; counts here come from XML.
- Evidence: `misc/dmsg-android-validation/2026-11-16/file-management/summary.json`, original/final XML and three pulled/visually inspected captures. QA ARM64 fingerprint `c85baad70b70cec0e380e501d41cfa54e1b48be7e7e655fdcb141690a5f7dbec`.
- Device coverage includes metadata conflicts preserving authored note while merging untouched updated tags/favorite, fixed historical text/PNG types and current-version switching, partial move failure preserving selection, refusal to purge a live folder, recursive trash purge, and local saved-message attachment preview without a download. Message metadata is synthetic; actual encrypted files/local lookup/verification are used. Remote delivery, old-epoch failures, physical hardware and full updated release/iOS artifacts remain separate gates.


## 2026-11-16 — channel management and participant identities

- Shared channel create/join/control/resume now use Hold; native control lanes serialize per channel. Durable creation intents keep the original name/type/channel/nonce/request across failure and restart. Original invitation terms/control payload survive retries. Successful leave is separate from the historical cached ledger, so Android does not retain publish/invite controls from that history. Full Rust `dmsg-android-channel-final-core-tests.log`:665 client passed/5 ignored;43 mobile passed.
- New typed Android settings, timed mute, hidden/show restoration, role controls and confirmations; account-bound create/invite/join review, QR/copy/share and exact channel Intent routing. Participant names use core certified caches, avatars use bounded core-authorized reads, full IDs remain inspectable. Known-account selection never requests phone contacts. Composer UTF-8 limit corrected to the core's16,000 bytes.
- Initial `dmsg-android-channel-full-qa.log`:55 JVM passed;30 device tests,27 passed/2 failed/1 skipped. One test addressed an uncomposed lazy-list item; changed it to scroll the actual list. A duplicate global error Snackbar covered the lower creation recovery action; form errors now remain inline and avoid that obstruction. Final `dmsg-android-channel-final-qa.log`:55 JVM passed;30 device tests,29 passed/0 failed/1 skipped.
- Evidence and initial/final XML: `misc/dmsg-android-validation/2026-11-16/channels/`. Four captures were pulled; member identity was inspected during the initial run, and creation retry/compact invitation review/archive confirmation were visually checked after the final run. QA native fingerprint `0fd4ab8f0d0684fd12dc4ea2476869296f5b58f0405ae9aabd45b23a1a7bc4ff`.
- Device coverage is real local mute/name/hidden state and sealed creation retry/discard, with synthetic membership and name-cache fixtures. No cloud channel mutation, actual invitation acceptance, identity certificate verification or physical-device interoperability is claimed. Pending invitation discovery after process death, all external-link lifecycle conditions and refreshed ordinary Release/Swift artifacts remain outstanding.


## 2026-11-16 — receiving policy, local blocks and report review

- Added typed cached receiving rules (missing differs from known absent/closed), policy confirmation preserving other fields, known-account selection, local block/unblock confirmation and separately retriable online synchronization. Message, reference and inbox rows honor the local block list. Letter detail supports new/existing-conversation invitations and reporting; session-owned letter navigation retains folder/query/selection.
- Native setBlockedAccountLocal commits local intent before network work. reportDetails reopens the immutable selected payload for explicit review; submitReport uses a per-report lane. UI prepares once, reviews disclosed text, then submits that original ID. Account reports cannot include a message body; attachments/keys are excluded. New core disclosure test passed in `dmsg-android-safety-core-tests.log`; this was one targeted test, not another full client run.
- `dmsg-android-safety-full-qa.log`:57 JVM passed; actual device XML32 tests,31 passed/0 failed/1 skipped. The two new device flows verify local blocks remain applied when remote sync fails and explicit unblock cancellation retains them; selected report content excludes message text, failed submit remains pending, retry uses the same ID, and saved report detail reopens exactly the original note. The test endpoint is unreachable: no moderation report or real online policy mutation was sent.
- Evidence: `misc/dmsg-android-validation/2026-11-16/message-safety/`, with full XML, summary and two pulled/visually inspected captures. Native QA fingerprint `cd12cbe0867ae0d146da7a948e7645e1521f54d2555f144f8d4a313d403ef940`. A subsequent UI-only change stores submit errors inline and further fences a report from another target; its focused result is recorded separately below.

- Follow-up result: dmsg-android-safety-report-footer-qa.log passed57 JVM and1 focused device case. Earlier focused runs failed before the native prepare call: captured semantics showed report state Idle, no running operation, scroll reset to0, and Review below the IME viewport. Focus restoration had moved the button after a scroll. The review action is now fixed outside the scrolling form and clears focus when invoked. The test physically clicks it with the keyboard open, without a pre-click screenshot delay. Same-report reload keeps its prior snapshot, submit errors remain inline, and untouched drafts show no fake loading indicator. Failure screenshot and focused XML are retained; the earlier diagnostic pass is not presented as the final fix.


## 2026-11-16 — account workspace, public-profile drafts and invitation recovery

- Added typed account list/name cache, profile editor/publication states, local-only risk summary and display-cache controls. Avatar intake is root-registered and owner-fenced; ImageDecoder applies orientation and resizes, a new JPEG strips metadata, and upload only follows explicit publication. Existing native publication journal/status/retry remain authoritative.
- Full dmsg-android-account-full-core-tests.log:667 client passed/5 ignored;43 mobile passed. Added original invitation discovery before POST and actual disk-reopen recovery. The later verified-own-Accept projection fix also passed the same full counts in dmsg-android-membership-core-tests.log.
- Android dmsg-android-account-full-qa.log:60 JVM passed;35 device cases,33 passed/1 failed/1 skipped. The failure initially matched the title as well as the confirmation button. After selecting the actual clickable node, a targeted run exposed a real bug: locking before removeLocalAccount invalidated the core's unlocked-account precondition. Removal now cancels/conceals the UI while the native transition validates and removes the original account. dmsg-android-account-removal-fixed-qa.log passed60 JVM and1 focused device case, using a fresh scoped handle to verify the account disappeared.
- Other new device cases passed: orientation6 JPEG1600×800 becomes256×512 and stays≤512KiB; chosen photo/name remain local; navigation retains the profile draft; clearing display cache preserves an actual encrypted library file; an invitation whose form was lost is recovered by its original sealed ID. The public-profile cache is synthetic and online publication remains unavailable.
- Evidence: misc/dmsg-android-validation/2026-11-16/account-workspace/ with initial/focused XML, summary and two pulled/visually inspected captures. QA native fingerprint1ab41a3ba04b30c70f66866d10dc792f1b49174963fe99ffd0e1def79c60aa0b predates the small own-Accept projection fix. After viewing the portrait fixture, avatar displays changed from Fit to Crop inside the circular frame; this UI polish and the later offline-about/privacy additions await their next Android run.

## 2026-11-16 账号、离线隐私与完整产物回归

- 共享核心全量：dmsg_client 667 通过、5 忽略；dmsg_mobile 43 通过。本人重新接受频道邀请后解除本机 left 状态，不将陈旧列表当作恢复权限。
- Android：普通及 QA JVM 各 60 通过；最终设备 XML 为 36 项，35 通过、1 联网探针跳过、0 失败。初次本机移除失败与修复后的单项/完整复跑均保留，不累计重叠用例。
- 头像编辑器展示完整归一化图片，头像圆形裁剪仅用于身份展示。实际 EXIF 旋转/去元数据、草稿不自动发布、本机移除风险/取消/删除、锁定后隐私/账号元数据通过。23 份政策来自 iOS 公开资源，不代表 Android UI 已完成 23 语言。
- 普通源码指纹 64b4fea3a1a2b458fa580f064504a52d49f839730b7d445c290c1e1f632c818c；QA 指纹 6c29171c0b9b57b151063c84511424245c101f6247441a517a87bd23e8a06754。四 Debug 变体与双 ABI Release APK/AAB 构建通过。普通 APK 10 个原生库 ELF/ZIP 对齐 16 KiB，无 QA 标记。
- API 36 / emulator-5558 / 16 KiB：普通 Release APK 安装、冷启动 754 ms；bundletool 生成设备分包后安装、冷启动 581 ms。只验证无账号欢迎页启动。使用本机 Android Debug 证书，非正式生产签名。
- iOS：当前共享核心重新生成 device/simulator 桥接；构建启动通过；dMsgTests 213 通过、0 失败、0 跳过。未重跑全量 iOS UI 测试。
- 证据：misc/dmsg-android-validation/2026-11-16/account-workspace/ 与 release-account/。真实服务发布、Play、FCM、原生 Passkey 和硬件验收仍未通过。

### 2026-11-16 会员、通知偏好和提醒中心

最终 Gradle session 53027 成功：66 JVM；39 项设备 XML = 38 通过、1 联网探针跳过、0 失败。四张截图已 pull 并检查，证据保存在 misc/dmsg-android-validation/2026-11-16/membership-notifications/。真实核心 CAS、独立账号同意、冲突后保留输入/明确重试、精确版本已读、稍后提醒、全部已读不推进 observed 和锁定清理通过；会员目录和提醒来源是合成数据，不是远程权益/投递验收。免打扰单测覆盖跨夜、夏令时重叠与跳时。

钱包 W01–W05 开始：暂存 WalletModels/Coordinator/Screens 已复制进当前 worktree，后续页面仍在实现，尚未编译/验证；服务删除 C08 等依赖后续钱包读取。N02/N04/N05、P/A/D/E/G02–G04/U/Q 等继续推进，整体 121 项未全部完成。

## 2026-11-16 钱包阶段与删除增量

钱包首轮 70 JVM、40 设备通过、1 联网探针跳过。覆盖 u128/本地数字精度、未知余额、原提案编号的保留、Viewer 不能提案与未核验地址不能复制；两张截图已检查。数据是合成展示，无真实资金变动、canister 交互或硬件协签证明。

随后增加服务删除依赖核对、固定钱包操作栏、恢复成员控制入口和 60 秒预览期限测试。编译/JVM 成功，设备运行在 4/42 时模拟器 shell/logcat 无响应；仅重启本任务端口 5558 的 AVD，保留磁盘。中断日志/报告保存在 wallets/deletion-interrupted/，不标记通过，正在重新验收。

## 2026-11-16 浏览器连接、审批及服务删除回归

- 最终 dmsg-android-connect-approval-final-qa.log：76 JVM 通过；设备 XML 43 项，42 通过、1 联网探针跳过、0 失败。证据：misc/dmsg-android-validation/2026-11-16/connect-approvals/summary.json、final-jvm/、final-device/；QA 原生指纹 a280970d2bae9724103474131f70bc87d791081bca2129f6dbe411220a641939。
- 原生系统相机拒绝用例初次失败：测试未等 Compose 分发点击，且触摸可能处于权限窗口进入动画。测试现在等实际权限窗口返回应用才认定处理完成；未绕过应用权限。原失败截图/树和重跑证据保留。
- 截图复核发现 SelectionContainer 多个直接子节点重叠，八处 ID/摘要容器改为 Column。dmsg-android-approval-layout-qa.log 定向 1 项通过，新增 ID/摘要上下边界断言。qa-approval-final-review-fixed.png 已查看，完整标识与摘要可读。
- 真实本机审批快照/取消和系统权限拒绝已验证；外账号链接不新增审批。钱包、目录、删除状态为合成数据；没有执行真实资金动作、服务删除、在线浏览器配对或硬件审批。
- 新 Agent 分页核心检查随后完整通过：dmsg_client 669 通过/5 忽略，dmsg_mobile 43 通过（dmsg-android-agent-core-tests.log）。先前命令重复 --lib 被 Cargo 拒绝，修正后运行上述完整测试；旧产物不作为此增量当前产物证据。

## 2026-11-16 Agent 与设备替换

- Agent 全量 dmsg-android-agent-full-qa.log：79 JVM、44 设备通过、1 探针跳过。实际本机身份创建/暂停/移除、待审批权限、托管密钥不可显示/分享、Principal 读取失败不误判未启用通过；三张截图已检查。未执行真实目录发布或委托服务变更。
- 换机初次全量 81 JVM 通过；设备 47 项有 1 失败、1 跳过。诊断取得真实 native 状态后确认会话 ID 是 16 字节，界面错误地套用了设备 ID 的 32 字节校验；分开校验。随后定向测试的旧 native handle 在 resume 后被正确拒绝，测试改为取得当前 generation 的 handle。
- 最终 dmsg-android-handover-final-qa.log：81 JVM 通过；设备 XML 47 项，46 通过、1 联网探针跳过、0 失败。真实接收 socket 在退后台/锁定后停止，原 16 字节会话可恢复；错误替换令牌不创建审批。两张截图已检查。
- 两阶段 QA 源码指纹 4524b06f7f9a61d617411413a2e072cefdd8ea2ba37c3e9fd2190fa333782299；证据 agents/、handovers/。没有将接收器生命周期等同 Android↔iOS/桌面文件互通、真实钱包交接或真机 NSD 权限验收。

## 2026-11-16 受限后台密文传输

- F10 使用 Android 14+ 用户发起的数据传输 Job，旧系统使用可中断 JobScheduler 任务；工作进程只读取独立密文目录、固定 scoped HTTPS URL 和原 id/attempt，不持有或创建 MobileEngine。前台原账号解锁后再接纳分块、完整验证和提交。
- Kotlin 编译通过；首轮设备测试编译引用了不存在的 libraryPage，改为真实 files 绑定。最终 dmsg-android-cipher-final-qa.log：84 JVM 通过；设备 XML 48 项，47 通过、1 联网探针跳过、0 失败。
- 3 项存储测试覆盖 URL/大小边界、分块完整性、安装身份、重启、旧 attempt 完成拒绝、有限重试、到期和字节完成不等于提交。真实 API 36 JobScheduler/JobService 用已到期的测试授权拒绝开始 HTTP 连接，原生文件库没有变化；不声明实际云传输成功。
- 证据 cipher-transfers/；截图已检查。发现普通前台队列说明和“无待处理任务”会与后台任务冲突，随后修正文案/空态并增加断言；此项后续定向结果另记。真实 cloud grant/上传下载、杀进程/重启、旧系统 Job 及 1 GiB 网络性能尚未验证。
- Agent 分页对照服务实现后进一步修正：最后页允许省略 next_cursor；result 仍必需且必须为数组。此前测试只覆盖显式 null 末页，不能证明实际省略字段兼容。当前已补省略字段测试，正在重跑共享核心和产物。


## 2026-11-16 共享核心、产物及系统入口增量

- 末页游标修正后，dmsg_client 669 通过/5 忽略，dmsg_mobile 43 通过；iOS device/simulator 桥接构建成功，当前工作区 iOS 测试 213 通过、0 跳过。证据 release-handover-cipher/shared-core-ios.json。
- 普通变体矩阵 dmsg-android-handover-cipher-normal-matrix.log：84 JVM、四个 Debug 变体 Kotlin、arm64-v8a/x86_64 Release R8 APK/AAB 通过；APK 十个 ELF 与 ZIP 16 KiB 对齐，无 QA 入口。实际 16 KiB 模拟器安装 APK/四个 AAB splits 并冷启动，单次 1348/883 ms。使用本机开发证书，未验证正式签名、真实账号或商店分发；这些产物早于以下 E01/E04 Kotlin 增量。
- 外部分享接收：独立安装密钥加密元数据及分块，10 项/单项 100 MiB/队列 200 MiB；明确账号、文件夹后才导入。测试覆盖实际外部 UID ContentProvider、源/摘要重复导入、锁定提示、自有私密 URI 拒绝和阻塞提供方时取消/清理不死锁。此前全量 87 JVM、49 设备通过/1 跳过；修改后定向 88 JVM、2 设备通过。证据 incoming-shares/。
- 当前全量 dmsg-android-workspace-entries-full-qa.log：89 JVM 通过；设备 XML 53 项，52 通过、1 网络探针跳过、0 失败。含独立小组件/快捷方式和锁定后导航用例；QA 原生指纹 91a6fa9037dcdfeb56cd7e9a1927e5c3e7672a3a48a54499a6c70630e958a0cd。证据 workspace-entries/。截图初次在 RemoteViews 绘制前取得空白帧，补实际布局/绘制等待后定向 2 项通过；不把初始空白图作为布局验收。
- 系统自动填充、文档提供方、全部窗口/无障碍、生产推送与商店购买等仍在实施，以上不代表 121 项全部验收。


## 2026-11-16 独立系统副本与离线策略回归

- 初次 dmsg-android-system-exports-full-qa.log：95 JVM 通过；57 设备项为 55 通过/1 失败/1 跳过。失败在副本页首次状态载入：hardwareSecurity 会刷新联网账号权威。改为共享核心 system_export_policy 本机策略读取，绑定材料缺失仍拒绝；Android 与 iOS 调用方同步更新。首轮 Rust 编译缺少 Hold trait 导入，修正后移动核心 44 项全部通过。
- dmsg-android-system-exports-offline-qa.log：95 JVM 全通过；设备 XML 57 项，56 通过、1 联网探针跳过、0 失败。QA 源码指纹 82c9f8429e96b6d3d7f01561fa8601213e795eb6b63a3a6649bee3e90e856985。两张副本设置截图已查看；没有敏感值展示。
- 后续加入独立认证页搜索/期限、空态按钮与跨账号快照核对。dmsg-android-system-exports-platform-qa.log：95 JVM 与 5 项定向设备测试通过。真实系统 Autofill 请求只给独立认证入口，缺少副本密钥拒绝交付；测试外部表单处于另一 UID，口令保持空。未替换生产认证路径，也未宣称真实生物认证/成功填充已测。
- DocumentsProvider 使用真实 Binder/ProxyFileDescriptor 验证只读、头部读取、锁定后新打开/未缓存尾部拒绝；另一个 UID 无系统 grant 的查询被拒绝。独立认证 Activity 保留 FLAG_SECURE，Application 主 Client 初始化状态不变。文件访问会话由测试内合成密钥建立，单独的认证边界仍待真机。
- iOS device/simulator 桥接、安装启动通过；Xcode test_sim 213 通过/0 失败/0 跳过（79.9 s）。日志 test_sim_2026-11-16T01-28-53-562Z_pid45641_860c7499.log，结果包 test_sim_2026-11-16T01-28-53-562Z_pid45641_52919a34.xcresult。
- 本阶段证据 misc/dmsg-android-validation/2026-11-16/system-exports/summary.json。普通双 ABI/R8/Release APK/AAB 尚未针对这次 FFI 与系统组件重新构建；之前产物仅代表其对应阶段。API 31/4 KiB、实际硬件认证、生产签名与真实网站应用联调未运行。


## 2026-11-16 提醒导航、实际查看与原生通知

- 97 JVM 与可见性定向用例先通过；首次完整设备 61 项为 59 通过/1 失败/1 探针跳过。恢复详情布局回调反复触发 notification_reference，原生状态变化又触发布局，产生真实刷新循环。改为可见性/焦点/生命周期/实际内容版本变化驱动，未放宽测试超时；随后 97 JVM 和三项定向设备测试通过。
- 最终 dmsg-android-notice-presentation-full-qa.log：99 JVM 通过；设备 XML 63 项，62 通过、1 联网探针跳过、0 失败。QA 原生指纹 207280bfdcd94a308436ef2b0326707b141b4f23fdcd7de3522e0f5a7a3a02a0。共享 dmsg_client 670 通过/5 忽略、dmsg_mobile 44 通过（dmsg-android-notice-final-core-tests.log）。
- 实际 Native 策略消费合成提醒投影，应用内提示和收起保持 read/observed 不变；窗口被弹窗覆盖时不提交查看，恢复焦点并覆盖消息水位才提交。错账号保持当前账号、原目标可保存/恢复/锁定等待，未知路由不创建审批；恢复详情完整摘要不匹配时不展示新请求的取消动作。
- 实际 NotificationManager 两次提交同一 system_id 后仅一组，私密正文与锁屏通用副本分开，固定类别不改变既有 importance/sound；没有初始化另一个主 Client。只对临时 QA 包授予 POST_NOTIFICATIONS；运行后 Gradle 已卸载该包，adb pm path 无返回，测试授权随包清理。
- 23 语言通知资源生成与 --check 通过；四张最终页面图已查看。证据 misc/dmsg-android-validation/2026-11-16/notification-navigation/summary.json。没有执行真实 FCM、服务注册、正式签名、真机 Doze 或云端推送。
- 新增恢复摘要字段在最近 iOS 213 通过之后；该字段不改变 FFI 方法形状，但后续共享产物/普通双 ABI/R8/APK/AAB 与 iOS 构建回归仍需刷新，不能引用此前产物为当前版本证明。


## 2026-11-16 Android 安装证明接缝

- 测试通知后续 dmsg-android-notice-test-button-qa.log：99 JVM、2 项定向设备通过；确认系统提交反馈、测试通知固定设置入口及前台提示。不是整套设备重跑。
- 公开 Android 安装证明合同与向量提交 e63962bb76d62f3456d3444afde3eb985b4ffe70；绑定字段逐项变更的 Rust 向量测试通过。仅该独立合同提交，其他开发成果保留在当前 worktree。
- 共享注册新增 Android provider 引用、能力判断、独立安装标识与 Android 配置/challenge/register/unregister FFI。两次初始 Rust 编译分别发现 Zeroizing 解引用和错误类型转换，修正后 dmsg-android-provider-native-tests.log 为客户端 670 通过/5 忽略、移动核心 44 通过。
- 配套私有 Android 服务独立 worktree 完成安装证明、修订撤销、内部 Google 网关与 FCM data-only 投递；本地 Google Worker 6 项、Core Android/通知/Apple 重放 36 项定向通过。首次全 workspace 的一个原有文件同步测试受并发超时，10 项定向复查通过；限制 workspace 包并发后完整 format/check/test/build 通过（Core 544，Google Worker 6）。不透明登记/投递 ID 和容量/期限补充后再次完整验证通过。之后新增设备在 Google 调用期间被撤销的用例，Android 定向文件 6 项和 Core 类型检查通过；未把它宣称为完整 545 项已重跑。
- Android GMS 接收/配置 UI 已接通，后续记录见下；真实 Google/FCM/Play 项目和真机仍未提供，未验收。


## 2026-11-16 GMS/none 接收与冷启动验证

- Play Integrity 1.6.0 来自官方 Google Maven，AAR SHA-256 e6a0a877245b1f8c59aeccfa6c2b9bc07a7e44b551e3a372ce091533976b5188。更新依赖锁和校验元数据，移除未使用的 lifecycle-process。第一次 QA 单元测试受旧 QA 锁记录阻塞，按 QA 配置更新后 production/staging GMS Kotlin 和 none 的 101 JVM 通过。
- 初次完整 dmsg-android-google-full-qa.log：101 JVM；设备 XML 65 项为 63 通过/1 失败/1 跳过。新增两项后台接收用例通过；原系统自动填充建议文字已出现但父节点暂不可点击。改为在原 12 秒内重新取得当前可点击节点，不延长超时、不改变产品认证；dmsg-android-google-focused-qa.log 的 101 JVM/6 设备通过。
- 最终 dmsg-android-google-final-qa.log：101 JVM，XML 65 项为 64 通过/1 联网探针跳过/0 失败（Gradle 尾行 Finished 66 不能当作测试计数）。native 指纹 cb8f1cd753d4ff1620899b7827847a66ea3be18636f6ea2475ca6b19116059eb，生成当前 arm64 QA GMS 包。
- dmsg-android-google-gms-qa.log 的 GMS 实包 4 项通过：Firebase 自动 Provider 已移除，无配置时 SDK/主 Client 不初始化，FCM Service 非导出；真实 NotificationManager 分组/私密副本/重复投递静音。接收租约和 provider 结果为测试数据，不是真实 Google 证明或远程推送。
- 独立安装当前 GMS QA，实际 MainActivity 冷启动未知 opaque 路由只显示提醒不可用；杀进程再启动保留目的地，关闭后再次重启保持欢迎页。启动分别 1319/1378/1164 ms，仅功能冒烟。未读取或操作真实账号；测试安装随后卸载。查看了本轮通知设置截图。
- 证据 misc/dmsg-android-validation/2026-11-16/android-push/summary.json，含初次失败、定向、GMS、最终 XML 与冷启动 UI 树。正式配置/证书/真实 Play 安装/FCM/Doze/生产部署均未完成。普通双 ABI/R8/Release APK/AAB 与 iOS 213 回归仍停留在之前阶段，不能引用旧产物作本次证明。

- 构建配置补充：传入合成 service_account JSON 时，Gradle 在生成 BuildConfig 前按预期拒绝，日志未回显测试标记；默认无配置的 none 101 JVM/GMS Kotlin 仍通过。此检查避免误把服务账户文件嵌入应用，不含真实凭据。


## 2026-11-16 — Google Play purchase and recovery

- Production none QA: 107 JVM tests passed; focused connected XML: 4 tests, 0 failures/errors/skips. A direct instrumentation replay of the two purchase presentation tests and real Android Keystore ledger test passed (3 tests).
- Purchase review uses real SDK price/offer models; fixture screenshots are explicitly synthetic and never charge or grant membership. Captured light review, recoverable failure and dark pending payment were visually inspected. Fixed duplicate progress semantics below an open review sheet.
- GMS Kotlin compilation and Billing 9.1.0 dependency locks/checksums passed before the final shared presentation fix. Final complete GMS/package regression remains due.
- Shared native: 671 client tests passed (5 ignored), 45 mobile passed. Public commerce/protocol: 68 commerce passed (2 capacity ignored), 95 protocol unit, 20 vector and 3 doc tests passed; clippy passed. Two actual PocketIC tests cover authoritative delivery and old commerce Wasm upgrade, including deferred linked subscriptions and original storage periods.
- Private counterpart completed its format/type/test/Worker dry-run suite; no private implementation or raw private logs are included here. No live Play, FCM, Integrity credentials, store installation or physical device was available.
- Evidence: ignored local `misc/dmsg-android-validation/2026-11-16/google-play/final-device/`. QA package is `net.dmsg.android.qa`; normal `net.dmsg.android` is a separate earlier release installation. Connected Gradle uninstalls QA after tests, so screenshots were captured with an explicit install and direct instrumentation replay.


## 2026-11-16 — Localization foundation, navigation and adaptive layout

- Replaced all `l(en, zh)` calls with Android string resources; a checked offline generator reuses public iOS vocabulary and tracks missing translations. 23 locale configurations and six plural groups are present. Translation coverage is still being completed; resource resolution does not prove full language coverage.
- Language picker in welcome/account; framework LocaleManager from API 33, installation preference/configuration context on API 31–32. Wallet amounts use exact BigDecimal localized formatting; new JVM test round-trips full uint128 values in five locales without rounding.
- Shared hinge-aware list/detail layout now includes account pages. Two on-device geometry tests cover vertical hinge/RTL index order and horizontal fold/200% text. These are synthetic fold bounds, not physical fold hardware. Navigation saves integer scroll anchors by original account, including recreation and late disposal.
- Automatic OTP display no longer exposes the code in accessibility text; explicit field actions remain.
- Initial targeted run: 108 JVM and 4 device tests passed. Complete updated run: **109 JVM passed; connected XML 72 total = 71 passed, 1 network probe skipped, 0 failures/errors**. Gradle's “Finished 73” is not the XML test count.
- Local evidence: `misc/dmsg-android-validation/2026-11-16/adaptive-localization/first-pass/` and `full-none/`. QA package is uninstalled by the connected test runner after completion. Current localized resources and Kotlin changes still require ordinary dual-ABI Release/AAB and iOS shared bridge revalidation before final delivery.

### Android 12 compatibility and accessible selection (2026-11-16)

An AOSP API 31 ARM64 emulator reports 4096-byte pages. Three targeted drag/share tests pass, including a real cross-UID OS drag from a non-exported provider, revocation of the temporary read grant after encrypted intake, and explicit-import/deduplication checks. Evidence: `misc/dmsg-android-validation/2026-11-16/adaptive-localization/api31-drop`.

The first complete API 31 run found two runtime API mismatches (`InputStream.readNBytes` and `JobInfo.Builder.setMinimumNetworkChunkBytes`) and an unreachable test target in the lazy password-import list. Reads now use bounded, short-read-safe chunks on Android 12; minimum network chunk hints are guarded at API 33. The import action remains visible below the scrolling list. 110 JVM and eight focused device checks passed. A second full run reported 76 tests: 74 passed, one channel-creation interaction failed, one opt-in network probe skipped. Scrolling the create action into view before clicking fixed that interaction in the focused follow-up; a new full final run is still due.

Checkbox/radio selections now expose one labelled row, including file copies, message attachments, browser trust, wallet signers and addresses, history choices and appearance. Two dedicated selection tests pass (200% text, keyboard, selected and disabled state). Switching Simplified/Traditional Chinese, German and Arabic through the real language picker preserves the QA client, account and account-list position across Activity recreation. These are offline fixture flows; full OS screenshot rendering and RTL geometry remain under review. Do not use the prematurely captured transition images as screenshots of the resulting screen.

The Android localization catalog has 2049 strings. English, Simplified Chinese, Traditional Chinese, German and Arabic are complete; the other 18 languages still lack Android-specific translations. Plural categories and offline bundle language delivery have been corrected. Full strict localization/Lint acceptance is not yet complete.

Current API 31 full regression after these fixes: XML reports **110 JVM passed; 81 device tests, 79 passed and two opt-in probes skipped; zero failures/errors**. The console's changing “81/83” progress count is not used as the result count. Evidence: `adaptive-localization/api31-full-current`. The separate 1000 MiB native local-file probe passed: 21,459 ms staging/import, 16,478 ms verification/private copy, 80 memory samples, peak PSS 175,713 KiB. See `performance/api31-1000mib/README.md` for exact environment and limitations.

Actual window RTL is fixed on Android 12 by applying the Activity configuration's layout direction to DecorView before Compose content. Navigation geometry, four representative language changes, real Activity rotation, injected WindowInfoTracker hinge bounds and system 200% font fallback pass. Valid localized images use Compose PixelCopy roots in `api31-window-integration/locale-screens`; final fold/large-text images are in `final-window-screens`. They exclude system bars and do not claim physical fold-sensor validation.


## 2026-11-17 — Current iOS reference, account navigation and localization

- Regenerated current iOS device/simulator artifacts and verified the fingerprint. Normal simulator signing: **213 unit tests passed**, zero failures/skips. English/Chinese native screenshot flows: **2 passed**, ten attachments exported. An initial unsigned run had three Keychain entitlement failures; signed focused and full reruns passed without a production Keychain change. Evidence: `misc/dmsg-android-validation/2026-11-16/ios-regression/`.
- Account now groups identity/security, sync/storage and settings. System integrations and advanced tools have their own routes. Profile/membership share a compact header and stack with large text; unknown membership is not shown as Free. Lock is fixed in the account toolbar; approvals include the current count. File scopes and workspace search controls are consistent. 110 JVM and three focused API31 tests passed, including nested return paths, real language recreation and injected fold/200% font handling.
- Full API31 none XML: **82 tests = 80 passed, 2 opt-in probes skipped**. Initial GMS XML: **83 = 80 passed, 1 failed during fixture startup, 2 skipped**. The fixture had a fresh native database but inherited installation `pending_workspace` from a previous test. Fresh test preferences remove that cross-test route. The complete GMS rerun passed **81 of 83, two skipped**, and its log confirms the carried key was removed before the formerly failing attachment test. Production pending-route persistence was not changed. Evidence: `2026-11-17/account-navigation/{full-initial,gms-isolated}`.
- Subsequent template-label regression: **110 JVM and seven device tests passed**. A separate date-digit follow-up: **two device tests passed**. All 29 built-in field names reuse public iOS vocabulary; custom labels remain verbatim. Secret detail, editor, comparison and share selection use the same names, and technical field values keep LTR ordering in Arabic. Arabic date digits are asserted in the actual UI. Four localized secret-detail screenshots were inspected; synthetic passwords remain concealed. Evidence: `account-navigation/{template-labels,localized-dates}`.
- Catalog now has **2070 strings**. Eight complete languages: English, Simplified/Traditional Chinese, German, Arabic, Spanish, French, Japanese. Fifteen languages still need Android-specific text; Indonesian is in progress. Format/source freshness and notification checks passed at their recorded checkpoint; strict completeness correctly failed. Do not report the release localization gate as passing.
- Added a secret-free Android PR workflow and local `scripts/android/verify.sh` stages. Bash syntax and workflow YAML parse passed; remote CI was not run. APK/AAB inspector checks all native ELF files and fixture absence; APK ZIP alignment is checked separately from bundle contents. Inspection of an earlier ordinary dual-ABI APK and AAB passed for ten libraries each; those files are explicitly historical, not the final current-source packages. Native fingerprints include actual producer scripts, not test runners or package inspectors.
- Local review board: **61 Android stage images, five current native iOS comparisons and 121 task entries**. Protected runtime check (28 files) and TypeScript/Vite build passed. In-app browser verified comparison switching, iOS image/source/SHA modal and C01 filtering with loaded images. It remains a review artifact, not proof of real-service, physical-hardware or full task-state acceptance.

Final current-source four-variant/dual-ABI/R8 APK+AAB, API36/16KiB refresh, the remaining language coverage, full strict Lint, broader performance/background lifecycle and live cross-platform/hardware gates remain open. No production service was deployed, no real purchase was made and no store-signed app was published.


### Localization and ordinary build checkpoint (2026-11-17)

- Catalog: 2070 strings. Thirteen languages now complete: en, zh-Hans, zh-Hant, ar, de, es, fr, id, it, ja, ms, nl, pt-BR. Remaining ten: bn, fil, hi, ko, pl, ru, th, tr, ur, vi. Each new Android-specific translation set has 1233 entries; positional arguments and numeric limits were checked. Resource generation and notification checks passed at the checkpoint. This does not claim native-speaker review.
- Current ordinary arm64-v8a and x86_64 native libraries were rebuilt and verified: source fingerprint `118728e554b2e18dd62b158172f9c0acc4f8791afeaf3fc9dd129b6c76ecf55d`. Verification requires the same ANDROID_HOME/NDK metadata as the build; an invocation without ANDROID_HOME correctly mismatched the input fingerprint. Final APK/AAB packaging and runtime verification remain due.
- All four ordinary Debug Kotlin variants compiled. The first ordinary Debug unit-test configuration failed because its dependency lock still referenced WindowManager 1.5.0 while the declared and QA version was 1.5.1. Resolved the declared dependencies for all default build/test configurations and persisted locks. The rerun passed **110 JVM tests**, zero failures/errors/skips.
- Separate none and GMS Lint reports each contain **1233 MissingTranslation errors and no other error categories**, at their respective 11- and 12-language checkpoints. Both Lint runs failed and are not release acceptance. Remaining warnings include version-update advice, conservative storage checks and application-only configuration Context retention; they are not silently counted as errors fixed.
- Evidence: `misc/dmsg-android-validation/2026-11-17/localization-build/`. No emulator tests were rerun in this checkpoint.

- Follow-up framework locale test: initial **4 device cases, 3 passed / 1 failed** because Android 12 resolved Indonesian `values-b+id` to English. Both string and notification generators now emit `values-in` while locale selection/config keep BCP-47 `id`; obsolete owned generated resources are removed and rejected by `--check`. Identical API31 rerun: **4 passed**, no failures/errors/skips. It checks thirteen complete language variants against actual compiled Android resources, alongside plural forms and custom-field preservation. This fixed a real fallback, not a translation-completeness counter.
- Turkish's 1233 Android entries were then completed and generated, bringing coverage to **14/23**. Remaining: bn, fil, hi, ko, pl, ru, th, ur, vi. Resource/source-format and notification freshness pass; strict completeness still fails until these nine are finished. The new Turkish resources were not part of the thirteen-language device run.

- Polish and Russian Android-specific text completed after Turkish; catalog coverage is now **16/23**, with bn, fil, hi, ko, th, ur and vi remaining. Each 1233-entry set passes positional-argument and numeric-limit checks; generated resources are fresh. These three additions were not included in the thirteen-language device test, and strict completeness/Lint acceptance remains open.


### API31 full regression and nineteen-language framework matching (2026-11-17)

- Current GMS full XML: **85 cases, 83 passed, two opt-in probes skipped, zero failures/errors**. Current none full XML after test-harness corrections: **84 cases, 82 passed, two probes skipped, zero failures/errors**. The console's Finished 87/86 messages are not case counts. The earlier JVM run passed 110 cases.
- Preserved failures: first none run missed the expected synthetic missing-key message after an Autofill UI click. A four-case isolated run passed; this initial failure was not reproduced, so it is not claimed as a production authentication fix. Test clicks now wait for the QA app window to be active/focused, with the original 12-second bound and unchanged no-fill assertions. A second full run passed Autofill but failed to find an uncomposed lazy member row; it now scrolls the lazy parent to the ID before asserting visibility. Focused system/language/channel run: nine passed.
- A subsequent none run timed out opening a localized secret detail. Language recreation now waits for the actual Activity locale and window focus as well as the selected setting. Bounded synthetic UI/route diagnostics remain on timeout. Original-order account/window/language/system/channel sequence: ten passed; complete none rerun then passed. These are harness changes, not relaxed product authorization or removed assertions.
- The resource test checks **19 completed languages** through Android's compiled-resource selection, including Indonesian, Filipino, Korean, Vietnamese, Polish, Russian and Turkish. Shared iOS terminology was aligned for Korean Vault and Vietnamese Snapshot after the GMS run; none was rebuilt afterward.
- Evidence: `misc/dmsg-android-validation/2026-11-17/full-current-18/` and `full-current-19/`, including initial failures and final XML. These are API31 AOSP/4KiB QA flows with synthetic offline accounts, not real Google delivery, purchases or physical hardware.
- Hindi's 1233 Android entries were completed **after** those device runs. Coverage is now **20/23**; bn, th and ur remain incomplete. Source/format and notification freshness pass; strict coverage and release Lint remain open.

### Complete localization sources (2026-11-17)

All **23 locales now contain 2070 strings each**, plus six localized plural groups and notification resources. Hindi, Bengali, Urdu and Thai complete the Android-specific catalog. `sync-localizations.py --check --complete` and `sync-notifications.py --check` pass. Directly authored translations were checked for positional arguments and numeric limits; this is not native-speaker approval.

The framework resource test now requires exactly the 23 supported locale keys. The real picker/recreation flow now visits all 23 locales, deriving RTL from the selected Locale (including Urdu). It checks preserved client/account/navigation position, actual Activity locale/focus, navigation direction, concealed secrets and the Arabic date digits. The current API31 full run is still in progress; its language flow has produced 92 screenshots. Six account screenshots (de/ar/ur/pt-BR/fil/th), six details (hi/bn/th/ur/ko/ja) and four files pages (bn/hi/th/ur) were visually inspected. English synthetic account names and notes are fixture content, not application fallback. Evidence: `misc/dmsg-android-validation/2026-11-17/all-languages/screens/`.

The completed run passed **110 JVM tests** and **84 device cases: 82 passed, two opt-in probes skipped, zero failures/errors**. Both none/GMS Lint checks passed: zero errors, 66/65 warnings and one hint respectively. Warnings remain documented rather than suppressed. The all-23-language resource/picker flows are included in this passing run. The new opt-in AndroidSecurityDeviceTest was added after its APK compiled and is not included in those 84 cases. Final XML, JVM reports, Lint XML and build log are preserved in `all-languages/api31-none/`.

The opt-in ordinary Debug hardware probe compiled, but its **one API31 case failed** at production `AndroidSecurity.hardware()` with `HARDWARE_UNAVAILABLE` before any credential window or private operation. A temporary PIN was set only on the dedicated AOSP emulator and removed afterward. No hardware policy was relaxed; positive RSA/P-256 CryptoObject authentication remains unverified on compatible hardware. Preserved result: `2026-11-17/hardware/api31-unavailable/`. This is not an application test pass or physical TEE evidence.

### API36 / 16 KiB current matrix (2026-11-17)

- First complete none run: 86 cases, 80 passed / three failed / three opt-in probes skipped. Preserved failures were transient language-window visibility, a test assumption that appending a page scrolls to its last row, and cross-UID drag targeting behind a floating source task.
- The drag source now reveals the recipient task after starting the **real OS global drag**. Android16 had logged `Drop outside a valid window null`; the corrected fixture received entered/drop and accepted=true. The unchanged assertions verify the source is another UID, no library write occurs before explicit import, data stays encrypted and the temporary read grant is revoked. Temporary event diagnostics were removed from product code.
- Final complete none run passed **86 cases: 83 passed, three opt-in probes skipped, zero failures/errors**. It includes all 23 language flows and a 1000-record/125-favorite native encrypted-vault pagination/search/concealment/lock probe. Evidence: `api36-current/none-final/` and `performance/api36-scale-instrumentation/` under the 2026-11-17 local evidence root.
- GMS first and second runs each had **87 cases: 82 passed, one language-window assertion failed, four opt-in probes skipped**. NL/FR screenshots show the control fully visible with no IME. Waiting only for locale/focus, then also for replacement Activity identity, did not eliminate the transient assertion. Do not describe either attempt as a production locale fix.
- The test now uses fixed expected localized text, waits for both Activity/application locale and replacement identity, and uses its existing accessibility/PixelCopy rendering barrier before final visibility assertions. The readiness bound stays 15 seconds. The original account/window/all-language sequence then passed **five cases**, system window/transition animations still 1.0. A complete GMS rerun remains in progress at this checkpoint.
- Source records, XML, failure screenshots and targeted logs: `api36-current/{initial,diagnostic,drag-debug,drag-fixed,gms-recreation-failure,gms-recreated-failure,gms-window-sequence}`. Instrumentation-clock performance numbers are explicitly not real-clock frame-rate evidence.

## 2026-11-19 — 登录时选账号与按目标记忆

普通 Debug 和 QA 的 115 个 JVM 测试均通过，含目录重建后保留多个账号的使用顺序、目标与工作区隔离、撤下旧目录和旧版副本到期。Production GMS Debug 的 Kotlin 编译通过，23 种语言的资源检查通过。

Android 16 / API 36 的只读 ARM64 模拟器通过四个定向设备测试：从真实 Rust 测试工作区直接取得登录项目录、外部应用调起系统选择器且缺少密钥时拒绝回填、认证后的多账号排序和搜索，以及按目标显示「仅填充这一次」和「填充并记住」。模拟器的 Keystore 返回软件安全级别，完整的硬件认证到跨应用回填测试跳过。可在设有测试 PIN 的专用硬件设备上通过 instrumentation 参数 `autofillTestPin` 运行 `AutofillPickerDeviceTest`，核对两个账号实际填入的用户名和密码。

## 2026-11-20 · 自动填充列表与单次确认

Android 16（API 36，ARM64）模拟器上通过 6 个相关用例。覆盖同一站点多个账号的排序和搜索、键盘打开时点选账号并确认一次、已知与未知目标的授权选项、从已解锁保险库读取目录、同一表单再次打开选择器，以及从独立 App 唤起系统自动填充后缺少有效密钥时字段保持为空。另在简体中文的浅色界面和 150% 字号的深色界面重复搜索、点选与确认，并检查了样例界面截图。

Debug 和 QA 单元测试各 118 项通过，GMS Debug 的 Kotlin 编译通过。新增加密目录测试覆盖新目录中同一登录项的密码已更新，以及换账号、换站点、换条目和旧版条目不能沿用确认。23 种语言资源检查通过。

带临时 PIN 的独立模拟器报告缺少硬件支持的逐次认证密钥，跨 App 成功回填的硬件测试因此跳过。该用例现在还会在首次确认时更新密码，检查再次系统认证后的宿主字段，并切换和复用另一个账号。真实 Android 硬件仍待验证。

## 2026-11-21 · Pixel 7a 本机账户创建

Android 17（API 37）的 Play 内测版 0.1.0 在系统锁屏验证后创建本机账户失败。系统日志中，三次验证均记录了认证令牌，随后该应用的 StrongBox 私钥操作返回 `KEY_USER_NOT_AUTHENTICATED`。

在同一台 Pixel 上并行安装独立包名的 `productionNoneDebug` ARM64 验证版。新建允许锁屏凭据解封的 RSA 密钥时使用默认 Android Keystore，并继续核对硬件安全等级与逐次认证策略。用户以相同锁屏方式完成验证，确认已进入本机保险库。用户随后在此验证版锁定同一本机保险库，并确认再次解锁成功。严格生物认证和 Play 分发构建仍待真机验证。

## 2026-11-21 · Android 17 通行密钥签名轮换

在 Pixel 7a（API 37）上核对了 Google Play 安装的 0.1.1（2）。系统报告当前签名为后量子证书，签名轮换链依次包含旧传统证书、混合签名传统证书和当前后量子证书。两个域名均已通过系统关联验证；线上 dmsg.net 凭据关联与 dmsg_auth 准入名单也包含这三枚证书。用户在系统确认创建凭据后收到本地签名身份不匹配提示。原版应用日志没有记录该回包使用的具体证书。

修正版用当前证书进行请求前准入检查，用系统验证的同一安装包签名链精确匹配返回的原生 origin。GMS 的 119 项 JVM 测试通过，其中回归用例覆盖注册和登录时轮换链中的各枚证书、外部证书、错误挑战及原始客户端数据保留。Rust 的 6 项 Passkey 协议测试通过。修正版经 Play 分发后的真机注册和登录仍待验证。
