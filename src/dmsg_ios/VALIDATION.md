# iOS implementation validation

## Vault detail and large type (2026-11-07)

Vault details now copy a field by tapping its value. Native field menus provide
copy, reveal/hide, large type, HTTP(S) website opening and field references.
Selecting reveal or large type performs that explicit action immediately; the
old reveal confirmation, its grace-period state and its unused translations
were removed. Desktop interaction is unchanged.

The detail header and rows are compact, unfilled fields are omitted only while
reading, timestamps move to Item information, and editing remains a text action.
Favorites live in the item menu. Copy feedback replaces the field's menu symbol
without changing the value's available width. Large type supports wrapping and
scrolling, a native bottom copy action, loading/failure/retry, OTP expiry, and
clearing when dismissed, backgrounded, locked or switched to another account.

Validation:

- iPhone 17e / iOS 27.0: all 111 Swift unit tests and three initial field-action
  UI tests passed (`VaultDetail-Smoke.xcresult`). The broader vault/navigation
  run had 29 UI passes, one iPhone skip for the expanded-sidebar case and two
  failing test assumptions: a field below the fold at maximum text size, and
  a two-second fixture code expiring before a menu animation finished. After
  correcting those tests, the final seven-case field-action run passed
  (`VaultDetail-UI27-Final.xcresult`). Final large-type layout/copy/background
  checks passed again in `VaultDetail-UI27-Delivery.xcresult` (2 tests).
  Managed-key navigation also passed with an explicit assertion that managed
  fields expose no field menu (`VaultDetail-Managed.xcresult`).
- iPhone SE (3rd generation) / iOS 18.4: ten selected UI cases passed after
  focused reruns (`VaultDetail-SE-Final.xcresult` has five passes, with its five
  menu-tap failures corrected and passing in `VaultDetail-SE-Actions.xcresult`).
  Native menus have different legacy/modern AX types on iOS 18; tests now tap
  the observed menu frame and verify that its actions actually appear. Final
  presentation checks passed in `VaultDetail-SE-Delivery.xcresult` (2 tests).
- iPad (A16) / iOS 27.0: all five selected UI tests passed
  (`VaultDetail-iPad.xcresult`), followed by the final toolbar copy, feedback
  placement and background-clearing check (`VaultDetail-iPad-Delivery.xcresult`).
- Inspected Chinese light/dark, Arabic RTL and German at maximum Dynamic Type,
  including long keys, the end of scrollable values, field menus, last-field
  access above the tab bar, errors and copy feedback. The copy action now uses
  the native bottom toolbar so feedback stays above it. Final screenshots are
  in `.build/VaultDetailScreens/Delivery-SE`, `Delivery-27` and `Delivery-iPad`.
  Field-control hit-region, trait and description accessibility audits passed.
- Debug simulator and unsigned generic ARM64 iOS Release builds passed. Fresh
  Debug/Release localization extraction passed for all 23 languages; native
  artifact/source fingerprints and `git diff --check` passed.

These UI tests use local fixtures. Physical-device clipboard handoff, manual
VoiceOver reading, real-account relay behavior and the unrelated full UI/Rust/
desktop suites were not rerun. No signed archive or App Store upload was made.

Implementation and validation: Codex (OpenAI).

### Review of the vault detail change

Any content sync reloaded the detail, and every reload hid revealed values, closed
large type and cancelled a copy in progress, even when that item had not changed.
Plaintext now clears only when the item's revision changes, a reload fails or the
session epoch changes, along with the existing dismissal, background and lock cases.
The orphaned "Field reference and dependencies" string left all 23 languages, and
the managed-key test also asserts that its field has no copy action.

Validation:

- `testSyncWithoutANewRevisionKeepsValuesAndLargeType` failed before the fix: large
  type had closed when the test went to dismiss it.
- iPhone 17 / iOS 27.0: all 7 `VaultTests` and 10 selected UI cases passed: the new
  sync and revision cases, field menu, large type, failure and retry, favorite and
  website, historical code, incoming revisions and managed-key navigation.
- iPhone 16e / iOS 18.4 and iPad (A16): the two new cases passed.
- Unsigned generic ARM64 iOS Release build and localization extraction passed.

The rest of the UI suite, physical devices, manual VoiceOver and real-account sync
were not run, and no new screenshots were inspected.

Review and fixes: Claude (Anthropic).

## First account setup on a slow network (2026-11-07)

The reported App Store installation completed its initial setup after the user
enabled a VPN. This establishes network dependence for that incident; the
specific failing request or endpoint was not identified.

The setup button previously had no working state and refreshed its session only
after success. A failed native transition also advances the session generation,
so a later attempt using the old handle was rejected. Setup now refreshes that
handle after failure, reconciles an uncertain account/root operation once using
the existing journal, and restores retry controls if confirmation still fails.
The access actions are disabled while setup runs, the button shows a spinner and
working label, and guidance changes to a slow-network message after 15 seconds.
The new copy is present in all 23 languages.

Validation on iPhone SE (3rd generation), iOS 18.4 Simulator:

- 29 selected tests passed: `AccountInitializationTests`, `CoreBridgeTests`,
  `SessionPolicyTests`, and `AccountInitializationUITests` (27 unit, 2 UI).
  The bridge regression exercises real failed Rust transitions and verifies that
  a fresh handle can start another attempt. UI fixtures cover slow confirmation,
  one automatic reconciliation, bounded failure, disabled competing actions and
  a successful user retry that continues the already-created account.
- Inspected Chinese light and Arabic RTL dark screenshots at 375 × 667 points,
  including the spinner, disabled actions and the long-wait explanation.
  Final screenshots are in `.build/AccountInitializationFinalScreens` locally.
- The simulator build, native artifact/source fingerprint, `git diff --check`,
  and all localization checks with fresh compiler extraction passed. The fresh
  extraction is in `.build/AccountInitializationDerivedData`; older shared
  DerivedData still contains strings removed by earlier changes.
- The generic ARM64 iOS Release build passed with `CODE_SIGNING_ALLOWED=NO`,
  using `.build/AccountInitializationReleaseDerivedData`. This verifies that the
  production configuration builds with UI fixtures excluded; it is not a signed
  installation or App Store archive.

The full Swift suite, physical-device weak-network/VPN switching, live II login
and system-authentication prompts were not rerun. UI fixtures do not prove live
network behavior. No App Store build was uploaded.

## File link review (2026-11-05)

Reviewed `ab5579b..=b6dc1c5` inclusively, retaining the subsequent root-title test
fix `76379106`. Two defects were corrected:

- Temporary backup reads for a share link no longer update an existing restore
  job. Previously, even a completed job could become `verifying` and restore an
  evicted local copy again on the next download pass. The regression test failed
  before the fix and now preserves completed, queued and retrying jobs, including
  the completed job's exclusion from the next pass.
- The iOS revoke confirmation reuses the desktop's wording in all 23 languages:
  new downloads stop, while started downloads and saved copies cannot be recalled.
  It no longer promises immediate removal of the cloud copy.

Validation:

- Rust: 578 client, 115 CLI and 26 mobile tests passed; 11 existing tests ignored.
  All-target Clippy with warnings denied, Rustfmt and `git diff --check` passed.
- Desktop: `pnpm check`, 173 tests, build and the isolated Electron `transfers`
  flow passed. Cloud-link progress, failure/retry and keyboard checks passed;
  Chinese light/dark, French and Arabic RTL screenshots were inspected, including
  the 760-pixel minimum window width.
- Web: `pnpm check` passed; 4 heartbeat tests and 107 Vitest tests passed, with the
  live share test skipped.
- Rebuilt debug native device/simulator slices and Swift bindings; artifact/source
  fingerprints and all 23 localizations, including fresh compiler extraction,
  passed. A separate DerivedData directory avoided stale extraction from old builds.
- iPhone 17e / iOS 27.0: all 100 unit tests and 50 selected UI tests passed, with
  one iPad sidebar test skipped and no failures. The UI selection covered Files,
  links, capture fixtures, messages, inbox, sync and the changed settings tests.
  Final evidence: `.build/Review-ab5579b-b6dc1c5.xcresult` reports 151 tests,
  150 passed and 1 skipped. The native run continued after the tool's wait timeout;
  its completed result bundle was checked independently.
- The revoke confirmation was also inspected in English light, Chinese dark,
  Arabic RTL dark and German at maximum Dynamic Type.

Physical photo/video capture and microphone authorization, iOS 18.4/iPad runs,
real-relay/PocketIC integration and production behavior were not revalidated.
The iOS UI tests use fixtures and do not establish live mobile sharing behavior.

Implementation and validation: Codex (OpenAI).

## Root title test on iOS 18.4 (2026-11-05)

`testWorkspaceRootShowsNoTitleButPushedScreensDo` failed on iOS 18.4 because of the
test, not the app. The screenshot shows no title, but iOS 18 keeps the hidden title in
the accessibility tree and reports it as hittable, while iOS 27 does not expose it. The
test now checks the screen: where a title element sits, it must find one flat color.

- Only this test was run for the fix. It passes on iPhone 16 / iOS 18.4 and on
  iPhone 18 Pro / iOS 27.0.
- With `.toolbar(removing: .title)` temporarily removed, it fails at that assertion on
  iOS 18.4, so it still catches a visible title. The app code was restored afterwards.

Implementation and validation: Claude Opus 5.5.

## Share by link (2026-11-05)

Files gains **Share by link** on file rows, file details and cloud backups, and
**Share links** to list and revoke them. A backup that is not on the device is fetched
only to encrypt the new copy, and the library does not change.

- iPhone 16 / iOS 18.4: all 100 Swift unit tests passed. Of the 25 `FileLibraryUITests`,
  23 passed, one was skipped and `testWorkspaceRootShowsNoTitleButPushedScreensDo`
  failed at its root-title assertion. That was a test defect, fixed in the section above.
- iPhone 18 Pro / iOS 27.0: the title test and the local, backup with failure and
  retry, oversized, list and revoke, language and Dynamic Type link tests passed.
- The localization check with compiler extraction passed for all 23 languages.
- Screenshots were inspected for English light, Chinese and Arabic RTL dark, German,
  maximum Dynamic Type, the saved list after a failed refresh and the revoke
  confirmation.
- These UI tests use the fixture core. The same `dmsg_client` paths ran against a local
  relay and PocketIC in `desktop_content_and_sync`, `desktop_large_file_sharing` and
  `desktop_presigned_share`, where the share page's decryption opened a link made from
  a backup. A link was not created from the iOS core against a relay or on a device.

Implementation and validation: Claude Opus 5.5.

## Link and usage UI test fixes (2026-11-05)

The five failures recorded in the next section were test defects, and no app code
changed. SwiftUI exposes `Link` as a button on iOS 18.4 and as a link on iOS 27.0, so
`app.buttons` found nothing on iOS 27; those checks now accept either type. The usage
test waited for a label inside a merged `LabeledContent` row to become hittable, which
never happens, and scrolled `Used` out of the list; it now scrolls until the label
exists.

- All five pass on iPhone 17e / iOS 27.0 and iPhone SE (3rd generation) / iOS 18.4.
- A full run on iPhone 17 / iOS 27.0 was stopped before it finished, with 163 tests
  passed, 1 skipped and 2 failed. `NavigationTests` did not complete, and the
  notification, sync and vault UI classes did not run.
- Two failures from that run are open and undiagnosed; both passed in the earlier
  iPhone 17e runs, and neither was run on iPhone 17 before these changes.
  `testPublicProfileKeepsDraftWhenLeavingAndLabelsRemainVisible` found several
  `profile.name` fields after returning from Home. `testFrenchLargeText` did not find
  the reply recipient field within 5 seconds.

Validation: Claude Opus 5.5 <noreply@anthropic.com>.

## Files camera and workspace layout (2026-11-05)

**Import photos or videos** did nothing because a `PhotosPicker` placed inside a
`Menu` closes with it; the picker is now presented from the screen. The **+** menu
gains **Take photo or video**, which encrypts the capture into the open folder
without touching the Photos library. Messages now uses the capsule tabs and the
**+** then **⋯** toolbar order of Vault and Files, through one shared `BrowseTab`.
Workspace roots show no title, and File transfers moved from a list row to an icon
in the tab row, with a count while transfers are active.

- The picker regression test fails with the picker back inside the menu and passes
  with the fix. On Simulator a photo was picked and imported with the fixture core.
- Final run on iPhone 17e / iOS 27.0: all 98 Swift unit tests and 45 UI tests
  (Files, transfers, messaging, letters and sync) passed, with one skipped test that
  needs the expanded sidebar. iPad (A16) / iOS 18.4 ran the picker, the camera
  fixture and its denied alert, four languages, sidebar selection, pushed titles,
  the Letters tab and the large-text scope menus; the last two exposed that a menu in
  a selectable list row never opened, now fixed for Files and Messages.
- An earlier full run of all 120 UI tests, before the settings icon and that iPad
  fix, had five failures outside this work. The same five fail with identical
  assertions on unmodified `17c66bf1`: `testUsageShowsConsumptionSeparatelyFromLimits`,
  `testAccessActionsRemainReachableInDarkArabic`,
  `testAccessHasNoDemoAndPrivacyIsAvailableOffline`,
  `testExpandedLanguageAccessScreens` and
  `testLanguageSettingsAvailableBeforeLoginAndInAccount`. They are fixed in the
  section above.
- Release ARM64 Simulator build and the localization check with compiler extraction
  passed. Screenshots were inspected for Chinese light, English, French dark and
  Arabic RTL, on iPhone 17e and on iPhone SE (3rd generation) / iOS 18.4.

Simulators have no camera, so the capture path ran only through a fixture image and
the fixture core. A real photo and video, the camera and microphone prompts, encrypted
import through the Rust core and absence from the Photos library on a physical device
remain open; see [Review/README_zh.md](Review/README_zh.md). Rust and desktop checks
were not run because neither changed.

Validation: Claude Opus 5.5 <noreply@anthropic.com>.

## Sync review simulator follow-up (2026-11-05)

Validated `db088dc1`, the follow-up fixes for `0eddbd98`, on iPhone 17e /
iOS 27.0 Simulator. The native debug artifacts and source fingerprint were
verified before building and launching the app.

- All 94 Swift unit tests and 16 selected UI tests passed: 110 total, zero
  failures, zero skipped tests and no runtime warnings in the result summary.
- UI coverage comprises seven conversation cases, seven sync cases, AutoFill
  selection across search/reopening, and account profile/manual-sync progress.
  It covers sends surviving navigation, retained drafts after failed sends,
  saved messages during refresh, initial sync, retry deadlines, server cooldowns
  and uninterrupted browsing during sync failures.
- Exported screenshots were inspected for AutoFill selection, conversation
  delivery/drafts, quiet message browsing, English/French light presentation,
  Chinese dark presentation and Arabic RTL sync status.
- The MCP wait timed out after 300 seconds while Xcode continued the same run.
  The completed log reports `TEST EXECUTE SUCCEEDED`; the finished result bundle
  independently reports all 110 tests passed. No duplicate run was started.

Local evidence is in `.build/review-db088dc1/summary.json` and its `attachments/`
directory. The full result bundle is named
`test_sim_2026-11-05T10-18-46-466Z_pid43211_027d365d.xcresult` in the local
XcodeBuildMCP workspace. No application-code changes were needed after this run.

These UI cases use synthetic presentation fixtures. Live two-device hint
delivery, system AutoFill authentication/publication on a physical device,
the actual iOS background time allowance, APNs and production relay deployment
remain separate checks. The earlier Rust and desktop results are recorded in
`db088dc1`; they were not rerun in this simulator-only follow-up.

Validation: Codex <noreply@openai.com>.

## Vault browsing, file moves and quiet sync status (2026-11-04)

Vault search now uses the same always-visible navigation drawer as Files.
Category counts retain their search/tag context during category and sort changes;
the bar reveals offscreen selections without recentering visible ones. Capsule
hit regions are stable, and both browse bars leave 8 pt before list content.

The old move flow only selected a destination in the folder picker, then required
a second submit on the previous page; its Done button could close that page
without moving anything. The picker now explicitly says **Move here** and executes
the move before showing per-file results. Pending operations show progress;
failed items retry without repeating successes. The move task is cancelled on
departure instead of being gated by a separate page-visibility flag. All 23
languages include the new confirmation label.

Sync errors no longer produce the automatic Sync paused overlay or its VoiceOver
announcement. Messages no longer inserts a sync panel before conversations.
Detailed status and retry remain in the toolbar entry and account sync settings;
recovery and conflict notices retain their existing behavior.

Validation:
- All 95 Swift unit tests passed. Across the final targeted runs, four vault UI
  cases, four file-move cases and three sync UI cases passed on iPhone 17e /
  iOS 27.0. Coverage includes searching during delayed reads, stable category
  positions, picker cancellation/reopening, delayed moves, partial batch failure,
  destination changes, retry, destination contents and quiet browsing.
