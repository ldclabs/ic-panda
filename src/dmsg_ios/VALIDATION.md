# iOS implementation validation

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
- APNs registration, its authenticated public wire contract and the private
  provider service are not implemented or deployed. The settings page explicitly
  reports this. Foreground synchronization is available without push permission.
- Legacy account/control methods use the cancellable dedicated owner. The full
  shared-`Hold` conversion remains incomplete; local reads may queue behind a
  network operation. Lock/pause cancel it before clearing keys.
- Agent administration, wallet execution, purchasing, system extensions,
  long-running background transfers and mobile-browser connect redirects retain
  separate implementation/release gates. Paid-letter approvals are disabled;
  the inbox currently supports receiving, reading and archiving, not composing.
- Minimum-device performance and large-file memory budgets, Duo physical Touch ID,
  live activities/camera reserved regions, App Store account deletion,
  reporting/blocking, privacy labels and distribution review remain unverified or
  require the corresponding service/product work. No canister stable layout was
  changed, and no canister upgrade test is claimed.

The 8 MiB secret-package ceiling limits payload buffers, not process RSS. Chunk
buffers, the manifest, Swift UI metadata and password derivation have additional
costs; minimum-device peak memory remains a physical-device acceptance check.
Foreground recovery checks are independent of content-sync opt-in and cannot
promise notification delivery while the app is suspended or protected data is
unavailable. Desktop-local approvals and grants are not mirrored to the phone.

Automatic foreground synchronization is off until the user opts in for that
account. File backup still needs a separate explicit choice. Interrupted import
staging is retained rather than erased on startup. These deliberate differences
from automatic defaults are also described in the README.

Implementation and validation: Codex (OpenAI).
