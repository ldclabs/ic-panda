# iOS implementation validation

Date: 2026-10-25. Branch: `codex/ios-app`, originally based on `58197c35`,
rebased onto local `main` at `537e28ae`. Fetched `origin/main` was `1ac82a94`;
the one additional local commit adds only `proposals/proposal-568.sh`.
The user requested retaining the worktree. Nothing is merged into `main`.

The implementation used the locally revised iOS brief v0.2 and compact UI
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

The mobile adapter rejects biometry-only requests instead of downgrading them to
device-passcode fallback. Hardware approval-key signing is still not implemented
by the iOS host. Chinese UI terminology follows main's “保险库” naming.

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

## What the tests establish

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

## Remaining implementation and release gates

This is a working native implementation and local validation record, **not a
claim that every P0–P3 milestone or the App Store release is complete**.

- Deploy the updated login page, fixed callback page and AASA using real Apple
  identifiers. Verify the id.ai popup/passkey flow and full fragment callback in
  `ASWebAuthenticationSession` on a physical phone. No such login was performed.
- Verify iOS Secure Enclave, device-password fallback, locked-device data
  protection, cross-app paste, cross-device edits/conflicts, approval/revocation
  and recovery against a real account. macOS SE success does not establish iOS SE.
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

Automatic foreground synchronization is off until the user opts in for that
account. File backup still needs a separate explicit choice. Interrupted import
staging is retained rather than erased on startup. These deliberate differences
from automatic defaults are also described in the README.

Implementation and validation: Codex (OpenAI).
