# Native Apple FSKit secret files

Self-written FSKit/ExtensionKit module; no macFUSE runtime, license, installer or FUSE compatibility layer. Current sources use the macOS 27 SDK's cache and access callbacks. Other OS/SDK combinations are unverified.

- `SecretsFS.swift` and `IPC.swift`: production module, `net.dmsg.desktop.SecretsFS`.
- `Probe.swift`: separate synthetic-only test module. It never accesses a Vault or daemon account.
- `DevelopmentHost.swift`: optional independent development host. Do not substitute it for the packaged desktop host.

## Build and signing

Install Xcode and XcodeGen, then build into an external directory:

```sh
python3 build-module.py --output /absolute/build-directory --team YOUR_TEAM_ID
```

An FSKit Module provisioning profile is required in addition to a code signature. Pass `--provision` only when authorized to let Xcode create/update the App ID and development profile in that Apple Developer account. The build script does not launch or enable the module. For distribution use `--configuration Release --identity 'Developer ID Application: …' --profile 'Your FSKit distribution profile'` with an appropriately provisioned profile.

To embed a previously signed module, set `DMSG_FSKIT_EXTENSION` to its absolute `.appex` path when packaging Electron. Packaging checks the exact identifier, entitlement, embedded profile and same signing team as the bundled daemon and app. It preserves the module's own signature and profile rather than resigning it with Electron's entitlements. Release packaging rejects an Apple Development module. The app's existing notarization flow remains responsible for the complete bundle. An ordinary package without this variable does not bundle the extension; mount attempts report unavailable.

## Enable and lifecycle

macOS ships file system extensions turned off. The desktop's secret-files page tells the person to turn on **dMsg secret files** and opens System Settings at Login Items & Extensions; a failed start keeps a fixed cause on the mount's card (`FSKIT_EXTENSION_DISABLED`, `FSKIT_EXTENSION_MISSING`, `FSKIT_EXTENSION_REFUSED`, `FSKIT_DAEMON_UNSIGNED`, `FSKIT_UNAVAILABLE`). Release packages carry the module only once a Developer ID module with an FSKit distribution profile is embedded through `DMSG_FSKIT_EXTENSION`; the release workflow does not do that yet.

After installing a correctly signed bundle, the user enables it in System Settings → General → Login Items & Extensions → **By Category → File System Extensions**. The generic app-detail FSKit switch is not sufficient evidence that a filesystem module is enabled. A build/sign success is not an enablement or mount success.

Stop every secret mount before updating/removing the module or host. Locking and account switching first invalidate core grants and pending IO, then attempt unmount. An unconfirmed unmount is an error; a mount left by an earlier daemon is reported separately and can be explicitly stopped. Update must not revive an old grant. Remove project symlinks manually only after verifying they still point to the dMsg mount; dMsg never overwrites or deletes an unrelated `.env`.

## Contract and limitations

The module receives metadata and approved bytes through a private Unix socket in a 0700 directory of the team's App Group container `<TEAM>.net.dmsg.secrets`: App Sandbox lets the module connect to a socket only there, not in the mount source it is given. The module's entitlements name that group (`$(TeamIdentifierPrefix)net.dmsg.secrets`), and the daemon must be signed by the same team; release signing (`scripts/dmsg/sign-macos.sh`) gives `dmsg` the same group entitlement. The daemon authenticates the module's actual OS peer UID and signature. It never calls that PID the project application's PID. Approval covers an exact file, immutable versions and owner UID for at most 60 seconds. Other programs of that same UID can open/read that file inside the window.

`noCache`, denied cache upgrades, fixed continuous-clock deadlines and explicit cache revocation are required. `isDataCacheInhibited` alone is not the security mechanism. Write, truncate, rename and executable access are rejected. mmap is unsupported: an OS may still create a mapping; already delivered pages cannot be recalled, and future faults can fail with SIGBUS after revocation. Programs that require mmap need another delivery method.

Native configuration does not contain an owner API token, decryption key or plaintext backing file. Source bytes are delivered only after the daemon's approval. A source change stops the volume and requires an explicit start; this is not a promise of automatic IDE reload.

See [validation scope](../../../../docs/protocol/secrets_zh.md#验证入口与边界) for actual evidence. The enabled, development-signed production module passes `real_fskit_window_reads_expire_revoke_and_stop`; cross-UID checks, sleep, distribution notarization and upgrade/uninstall acceptance still require separate evidence.

## Production module test

Build the development host (it only registers the module), then enable **dMsg secret files** as above:

```sh
xcodebuild -project <generated>/DmsgSecretsProbe.xcodeproj -scheme DevelopmentHost -configuration Debug -derivedDataPath /absolute/build-directory CODE_SIGN_STYLE=Automatic CODE_SIGN_IDENTITY='Apple Development' DEVELOPMENT_TEAM=YOUR_TEAM_ID build
```

The test's daemon must be signed by the module's team with the App Group entitlement (a plist with `com.apple.security.application-groups` = `YOUR_TEAM_ID.net.dmsg.secrets`):

```sh
cargo test -p dmsg_cli --bin dmsg --no-run
codesign --force --sign 'Apple Development: …' --entitlements group.plist <the test executable>
<the test executable> --exact secret_integration_tests::real_fskit_window_reads_expire_revoke_and_stop --ignored --nocapture
```

It mounts synthetic sources only. `DMSG_FSKIT_UNAVAILABLE=<code>` checks instead that starting fails with that cause and no plaintext fallback: `FSKIT_EXTENSION_DISABLED` with the module turned off, `FSKIT_DAEMON_UNSIGNED` with an ad hoc signed test executable. The cross-UID read runs only when `sudo -n` works, for example after `sudo -v` in the same terminal.

FSKit supplies UID/GID in access/open contexts; its read callback does not identify an individual reader. Cross-UID transfer of an already open descriptor has not been validated and must not be inferred safe from ordinary pathname-permission tests. This is part of the pending production-module gate.