- Three focused UI cases passed on the 375 pt iPhone SE simulator / iOS 18.4:
  vault search/category stability, delayed file move after picker cancellation,
  and suppressing the sync overlay. The final Messages-panel removal was tested
  on iOS 27.0.
- Inspected Chinese light and Arabic dark/RTL vault screenshots, Chinese Files,
  Messages, and the small-screen move result. The initial intermediate UI failures
  were resolved: capsule hit-region differences, a too-short progress fixture,
  and an ambiguous Cancel selector after the move flow changed.
- All 23 mobile Rust tests passed, including real folder creation/move while an
  inbox request is stalled, checking source/destination listings and unchanged
  version/hash. The shared-client folder/file/version round-trip test passed.
  Mobile all-target Clippy with `-D warnings`, Rustfmt, all 23 localization tables
  plus Swift extraction, and `git diff --check` passed.

UI tests use isolated presentation fixtures; the Rust test exercises real local
storage. No signed-device installation, Release archive, TestFlight upload or
production-account move was performed for this change. Full iPad/Duo, maximum
Dynamic Type and hardware-keyboard matrices were not rerun.

## TestFlight saved-account preflight (2026-11-04)

With Xcode **27.2 (27B52)**, the open Apple Accounts window showed the team's
signed-in Admin, while both persisted Xcode account lists were empty. Local
signing identities included development and Developer ID certificates, but no
distribution identity for this team. Exporting the existing **0.1.0 (11)** archive
outside the execution sandbox reproduced `No Accounts` and missing distribution
certificate errors. The GUI login alone did not establish command-line access.

The packaging script now detects this specific empty-account condition before
changing the build number or creating build/export output, while allowing API
keys and existing local distribution identities. Unknown or unreadable account
metadata is left to Xcode's actual validation; this is not a live authentication
or provisioning check. Error guidance distinguishes GUI state from saved state.

- All **23 iOS tooling tests** and `git diff --check` passed.
- Both dry-run entry points passed. Real package/export invocations stopped at
  the new preflight; byte comparison confirmed `project.yml` was unchanged,
  no build 12 directory was created, and no new export directory was created.
- After the user reauthenticated the same account in Xcode, the saved identifier
  list contained one account. Exporting the same build 11 archive through the
  updated script succeeded, using normal Keychain access outside the sandbox.
  Output: `.build/TestFlight-0.1.0-11/Export-x72a63ir/Distribution/dMsg.ipa`.
- All six IPA bundles retained **0.1.0 (11)** and passed
  `codesign --verify --deep --strict`, with this team's Apple Distribution
  authority. Embedded profile properties had `beta-reports-active`, no device
  list, and no development debugging entitlement. `security cms` was blocked
  from importing certificates in the sandbox; profile metadata was instead
  decoded with OpenSSL CMS signature verification, without trust-chain validation.
  The generated SHA-256 sidecar matched the IPA bytes.
- The original build 11 archive, earlier logs and `project.yml` were retained.
  No new archive, application tests, upload, or App Store submission was performed.

## Review of eb6caf1 and 90c33e7 (2026-11-04)

Reset the file-detail lifetime when the split-view selection changes. Conversation
and secret attachment entry points now reload library details, and conversation
attachment availability is invalidated when the library changes. Reply previews
respect hidden/unreadable originals. Channel managers retain the invitation path
that can complete a required key rotation, while publishing remains paused.
Changing a batch move's destination resets its completed items and outcomes;
retrying the same destination retains successful items.

Validation performed with Xcode **27.2 (27B52)**:

- `cargo test --locked -p dmsg_client --lib`: **564 passed, 5 existing ignored**.
  `cargo test --locked -p dmsg_mobile`: **23 passed**, including invitation
  availability checked against the core control permissions for each role and
  active/rotating/archived states. Mobile all-target Clippy with `-D warnings`,
  client/mobile formatting and diff whitespace checks passed.
- Rebuilt the Debug device and Simulator Rust slices and UniFFI bridge; native
  artifact hashes and source fingerprint verification passed. The simulator
  Debug link reported the large `__eh_frame` warning, and targets without App
  Intents reported skipped metadata extraction. This is not a signed archive.
