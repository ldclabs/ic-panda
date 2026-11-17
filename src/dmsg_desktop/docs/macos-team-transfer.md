# macOS signing-team transition

The **1.5.8** client release is the LDC-signed bridge to Yiwen. This is the
Developer ID distribution path for the Electron app and `dmsg`, separate from
the iOS App Store transfer and its shared Keychain group.

## Policy and update path

[`scripts/dmsg/macos-signing.json`](../../../scripts/dmsg/macos-signing.json)
is the reviewed source of identities:

| Setting | Value |
| --- | --- |
| Source team | LDC Labs Limited, `S9Z2WAMAV8` |
| Recipient team | Yiwen AI Limited, `63G8VNQPXB` |
| Current release signing team | `S9Z2WAMAV8` |
| Bridge version | `1.5.8` |
| Desktop identifier | `net.dmsg.desktop` |
| CLI/daemon identifier | `net.dmsg.dmsg` |

Squirrel.Mac verifies an update against the installed app's designated
requirement. The old LDC app therefore accepts the LDC-signed bridge. The
bridge's outer app requirement accepts either pinned team, with the exact
desktop identifier and Apple Developer ID Application certificate constraints.
The custom signing hook runs before notarization and preserves nested code's
individual requirements and the bundled daemon's signature. It checks the
actual signer against `releaseTeam` after signing.

The CLI independently checks the exact daemon identifier and Developer ID
signature before installing or running a downloaded/shared binary. Only a
verified LDC `dmsg` accepts the recipient as well as itself. Yiwen accepts
Yiwen only. Neither an environment variable nor release metadata can add a
trusted team. Unsigned development binaries retain their local behavior.

Older stable updaters only know GitHub Latest. Keep that pointer on 1.5.8:
old stable → LDC bridge → latest supported release. The bridge and later
clients enumerate all release pages, selecting the highest complete version
for their platform and channel. Later stable releases use `make_latest: false`;
CI refuses to publish one if Latest is no longer 1.5.8. Each stable release
still updates Homebrew. Existing `/releases/latest` download links and default
install scripts first deliver the bridge, so a fresh install may need one update.
CLI mirrors explicitly configured with `DMSG_RELEASE_URL` retain their existing
`/latest` JSON contract and must manage their own transition routing.

**Old Beta installations are an exception:** they already enumerate releases,
so the Latest pointer cannot force them through the bridge. Before switching
teams, ask Beta users to install 1.5.8. A user who misses it can turn off Beta
and update through 1.5.8 if their version is lower, or manually install the
bridge package. Do not tell users to delete their workspace. Homebrew users
update both the formula and cask with `brew upgrade dmsg dmsg-desktop`.

## Keychain, hardware keys and FSKit

The daemon uses the file-based macOS Keychain through `keyring`, under service
`dmsg`. This release preserves service/account names, workspace paths and
stored key handles. It does not migrate desktop credentials into the iOS App
Group, export hardware keys, rewrite all Keychain ACLs or grant all applications
access. Changing the app's designated requirement does not rewrite existing
Keychain item permissions.

When the daemon later changes teams, macOS may request permission to read an
existing item. Allow the verified Yiwen-signed `dmsg` when the system requests
it, then retry the operation if needed. The storage adapter reports access
errors as unavailable; only an actual `NoEntry` is treated as absent. Do not
reset `~/.dmsg`, remove Keychain items or rotate identities to work around a
permission error. Test consent, denial and retry with disposable credentials
on a separate test account before the Yiwen release.

Secure Enclave keys retain their opaque CryptoKit handles and public keys.
This implementation does **not** establish that every existing hardware handle
survives the team change. The release gate is a real old → bridge → Yiwen test
covering hardware anchor, Touch ID unlock, approval signing and hardware cosign.
If any handle becomes inaccessible, stop the team cutover and investigate;
do not treat that error as permission to replace the user's keys.

FSKit keeps its same-team peer authentication. The app, daemon and
`net.dmsg.desktop.SecretsFS` module must use the same team after the cutover;
the packager checks this when embedding the module. Yiwen needs its own valid
FSKit provisioning profile and `<team>.net.dmsg.secrets` App Group entitlement.
The group contains transient IPC sockets, not the user's workspace. Keep the
existing group for the LDC bridge. Quit the app and stop the old daemon/mounts
before starting the new complete installation; enable the new module if macOS
requires it. Updating only the CLI while retaining an old-team filesystem
module does not provide cross-team FSKit access.

