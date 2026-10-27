# dMsg for iOS

Native SwiftUI client with an in-process `dmsg_mobile` / UniFFI bridge to
`dmsg_client`. There is no daemon, loopback server or embedded web client.
The local implementation brief is `misc/dMsg_iOS_technical_architecture_zh.md`
v0.4 and the compact layouts in `misc/dmsg-ios-uiux` (these are not public
build dependencies).

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

The manual `dmsg-ios.yml` workflow runs the same checks on a runner with the
reviewed SDK. A missing SDK is an explicit failure, not a lower-version pass.
The workflow has not been run on GitHub as part of local development.

## Implemented paths

- System authentication with the fixed HTTPS callback, account initialization,
  device pairing by text/share/QR, second-login binding, account switching,
  device revocation and delayed recovery through the existing core.
- Vault search, compact favorites, field-level reveal/copy, TOTP, editing,
  conflict selection, field references, attachment relationships and local/sync
  status. App-managed keys remain read-only.
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
- Four system tabs and adaptive `NavigationSplitView`, six-language String
  Catalog, Dynamic Type, dark appearance and system accessibility controls.

Production trust parameters come from `dmsg_client/production.toml`, also used
by the desktop. No deep link can replace endpoints, roots or canister pins.
The app's core enforces a 100 MiB import limit; this is an initial engineering
limit, not a measured minimum-device performance guarantee.
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

The app follows iOS language preferences unless a per-app language is selected.
The **App language** link on the access screen and in **Account** opens iOS
Settings, where the dMsg language option is always shown. Supported languages
are English, Simplified Chinese, French, Spanish, Russian and Arabic. This uses
the system's per-app setting for native UI and Foundation strings; no in-memory
locale override is installed.

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

Automatic synchronization is **off by default** and enabled only by an explicit
settings confirmation for the current account. It runs only while the app is
open and unlocked; interaction cancels it in favor of the user's operation.
It does not select files for backup. Files require their own explicit selection
and confirmation. This opt-in differs from the brief's automatic-sync default.

Manual synchronization shows an activity indicator until completion or failure.
Both manual and opted-in synchronization refresh the verified cloud-backup
inventory. Files lists cloud backups directly alongside local content; restoring
file bytes onto this device remains an explicit action. The Account header reads
the official public name and avatar, with anonymous, bounded avatar downloads and
an explicit retry when loading fails.

Recovery checks use public certified metadata while the app is active, including
when the content workspace is locked. They do not depend on content-sync opt-in,
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
   current-biometry invalidation, real copy-to-another-app behavior and the
   cross-device login/edit/revoke slice. Confirm user home supports the current
   AccountInfo/hardware-proof contract before testing real protected accounts.
   Simulator/software-vector checks do not establish native ACL or presence behavior.
3. APNs, App Attest and StoreKit now have native APIs, provider/verification
   services and a governed commerce adapter. Apple products and deployment
   credentials must be provisioned; no private key belongs in this repository
   or app. Production refuses sandbox purchase facts. TestFlight and App Store
   Release builds both use the fixed production services; a sandbox receipt never
   changes business endpoints. Testers register with their own Internet Identity
   passkey. No shared review account or StagingDeployment.json is required.
   This build sets DMSG_IN_APP_PURCHASES_ENABLED=NO: product loading, purchase,
   restore, transaction listeners and delivery are disabled, while current plan
   and usage remain readable. Purchases need a separate release decision and
   verified Apple/service configuration before this build switch is enabled.
   Staging remains an explicit Debug-only option with separate storage, Keychain
   namespaces and a visible banner; it is not selected by TestFlight.
4. Duo closed, open and partially open presentations have been exercised in
   Apple's iOS 27.1 Duo simulator. The remaining device matrix includes side
   Touch ID, system reserved regions with live activities/camera, real keyboard
   and fold transitions, and App Store screenshots. Simulator layout evidence
   is not Duo hardware evidence.
5. Wallet execution/management, agent management, Share/AutoFill/Files extensions,
   widgets, App Intents, browser app-hop navigation and bounded background ciphertext
   transfers are implemented. Existing product-level wallet creation pause and the
   paid-letter sending gate remain. iOS is not a persistent agent daemon.
6. Service-account deletion, report/block, offline/privacy pages and a public local
   demo are implemented. The offline tour never authenticates or purchases. Real
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