- **4 distinct new iPad Pro 13-inch (M4) / iOS 18.4 UI regressions passed**:
  selecting another file, removing/restoring a conversation attachment, hiding
  an original message's reply preview, and changing a partially completed batch
  move's destination. The selection test first reproduced the old-detail bug.
  Final passing cases are in `Review-messaging-files-ipad-direct.xcresult`
  (3 passed; its remaining failure was the test's ambiguous Done selector) and
  `Review-messaging-files-move-verified.xcresult` (1 passed after fixing that
  selector). Earlier fixture expectations and presentation waits were corrected.
  A packaged-tool run executed only one of four requested cases; its success is
  not counted as four cases. The direct Xcode results were checked individually.
- On **iPhone SE (3rd generation) / iOS 18.4**, all **95 Swift unit tests** and
  **19 distinct selected UI tests** passed across the final results. The main
  `Review-messaging-files-phone.xcresult` passed 95 unit and 18 UI cases; its
  French large-text case failed because a collection index changed during
  navigation. The Inbox detail list now has a stable accessibility identifier,
  and `Review-messaging-files-layout-final.xcresult` passed all 3 language cases
  after the selector fix. Across phone and iPad there are **20 distinct passing
  UI cases**, not a complete run of the application's UI suite. Coverage includes
  drafts/sent letters, queued sends across navigation, reply hiding, attachment
  composition/restoration, version restore, rename retry and changed batch
  destinations. Exported screenshots were inspected for those flows, Chinese
  dark mode, Arabic RTL and maximum Dynamic Type; the selected file accessibility
  description/hit-region audit passed. Keyboard entry/submission is automated,
  rather than physical-keyboard acceptance.
- All **23 localization catalogs**, compiler-extracted strings, and **18 iOS
  tooling tests** passed. No localized copy or public protocol fields changed.

The UI runs use DEBUG fixtures. No production messages or file mutations, real
two-device delivery/restore, physical-device background transfers, manual
VoiceOver/physical-keyboard acceptance, or signed distribution archive were run.
Review, fixes and validation: Codex <noreply@openai.com>.

## File library and backup workflows (2026-11-03)

Completed the implementation items F01–F22 in `IMPLEMENTATION_CHECKLIST_zh.md`.
The Files screen separates the local library from cloud backups, consolidates
backup status and retry controls, and provides explicit restore destinations.
Folder management, batch operations, import outcomes, historical versions, local
copy removal and individual transfer controls use the existing core contracts.
New copy is translated in all 23 supported languages.

Validation performed with Xcode **27.2 (27B52)**:

- Rust: `cargo check --locked -p dmsg_mobile`, mobile all-target Clippy with
  `-D warnings`, formatting and diff whitespace checks passed. The mobile library
  suite passed **22 tests**; the client library filter passed **49 tests**, with
  **1 existing ignored test**. These were local core checks, not production cloud
  transfers.
- Both ARM64 Debug native slices (device and Simulator) and UniFFI bindings were
  rebuilt. Native artifact hashes and the Rust source fingerprint passed
  verification. Debug app/test compilation and Release ARM64 Simulator app and
  extension compilation passed. The latter used the verified Debug native bridge
  with signing disabled; it is not an optimized native release or a signed archive.
  The initial Release build reported four extension App Intents metadata-skipped
  warnings; the final incremental build reported no warnings or errors.
- Across the final passing results, **95 distinct Swift unit tests** and
  **17 selected UI regressions** passed. This is the unit suite plus FileLibrary,
  FileTransfer and three selected Navigation UI tests, not the entire UI suite.
  `acceptance.xcresult` passed **101/101** (94 unit and 7 UI tests) on iPhone 17e /
  iOS 27.0. After the final error-handling fix, `final-errors-layout.xcresult`
  passed **16/16** (12 unit and 4 UI tests) on iPhone SE (3rd generation) /
  iOS 18.4. This includes the added session-revocation/cancellation regression.
  The final cloud-entry navigation check passed **1/1** in
  `navigation-final.xcresult` on iPhone 17e.
- File interactions exercised cloud refresh failure/retry, scope-specific search,
  historical restore, rename failure with draft retention and keyboard submission,
  batch backup admission results, local-copy removal, folder creation/trash
  restoration and explicit move destinations. Tests use DEBUG fixtures for these
  operations. They do not perform destructive operations against live accounts.
- Compact-width and maximum Dynamic Type checks passed on the 375-point iPhone
  SE. On iPad Pro 13-inch (M4) / iOS 18.4, multilingual layout and historical-version
  restore checks passed **2/2** in `ipad.xcresult`. The automated accessibility
  check covered sufficient element descriptions and hit regions; it does not
  replace a full spoken VoiceOver review.
- Screenshot review covered English/light, Chinese/dark, long French copy,
  Arabic RTL, maximum Dynamic Type, iPad split navigation, transfer stages,
  batch results and rename retry. Review exposed clipped scope labels and a fixed
  large-text header that crowded out the list. Long labels now use an adaptive
  menu, the large-text scope header scrolls with the list, and file rows retain
  their complete accessibility names. UI tests also stopped targeting inherited
  identifiers, hidden adaptive controls and swipe gestures starting under the
  tab bar. Final screenshots were inspected after those fixes.
- All **23 localization catalogs** and compiler-extracted strings passed validation
  for Debug and Release. All **18 iOS build-script tests** passed.

Local test bundles and logs are in `/private/tmp/dmsg-files-final`. Selected
screenshots and JSON result summaries are retained in the ignored local directory
`.build/Files-2026-11-03`; generated build artifacts are not committed.

Real two-device cloud restores, physical-device Photos permissions, sustained
background transfers, a full manual VoiceOver review, signed archive/TestFlight
packaging and upload were not performed. Simulator fixtures establish interaction
and presentation coverage, not production-service or hardware acceptance.

Implementation and validation: Codex <noreply@openai.com>.

## Account switcher names (2026-11-03)

The switcher now reads names for every local account from the existing certified
public-name API used by desktop, first from the local core cache and then through
its normal refresh/TTL path. This fills names for accounts created before the
Swift identity cache existed, without opening their encrypted workspaces. Known
names persist in protected local preferences; missing responses do not erase them.
The current account retains its own newer profile name when publication is pending.

Rows show a name above the full ID, or the ID alone when no name is published.
The current account is a selected, non-interactive information row with a green
checkmark, so its name stays readable instead of inheriting a disabled button's
fade. Other accounts keep full-row selection and the existing unlock behavior.

Validation: **9 account-settings unit tests and 2 UI tests passed** in
`AccountNames-Tests.xcresult`. The added unit regression checks older-account
name hydration, rename/removal, own-profile precedence and rejected late writes
after locking. The English/light and Chinese/dark UI tests passed again after
fixing current-row contrast (`AccountNames-Final.xcresult`, **2/2**); exported
screenshots were inspected. All 23 localization catalogs/compiler extraction and
diff whitespace checks passed. The fixtures exercise named and unnamed accounts.
Final Debug test compilation and Release ARM64 Simulator compilation passed
without reported warnings or errors (Xcode 27.2; Release signing disabled).

The Rust/native API did not change and its suites were not rerun. Live production
name retrieval, real account switching and physical-device checks were not run.
Implementation and validation: Codex <noreply@openai.com>.

## Centered account actions and direct ID copy (2026-11-03)

Standalone actions in Account and its related settings forms now use a shared
native-button component with centered labels. Primary publish/save actions use
the prominent system style. Navigation and field content retain leading alignment;
actions belonging to an individual item retain their contextual placement.
Destructive roles, disabled states and confirmation steps are unchanged. Row
separators keep a consistent leading inset rather than following centered text.

Account and the public-profile editor now show a direct account-ID copy button
with a 44-point target, a localized accessibility label, the complete ID as its
accessibility value, and copy feedback. Copying is separate from navigation and
uses the existing device-local clipboard behavior.

Follow-up validation:

- **7 distinct UI regressions passed** across `AccountActions-Phone.xcresult`
  (4/4) and `AccountActions-Copy.xcresult` (4/4; one repeated test). They cover
  the independent copy action, profile draft navigation, AutoFill selection,
  local-removal gating, member recovery dispute, and notification saving in
  English and Arabic RTL. No real account or financial operation was performed.
- Final **Debug and Release ARM64 Simulator builds passed without warnings or
  errors** with Xcode 27.2. An initial PhotosPicker label isolation warning and
  its first attempted fix were resolved by keeping the picker label native.
- Inspected Chinese light screenshots of centered lock/sign-out, direct ID copy,
  and profile publication in enabled and disabled states; also inspected the
  dark Account footer on a 375-point iPhone SE at accessibility XXXL text size.
  Native button labels wrap and remain reachable. Arabic notification screenshots
  were reviewed for RTL layout; the automated test exercised the save action.
- All 23 localization catalogs, compiler-extracted strings and diff whitespace
  checks passed. No new localized copy or core behavior was introduced.

The earlier Rust and Swift-unit results below were not rerun for this UI-only
follow-up. No physical-device, signed archive, TestFlight or App Store action was
performed. Validation: Codex <noreply@openai.com>.

## Account settings and form completion (2026-11-03)

Removed the public local tour and the combined safety/support screen, including
the login shortcuts. Offline privacy and contact information remain reachable;
reporting/blocking moved to message settings and service-account deletion remains
under account security. Historical tour references below describe previous builds.
Full App Review account access remains a separate release responsibility.

The Account screen now groups identity/security, synchronization/storage and app
settings. This delivery covers public profile links and avatars, local-account
removal, cloud/execution usage, device labels and permissions, recovery state,
AutoFill/File Provider snapshot status and selection, receiving rules, browser
invitation closure, agent delegations and exact-amount wallet previews. Settings
forms retain drafts only within the unlocked session. Failed reads remain distinct
from verified empty states; editing invalidates previews, and asynchronous results
are checked against the originating session.

Validation performed on the local working tree:

- `cargo check --locked -p dmsg_mobile`, client/mobile all-target Clippy with
  `-D warnings`, formatting and diff whitespace checks passed.
- All **21 mobile Rust tests** passed. The focused browser-candidate regression
  passed after changing it to exercise the real invitation-forget method; deleting
  the local invitation secret still prevents later approval of a fetched candidate.
- Both optimized ARM64 device and Simulator cores and UniFFI bindings rebuilt.
  Release native artifact hashes and the Rust source fingerprint were verified.
- Xcode **27.2 (27B52)** compiled the app and extensions in **Release** for ARM64
  Simulator with `CODE_SIGNING_ALLOWED=NO`; the build succeeded with no reported
  warnings or errors. This was a compile check, not a signed device archive.
- All **74 Swift unit tests** passed in `AccountImprovements-Tests4.xcresult`,
  including exact monetary amounts, public-link encoding, draft version retention,
  lock-time draft clearing, recovery wire decoding and export-header compatibility.
- Navigation, notification and new account-settings UI suites were run on iPhone
  16e / iOS 18.4. The initial run found a hidden AutoFill Done button while searching
  and test assumptions about virtualized rows, switch hit targets and cache fixtures.
  The button now stays in a bottom action area; the corrected focused run passed
  **6/6** (`AccountImprovements-Tests5.xcresult`). The pre-login notification check
  uses the normal native core; account-editing tests use DEBUG fixtures.
- On iPhone SE (3rd generation) / iOS 18.4, all **4 selected UI checks** passed:
  cross-search AutoFill selection, ordinary-member recovery dispute, Arabic RTL
  with accessibility-size text, and a long Chinese account header in dark mode
  (`AccountImprovements-SE.xcresult`).
- The initial iPad Pro 13-inch / iOS 18.4 run passed profile draft restoration,
  usage and the long dark account header. AutoFill search placement and an existing
  test's assumption of an always-visible split-view sidebar needed adjustment.
  Both focused checks then passed (**2/2**, `AccountImprovements-iPadRetry.xcresult`).
  Across these runs, **46 distinct UI tests** have a passing final result; these are
  the Navigation, Notifications and AccountSettings suites, not every iOS UI suite.
- On iPhone Duo / iOS 27.1, AutoFill and the long dark Chinese account header
  passed. The profile-draft test initially searched only the navigation bar for
  Back; this OS puts it in the floating toolbar. Using the button's accessibility
  label fixed the test and its focused retry passed (**1/1**). Results are in
  `AccountImprovements-Duo.xcresult` and `AccountImprovements-DuoRetry.xcresult`.
- Exported screenshots were inspected for Chinese light/dark, English forms,
  Arabic RTL with large text, SE compact width, iPad sheets and Duo toolbars.
  Search and Done remain visible in the AutoFill picker and draft labels stay
  visible after typing. This does not replace a complete spoken VoiceOver audit.
- All **18 iOS build-script tests**, 23-language resources and Swift compiler
  extraction, notification-copy parity and 31 shared-icon checks passed. Node emits
  the existing typeless-module warning for the shared notification source.

No physical-device authentication, multi-device recovery/pairing, actual account
removal, production profile publication, live wallet transaction, APNs delivery,
StoreKit purchase, private Worker/PocketIC acceptance, TestFlight packaging/upload
or App Store submission was performed. Desktop code was not changed and its UI
suite was not rerun. Simulator fixtures do not establish production-service or
hardware acceptance.

Implementation and validation: Codex <noreply@openai.com>.

## New-device synchronization continuity (2026-11-02)

Completed the six-item review of the locally archived build 9:

- Navigation and ordinary editing only record activity; account/lifecycle changes
  cancel the automatic pass. Manual sync joins an existing full pass, but still
  sends local work after an inbox-only receive.
- Automatic retries use the core deadline instead of adding a 60-second UI gate.
  Server cooldown is visible, and its retry control becomes available on expiry
  even when the status record itself has not changed.
- Inbox key recovery shares the sync lane, cooldown and result accounting. An
  existing baseline uses incremental pages; recovery never uploads local drafts.
- Initial sync, verified inventory progress, ciphertext bytes, failures, next
  retry and actual last success are exposed through a named mobile status API.
  Vault/channel views distinguish initialization from a verified empty account.
- Manual sync refreshes account content and cloud backup inventory independently.
  A cached file-list read cannot supersede a pending network refresh. Conversation
  and inbox message reception and explicit file restoration remain separate.
- Fixed-category diagnostics record trigger, queue wait, phase, counts, bytes,
  cancellation, failure and retry time without account IDs or content.

Validation performed:

- Rust client/mobile/CLI suites: **549 + 21 + 105 passed**, with the existing
  **11 ignored** tests retained. Final focused sync runs: **36 client + 8 mobile**
  passed. All-target Clippy with `-D warnings`, formatting and diff checks passed.
- A loopback regression resumes a preverified 240-entry inventory containing
  120 encrypted vault items, interrupts after downloading some chunks, checks
  that no unverified item was published, then completes without redownloading
  saved chunks. The final measured run took 678 ms, received another 154,294
  bytes and reused 5,316 bytes. Requests have an artificial 5 ms body delay;
  this is a resumability test, not a production latency claim or an IC proof test.
- Both ARM64 device and simulator debug cores and UniFFI bindings rebuilt; source
  fingerprint verification passed. The source-change guard rejected an earlier
  in-flight build, which was rebuilt from the final Rust sources.
- Xcode 27.2, iPhone 17e / iOS 27.0: **61 Swift unit tests and 17 distinct UI tests**
  passed across the primary and focused runs. Coverage includes the real Rust
  cancellation token after navigation/background, retry deadlines, manual sync
  progress, initial sync, inbox retry and existing conversation behavior.
- The primary MCP call exceeded its 300-second response wait; the continuing
  xcodebuild completed successfully. Its final log and result bundle were checked.
  Results are in `.build/SyncReviewTests.xcresult`, `.build/SyncReviewRetry.xcresult`
  and `.build/SyncReviewFinal.xcresult`. The final targeted build has no warnings.
- Ten sync screenshots were exported to `.build/SyncReviewScreens` and inspected:
  English light, Chinese light/dark, French long copy, Arabic RTL, progress,
  cooldown and account scope. New status copy fits the existing theme and layout;
  the existing French vault type picker still abbreviates its password label.
- All 18 iOS script tests, 23-language resources/compiler extraction, notification
  copy parity and icon checks passed.

Not run: physical-device pairing and synchronization with a newly authorized iPhone, production network timing,
PocketIC/private Worker acceptance, the unrelated UI suites, Release/TestFlight
packaging or deployment. These results do not establish a production root cause
for the reported ten-minute wait. Implementation and validation: Codex
<codex@openai.com>.

## Max backup capacity (2026-11-02)

New explicit cloud backups use the relay's current per-file admission limit:
1000 MiB for eligible Max memberships, 100 MiB for other tiers. Unknown limits
fail with `MEMBERSHIP_STALE`; existing backups restore independently of the
current tier. The client keeps the shared-file/secret-package bounds separate.
The library supports 1000-chunk plans and 512 KiB backup manifests. iOS document
imports accept 1000 MiB, and background transfer metadata supports 1001 slots
including the manifest. Photo staging, vault imports and explicit File Provider
exports retain their own limits.

The new sparse-file import test initially exposed cached URL size metadata:
reusing a URL after the source grew could bypass the staging size check. Staging
now reads current filesystem attributes. The unchanged regression passes at
1000 MiB and rejects one byte more; vault import still rejects that large file.
iOS also handles a skipped oversized backup as an error instead of reporting
saved progress.

Validation on the local working tree: client 548 tests and mobile 17 tests passed;
5 client tests were explicitly ignored. CLI passed 105 tests with 6 explicitly
ignored when run serially; the Secure Enclave key-creation test failed in parallel
but passed independently and in the serial suite. All-target Clippy with warnings
denied and Rust formatting passed. Desktop type checks, 159 tests, build and an
isolated Electron size/backup/share-action matrix passed (Chinese/French/Arabic,
light/dark and minimum window); 23-language resources passed validation.
The membership display now lists 100 MiB for Free/Plus/Pro and 1000 MiB for Max.
Desktop current limits use the relay response; plan comparison cards and the iOS
comparison list label the tier policy explicitly. The `plan-benefits` Electron
suite passed with Chinese/French/Arabic theme/width screenshots. Three iOS plan UI
tests passed (English light, Chinese dark and Arabic dark), with all four tier
values asserted. These UI fixtures do not verify production prices or purchases.
Both ARM64 native slices and Swift bindings were rebuilt. On iPhone 17e / iOS 27.0
Simulator, all 59 Swift unit tests passed, including large document staging,
thousand-chunk metadata reopening and the real bridge load. The result bundle
is `dmsg-max-backup-verified.xcresult` from this local run.

The large Rust upload/manifest and download-grant boundary tests use synthetic
metadata; sparse-file staging does not measure encryption or network throughput.
No physical-device 1000 MiB encrypted upload/restore, public-network end-to-end,
minimum-device memory/power qualification, production deployment or release
packaging/upload was performed.


## Inbox receiving and retry (2026-11-02)

A missing synced inbox key previously surfaced as `NOT_FOUND`; mobile inbox
refresh only retried letters and never fetched the key. The core now reports
`RECOVERY_INCOMPLETE`. Mobile performs one encrypted account-content pull in the
existing sync lane, without flushing or uploading local work, then retries from
the unchanged inbox cursor. Signature and HPKE validation still gate page commits.
If the key is not available from the relay, the view explains that the device
which enabled receiving must sync first.

The inbox keeps verified local letters visible, distinguishes loading/empty/failure,
and prevents a late cached read from replacing a newer result. Read, archive and
restore acknowledgements update the detail immediately, including notification
destinations. Missing-key copy is provided in all 23 interface languages.

The missing-key regression failed with `NotFound` before the fix and passed
afterward. New client tests use locally generated BLS certificates, device-signed
commands and HPKE ciphertext, including a different sending account, tampered-page
rollback and read/archive/restore HTTP acknowledgements. A mobile loopback test
verifies the content-pull/retry sequence and preservation of an unsent local note.

`cargo test --locked -p dmsg_client -p dmsg_mobile -p dmsg_cli` passed: 546 client,
17 mobile and 105 CLI tests; 11 tests remain explicitly ignored. All-target Clippy
with `-D warnings`, Rust formatting and diff whitespace checks passed. All 18 iOS
script tests, 23-language resource/notification checks and shared-icon checks
passed. Both ARM64 native slices and Swift bindings were rebuilt, and the native
artifact/source fingerprint was verified.

On iPhone 17e / iOS 27.0 Simulator with Xcode 27.2, all 57 Swift unit tests and
six ConversationTests passed (`target/ios-inbox/InboxTests.xcresult`). The first
inbox UI run exposed a test-selector mistake: tapping the full SwiftUI toggle
row did not toggle its child switch. After targeting the actual switch and
asserting its value, all four InboxUITests passed with no failures or skips
(`target/ios-inbox/InboxUITestsVerified.xcresult`). Coverage includes missing-key
failure/retry, read/archive/restore, slow loading, verified empty state and Arabic
RTL. Chinese dark, English light and Arabic RTL screenshots were exported and
inspected. Swift-extracted localization keys also passed the resource check.

Physical-device and production two-device delivery, private Worker acceptance,
TestFlight packaging/upload, unrelated iOS UI suites and desktop UI checks have
not been run.

Implementation and validation: Codex <codex@openai.com>.

## Sync retry review (2026-11-02)

Review of `7650e91..=2c07800` found that mobile manual sync shared the automatic
pause gate and could not retry a repaired failure or resume quota-paused uploads.
It also dropped the new `upload_error` when projecting session state. The mobile
entry now receives the existing Swift automatic/manual choice, keeps server busy
deadlines for both paths, and exposes upload failures through `syncError`.

Both defects were reproduced with failing tests before the fix. Three mobile tests
using a loopback HTTP relay now cover manual retry after an integrity failure, automatic
pause, continued incoming reads while uploads are blocked, explicit upload retry,
quota error presentation and server backpressure. `cargo test --locked -p dmsg_client
-p dmsg_mobile -p dmsg_cli` passed: 543 client, 16 mobile and 105 CLI tests; 11 tests
remain explicitly ignored. Formatting, diff whitespace and all-target Clippy with
`-D warnings` passed.

Desktop checks, all 159 unit tests, the control audit and production build passed.
The scoped sync and channel Electron suites passed with a loopback fixture and
temporary profile. Chinese light/dark, French and Arabic RTL screenshots were
inspected, including the 760 × 540 minimum window, retry/queued states and history;
the suites also exercised keyboard actions and navigation during delivery.

Both ARM64 native libraries and Swift bindings were rebuilt, and the source
fingerprint verified. The app built and launched on iPhone 17e / iOS 27.0 Simulator;
all 52 Swift unit tests and six ConversationTests passed, with zero failed or
skipped (`target/ios-review-sync/ReviewSync.xcresult`). The five conversation
screenshots were exported and inspected, covering queued sends after navigation,
Chinese dark retry, the history boundary, inline uncertain sends and slow refresh.

PocketIC/private Worker acceptance and physical-device/live two-device checks were
not rerun; the unrelated iOS UI suites were not run. Review, fixes and validation:
Codex <codex@openai.com>.

## Recent pages and session-owned delivery (2026-11-01)

Desktop and iOS now show saved messages first, fetch the latest bounded interval
for an uncached conversation, and fetch older intervals only when requested.
An explicit history cursor handles display-empty pages and stops at the beginning.
The recent verified interval remains separate from the complete delivery prefix;
loading the tail never reports missing history as delivered. Existing conversations
continue incrementally, and newly granted history still reopens.

Every outgoing intent is persisted before ordered delivery by the account session.
Navigation does not own the send task, the next draft stays editable, and verified
ACKs update message rows directly. Uncertain outcomes keep the original retry ID
inline. iOS pending rows have a different view identity from confirmed bubbles:
the Simulator regression exposed LazyVStack retaining the old sending view when
both used the same ID. Session resets clear presentation and stop queued work.
Fixed phase timings record elapsed milliseconds without content or resource IDs.

Final `cargo test --locked -p dmsg_client -p dmsg_mobile -p dmsg_cli` passed:
530 client, 13 mobile and 104 CLI tests, with 11 explicitly ignored tests.
New regressions cover bounded cold reads with and without a head hint, empty-page
cursor progress, history-gap merging, grant reopening, original send IDs and
immutable attachment versions. Formatting and Clippy (`--all-targets -- -D warnings`)
passed. Both ARM64 native slices and Swift bindings were rebuilt, and the source
fingerprint verified. All 23 localization catalogs and offline privacy copies passed.

On iPhone 17e / iOS 27.0 Simulator, 52 Swift unit tests and all six conversation
UI tests passed (`MessageExperienceVerified.xcresult`, zero failed or skipped).
Coverage includes consecutive sends before confirmation, leaving and returning
to a channel, replacement of pending rows by confirmed bubbles, inline uncertain
outcomes with the next draft preserved, slow reads, retry, empty channels and the
history boundary. The tests now leave the detail before tapping the tab bar,
which the software keyboard otherwise covers; pending containers also preserve
their retry button's accessibility identifier. English send/slow-read and Chinese
dark retry/uncertain-send and history screenshots were exported and inspected.
After aligning iOS with desktop so a verified local copy of an own message also
suppresses its pending row, all five sender unit tests and both sending UI tests
passed again (`MessageExperienceReceipt.xcresult`, seven passed, zero failed).

Desktop type/Svelte checks, all 155 unit tests and the production build passed.
The scoped Electron channel suite passed with a loopback fake daemon and temporary
profile: immediate durable queuing of consecutive sends, route recreation,
ACK display before later sync, original-ID retry, history failure/retry, discarded
late history replies and a terminal history boundary. Chinese light/dark, French
and Arabic RTL screenshots at the minimum 760 × 540 window were inspected, along
with keyboard retry and the history display. Old toasts were dismissed before
capturing queued-message screenshots so they do not obscure the tested rows.

These checks use local fixtures and do not establish live two-device latency.
No production message, cloud deployment, TestFlight upload or physical-device test
was performed; the complete unrelated iOS UI suite was not run.

Implementation and validation: Codex <codex@openai.com>.

## Channel history and send confirmation (2026-11-01)

Conversation scrolling no longer invokes pull-to-refresh at the history boundary.
Local pagination stops at the oldest page and preserves explicitly loaded history
across refreshes. Text synchronization completes independently of attachment
deletion metadata; desktop delivery reports also run outside the foreground read.
The shared core probes a slow send's original operation after two seconds and
reconciles a lost ACK without another publication. A matching durable receipt
stores the message and clears both send queues atomically. A safe GET rejected
with `POLICY_STALE` refreshes certified evidence and re-signs once; mutation
requests are not replayed by this policy recovery.

Both iOS native slices and the Swift bindings were rebuilt, and the source
fingerprint verified immediately after that rebuild. Concurrent account-pairing
edits appeared later in the workspace and are outside this native build's coverage.
On iPhone 17e / iOS 27.0 Simulator, 47 Swift unit tests and
all four ConversationTests passed. The XcodeBuildMCP call exceeded its 300-second
tool timeout, but the continuing xcodebuild process completed successfully;
`ChannelRegression.xcresult` reports 51 passed, zero failed and zero skipped.
Chinese history-boundary, Chinese dark retry and English slow-refresh screenshots
were exported and inspected. These are isolated presentation fixtures.

`cargo test --locked -p dmsg_client -p dmsg_mobile -p dmsg_cli` passed: 523 client,
13 mobile and 104 CLI tests, with 11 explicitly ignored tests (native integration,
fixture producers and large/performance runs). New regressions cover dropped and
stalled ACKs, absent or mismatched receipts, bounded policy refresh for reads,
no automatic mutation replay, terminal history and independent text completion.
`cargo check` for mobile/CLI, `cargo fmt --all -- --check`, and Clippy with
`--all-targets -- -D warnings` for client/mobile/CLI also passed. Clippy completed
after waiting for the shared build-directory lock; it did not bypass that lock.

Desktop `pnpm run check` and all 151 unit tests passed. The scoped channel Electron
regression passed, including stable retry IDs, early ACK display, history order,
keyboard retry, Chinese light/dark, French and Arabic RTL at the 760 × 540 minimum
window. Screenshots were inspected. Electron required an unrestricted test process
to launch; it used the smoke suite's loopback fake daemon and temporary profile.

No production messages were sent, no real-device or live two-device latency tests
were run, and no TestFlight build was uploaded. These checks do not establish
production latency or identify which remote hop delayed the reported messages.

## Organizer distribution environment (2026-11-01)

Build `0.1.0 (8)` was already exported successfully by `testflight.py`, but
Organizer's Validate App / Distribute App failed at `IDEDistributionCreateIPAStep`.
The distribution pipeline log showed `/usr/bin/rsync -8aPhhE` starting a Homebrew
3.5.0 peer that rejected Apple's `--extended-attributes`; ContentDelivery HTTP
200 responses did not indicate a completed validation or upload.

`scripts/ios/open-organizer.sh` opens the selected Xcode in a new process with
the same system-only PATH used by the command-line export. A new process is
necessary because opening the archive in an existing Xcode retains its old
environment. The helper leaves other Xcode sessions, shell settings and Homebrew
tools unchanged; the packaging output and README now point to this entry.

On Xcode 27.2 (27B52), the exact rsync flags reproduced the failure with a
Homebrew-first PATH and succeeded with a system-only PATH, preserving file bytes
and a test extended attribute. All 13 packaging tests, shell syntax and invalid
archive argument checks passed. The helper opened Organizer with build 8.
Re-exporting the unchanged archive succeeded; rsync and ditto both exited 0.
Both the original IPA and the new export passed ZIP integrity, SHA-256,
version/bundle checks and `codesign --verify --deep --strict`. All six bundles
have `get-task-allow=false`; the archived executable hashes match Organizer's
copy, and all six arm64 executable/dSYM UUID pairs match. Build numbers and
application sources were not changed.

The new IPA, export log, checksum and `verification.json` are retained under
`.build/TestFlight-0.1.0-8/Export-6tuciwot/` (excluded from Git). App Store Connect
Validate was run with the user's authorization in the new Xcode process and
passed: **dMsg 0.1.0 (8) validated — Your app successfully passed all validation
checks.** The GUI pipeline also recorded rsync exit 0. The old Xcode process was
quit normally after UI automation initially selected its unchanged environment;
use the new window when multiple Xcode processes are open. No TestFlight
distribution or review submission was performed. Swift/Rust rebuilds, Simulator
and physical-device tests were not run for this tooling-only change.

## Single account selection (2026-11-01)

Account → Switch account is the only workspace selector. Reminders now follow
the active session; their former account picker and switch action are removed,
as is the direct switch action in browser requests. Foreign notification and
browser targets retain their account hint and cannot change the active workspace.

Debug build and native artifact fingerprint verification passed. On iPhone 17e /
iOS 27.0, 47 unit tests and nine NotificationUITests ultimately passed. Two new
tests initially used an exact text query for a combined SwiftUI accessibility
label; after correcting the locator from the captured hierarchy, both reruns
passed. English, Chinese dark, German and Arabic RTL reminder screenshots and
both cross-account prompts were inspected. All 23 localization and notification
catalog checks passed. These are isolated fixtures; physical-device and live
multi-account push delivery were not tested, nor was the complete iOS UI suite.
See the [shared validation record](../dmsg_desktop/docs/notifications-validation.md).

Implementation and validation: Codex <codex@openai.com>.

## Channel timeline loading (2026-11-01)

The conversation now reads verified local pages during a remote refresh, with
explicit loading, empty and retry states. The Simulator channel UI tests cover
a first page arriving before a deliberately stalled refresh completes, an
initial failure followed by retry, and an empty channel. All three passed on
the iPhone 17e simulator (iOS 27.0). Light English and dark Chinese screenshots
were inspected. These use isolated Debug fixtures, not a production account;
physical-device and live cross-device message delivery were not tested.

## Apple 回调合同同步（2026-10-30）

本次只更新公开接口与发布文档。基于本地 `c0a09dff`，静态核对
[`dmsg_mobile::apple`](../dmsg_mobile/src/apple.rs)、[`PurchaseModel`](App/PurchaseModel.swift)、
[`MobileDeployment`](Platform/MobileDeployment.swift) 和 [`project.yml`](project.yml)：原生端已经
统一调用 Core 的设备接口，不需要更改 Rust/Swift 调用路径或引入 Apple 服务凭据。当前 TestFlight
固定连接生产，内购加载、购买、恢复和交易监听/提交保持关闭；Debug staging 仍为显式选项。

[公开移动合同](../../docs/protocol/mobile_zh.md#31-app-store-服务端通知)补充由 Core 接收的 App Store
Server Notifications V2 地址、签名验证和可靠接收语义，审核文档与清单同步。已检查文档相对链接、
新锚点和 `git diff --check`。未重跑 Rust、Swift、模拟器或真机测试；未拉取远端，未部署，
未更改 App Store Connect，也未执行真实购买、续订、退款或回调验收。

## Device replacement, transfers and wallet keys (2026-10-30)

Adds the iPhone screens for replacing a device and moving to a new one. Devices
links to Replace a device (review a new device's request with what only this
device holds, approve it once its device ID is entered, then run, leave or stop
each side's steps) and to Transfer between devices (receive, send to a session
code and device ID with an optional address, resume a stopped transfer). A new
iPhone asks to replace an administrator device from the access screen (QR code,
share, copy). A copy of this account's administrator device approves this iPhone
before it joins, and its file library imports afterwards. Files and the
device-bound setting warn when files exist only here. Each wallet gains This
iPhone's keys: the native Taproot key root and its protectors, moving native
addresses through L1 or L2, replacing a key this device does not hold, and
unbinding revoked devices. `dmsg_mobile` adds `succession.rs`, and wallet unbind
approvals become decidable on the phone.

`cargo fmt --check`, `cargo clippy --all-targets -D warnings` and
`cargo test -p dmsg_mobile` (11 tests; new: wallet unbinding is decidable on the
phone) passed. Both ARM64 slices and bindings were rebuilt and the source
fingerprint verified. On iPhone 16 Pro / iOS 18.4, the full `xcodebuild test`
passed: 45 unit tests and 29 UI tests, 1 skipped (it needs a regular-width iPad),
0 failures. Three new UI tests (Devices' handover entries in English and in dark
Arabic, a replacement request before joining in Chinese) and the two access-action
tests then passed after the six new screens moved to inline titles: the English
large title of Transfer between devices was cut off, and several translations of
the other titles are longer still. Screenshots of Devices, Replace a device and
Transfer between devices in English and in dark Arabic RTL, and of the Chinese
request with Create request enabled, were inspected. Each of the 23 localizations
gained 137 keys; the catalogs, the Swift string extraction and 12 Python tests
passed. Not covered: fixture mode has no core, so these screens show only their
entry points. Creating, reviewing and approving a request, running, leaving and
stopping steps, transfers, copy imports and the wallet key screens were not
exercised on iOS; the mobile wrappers call the same `dmsg_client` operations as
the desktop but were only compiled here. Nothing ran on a physical device: Face
ID, the Secure Enclave, the local network prompt, Bonjour on a real network and a
live two-device handover remain unverified.

Implementation and validation: Claude Opus 5.5.

## Notification consent, setup prompt and privacy text (2026-10-29)

Closes three client gaps before remote notifications are enabled publicly; the
notification policy itself is unchanged. The offline privacy policies (23
languages, byte-identical to the website) now list what the notification service
processes and when it is cleared. `PrivacyInfo.xcprivacy` adds Product Interaction
for view receipts; the README maps the App Store Connect labels. Turning on an
account while another local account already receives notifications shows a native
confirmation that names the other accounts. Reminders and Login identities show a
non-modal security-alert prompt that only opens settings; "Not now" persists per
account in the private preferences file, and a newly bound login shows it again.

Rust core unchanged; both ARM64 slices and bindings were rebuilt in a clean
worktree and the source fingerprint verified. On iPhone 16 Pro / iOS 18.4,
`xcodebuild test` with `dMsgTests` and `NotificationUITests` passed: 41 unit tests
(new: alert-state rule, dismissal persistence with older preference files) and
6 UI tests (new: consent in English and in Arabic dark, prompt set aside in
Chinese), 0 failures. Screenshots of the Chinese prompt and the Arabic RTL
confirmation were inspected. 23 localizations with offline privacy parity and
notification copy parity passed. Not covered: the prompt in Login identities
(fixture mode has no security engine), a real second-account registration
against the service, and anything on a physical device.

Implementation and validation: Claude Opus 5.5.

## Notification review fixes (2026-10-29)

Reviewed the inclusive public range `29b37f46..4e93a9af`. Notification navigation
now reads the original business object when the local reminder has not arrived,
has been read, or has completed. Original-account and unlock checks remain.
Quiet hours use actual IANA timezone intervals through repeated and nonexistent
wall-clock times. Shared receiver cleanup survives local account removal and
rechecks replacement registrations before deleting credentials.

Both ARM64 Rust slices and bindings were rebuilt and source fingerprints verified.
XcodeBuildMCP built and launched the Debug fixture app on iPhone 16 Pro / iOS 18.4;
39 native unit tests and the 3 notification UI tests passed, with no failures or
skips. The quiet-hours test includes fall-back 01:45 before the second 01:30 and
the spring-forward 02:30 boundary. All 23 localizations, 31 shared icons and 12
Python tests passed. The tests do not exercise authenticated live-account push
clicks; real APNs, lock-screen/cold-start delivery and multiple devices remain
unverified. Full shared results are in the desktop notification validation record.

Review and implementation: Codex <codex@openai.com>.

## Unified notifications (2026-10-29)

Continues public `main@00dbfdf7` and the existing uncommitted notification work.
No remote repository or production environment was fetched or changed.
The native settings/reminder views now use named Rust APIs, shared durable policy,
versioned settings and original-account navigation. Foreground callbacks do not
schedule another local copy of shared APNs events. Actual loaded business views
report visible versions; a reminder list or push click does not mark business read.
Channel read cursors advance only on the visible latest message, and changing a
channel name preserves a timed mute. Offline route resolution retains retry.

- Both ARM64 Rust slices and Swift bindings rebuilt; debug artifact/source
  fingerprints verified. Swift simulator build succeeded on iPhone 16 Pro / iOS 18.4.
- Rust: client 417 passed / 5 existing ignored, daemon 96 passed / 6 existing ignored,
  mobile 10 passed. Formatting and all-targets Clippy with warnings denied passed.
- 39 native unit tests passed, including real UniFFI settings/CAS/session fences,
  opaque route and exact version decoding, cross-midnight quiet hours and DST.
- All 23 notification catalogs match the desktop vocabulary and APNs keys.
  Existing translations were preserved except the intentional generic notification
  wording update. Localization extraction, icon parity and 12 Python tests passed.
- The first three UI tests reached the settings but tapped the SwiftUI switch row's
  label center. The test now taps the native thumb (leading in RTL), and finds
  Save again after the pending-sync explanation changes the scroll layout.
  All three UI flows then passed: English light, Chinese dark and Arabic RTL.
  Screenshot review caught a missing generic Open translation; all 23 catalogs
  now include it and Swift extraction passes. Final screenshots are retained in
  `.build/notification-shots-localized/`.

These tests use local presentation fixtures and a real local Rust bridge without
an authenticated live account. Real App Attest/APNs, physical-device permissions,
Focus, lock screen, token replacement, revoked-account cold starts and multiple
registered accounts remain unverified. Wallet reminders require member-authenticated
reads; an independent remote wallet source is not implemented. Android is deferred
until its host is merged into main. No signed archive, upload or deployment was run.

See the [shared acceptance matrix](../dmsg_desktop/docs/notifications-validation.md)
for N01–N32 evidence and remaining platform gates.

## Second-stage languages (2026-10-28)

Added Vietnamese, Thai, Filipino, Malay, Bengali, Urdu, Polish and Dutch using
per-language native resources. The app and all five extensions now contain
23 locales. The existing 15 locales are unchanged. Permissions, shortcuts and
offline privacy policies include the eight additions. System language matching
covers regional preferences and `tl-PH` → `fil`; Urdu follows native RTL.

- `scripts/ios/localizations.py --derived-data ...` and all 12 Python tests passed.
  This checks complete keys, format arguments, plural forms, Swift extraction
  and parity with the website's privacy policies.
- Debug simulator build and five targeted tests passed on iPhone 16 Pro / iOS 18.4:
  all locale bundles, regional matching, native plural formatting (including
  Polish 1/2/5/12/22, Dutch and the new Asian languages), 19 access-screen locales,
  and the language entry before login and in Account.
- All 276 compiled `Localizable.strings` / `Localizable.stringsdict` tables
  (six bundles × 23 locales × two tables) match source after property-list decoding.
- Reviewed screenshots of all eight new access-screen languages across light
  and dark appearances. Result bundle: `.build/LocalizationPhase2Final-20261028.xcresult`.

Copy review focused on concise actions, software/security terminology, counts,
placeholder integrity and avoiding literal English metaphors. Native-speaker
proofreading, physical devices, the full iOS test suite and Release/TestFlight
packaging were not run for this expansion.

## Per-language localization resources (2026-10-28)

Replaced the large application `Localizable.xcstrings` with 15 language folders
under `Resources/Localizations`. Each contains `Localizable.strings` (997 plain
messages) and `Localizable.stringsdict` (three plural entries), exported with
Apple's String Catalog compiler. No translated values or production Swift
lookup calls changed. InfoPlist and AppShortcuts keep their existing catalogs.

- All 180 compiled resource tables in the app and five embedded extensions
  match the pre-migration compiler output after property-list decoding.
- Twelve Python tests passed, including new checks for missing translations,
  format arguments, plural fallback forms and numeric types. The 15-language
  resource check, Swift compiler-extraction check and offline privacy parity
  check passed.
- Debug simulator build/launch and five targeted tests passed on iPhone 16 Pro
  / iOS 18.4: resource availability, language matching, native plural formatting
  (including Russian and Arabic), localized access screens, and language settings.
  The Traditional Chinese launch screenshot was inspected.

The files are maintained as native UTF-8 XML property lists; no generated
combined catalog or runtime loader is needed. New keys must be added to the
language files; compiler-extraction validation detects omitted Swift keys.
Physical-device tests, the full iOS suite and Release/TestFlight packaging were
not repeated for this resource-only refactor.

## Fifteen localizations (2026-10-28)

Added Hindi, Indonesian, Brazilian Portuguese, Japanese, German, Korean,
Italian, Turkish and one Traditional Chinese catalog (`zh-Hant`). Native
per-app language settings remain authoritative. String Catalog entries marked
stale are also checked: custom views still use many of them at runtime.
Conditional labels, dialog titles and secure-field prompts now use localized
keys instead of the verbatim String overload.

- Simulator build and four targeted tests passed on iPhone 16 Pro / iOS 18.4:
  bundled resources, regional/script matching, 11 localized access screens,
  and language settings before login and within the account screen.
- Catalog/compiler-extraction checks cover all 15 locales, application and
  extension permissions, shortcut placeholders and matching offline privacy
  policies. Original translations and catalog metadata were preserved.
- Light/dark screenshots were inspected, including Traditional Chinese,
  Devanagari, Japanese, Korean, long labels and Arabic RTL.

Translations were drafted with machine translation and reviewed for core
terminology, placeholders and representative interfaces. Native-speaker review,
physical devices, the full iOS test suite and Release/TestFlight packaging were
not part of this localization validation.

## One-command TestFlight packaging (2026-10-27)

`python3 scripts/ios/testflight.py` increments all app/extension build numbers
from the local maximum, reuses the optimized archive helper, exports an App Store
Connect IPA, validates its bundle versions and writes a SHA-256 file. `--dry-run`
shows the next number without changing files. The script retains failed build
numbers/logs and never uploads or submits a build.

Validation:

- Seven Python packaging tests passed, including mixed app/extension numbers,
  preservation of unrelated text, dry-run, existing-output protection, failed
  archive handling, export options and stale/missing extension rejection.
  These run from `scripts/ios/verify.sh`; export tests use fixture IPAs and do
  not establish Apple signing eligibility.
- The real dry-run found app build 4 / extension build 3 and planned build 5.
  The first actual attempt retained build 5 after the execution sandbox denied
  SwiftPM cache access. A retry with expanded execution permissions produced a
  successful optimized, development-signed Release archive for `0.1.0 (6)`.
- Both automatic export and one direct export retry failed with Xcode's
  `No Accounts` / missing `iOS Distribution` certificate errors in this execution
  environment. **No build 6 IPA was produced or uploaded.** The archive and
  stage logs remain in `.build/TestFlight-0.1.0-6/`; final distribution export
  still needs verification in a session with working Apple signing access.
- Shell syntax, ShellCheck for the updated verification script and whitespace
  checks passed. App unit/UI tests and device acceptance were not rerun for this
  packaging-only change.

## Secret and file detail navigation hangs (2026-10-27)

The old implementation reproduced a stalled second secret detail on iPhone 17 /
iOS 27.0 with a delayed local read, and a permanently loading detail after
background interruption on iOS 18.4. The collapsed split view mixed list
selection, copied detail selection and manual column changes. Detail tasks also
ignored the session epoch/foreground state, so an invalidated result could leave
the existing page loading forever.

Compact widths now use a `NavigationStack`; regular widths retain a
`NavigationSplitView`. Vault, files and messages share one navigation selection.
Secret detail reads follow item/session/visibility changes, reject late results,
and offer retry after failure. Managed keys remain masked and read-only.

Validation:

- iPhone 16 Pro / iOS 18.4: all 32 native unit tests and 8 selected UI tests
  passed. These cover the Rust bridge/session fences, repeated managed-secret /
  ordinary-secret / file navigation, background recovery, cancelled-read retry,
  leaving a pending read, lock clearing, recovery banners, drafts and nested
  snapshot navigation through rotation.
- iPhone 17 / iOS 27.0: 5 selected UI tests passed, including repeated navigation,
  foreground recovery, retry, dark Chinese and Arabic accessibility text.
- iPad Pro 13-inch / iOS 27.0: 2 UI tests passed, covering selection replacement
  during a pending read, favorites and draft preservation through rotation.
- Debug and Release simulator builds, six-language catalogs, icon parity,
  existing native-artifact/source fingerprints and diff whitespace checks passed.
  Native screenshot attachments were inspected; exported copies are in
  `.build/navigation-fix-20261027/` and `.build/navigation-fix-ipad-20261027/`.

The UI tests use fictitious local presentation data and inject delayed/cancelled
reads; they do not exercise a real account's network or biometric flows. Rust
sources were unchanged and Cargo tests were not rerun. No physical-device test,
distribution archive or TestFlight upload was performed for this fix.

## Compact lists, access guidance and TestFlight build 3 (2026-10-27)

Vault, channel, inbox, file, approval and device lists use smaller icon tiles
and shared row insets instead of stacked vertical padding. Ordinary two-line
rows target 56 pt and retain at least 44 pt of tappable content. Titles and
previews can grow at accessibility text sizes; inbox dates move below the
sender in that mode. The access footer now uses two adaptive rows, a smaller
language glyph and the shorter Language label in all six languages. Its notice
distinguishes first sign-in, workspace creation, existing-account authorization
and returning unlock instead of presenting device approval to every new user.

Validation and delivery:

- 49 native tests passed, none failed or skipped, on iPhone 16 Pro / iOS 18.4.
  This includes the compact-row height check, normal startup guidance, language
  entrypoints, Arabic accessibility text, rotation, lock and draft regressions.
- Six-language catalogs/compiler extraction, icon-source parity, optimized Rust
  artifact/source fingerprints, plist and diff checks passed. Rust sources were
  unchanged; Cargo tests were not rerun.
- A signed Release archive and App Store IPA export succeeded for version
  `0.1.0 (3)`. All six distribution signatures/profiles were verified, including
  no debugging entitlement or device-list restriction. Production backend,
  APNs/App Attest, disabled purchases and the opaque app icon are retained.
  Screenshot fixture flags are absent from the Release executable.
- Build 3 uploaded successfully and completed Apple processing. App Store
  Connect shows Ready to Submit and the existing internal dMsg group with one
  tester. Bilingual testing notes describe these changes and production access.
  No new testers were invited and no App Store review was submitted.
- 24 RGB PNGs replaced the English (US) and Simplified Chinese store screenshots:
  each locale has five 1206 × 2622 iPhone images, five 1320 × 2868 iPhone images,
  and two 2064 × 2752 iPad images. Native views use fictitious presentation data;
  their content was inspected. The temporary capture test was removed afterward.

Local outputs: `.build/CompactLists-build3-20261027.xcresult`,
`.build/TestFlight-build3/` and `.build/AppStoreScreenshots-build3-20261027/`.
The exported IPA is 21,737,224 bytes; SHA-256
`75e61f596d803cbc890952ef2bdc579c93c639b8627aa78e1fcb13a3ec39fe35`.
Identical promotional PNGs and a SHA-256 manifest are saved under the repository's
local `misc/dmsg-ios-screenshots/2026-10-27-build3/` directory. User feedback
confirmed that the language link reaches the system language picker. Physical
device biometric behavior and installation of this TestFlight build were not
validated by this run.

## Automatic unlock and language settings (2026-10-27)

Cold startup and foreground return now request the existing system quick-unlock
flow once when the workspace is locked and quick unlock is enabled. A cancelled
or failed attempt stays on the access screen with manual retry. Inactive/active
authentication transitions do not rearm the request; explicit and idle locks
suppress it until the next background visit. Presence and key validation remain
in the existing Rust/AppleSecurity path.

The access screen and Account include an App language link using the public
iOS settings URL. `UIPrefersShowingLanguageSettings` keeps the system language
option visible even with one preferred device language. The installed bundle
contains all six localizations and the setting was checked in its built plist.

Validation: 49 native tests passed (32 unit, 17 UI; none skipped), including
three automatic-unlock policy tests and language-entry checks in English and
Chinese. Debug build/launch, six-language catalogs/compiler extraction,
icon-source parity and plist/diff checks passed. Language-entry screenshots
were inspected in `.build/startup-language-20261027/`; the full result bundle is
`.build/StartupLanguage-20261027.xcresult`. The language link opened the system
Apps settings list; manual navigation to the language choices was interrupted
by the Mac locking. Real-device biometric/cancellation behavior, an actual
language change, Release archive and TestFlight upload remain unverified in
this pass. Rust sources were unchanged; Cargo tests were not rerun.

## Business icon alignment (2026-10-27)

Navigation, main account destinations, related list/detail/empty states and
all eleven vault item types now use the desktop's Remix paths. Template SVGs
retain vector rendering and native tinting. `scripts/ios/sync-icons.py --check`
checks the shipped assets, license and vault type mapping against the desktop.

The Debug build succeeded and six targeted navigation UI tests passed on
iPhone 16 Pro / iOS 18.4, covering locking, message drafts across rotation,
files, Chinese dark appearance, Arabic accessibility text and the public demo.
The English vault, Chinese dark account, files and Arabic detail screenshots
were inspected. Artifacts are in `.build/icon-alignment-20261027/` and
`.build/IconAlignment-20261027.xcresult`. No desktop implementation changed.

## App icon proportions (2026-10-27)

The centered D mark now measures 630 × 720 px on the 1024 px canvas (70.3%
height, previously 51.6%). Its paths, colors and background are unchanged.
`AppIcon.svg` and the generated RGB PNG are synchronized; the PNG has no alpha.
The 48 px and 64 px previews and the installed Home Screen icon were inspected.
The Debug build and launch succeeded on iPhone 16 Pro / iOS 18.4 Simulator.
Comparison and Home Screen images are in `.build/icon-proportion-20261027/`.
Unit/UI suites, a physical-device check and a new TestFlight upload were not run
for this asset-only change.

## TestFlight upload and store screenshots (2026-10-27)

Source changes were committed on local main as `6970564d`; the opaque icon repair
is `2961c27e`, both with Codex co-author trailers. The repaired `0.1.0 (2)` archive
was uploaded successfully and Apple processing completed. App Store Connect shows
build 2 as Ready to Submit under TestFlight. Bilingual testing instructions explain
production access, passkey self-registration and disabled purchases. No tester
group was created, no invitations were sent, and no App Store review was submitted.

20 original simulator screenshots were saved in the store version's media slots:
English (US) and Simplified Chinese each have four 1206 × 2622 iPhone screenshots,
four 1320 × 2868 iPhone screenshots, and two 2064 × 2752 iPad screenshots. These
show actual views populated with fictitious in-memory presentation data. The RGB
PNG dimensions/no-alpha format and visible content were inspected. The temporary
XCTest capture driver was removed after capture; it is not part of the app.

Local artifacts are under `.build/TestFlightProduction-build2-opaque/` (the repaired
archive, IPA and upload verification record) and `.build/AppStoreScreenshots-20261027/`
(screenshots and manifest). The repaired IPA is 21,627,651 bytes, SHA-256
`4a611d99390bff85a899d803f5e492782be85ae90f06aa37bf00ac3c682a79c2`.
It supersedes the earlier local IPA with the transparent icon. The App Store
Connect media slots, not TestFlight's tester-feedback screenshots, contain the
uploaded marketing screenshots. This is delivery evidence, not review approval.

## App Store icon validation repair (2026-10-27)

The first upload attempt for build 2 was rejected before acceptance because the
1024px app icon contained an alpha channel. The icon is now rendered from the
existing dMsg vector paths in `AppIcon.svg` onto a full opaque green canvas, with
RGB PNG output and system-applied corner masking. The 1024 × 1024 dimensions and
absence of an alpha channel were checked, and the rendered icon was inspected.
The build number remains 2 because the original binary was not accepted.
This asset-only repair does not change the previously tested native behavior.

## Production TestFlight, purchases disabled — build 2 (2026-10-27)

At the user's request, Release/TestFlight now use the fixed production backend.
Apple's sandbox receipt no longer chooses a staging business environment. Testers
register through Internet Identity with their own passkeys; no shared test account
or StagingDeployment.json is required. Explicit Debug staging remains optional.
This supersedes the staging requirement recorded for the earlier build below.

The current build sets `DMSG_IN_APP_PURCHASES_ENABLED=NO`. It hides subscription,
storage-purchase, restore and purchase-management controls from Plan and usage;
existing entitlement/usage reads remain available. Product queries, purchase,
restore, unfinished/update listeners and transaction delivery all return before
StoreKit/service work. Production's rejection of Sandbox payment facts is unchanged.

Validation performed:

- 45 distinct native tests passed across the full run and a targeted rerun:
  29 unit tests and 16 UI tests. Two initial UI assertions looked for a separate
  `Free` text element; SwiftUI exposes LabeledContent's label/value together.
  Screenshots showed the correct data, and the corrected accessible-element
  queries passed in both English light and Chinese dark appearance.
- Normal app startup with the real Rust bridge reached an enabled Internet
  Identity login button without a staging configuration or presentation fixture.
  Disabled purchase loading, restore and listeners returned without work. Public
  demo/privacy, lock, sync, files and existing navigation regressions passed.
- Six-language catalogs, formats, compiler-extracted keys and offline policy
  parity passed. Optimized native artifact/source fingerprints, plist/JSON and
  diff whitespace checks passed. Rust source was unchanged; Cargo tests were not
  rerun in this configuration/UI pass.
- A signed optimized Release archive and App Store IPA export succeeded. All six
  bundle signatures/profiles were verified for `S9Z2WAMAV8`, version `0.1.0`, build
  `2`, Apple Distribution, no debugging entitlement and no device-list restriction.
- The exported main app has production APNs/App Attest, private Keychain group
  first, `DMsgUseStaging=NO`, `DMsgInAppPurchasesEnabled=NO` and the existing
  `ITSAppUsesNonExemptEncryption=false` declaration. No staging configuration,
  `sandboxReceipt` routing string or debug fixture launch flags are shipped.

Outputs (excluded from Git): `.build/TestFlightProduction-build2/dMsg.xcarchive`,
`.build/TestFlightProduction-build2/Distribution/dMsg.ipa` and
`.build/TestFlightProduction-build2/verification.json`. The IPA is 21,688,771 bytes.
Export used the system-only command-local PATH described below. The version was
not uploaded to TestFlight or submitted for review. Physical-device passkey
registration, installed TestFlight behavior and live service acceptance were not
performed by this run; no production account was created or deleted.

## Encryption declaration and signed export (2026-10-27)

Starting source: local main `a075ff4c`, plus the documentation-exemption setting
recorded in [ExportCompliance.json](Review/ExportCompliance.json). App Store
Connect's questionnaire displayed that no encryption document upload was needed
for the recorded answers and storefronts. This is not a BIS classification ruling
or an Apple review approval. No compliance code was issued.

- Optimized host, ARM64 device and ARM64 simulator Rust libraries built; generated
  bindings and the transitive source/artifact fingerprint verification passed.
- The Release archive succeeded with the main app and five extensions, signed
  with the team's Apple Development identity.
- App Store IPA export succeeded with Apple Distribution signing for team
  `S9Z2WAMAV8`. All six bundle signatures were verified; each reports version
  `0.1.0`, build `1`, and a distribution profile with debugging disabled and no
  device-list restriction. The main app's APNs and App Attest entitlements are
  production, and its private Keychain group remains first.
- `ITSAppUsesNonExemptEncryption=false` was checked in both the archive and the
  exported IPA. Debug UI launch flags were absent from the main executable.
- Two initial exports failed in packaging: Apple's `rsync` invoked the Homebrew
  `rsync` server, which rejected `--extended-attributes`. Exporting with the
  command-local `PATH=/usr/bin:/bin:/usr/sbin:/sbin` succeeded. No global tool or
  security setting was changed.
- Plist/JSON validation and diff whitespace checks passed. This configuration
  change did not rerun the complete Rust or native test suites.

Local outputs (excluded from Git): `.build/AppStore-20261027/dMsg.xcarchive`,
`.build/AppStore-20261027/Distribution-SystemTools/dMsg.ipa` and
`.build/AppStore-20261027/verification.json` (bundle checks and IPA SHA-256).
The IPA is 21,691,925 bytes. Nothing was uploaded to TestFlight or submitted for
App Review. The user confirmed that the separate test deployment is not yet
deployed; `Resources/StagingDeployment.json` remains absent. Signing success
does not establish service deployment, real-device acceptance or review access.

## Native capabilities and review requirements (2026-10-27)

This pass starts from local public main 5242f364 and preserves the pre-existing
plist/catalog edits. It implements the [Chinese checklist](IMPLEMENTATION_CHECKLIST_zh.md)
and [mobile contract](../../docs/protocol/mobile_zh.md). Private service implementation
stays in its own repository. No production deployment, real payment, push delivery or
account deletion was performed.

| Check | Observed result |
| --- | --- |
| Client Rust | 384 passed, 5 ignored |
| Mobile Rust | 10 passed |
| Protocol | 84 unit tests, 20 signing tests and 3 doctests passed |
| Types | 18 contract tests and 3 doctests passed |
| Commerce | 41 passed, 2 ignored |
| CLI | 92 passed, 6 ignored in the shared-core regression run |
| Strict lint | Client/mobile/CLI/commerce/types/protocol all-target Clippy with warnings denied passed |
| Formatting | Selected Rust packages pass cargo fmt check |
| StoreKit canister adapter | Real optimized commerce Wasm: authority rejection, account/environment binding, duplicate subscription/storage transactions, status freshness, refund rollback refusal and same-version stable-state upgrade passed in PocketIC |
| Private services | Frozen dependency install and full workspace format/type/test/dry-run-build checks passed in both the isolated source copy and the final real checkout; core 407, Apple gateway 5, agent kit 24, discourse 82 plus 3 script checks, mail 289, knowledge 681 |
| Private writes | Source-only patches passed automatic approval after review/tests, with source/destination hash checks; deployment was not part of the write |
| Website | Svelte check: 0 errors, 0 warnings; 99 tests passed, 1 skipped |
| Connect bridge | Type check, 10 unit tests and production asset build passed |
| Native Rust artifacts | Both ARM64 device/simulator libraries were built with the optimized Release profile; transitive local source, binding and library hashes verified |
| Translation/data preservation | All pre-existing plist values and localization values preserved; six-language catalogs and offline policy copies checked |
| iPhone 16 Pro, iOS 18.4 | 27 unit tests and 13 UI tests passed, 0 failed or skipped; Chinese dark, Arabic accessibility text, concealed fields, public demo and offline privacy screenshots inspected |
| Optimized device Release | Unsigned archive succeeded with all five extensions; optimized Rust profile/hash verification passed; debug fixture markers absent from the app executable |
| iPhone Duo, iOS 27.1 | All 13 distinct UI cases passed across two runs; the seven initial selector/visibility failures passed after tests handled the system side rail and scrolled to offscreen settings |
| iPhone 16e, iOS 18.4 | Five targeted cases passed: Chinese dark long profile, Arabic large text/access actions, hardware page and demo/privacy; screenshots inspected |
| iPad mini, iOS 18.4 | Four targeted cases passed: long Chinese profile, rotation/draft continuity, Arabic large text and demo/privacy; screenshots inspected |

Private channel tests timed out in runs sharing the machine with compiler or
simulator work. Both the isolated source copy and the final real checkout passed
full validation without that competing load; test timeouts were not raised.
Native checks caught and fixed missing API conformance,
an overlarge Swift view expression, and a directory-URL trailing-slash comparison
that incorrectly rejected valid staged files. Tests retained those regression cases.

Account deletion refreshes the remote encrypted inventory and channel controls
without flushing or uploading local changes. Optional wallet lookup failure is
shown explicitly and requires an independent access acknowledgement. Editable
public profile data is cleared before permanent service closure. Public identity/
ledger history and shared copies have separate retention boundaries.

The user and directory canisters' stable layouts were not modified. The commerce
test above upgrades its actual current Wasm and checks its new receipt/configuration
storage; it is not presented as an old-to-new user/directory migration test.

The shipped local demo is intentionally public and uses fictitious in-memory data.
It is not a production authentication bypass or proof of full App Review access.
Actual App Store products and test deployment IDs have not been supplied. Sandbox
distribution uses a separate configured Staging workspace; production commerce
still refuses sandbox purchase facts. App Attest, APNs and StoreKit environments
are independently configured for development, TestFlight and App Store.

Unrun external acceptance remains explicit: real Secure Enclave/biometric ACLs,
signed App Groups/Keychain access, actual credential/File Provider handoff, APNs,
App Attest, Apple purchases/renewals/refunds, physical-device background scheduling,
store submission and production readiness. No minimum-device latency or peak-RSS
numbers are claimed from source inspection or simulator tests. The full repository
PocketIC suite and desktop Electron UI checks were not repeated in this mobile pass.

Native evidence: XcodeBuildMCP results dated 2026-10-26 UTC (2026-10-27 local),
16:43:21 full iPhone suite; 16:50:27 and 16:56:21 Duo batches; 16:59:11 narrow
iPhone batch; 17:01:06 iPad batch. The unsigned device archive is generated at
`.build/FinalRelease/dMsg.xcarchive` and is excluded from Git. Initial archive
execution was blocked from writing the Xcode cache; the approved retry passed.
The archive contains Share, Widget, Credential Provider, File Provider and its
authentication UI. This is a build result, not a signing/entitlement acceptance.

Implementation and verification: Codex (OpenAI).


Updated: 2026-10-26. The original `codex/ios-app` implementation was based on
`58197c35` and rebased onto `537e28ae`. Historical checks are retained below;
the hardware-security and Secrets integration has its own verification section.

The original implementation used the locally revised iOS brief v0.2 and compact UI
prototype, not an assertion that the private product baseline has released
mobile support. Brief SHA-256:
`348f5470e6a86ae860d63adda446bf31ce1669b1af8dd38b4ae31bbe6b61b558`.
The private baseline was read from a clean local checkout at `cf72a3f`;
the remote was not checked. Its contents are not reproduced here.

## Original implementation checks (before rebase)

Toolchain: Xcode 27.1 (27A9275), Rust 1.98.1, UniFFI 0.32.2,
XcodeGen 2.46.0. Minimum deployment target: iOS 18.

| Check | Result |
| --- | --- |
| Rust iOS dependencies | Device and ARM simulator compilation succeeded, including SQLite, aws-lc, rustls platform verification and IC dependencies |
| XCFramework | Device and simulator slices plus generated Swift bindings built together; ABI exercised by XCTest |
| Release configuration | ARM64 simulator build passed; debug fixture identifier absent from app binaries. This used the tested debug Rust slice and is not an optimized-Rust performance or signed-distribution result |
| `dmsg_client` | 331 passed, 5 ignored |
| `dmsg_cli` | 78 passed, 5 ignored; macOS Secure Enclave subprocess check passed in the final run |
| `dmsg_mobile` | 3 passed: cancellation and generation-bound handle rejection |
| `dmsg_protocol` | 81 unit tests, 20 signing integration tests and 3 doctests passed |
| Formatting and strict lint | `cargo fmt --check`; client/mobile/CLI all-target Clippy with `-D warnings` passed |
| Web | 98 passed, 1 skipped; Svelte/TypeScript check: 0 errors, 0 warnings |
| Desktop | 110 unit tests passed; Svelte/TypeScript check: 0 errors, 0 warnings |
| Desktop Electron smoke | Passed with `DMSG_DESKTOP_NATIVE_FULLSCREEN=0`; original run stalled at native macOS fullscreen exit and was stopped. That OS window transition is not a pass |
| iPhone 16 Pro, iOS 18.4 | 12 XCTest/XCUITest cases passed, 0 failed, 0 skipped |
| iPhone Duo, iOS 27.1 (24A94425) | All 12 distinct XCTest/XCUITest cases passed across batches; RTL passed, and the draft case passed after its navigation setup learned to open the system sidebar |
| String Catalog | All 322 entries have en, zh-Hans, fr, es, ru and ar localizations |

Ignored tests include vector generators, local profiling, native WebDAV mounts,
FSKit and external signing-tool exercises. They are not counted as passes.
The test suite's internal Secure Enclave helper is already included in the CLI
count, not counted a second time.

## Rebase verification

Checks ran after integrating `1ac82a94`; the later proposal-only commit does not
change the tested client, protocol, native app or dependency sources.

The rebase preserves main's device-bound workspace format and hardware-policy
checks while retaining prepare/presence/complete fast unlock. Completion checks
both the session generation and the bound slot. Cancelling Host work drops its
temporary approval authorization. Desktop unlock preparation now uses `Hold` so
a failed account switch also invalidates the previous display snapshots; starting
an unlock renews activity before waiting for the system prompt.

At that rebase, the mobile adapter rejected biometry-only requests instead of downgrading them to
device-passcode fallback. Hardware approval-key signing was not yet implemented
by the iOS host. The 2026-10-26 integration below replaces those limitations.
Chinese UI terminology follows main's “保险库” naming.

| Check after rebase | Result |
| --- | --- |
| `dmsg_client` | 357 passed, 5 ignored |
| `dmsg_cli` | 92 passed, 6 ignored; final daemon-focused rerun: 26 passed |
| `dmsg_mobile` | 4 passed, including refusal to downgrade a biometry-only policy |
| `dmsg_protocol` | 84 unit tests, 20 signing integration tests, 3 doctests passed |
| Formatting / lint | Rustfmt and all-target client/mobile/CLI Clippy with `-D warnings` passed |
| Desktop / web login | 114 desktop unit tests and 8 web login contract tests passed |
| iOS bridge rebuild | Device and ARM64 simulator XCFramework slices rebuilt from the rebased Rust source |
| Swift / real Rust bridge | 8 XCTest cases passed on iPhone 16 Pro / iOS 18.4, 0 failed, 0 skipped |

The earlier Release, Duo UI matrix and Electron smoke results above describe
the original implementation; those full checks were not repeated for this rebase.

## Hardware-security and Secrets integration (2026-10-26)

Native approval keys, one-decision authorizations, current-biometry-only ACLs,
optional device-bound unlock, certified recovery warnings, selective secret
snapshot delivery/import, references, attachments, health/rotation and local
activity are implemented. The local architecture document is now v0.3 and remains
in the main checkout's ignored `misc` directory at the user's explicit request.

| Current check | Result |
| --- | --- |
| `dmsg_client` | 373 passed, 5 ignored (including main's old-file-link compatibility regressions) |
| `dmsg_mobile` | 10 passed |
| `dmsg_cli` | 92 passed, 6 ignored |
| `dmsg_protocol` | 84 unit tests, 20 signing tests and 3 doctests passed |
| Strict lint | Client/mobile/CLI all-target Clippy with `-D warnings` passed |
| Native artifacts | ARM64 device and simulator slices, generated bindings and the Swift app built successfully |
| iPhone 16 Pro / iOS 18.4 | 21 distinct XCTest/XCUITest cases passed: the 19-case suite plus 2 targeted lifecycle tests; 0 outstanding failures |
| iPhone Duo / iOS 27.1 | 3 targeted UI cases passed together: Arabic accessibility text, recovery-warning navigation/tab reachability, and explicit snapshot selection through rotation |
| Release configuration | ARM64 simulator app built successfully with test fixtures excluded; Rust slices use the tested development profile |
| Swift/Rust crypto vectors | CryptoKit verifies the three public Rust hardware commitment vectors; altered messages and the wrong prehash convention fail |
| Localization | 593 entries complete in six languages; String Catalog compilation passed |

The core regression additions exercise bound login/slot changes, late results,
cancelled hardware preparation, one-decision authorization release, refusal of
unprepared authentication, stronger ACL selection during key replacement,
package limits before download/approval/execution, receipt reuse, local history
pagination and refusal of secret packages through ordinary attachment saving.
Identity transitions also fence both entry and completion, preventing an old
or intermediate handle from reaching a replacement account. Tests cover delayed
polling, transition failure/cancellation, stale foreground resume, and keeping a
newer background pause intact. Swift invalidates cached views when the device
identity changes, even if the account ID stays the same.

Swift tests verify recovery freshness, account/generation isolation, retained
warnings on unknown observations, privacy lifecycle and real FFI rejection while
locked. UI fixtures verify no automatic snapshot selection, persistence through
rotation, hardware-page honesty without data, and navigation remaining reachable
under a recovery warning. A failed snapshot UI test was traced to a tap on the
middle of the Form row; tapping the actual switch confirmed the behavior and the
test now checks selection both before and after resizing. Visual QA also corrected
the warning layout and field-reference controls consuming content width.
Duo required an explicit switch to the detail column after ordinary list
selection; its system BackButton also differs from a conventional iPhone navbar.
The final targeted suite covers both. Background suspension/immediate lock is
synchronous, and a cancelled UI operation cannot clear a newer operation's busy
state. A copied value survives background lock until its normal expiration.

Final native evidence: iPhone suite `2026-10-26T02-05-42`, lifecycle cases
`2026-10-26T02-26-21`, Duo cases `2026-10-26T02-27-09`, Release build
`2026-10-26T02-29-24`. These are local XcodeBuildMCP result/log timestamps,
not assertions about production deployment or physical-device validation.

The shared approval-key status view keeps its existing shape; mobile-only local
binding/permission details use the explicit owner-security view. The pre-existing
main-checkout change to `approval_key_tests.rs` was not edited by this task and
was later committed by the user in `38694aba`. Delivery also includes main's
`3e60938d` file-link compatibility fix. The full Rust suite was rerun after
integrating those commits, and native libraries were rebuilt from that source.
The real Rust/Swift bridge smoke test passed again on the rebuilt artifact
(`2026-10-26T02-58-05`).
The full earlier Electron smoke matrix was not rerun for this integration.

## What the earlier tests establish

Rust tests cover cancellation during network work, stale queued operations,
concurrent lock generations, dropped callers, replay-safe send intents,
attachment integrity, strict fast-unlock policy and the fixed login callback
contract. A stale Swift handle cannot start a mutation after the session changes.

Native tests call the real Rust bridge for locked-content rejection, cancellation,
pause/resume, generation invalidation and rejection of a foreign callback. Swift
tests also check clipboard ownership, preserved interrupted imports, persisted
lock preferences, exact decimal display and inbox decoding. UI tests use explicit
Debug-only presentation fixtures for navigation, hidden fields, rotation with a
draft and Arabic accessibility text. Those fixtures neither unlock Rust nor prove
an authenticated network flow.

Manual checks used Apple's actual Duo simulator in Device Hub, not an iPad
substitute. Closed, open and partially open layouts retained the selected secret.
A message draft remained present with the keyboard open while moving from a
partially open portrait screen to the closed outer screen and back to open.
System tabs and keyboard insets followed those transitions. Initial vault
navigation shows its sidebar; selecting an item returns visibility to system
adaptation. A portrait regular-width tab can collapse its sidebar, which remains
reachable through the system's Show Sidebar control. Favorites no longer have
their selection cleared by the list.

The iOS 18.4 result bundle includes usable screenshots named `Secret fields
remain concealed` and `Arabic accessibility layout`, containing synthetic data.
The Duo screenshot API and XCTest attachment returned black frames in this
environment; they are not accepted as visual evidence. Duo visual checks used
the visible Device Hub window. Local build products and screenshots are excluded
from Git. The manual GitHub workflow was not dispatched.

## System web authentication isolation (2026-10-26)

A physical-device report showed `transaction -> get -> fetchSubnetKeys` failing
inside II's `id.ai` page, while the user confirmed Safari login worked on the
same phone. The served II bundle uses IndexedDB for this cache and propagates
cache read errors into authentication. The screenshot does not identify why
the IndexedDB connection failed.

The native session now requests ephemeral website storage to avoid reusing
persisted browser state between login attempts. This is a client-side mitigation;
the fixed HTTPS callback and Rust delegation checks are unchanged. Debug builds
for the ARM64 iOS simulator and generic iOS device passed with signing disabled.
After installing this change, the user confirmed that II login succeeded and
returned to dMsg on the affected physical iPhone. The test suite was not rerun;
this confirms that reported login path, not the full device/security matrix.
No II deployment was changed.

## Approved device and first unlock (2026-10-26)

The user confirmed desktop approval of the iPhone, followed by
`EXECUTION_UNKNOWN` on authorization refresh, II login and system unlock. After
enabling a VPN, system unlock entered the workspace with the same approved
device. This establishes that the reported failure depends on network access;
it does not identify a particular blocked host or transport failure. No device
revocation, re-pairing or workspace reset was needed.

The initially empty workspace was also verified: automatic sync was off, and
the user confirmed that Account > Sync now brought the existing account data
onto the iPhone. Unlock and content synchronization remain separate actions.

The mobile host now forwards only `dmsg::diagnostic` records validated by the
core's allowlist to the native console as `dmsg: <source> <reason> <status>`.
Other library logs and arbitrary messages are rejected. The authorization and
root-decryption rules are unchanged.

Validation: all 10 mobile Rust tests passed; mobile all-target Clippy with
`-D warnings`, Rustfmt and `git diff --check` passed. Device and ARM64 simulator
Rust libraries and bindings were regenerated, and the Swift simulator app built
successfully. Native UI tests and a new signed-device installation were not run
for this diagnostic-only change.

## Sync feedback, access layout and account content (2026-10-26)

Manual sync now owns a visible spinner and localized waiting label through
completion/failure; locking clears that state and a late completion cannot clear
a replacement operation. Access actions use equal widths and a primary/secondary
hierarchy. Mobile login and binding are fixed to `dmsg.net`; `panda.fans` and old
data migration remain desktop workflows at the user's explicit direction.

The Account header loads the official name and avatar with the core's bounded,
anonymous image reader. Files exposes the verified cloud-backup inventory inline;
manual and opted-in synchronization refresh that inventory, while file restoration
still requires a separate selection. Profile and backup results are guarded against
cancelled or replaced sessions, and lock clears cached backup metadata.

- 28 distinct XCTest/XCUITest cases passed across the full run and targeted
  reruns, with no remaining failures. The first file-row test could not address
  the restore button; an explicit bordered button and removal of the parent
  accessibility identifier corrected it. A new Chinese test initially
  used the wrong tab translation and passed after using the actual label.
- Screenshots were inspected for Chinese access actions, the account header,
  manual sync progress, inline cloud files, dark Chinese long names, and dark
  Arabic at the largest accessibility text size on iPhone 16 Pro / iOS 18.4.
- All 10 mobile Rust tests, mobile all-target Clippy with `-D warnings`, Rustfmt
  and diff whitespace checks passed. Device/simulator Rust slices and bindings
  were rebuilt. The simulator Debug app and unsigned device Release app built;
  test-fixture markers were absent from the Release executable. Rust slices used
  the development profile, so this is not a release-performance measurement.
- The user installed the update on the affected iPhone and confirmed that the
  account name, avatar and desktop-origin cloud backups all appeared correctly.
  A real-device restore/download of one of those backups was not separately tested.

## Apple device handover review (2026-10-30)

- Exporting the file library now goes through the existing high-risk approval
  flow. Permission names the receiving device, session and exact manifest,
  is consumed once, and is checked again after connection setup. Transfer
  reset clears prior status; delayed discovery and advertising results cannot
  revive a stopped session. Resuming preserves a manually entered address.
- Succession refresh observes enrollment and completed root rotation. Initial
  wallet bindings are named before handover; those approvals require a fresh
  succession request, with the superseded draft retained as cancelled. The old
  wallet binding remains until native-address work finishes. Content with
  skipped files or unsynced vault changes cannot settle the content step.
- ARM64 device and simulator Rust libraries and Swift bindings were rebuilt;
  artifact/source fingerprints verified. The Debug app built and launched on
  iPhone 16 Pro / iOS 18.4. All 45 unit tests and three handover UI tests passed
  (48 total, no failures or skips), including live simulator Bonjour discovery,
  CryptoKit vectors, English and dark Arabic handover entries, and the Chinese
  replacement request before joining. The exported screenshots were inspected.
- All 23 localizations passed resource and compiler-extraction checks; the 12
  localization/artifact Python tests passed. Shared-core regressions and two
  real desktop-daemon handover tests also passed against local PocketIC and the
  test relay. The user-canister schema 16-to-17 upgrade and Candid compatibility
  were verified with the historical Wasm; this does not claim production rollout.
- Physical Secure Enclave migration/restore, biometric and device-password
  behavior, transfer between two real Apple devices, and Bitcoin regtest L1/L2
  sweeps were not exercised by this review. Simulator fixtures do not validate
  those hardware or chain paths.

## File backup and download recovery (2026-11-02)

Requested file transfers now belong to the account scene, survive navigation,
and resume from encrypted local journals after unlocking or returning to the
foreground. Completed chunks are retained; transient failures respect retry
deadlines. The UI distinguishes uploading, cloud commit confirmation, downloading
and verification, and only reports completion after the corresponding core result.
Explicit URLSession ciphertext transfers retain bounded per-slot retry state and
do not race the foreground runners; their OS scheduling and unlock-to-finalize
boundary remain visible in the UI.

- 12 selected XCTest/XCUITest cases passed on iPhone 17 Simulator: file-transfer
  state, retry deadlines and persistence, content-sync lifecycle, and four UI
  cases. After the final title and progress presentation changes, the two
  file-transfer unit tests and all four UI cases passed again. Navigation retains
  the account-owned progress. English light, Chinese dark, French long labels and
  Arabic RTL screenshots were exported and inspected. Fixtures model 150 MiB
  commit/verification phases; they do not transfer that payload over the network.
- Shared Rust validation passed: client 557 tests (5 ignored), daemon 111 tests
  (6 ignored), mobile 21 tests; all-target Clippy with `-D warnings` and Rustfmt
  passed. Regression coverage includes interrupted downloads, cached-chunk reuse,
  integrity failure, server retry deadlines, per-chunk upload acknowledgement and
  ambiguous commit reconciliation.
- Device and simulator Rust artifacts and Swift bindings were rebuilt; source
  fingerprints verified. The final simulator Debug app built successfully.
  All 23 localizations passed validation.
- No physical-device background URLSession delivery, large-file memory/energy
  measurement, production cloud transfer or release build was performed in this
  review. Tests do not establish iOS background scheduling guarantees.

## Vault browsing, details and editing (2026-11-03)

The 23-task vault audit is implemented across the native screens and shared core.
The old four-item favorites grid is removed; ordinary recovery/sync messages no
longer insert or remove list rows. The toolbar retains the complete status and
explicit recovery/conflict/retry actions. Existing fonts, colors, navigation and
managed-key restrictions are retained.

| Audit task | Delivered behavior |
| --- | --- |
| H01 | Overlay status feedback, fixed toolbar entry, persistent urgent state and one active copy/save feedback host, including sheets and split columns. |
| H02 | Full searchable/paginated Favorites scope, no independent four-item query; Chinese uses 收藏. |
| H03 | Scrollable All/Favorites/Passwords/Note scopes, all 11 types, complete-query counts, selected type and type-aware creation. |
| H04 | Query-bound asynchronous results, pagination, distinct first-sync/loading/failure/empty/search states and recovery actions. |
| H05 | Exact-tag filtering, full-inventory name/update sorting, safe type-specific summaries and favorite/conflict/sync markers. |
| D01 | Per-value absolute OTP expiry, independent of later field operations; clipboard lifetime never extends the code's validity. |
| D02 | Tombstone metadata, deletion returns to browsing, trash detail is read-only and offers restore. |
| D03 | Current detail follows revision/content changes, conceals revealed fields and announces an update; an open editor retains its draft and original expected revision. |
| D04 | Session-scoped dirty drafts, Cancel/swipe discard decision, preserved inputs on failure, disabled mutation during saving and compatible type changes. |
| D05 | Ordered field sections, field reorder/delete and named custom fields. |
| D06 | Type, favorite, tags, update date, version/sync state and attachment count with direct editing and management entries. |
| D07 | Long/multiline/large-text values get separate action rows; machine-readable values use LTR; contextual accessible controls retain 44-point targets. |
| D08 | Separate contextual reveal/copy confirmations, waiting/failure/success feedback and local clipboard expiry. |
| D09 | Typed keyboards, optional date/month picker with explicit commit/cancel, inline validation, retained/replaced/cleared secret semantics and persistent labels. |
| D10 | Historical timestamp/device/sync/conflict metadata, current/version comparison and selected-field merge with expected-revision checking. |
| D11 | Managed-key and historical read-only explanations, unavailable/retry/empty states and no reveal/copy action for known empty fields. |
| F01 | Import totals, duplicate/skipped/incomplete/selection counts, search, nonduplicate selection, retryable staged preview and links to imported records. |
| F02 | Trash/history/conflict lists distinguish loading, failure and empty results; restoring has progress and feedback. |
| F03 | Local secure password generation and reviewed OTP setup-key/URI/QR entry. |
| F04 | Login detail links to explicit AutoFill snapshot selection; expiry and system authentication remain unchanged. |
| F05 | Local findings resolve item/field names and offer edit/detail/dependency actions; editing uses the same protected draft sheet. |
| Q01 | Real encrypted-core inventory/tombstone/conflict regressions plus an operational UI fixture for filtering, saving, deletion, OTP, incoming revisions and drafts. |
| Q02 | All 23 language catalogs, small/regular-width screens, light/dark, Arabic RTL and maximum accessibility text, keyboard shortcuts and native accessibility audit. |

The core tests browse 135 encrypted items across all 11 types, including nine
favorites and multiple pages. They verify that filters apply before pagination,
counts cover the entire query, update sorting changes after editing, conflict
markers clear after resolution, tombstones stay out of the main list, locking
interrupts reads, and summaries do not include hidden values. The mobile test
uses real create/delete/restore operations and verifies masked native metadata.

- Rust client: 560 passed, five existing ignored tests; mobile: 22 passed.
  All-target Clippy for both crates with `-D warnings` and Rustfmt passed.
  Device/simulator ARM64 libraries and Swift bindings were rebuilt, and the debug
  native artifact/source fingerprint verified.
- Swift: all 81 unit tests passed. All 14 vault UI cases passed across the
  acceptance batch and corrected targeted reruns on iPhone 16e / iOS 18.4;
  recovery navigation and both synchronization/cooldown cases also passed.
  UI regression scope includes complete
  favorites, draft discard/retry, OTP expiry after copying another field,
  deletion, stable row position while feedback disappears, optional date
  cancellation, an incoming revision refreshing current detail without replacing
  an open draft, keyboard creation/save and native accessibility checks for hit
  regions, traits and sufficient element descriptions. The incoming-draft test
  initially queried an inaccessible underlying list; its final assertion checks
  preserved input during the incoming update and the latest head after discard.
- iPhone SE (third generation), iOS 18.4: Chinese dark browsing/detail/editor,
  optional date cancellation, Arabic RTL synchronization, Chinese dark sync and
  maximum Arabic accessibility text passed. iPad Pro 13-inch, iOS 18.4: rapid
  selection, complete favorites and a single visible copy notice passed.
  Exported screenshots were inspected; this caught and removed an empty recovery
  row in the status sheet and overlapping scaled field-action icons.
- iPhone Duo, iOS 27.1: repeated managed-secret/file navigation, background
  recovery, explicit snapshot selection across rotation and leaving a pending
  detail read passed. The last case was rerun after making the test wait for the
  system's separate BackButton instead of tapping the first toolbar action.
- Debug and Release simulator configurations built. Release excludes all
  presentation fixtures; this compilation used the locally rebuilt debug Rust
  library and is not a distribution-archive validation. All 23 localizations
  passed resource/compiler-extraction checks, all 31 business icons matched the
  shared source, and notification-copy checks passed for all 23 languages.

UI fixtures perform local in-memory mutations and do not access a production
account. They establish native state and interaction behavior, not cloud delivery
or physical-device security. Real camera scanning, VoiceOver speech, cross-app
clipboard expiry, biometric AutoFill, live imports/sharing, cross-device cloud
conflicts, minimum-device performance and physical Duo posture changes were not
exercised. Native accessibility checks cover the stated categories, not a blanket
accessibility certification. No App Store archive, distribution signing or
production deployment is implied by simulator builds.

Implementation and validation: Codex <noreply@openai.com>.

## 2026-11-03 review of e9f26c44 and 67bea5fe

The follow-up review fixed seven regressions in the account and vault changes:

- Foreground policy refreshes retain an explicitly selected biometrics-only
  export policy; a required workspace policy can still strengthen it.
- Form drafts use the workspace view generation instead of the request epoch.
  A short background visit keeps drafts and subsequent edits; locking, account
  changes and protected-data loss still clear them. Notification refreshes also
  preserve unsaved input.
- Retrying profile publication preserves an unsubmitted draft and its expected
  version. Feedback distinguishes the published saved document from that draft.
- An expired pending recovery permits a new request. Completion is offered only
  within this device's recovery window, or to resume an already completed takeover.
- OTP scanning requests camera permission before checking scanner availability,
  and offers a localized manual-entry fallback when access is unavailable.
- Version history resolves local device labels in the current account's scope.
- Repeated notices receive distinct revisions, so an earlier timer cannot clear
  a later copy confirmation with the same text.

Validation performed on iPhone 16e / iOS 18.4:

- `cargo test --locked -p dmsg_client -p dmsg_mobile`: 560 client tests passed
  with five existing ignored tests; all 22 mobile tests passed.
- All 82 Swift unit tests passed. The account suite ran all 14 UI cases, alongside
  vault save-retry and Chinese dark editor/detail cases. This 98-test run had one
  failure: copying the account ID again after navigating to the profile lost its
  notice. After fixing notice revisions, that case, the single-host vault copy
  case and five recovery-policy unit tests passed (7/7).
- Earlier focused runs also passed optional-date cancellation and profile retry
  feedback. The final account run covers returning from background with a profile
  draft, notification edits and a biometrics-only export choice.
- Exported screenshots were inspected for expired recovery, preserved profile
  drafts and feedback, biometrics-only selection, and Chinese dark vault detail
  and editing. These are presentation fixtures, not production accounts.
- All 18 iOS script tests, 23-language resource/Swift-extraction checks,
  notification-copy checks, 31 shared-icon checks, native artifact fingerprint
  verification and `git diff --check` passed.
- Final Debug and Release simulator builds passed. Release compilation used the
  verified debug Rust library; this was not a distribution archive validation.

The full account test tool exceeded its 300-second wait, but Xcode continued and
completed; its log and result bundle were checked directly. Physical camera
permission/scanning, biometric Keychain ACL enforcement, live publication and
recovery were not exercised. The broader iPad/small-screen/RTL/VoiceOver matrix,
distribution archive/signing and production deployment were not rerun.

Review, fixes and validation: Codex <noreply@openai.com>.

## 2026-11-03 messaging and letter workflows

Implemented the 22 messaging audit items listed in
`IMPLEMENTATION_CHECKLIST_zh.md`: sent history, encrypted local drafts, pending
send management, explicit contact reopening, invitation routing, conversation
search and reply navigation, attachment composition, permissions, form state,
identity display and localized accessible controls. Sender history and drafts
remain device-local; this change does not claim replicated sent history.

Validation performed:

- `cargo test --locked -p dmsg_client -p dmsg_mobile -p dmsg_cli`: 698 tests
  passed (564 client, 22 mobile, 112 CLI), with 11 existing ignored tests. The
  three letter tests were rerun successfully after consolidating draft-save
  timestamps. Coverage includes persisted drafts, atomic draft-to-intent moves,
  original-request retries, uncertain send status, contact reopening, attachment
  filename search beyond 100 matches and attachment reply references.
- `cargo clippy --locked -p dmsg_client -p dmsg_mobile -p dmsg_cli --all-targets
  -- -D warnings`, Rust formatting and `git diff --check` passed.
- Rebuilt the native device/simulator libraries and Swift bindings; native
  artifact fingerprint verification passed. All 23 localization resource and
  Swift-extraction checks, notification-copy checks and 31 shared-icon checks
  passed.
- iPhone 16e / iOS 18.4: all 86 Swift unit tests and all 19 selected message UI
  tests passed (105 total, `dmsg-messaging-final.xcresult`). The UI suites cover
  inbox and conversation flows, draft restoration, sending and reopening a sent
  letter, attachment selection before sending, filename search, changed
  invitation input, read-only permissions, Chinese dark mode, Arabic RTL and
  French accessibility-size text.
- iPhone SE (3rd generation) / iOS 18.4: all three layout cases passed, including
  opening a letter, scrolling and replying in Chinese dark mode, Arabic RTL and
  French accessibility-size text. This device's successful run is recorded in
  `dmsg-messaging-layout-verified.xcresult`; that multi-device bundle also contains
  an earlier failing iPad run and is not an overall passing bundle.
- iPad Pro 13-inch (M4) / iOS 18.4: the final four cases passed in
  `dmsg-messaging-ipad-final.xcresult` (the three layout cases plus saving a draft,
  sending and reopening it). These runs found and verified fixes for the hidden
  initial messages sidebar and a mismatch between letter selection tags and
  detail routes. The split-view changes were verified in this final iPad run
  after the iPhone runs above.
- After the final draft-write ordering and suspended-operation cleanup fixes,
  all 86 Swift unit tests and five messaging UI cases passed again on iPhone 16e
  (91 total, `dmsg-messaging-draft-final.xcresult`). Done now joins the existing
  autosave queue, and suspension releases stale in-progress controls while
  retaining unsaved drafts for foreground recovery.
- Exported screenshots were inspected for small-screen and iPad letter lists,
  received/sent details, reply composition, Chinese dark mode, Arabic RTL and
  French accessibility-size layouts. The large-text sender/copy controls use
  separate rows. These screenshots use presentation fixtures, not live accounts.

An initial simulator-tool wait timed out while Xcode continued running; final
results were collected from the completed command-line runs and result bundles.
Earlier UI failures from navigation timing, localized selection and offscreen
lazy rows were corrected before the successful runs above. No production letters
were sent. Physical-device APNs/background behavior, real two-device delivery,
manual VoiceOver/physical-keyboard navigation and a distribution archive were
not exercised in this pass and remain separate acceptance checks.

Implementation and validation: Codex <noreply@openai.com>.

## 2026-11-06 file requests

Added **Collect files** to the Files tool menu: the saved links and then the
relay's, a composer that makes one link per name, and a link's details with its
files. A received file stays in the cloud until someone saves it. Saving runs in
the foreground, keeps the session active and goes to
`Received files/<title>/<label>`; there is no background path and no folder
choice on iOS.

Validation performed, on iPhone 17e / iOS 27.0:

- All 101 Swift unit tests passed, including decoding the shapes the core sends.
  `FileLibraryUITests` ran 29 tests with one skipped and no failures
  (`Full-20261105T181004Z.xcresult`). The four new cases cover the free plan
  leading to the plans, one link per name with a failure keeping the remaining
  names, saving a file only on request, closing a link, and Chinese dark mode,
  Arabic RTL and German.
- `scripts/ios/localizations.py` passed all 23 localizations, with compiler
  extraction from this run's Debug products. Pointing it at the whole shared
  DerivedData reports two `missing extracted key` lines, both from a 2026-11-03
  Release `SystemExportsView.stringsdata` older than the current copy; neither
  text is in the sources.
- Exported screenshots were inspected for the list, composer and details. They
  led to dismissing the keyboard that covered Create, anchoring each confirmation
  on its own row, hiding the empty state on a free plan and using text-only
  actions where German would wrap. These screenshots use presentation fixtures.

The UI tests use fixtures, so the iOS flow has not run against a real relay.
`desktop_file_request` takes the shared client core through a local relay, but
through the daemon rather than the mobile bindings. iPad, physical devices, a
Release build, VoiceOver and a real account were not exercised in this pass.

Implementation and validation: Claude Opus 5.5 <noreply@anthropic.com>.

## 2026-11-06 design and experience review

Carried out the iOS design and experience review; `IMPLEMENTATION_CHECKLIST_zh.md`
lists the changes and where they differ from the review. All results below are
from simulators (iOS 27.0, Xcode 27.2) on a 16 GB, 10-core Mac, using UI fixtures.

- iPhone 17, full suite: 239 tests, 0 failed, 2 skipped (both need a regular-width
  iPad), in 1426 s (`R-final-*.xcresult`). Later changes affect the split layouts
  and the tests, so these were rerun on iPhone 17 afterwards: all 106 unit tests
  and 14 UI tests passed, with the iPad-only one skipped.
- iPad Pro 11-inch (M5): 39 tests from `FileLibraryUITests`, `SecretNavigationTests`,
  `AccountNavigationTests` and `MessagingUITests`. The first run failed 13, which
  led to two fixes: the Vault list started collapsed on a portrait iPad, unlike
  Messages and Files, and a file's preview could not open its details because the
  detail column had no navigation stack. A rerun of the 15 affected tests leaves
  3 that rely on a Back button a split layout does not have; those now skip there.
  The other UI test classes were not run on iPad.
- `scripts/ios/localizations.py` passed all 23 localizations, with compiler
  extraction from this run's Debug products. The check found two missing keys
  ("Size" and the AutoFill explanation), now translated. Pointing it at the whole
  DerivedData still reports the 2026-11-03 Release copy noted above.
- `sync-notifications.mjs --check`, `sync-icons.py --check` and the 23 script tests
  passed. No Rust changed, so the cargo steps of `verify.sh` were not rerun, and
  `verify.sh` itself was not run from start to finish.
- Exported screenshots were inspected for dark Chinese, Arabic RTL, maximum
  Dynamic Type and the iPad Files split. They use presentation fixtures.

Test speed: the suite no longer takes about an hour on this machine.

| Run | Workers | Result | Time |
| --- | --- | --- | --- |
| Before, estimated from 62 tests summing 1548 s | 1 | not completed | about 60 min |
| All changes, 19-language access test | 4 | 239 tests | 1402 s |
| All changes, 6-language access test | 3 | 239 tests | 1330 s, 1426 s |

- Each class runs on its own simulator clone, so `FileLibraryUITests` and
  `NavigationTests` were split into smaller classes. `verify.sh` allows one worker
  per 5 GB and two cores, at most four. On 16 GB, four workers finished no sooner
  than three: swap reached 25 GB and the load average passed 700.
- `UITests/FastWaits.swift` replaces XCTest's wait, which looks only once a second.
  Automatic screen recording is off; a failure still attaches a UI snapshot and
  hierarchy. `testExpandedLanguageAccessScreens` keeps one language per script and
  text width, as `localizations.py` checks every catalog.
- `app.launch()` remains over half of a UI test: about 2 s to terminate, 3 s for the
  automation session and 3 s to go idle. `UIView.setAnimationsEnabled(false)` made
  no measurable difference (idle waits 104.4 s and 105.1 s over the same 18
  launches) and was removed.

Waiting less exposed races the one-second wait had hidden. Most were in the tests.
One was in the app: a notification settings refresh could overwrite a change made
while it was loading, which matters now that changes save themselves. It now keeps
such a change, and the form no longer waits for the system's permission answer,
which took over 5 s on a freshly booted simulator. No test covers that overwrite
directly, and `testKeyboardCanCreateAndSaveAnItem` failed once under full load and
passed alone and in the later full run.

Real devices, Face ID, the camera, the extensions and widgets as installed,
VoiceOver, physical keyboards, a Release build and a real account were not exercised
in this pass. `dmsg-ios.yml` was not run on GitHub.

Implementation and validation: Claude Opus 5.5 <noreply@anthropic.com>.

## 2026-11-06 review of 50fa5be

Reviewed the complete commit locally and fixed four regressions:

- Sensitive field actions now check the confirmation deadline when tapped. Only
  explicit confirmation starts the 30-second interval; another read does not
  extend it.
- OTP fields in version-history details keep updating their expiry display, even
  though only the current item's codes refresh automatically.
- Clearing the AutoFill picker now removes its exported snapshot, system
  suggestions and core selection through the existing removal path.
- Leaving notification settings cancels only the pending debounce. A save that
  has started finishes, and an obsolete workspace's form cannot start a new save.

Regression evidence on iPhone 18 Pro / iOS 27.0, Xcode 27.2:

- The final targeted run (`Review-50fa5be-fixed.xcresult`) ran all 107 Swift unit
  tests and 13 UI tests: 119 passed and one failed. The original notification
  foreground test failed its initial toggle assertion before pressing Home,
  and again after returning. An unchanged isolated rerun passed
  (`Review-50fa5be-foreground-retry.xcresult`); this remains evidence of a UI-test
  timing flake, not a clean first run. No tests were skipped.
- Before the fixes, the 30-second confirmation test and the delayed notification
  save/navigation test failed at their behavioral assertions. The first history
  test stopped at navigation; its fixture and test entry were corrected, then its
  expiry assertion passed with the fix. These are local UI fixtures, not live
  accounts or real notification delivery.
- The new credential-store unit test invalidates a real temporary export and
  checks that even an already opened credential can no longer be read. The
  AutoFill picker's empty-selection routing was also checked in source; this
  does not establish installed-extension or system-authentication behavior.
- All 23 iOS script tests passed. Catalog validation passed for all 23 languages,
  including this build's Debug Swift extraction. Notification copy and all 35
  business icons matched their desktop sources; `git diff --check` passed.
- Exported screenshots were inspected for the expired confirmation, hidden
  historical OTP, completed save after leaving, Chinese dark vault/settings,
  Arabic RTL settings and Arabic dark second-account consent.

The full UI suite, other simulator models/OS versions, physical devices, real
Face ID/Touch ID, installed system extensions, Release builds, Rust tests and
the end-to-end `verify.sh` were not run for this review. Rust was unchanged and
the existing local XCFramework was reused. No deployment or store submission.

Review and fixes: Codex <noreply@openai.com>.

## Remaining implementation and release gates

This is a working native implementation and local validation record, **not a
claim that every P0–P3 milestone or the App Store release is complete**.

- Release the frontend with the login page, fixed callback page and the AASA
  for `S9Z2WAMAV8.net.dmsg.ios` (committed in `static/.well-known`). Verify the
  id.ai popup/passkey flow and full fragment callback in
  `ASWebAuthenticationSession` on a physical phone. The affected-phone login and
  return were confirmed by the user after the isolation change above; the wider
  device and identity-provider matrix remains unverified.
- Verify iOS Secure Enclave, device-password fallback, locked-device data
  protection, cross-app paste, cross-device edits/conflicts, approval/revocation
  and recovery against a real account. Also verify actual Face ID/Touch ID ACLs,
  biometric changes, handle portability, signed key enrollment and the real
  one-prompt behavior. Software vectors and macOS SE success do not establish
  iOS hardware behavior. Check that user home supports the current AccountInfo
  contract before protected-account validation.
- APNs registration, the authenticated wire contract and provider adapter now
  have local implementations as recorded above. Deployment, real provider delivery
  and physical-device validation remain open. Foreground synchronization is
  available without push permission.
- Legacy account/control methods use the cancellable dedicated owner. The full
  shared-`Hold` conversion remains incomplete; local reads may queue behind a
  network operation. Lock/pause cancel it before clearing keys.
- Agent administration, wallet execution, purchasing, system extensions,
  long-running background transfers and mobile-browser connect redirects retain
  separate implementation/release gates. Paid-letter approvals are disabled;
  free letters support receiving, composing, local drafts and sender history;
  paid sending remains disabled.
- Minimum-device performance and large-file memory budgets, Duo physical Touch ID,
  live activities/camera reserved regions, App Store account deletion,
  reporting/blocking, privacy labels and distribution review remain unverified or
  require the corresponding service/product work. Local canister upgrade
  coverage for device succession is recorded above; deployment remains separate.

The 8 MiB secret-package ceiling limits payload buffers, not process RSS. Chunk
buffers, the manifest, Swift UI metadata and password derivation have additional
costs; minimum-device peak memory remains a physical-device acceptance check.
Foreground recovery checks are independent of content-sync opt-in and cannot
promise notification delivery while the app is suspended or protected data is
unavailable. Desktop-local approvals and grants are not mirrored to the phone.

Automatic foreground synchronization is on by default while the app is open and
unlocked; each account can turn it off. File backup still needs a separate
explicit choice. Interrupted import staging is retained rather than erased on
startup. The current synchronization scope is also described in the README.

Implementation and validation: Codex (OpenAI).

## 2026-11-06 one-tap copy

Copying a vault field now takes one tap on iOS, in details and on list rows, as it
does on the desktop. Only revealing a sensitive field asks; one confirmation covers
showing the item's fields for 30 seconds, and a copy does not start that interval.
Copies keep their protections: `localOnly`, expiry within 30 seconds and clearing on
lock. The three strings of the removed dialog left all 23 catalogs.

On iPhone 18 Pro / iOS 27.0 with native libraries rebuilt from the current source,
`-only-testing:dMsgUITests/VaultUITests` ran 19 tests with no failures, including
`testCopyingASensitiveFieldTakesOneTapAndConfirmsNothing` and
`testOneConfirmationCoversShowingTheItemForThirtySeconds`.
`testListRowCopiesThePasswordInOneTap`, added afterwards, passed in a targeted run.

`scripts/ios/localizations.py` passed on the source. With `--derived-data` it reports
removed keys while `.build/DerivedData` still holds a Release build from 2026-11-03,
so that check passed on a copy holding only the current Debug extraction.

The full UI suite, the Swift unit tests, copying from a row's context menu, other
simulators and physical devices were not run.

Implementation and validation: Claude Opus 5.5 <noreply@anthropic.com>.


## 2026-11-07 iPad presentation feedback loop and App Store preparation

App Store screenshot capture reproduced a UI hang on iPadOS 18.4 when moving
from Files to Account, opening its sidebar, or entering an account page.
SwiftUI body-change logging showed repeated `noticeHosts` invalidations of
`RootView`, followed by regenerated workspace selection bindings. A main-thread
sample stayed in SwiftUI/AttributeGraph updates.

`WorkspaceOverlays` now owns transient presentation and geometry observation,
so changing the active feedback host does not rebuild the workspace navigation.
`PrivateScreen` registers/unregisters a host only when membership actually
changes. The existing iPad split layout is retained. Temporary diagnostics were
removed. The regression test repeats Files → Account → Account security → Vault
three times; the existing copy-feedback test checks a single visible notice.

Final simulator verification (122 test executions, all passed):

| Device / runtime | Checks | Result bundle |
| --- | --- | --- |
| iPhone 16 Pro / iOS 18.4 | 111: unit tests plus copy feedback, lock hiding content and notification draft preservation | `.build/WorkspaceOverlays-iPhone.xcresult` |
| iPad Pro 11-inch M4 / iOS 18.4 | 2: repeated account navigation and copy feedback | `.build/WorkspaceOverlays18.xcresult` |
| iPad Pro 13-inch M4 / iOS 18.4 | 3: repeated account navigation and both screenshot languages | `.build/WorkspaceOverlays18-original-device.xcresult` |
| iPad Pro 13-inch M5 / iOS 27.0 | 4: repeated account navigation, copy feedback and both screenshot languages | `.build/WorkspaceOverlays27.xcresult` |
| iPad Pro 13-inch M5 / iOS 27.0 | 2: final screenshots, waiting for the account lock toolbar before capture | `.build/Store110-final-toolbar.xcresult` |

The `WorkspaceOverlays27` tool call exceeded its 300-second response window; the completed Xcode
log reports `TEST EXECUTE SUCCEEDED` and the result bundle reports 4 passed,
0 failed, 0 skipped. Earlier diagnostic/candidate bundles are not passing
verification evidence.

`python3 scripts/ios/localizations.py` verified all 23 languages. No Rust source
changed; the Rust test/clippy suite and the complete iOS UI suite were not rerun.
No physical-device or production-account end-to-end test was performed here.

`AppStoreScreenshotTests` captures the real UI with the existing in-memory,
DEBUG-only store fixtures. The fixtures now include backup receipts matching
their displayed backed-up status. Original PNGs are kept locally under
`misc/dmsg-ios-appstore-1.1.0-20261107`, with language/device folders and a
SHA-256 manifest. The App Store selection contains five images per iPhone size
and two per iPad language. Release notes are derived from `CHANGELOG.md`.

Build 15 retains version 1.1.0 and disables in-app purchases.

Release archive and signed IPA export succeeded for 1.1.0 (15). The main app
and all five extensions have build 15. The exported main Info.plist retains
production APNs/App Attest, `DMsgUseStaging=NO` and
`DMsgInAppPurchasesEnabled=NO`. Xcode confirmed upload success at
2026-11-07 11:57 (UTC+8); the App Store Connect build then entered processing.

App Store Connect accepted the 24 localized screenshots after one stalled
English medium-size image was re-uploaded. Version 1.1.0 (15) was submitted
on 2026-11-07; the verified status is Waiting for Review. The submission contains
only this app version, with manual release retained and no in-app purchases.

## 2026-11-07 TestFlight subscriptions on production accounts

This change starts from local public commit `67bb7857b328b9fd0e6d95220eff0bbfc5d94576`.
StoreKit's verified Apple environment is separate from the business deployment.
With the explicit service opt-ins enabled, Sandbox annual subscriptions use the
same production account and membership pipeline, with no tester allowlist.
Formal memberships take priority, Apple supplies the actual test expiry, and
upgrades/refunds only change contracts in the same Apple environment. Sandbox
history does not consume the first formal cash purchase's refund window.

The app reports a test membership, refreshes its plan after delivery, and finishes
a StoreKit transaction only after the authoritative service returns `applied`.
Xcode-local and unverified transactions are rejected. Business endpoint selection,
Core HTTP proof bytes and production APNs/App Attest selection remain independent of the Apple
transaction environment. No production deployment or App Store configuration
was performed in this change.

Actual local verification:

- `cargo test --locked -p dmsg_client -p dmsg_mobile -p dmsg_commerce --lib`:
  592 client, 27 mobile and 47 commerce tests passed; 5 client and 2 commerce
  tests remain ignored by their existing defaults.
- `cargo test --locked -p dmsg_types -p dmsg_protocol`: 130 tests passed,
  including old Candid and CBOR App Store records without the new optional fields.
- Strict Clippy passed for types, client, mobile and commerce with all targets.
- Three App Store PocketIC tests passed against the optimized commerce Wasm,
  including opt-in/opt-out, duplicate IDs across environments, formal priority,
  short Sandbox expiry, refunds and upgrade preservation. The normally ignored
  legacy test was explicitly run with a real commerce Wasm built from the
  pre-change commit above, not the new Wasm installed twice. Existing Production
  contracts survived and the absent Sandbox opt-in stayed disabled.
- The broader commerce/governance PocketIC selection passed 59 checks across
  two runs: 56 initially, then 3 after building the missing test-only account
  product Wasm. No business assertion or source change was needed for the retry.
  Nine tests were ignored by default; the legacy upgrade was run separately as
  described above. Capacity profiles were not run.
- Native Rust libraries and UniFFI bindings were rebuilt for device and simulator;
  `scripts/ios/artifacts.py verify` passed. All 23 localization catalogs passed
  `scripts/ios/localizations.py`.
- iPhone 16 Pro / iOS 18.4: 112 tests passed in
  `.build/IAP-Sandbox-iPhone.xcresult` (Swift unit suite plus two navigation checks).
  The final Chinese test-membership fixture passed a targeted UI run in
  `.build/IAP-Sandbox-Notice.xcresult`; its screenshot was visually checked for
  the notice, expiry and matching Max limits. This is a DEBUG fixture, not an
  actual purchase screenshot.

The iOS release flag remains disabled and the service opt-ins default off.
This source change does not change version 1.1.0 (15) or upload a new build.
Real TestFlight purchases, Apple renewals/refunds and callbacks, physical-device
App Attest, the full simulator UI matrix and production rollout remain unverified.

Implementation and validation: Codex (OpenAI).

## 2026-11-07 Review of the Sandbox subscription change

Review of `df85b8cd` fixed four defects. A Sandbox term raised a plan-expiry
alert for every renewal, because it always ends within a day and each renewal is
a new contract; Sandbox terms now raise none. TestFlight listed storage packs it
can never sell, with a hint implying a subscription would unlock them; Sandbox
now loads annual subscriptions only and the hint needs a storage product. A legacy
contract without its stored transaction now fails as `INTEGRITY_FAILED` instead of
reading as an absent subject. Contracts with the same start again keep the later
delivery in force, matching the client's choice.

Actual local verification:

- `cargo test --locked --no-fail-fast -p dmsg_commerce -p dmsg_client -p dmsg_mobile --lib`:
  48 commerce, 593 client and 27 mobile tests passed; 2 commerce and 5 client tests
  remain ignored. An earlier run failed once in
  `vault::tests::history_shows_and_reverts_versions`, which this change does not
  touch; it passed five isolated reruns and the full rerun.
- `cargo test --locked -p dmsg_types -p dmsg_protocol`: 130 tests passed. Strict Clippy
  passed for types, client, mobile and commerce; commerce vectors, the integration SDK
  check and `dmsg_commerce.did` are unchanged.
- Three App Store PocketIC tests passed, the legacy one against a commerce Wasm built
  from `67bb7857`. The commerce/governance selection passed 59 and ignored 9.
- Debug native libraries were rebuilt and `scripts/ios/artifacts.py verify` passed.
  iPhone 16 Pro / iOS 18.4: `DeploymentTests` (4) and
  `testSandboxSubscriptionNoticeInChinese` passed.

The purchase flag stays disabled, so the storage-pack change was compiled but not
seen on screen. No real Apple purchase, full simulator UI matrix or deployment was run.

Review and validation: Claude (Anthropic).

## Vault field editing and list previews (2026-11-08)

Field names now precede values and remain editable without opening field settings.
New fields focus their name; Next moves to the value, and the keyboard has a Done
action. Name and value errors appear beside the relevant input after leaving it
or attempting to save. Optional sections show their current name, while reordering
uses compact field rows with native move/delete controls. The add-field menu has
an explicit full-row hit area. Both added labels are translated in all 23 locales.

The client's list-summary policy now matches the desktop: login, API, server and
database entries may preview a non-hidden username or HTTP(S) host. Notes and
other types return no content preview and show their localized type in iOS.
Arbitrary field values, note bodies and URL credentials, paths and parameters are
excluded. The change does not alter the stored item or its detail fields.

Actual local verification on the working tree based on `209fd719`:

- `cargo test --locked -p dmsg_client vault::reading::tests:: --lib`: six passed;
  the existing performance comparison remained ignored. Coverage includes a note
  containing a custom `name` value, hidden usernames and URL credentials/tokens.
- `cargo fmt --check -p dmsg_client` and strict library Clippy passed.
- Release device and simulator native slices were rebuilt with `build-core.sh`;
  `artifacts.py verify --profile release` verified their source fingerprint.
- On iPhone 18 Pro / iOS 27.0 Simulator, seven Swift unit tests and eight distinct
  targeted UI tests passed across the final and regression runs. They cover
  name/value focus, validation, field preservation through reorder mode, note-list
  presentation, Chinese dark and German layouts, Arabic RTL at maximum Dynamic
  Type, cancel/discard, keyboard save, failed-save retry and optional date cancel.
  The final native build reran the seven unit tests and the custom-field save and
  multilingual layout tests (`VaultEditor-Final-20261108.xcresult`).
- App/extension catalogs, compiler-extracted strings, format/plural parity and
  offline privacy copies passed `localizations.py` for all 23 languages.

Initial UI runs found a non-responsive blank area in the add-field row; the
full-row label and borderless style fixed it. The test helper also closes the
previous input through Done before opening the menu with the software keyboard.
The simulator still logged non-finite-frame warnings during keyboard transitions;
the checked interactions and exported screenshots completed correctly.

These are local synthetic UI fixtures and native unit tests. Physical devices,
the complete device/OS matrix, full repository tests and live cloud/account data
were not exercised. No production deployment was performed.