## Publish the LDC bridge

1. Keep the existing `dmsg-release` CI secrets: LDC Developer ID certificate,
   its password, Apple notarization credentials, and
   `APPLE_TEAM_ID=S9Z2WAMAV8`. Keep `releaseTeam` set to LDC. The binary signing
   script and desktop release packager reject a mismatched team.
2. Build and validate version 1.5.8. Inspect the packaged app with
   `codesign -d -r- /path/to/dMsg.app`: its designated requirement must contain
   both pinned teams and `net.dmsg.desktop`. Its actual `TeamIdentifier` must
   still be LDC. Verify the bundled `dmsg` identifier/team, signature integrity,
   notarization and the complete release assets.
3. Exercise an in-place update from an old LDC installation on a test Mac,
   preserving its test workspace. Confirm the installed app/daemon versions,
   credential reads, unlock, approval signing and any enabled FSKit mount.
4. Publish the bare tag `1.5.8` through the existing workflow. Confirm the
   release is complete and GitHub Latest points to it. Leave its assets
   available and unchanged after switching teams.

This source change does not create a tag, publish a release or change secrets.

## Later Yiwen cutover

After the bridge is available and the cross-team checks below pass, change
only `releaseTeam` to `63G8VNQPXB` in the reviewed policy; keep `sourceTeam`,
`recipientTeam`, identifiers and `bridgeVersion` unchanged. Use a higher client
version. Switch signing/notarization CI secrets and any embedded FSKit profile
to Yiwen together. The same signing hook then emits a Yiwen-only outer
requirement. Do not republish 1.5.8 under Yiwen or mark a later release Latest.

Validate a complete Yiwen package on a separate test Mac/account: update from
the bridge, read old test credentials, deny and retry Keychain consent, unlock
with Touch ID, verify unchanged hardware public keys, and remount FSKit after
the old processes have stopped. Also test a long-offline old stable app taking
the two-step update. A downloaded but rejected update must leave the old
installation and workspace usable. New-team testing needs real Yiwen Developer
ID credentials; a requirement parser or an LDC-only fixture cannot substitute.

## Local validation, 2026-11-17

Implementation based on `b413ed1d` (working patch on local `main`).

- Desktop `pnpm check`: no Svelte or TypeScript errors/warnings.
- Desktop `pnpm test`: 203 tests passed; exact requirement syntax is also
  compiled by macOS `csreq`.
- Desktop `pnpm build`: passed. Frozen-lockfile offline dependency install passed.
- `cargo test --locked -p dmsg_cli`: 126 passed, 6 explicitly ignored;
  the 7 release tests include real loopback pagination, stable/complete-asset
  selection, one-way signer policy and replacement behavior.
- `cargo clippy --locked -p dmsg_cli --all-targets -- -D warnings`: passed.
- `DMSG_DESKTOP_SMOKE=updates node scripts/smoke.mjs`: passed with an isolated
  profile/mock daemon. Channel persistence, failed-save retry, late replies,
  keyboard use, update states and Homebrew restrictions exercised. All seven
  generated screenshots inspected: Chinese normal/narrow, Chinese/French/Arabic
  narrow in light/dark themes, Arabic RTL, including long version text.
- `node scripts/dmsg/test-macos-transition.cjs`: actual LDC Developer ID fixtures
  passed old → bridge requirement compatibility, daemon identity checks, and
  rejection of wrong identifiers, ad-hoc signatures, tampering and reverse
  team trust. No production application was installed or credential item read.

Not run: actual Yiwen signing (no local Yiwen identity), notarized distribution
package/update installation, cross-team Keychain consent, Touch ID or FSKit
handoff, remote CI, Windows/Linux packaging or release publication. The Electron
smoke uses update IPC fixtures; it does not execute Squirrel installation.

## References

- [Squirrel.Mac installer verification](https://github.com/Squirrel/Squirrel.Mac/blob/main/Squirrel/SQRLInstaller.m)
- [Apple DTS on file-based Keychain ACLs and partition lists](https://developer.apple.com/forums/thread/845980)
- [GitHub release API and `make_latest`](https://docs.github.com/en/rest/releases/releases?apiVersion=latest)
