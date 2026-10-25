# dMsg for iOS

Native SwiftUI client with an in-process `dmsg_mobile` / UniFFI bridge to
`dmsg_client`. There is no daemon, loopback server or embedded web client.
The local implementation brief is `misc/dMsg_iOS_technical_architecture_zh.md`
v0.2 and the compact layouts in `misc/dmsg-ios-uiux` (these are not public
build dependencies).

## Build

Validated toolchain: Xcode 27.1, Rust 1.98.1, UniFFI 0.32.2, XcodeGen 2.46.0.
Deployment target: iOS / iPadOS 18. The SDK version does not raise that minimum.
The current XCFramework and project support ARM64 devices and Apple Silicon
simulators; an Intel Mac simulator slice is not included.

From the repository root:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
bash scripts/ios/build-core.sh
open src/dmsg_ios/dMsg.xcodeproj
```

Build the `dMsg` scheme. `CoreBridge/Package.swift` consumes the generated
Swift module and the device/simulator XCFramework. Build artifacts and the
generated Xcode project are ignored; do not copy a library from a different
commit. The build records the Cargo lockfile and Swift binding hashes.
Use `DMSG_IOS_PROFILE=release` for optimized Rust slices. App code uses Swift 6;
generated UniFFI code is isolated in its Swift 5 language-mode package.

```sh
bash scripts/ios/verify.sh SIMULATOR_UUID
```

The manual `dmsg-ios.yml` workflow runs the same checks on a runner with the
reviewed SDK. A missing SDK is an explicit failure, not a lower-version pass.
The workflow has not been run on GitHub as part of local development.

## Implemented paths

- System authentication with the fixed HTTPS callback, account initialization,
  device pairing by text/share/QR, second-login binding, account switching,
  device revocation and delayed recovery through the existing core.
- Vault search, compact favorites, field-level reveal/copy, TOTP, editing,
  conflict selection and local/sync status. App-managed keys remain read-only.
- Channels, invitations, messages, encrypted durable send intents and immutable
  attachments. Retries retain the original message ID after restart. Incoming
  attachments are verified and re-encrypted a chunk at a time. The inbox has
  cached/refreshable lists, native reading, read marks and archive/restore.
- Private document/photo import, folder browsing, verified Quick Look/share
  copies, explicit cloud backup, public profile publication and approval review.
- Four system tabs and adaptive `NavigationSplitView`, six-language String
  Catalog, Dynamic Type, dark appearance and system accessibility controls.

Production trust parameters come from `dmsg_client/production.toml`, also used
by the desktop. No deep link can replace endpoints, roots or canister pins.
The app's core enforces a 100 MiB import limit; this is an initial engineering
limit, not a measured minimum-device performance guarantee.

## Session and privacy behavior

One application owns one engine. UI snapshots carry a generation, and Swift
also invalidates view work on lock/account changes. The core's bounded host
owns the client without holding a mutex across network waits. Lock cancels the
current future before clearing keys; existing core journals remain authoritative.
Heavy operations have explicit cancellation tokens, linked to Swift task
cancellation. Cooperative holds yield between streaming file steps.

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

The adapter supports user presence with device-passcode fallback. Main's
biometry-only policy is explicitly rejected until its native support is added;
it is never downgraded to passcode fallback. The iOS host does not yet provide
the new hardware approval-key signing backend or management UI.

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

Automatic synchronization is **off by default** and enabled only by an explicit
settings confirmation for the current account. It runs only while the app is
open and unlocked; interaction cancels it in favor of the user's operation.
It does not select files for backup. Files require their own explicit selection
and confirmation. This opt-in differs from the brief's automatic-sync default.

## Deployment gates

This tree is development code, not an App Store release assertion. In particular:

1. Configure the real Apple team and production/staging bundle identifiers.
   Deploy the updated web login and fixed callback page from this branch.
   Generate AASA with `node scripts/ios/aasa.mjs TEAM_ID net.dmsg.ios`, deploy it
   without redirects at `/.well-known/apple-app-site-association`, and verify
   association, the II popup/passkey and fragment callback on a physical phone.
   The fixed callback page clears fragments and offers no forwarding target.
2. Verify Secure Enclave, device-password fallback, lock-screen file protection,
   real copy-to-another-app behavior and the cross-device login/edit/revoke slice.
3. APNs registration/provider services and their authenticated wire contract are
   not implemented or deployed here. Notifications are explicitly shown as
   unavailable; foreground use works independently. No APNs signing key belongs
   in this repository or app.
4. Duo closed, open and partially open presentations have been exercised in
   Apple's iOS 27.1 Duo simulator. The remaining device matrix includes side
   Touch ID, system reserved regions with live activities/camera, real keyboard
   and fold transitions, and App Store screenshots. Simulator layout evidence
   is not Duo hardware evidence.
5. Wallet execution, purchasing, hosted agents, system extensions, long-running
   background transfers and mobile-browser connect redirects retain the brief's
   independent release gates. Unsupported financial approvals cannot be approved
   by this app. App Store account deletion, reporting/blocking, privacy labels
   and export-compliance submissions require the corresponding service/product
   decisions; signing out is not account deletion.

The exact local checks and remaining gaps are recorded in [VALIDATION.md](VALIDATION.md).

Implementation: Codex (OpenAI), 2026-10-25. The development worktree is retained
on `codex/ios-app`; changes are not merged into `main`.
