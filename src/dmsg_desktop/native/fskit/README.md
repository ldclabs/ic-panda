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

After installing a correctly signed bundle, the user enables it in System Settings → General → Login Items & Extensions → **By Category → File System Extensions**. The generic app-detail FSKit switch is not sufficient evidence that a filesystem module is enabled. A build/sign success is not an enablement or mount success.

Stop every secret mount before updating/removing the module or host. Locking and account switching first invalidate core grants and pending IO, then attempt unmount. An unconfirmed unmount is an error; a mount left by an earlier daemon is reported separately and can be explicitly stopped. Update must not revive an old grant. Remove project symlinks manually only after verifying they still point to the dMsg mount; dMsg never overwrites or deletes an unrelated `.env`.

## Contract and limitations

The module receives metadata and approved bytes through a private Unix socket. The daemon authenticates the module's actual OS peer UID and signature. It never calls that PID the project application's PID. Approval covers an exact file, immutable versions and owner UID for at most 60 seconds. Other programs of that same UID can open/read that file inside the window.

`noCache`, denied cache upgrades, fixed continuous-clock deadlines and explicit cache revocation are required. `isDataCacheInhibited` alone is not the security mechanism. Write, truncate, rename and executable access are rejected. mmap is unsupported: an OS may still create a mapping; already delivered pages cannot be recalled, and future faults can fail with SIGBUS after revocation. Programs that require mmap need another delivery method.

Native configuration does not contain an owner API token, decryption key or plaintext backing file. Source bytes are delivered only after the daemon's approval. A source change stops the volume and requires an explicit start; this is not a promise of automatic IDE reload.

See [validation scope](../../../../docs/protocol/secrets_zh.md#验证入口与边界) for actual evidence. The recorded implementation verification covered production signing only. Enabling or mounting the production module, kernel integration, cross-UID checks, distribution notarization and upgrade/uninstall acceptance require separate evidence.

FSKit supplies UID/GID in access/open contexts; its read callback does not identify an individual reader. Cross-UID transfer of an already open descriptor has not been validated and must not be inferred safe from ordinary pathname-permission tests. This is part of the pending production-module gate.
