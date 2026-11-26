# dMsg client releases

One release carries the `dmsg` CLI and daemon and the desktop app. Starting
with 1.3.0, its tag is the bare version: `1.3.0` or `1.4.0-beta.1`.
`src/dmsg_cli/Cargo.toml` and `src/dmsg_desktop/package.json` must agree.
Both **1.5.8** and **1.5.10** are published LDC-signed bridges to Yiwen.
GitHub Latest points to a bridge so older stable clients can upgrade. Clients from
1.5.8 onward enumerate complete releases and select the highest stable version;
Homebrew also advances with each stable release. The Windows installer selects
the highest available stable CLI from the official repository. The macOS and
Linux shell installer and website links using `/releases/latest` first deliver
the bridge. Beta releases are
prereleases and canister tags remain `canisters-v*`.
Historical `dmsg-v*` tags remain downloadable, but new releases do not create
compatibility aliases. Older CLI installations can rerun the current installer.

| Asset | Content |
| --- | --- |
| `dmsg-macos-arm64`, `dmsg-linux-x86_64`, `dmsg-linux-arm64`, `dmsg-windows-x64.exe` | The binary, each with `.sha256` |
| `dMsg-mac-arm64.dmg`, `dMsg-mac-arm64.zip` | The macOS app (the zip is what updates download) |
| `dMsg-win-x64.exe` | Authenticode-signed Windows installer |
| `latest-mac.yml`, `latest.yml`, `*.blockmap` | Stable desktop update metadata |
| `beta-mac.yml`, `beta.yml`, `*.blockmap` | Beta desktop update metadata |

## Scripts

| Script | Role |
| --- | --- |
| `install.sh` | macOS Apple Silicon and Linux: verify and install the CLI at the shared location, or remove it with `--uninstall` |
| `install.ps1` | Windows x64, the same; `-Uninstall` |
| `collect-signed-release.ps1` | Verify the Windows app, installer and bundled CLI, then copy the CLI and compute its release checksum |
| `sign-macos.sh` | Sign and notarize the macOS CLI, and build the signed FSKit extension for the desktop job |
| `macos-signing.json`, `macos-signing.cjs` | Pin Yiwen release signing, the published bridge versions and the Developer ID requirements |
| `test-macos-transition.cjs BRIDGE.app [YIWEN.app]` | Check upgrade compatibility using the on-disk signatures of existing bundles |
| `publish-homebrew.sh` | Write `Formula/dmsg.rb` and `Casks/dmsg-desktop.rb` (which depends on the formula) to `ldclabs/homebrew-tap`; `DRY_RUN=1` prints them |

## Releasing

1. Set the same version in `src/dmsg_cli/Cargo.toml` and
   `src/dmsg_desktop/package.json`, update `Cargo.lock` and the client changelog,
   commit, and push the bare version tag. `scripts/dmsg/release-config.cjs`
   validates `X.Y.Z` or `X.Y.Z-beta.N`, the two versions, and the tag.
2. `.github/workflows/dmsg-release.yml` builds the four binaries, signs and
   notarizes the macOS one, packages the desktop app around the release's own
   binary, verifies its update manifest and checksums, uploads to a draft and
   only then publishes it. Yiwen releases use `make_latest: false`.
   The stable release job checks that Latest still points to 1.5.8 or 1.5.10.
   Every stable release updates the tap. Beta releases have `prerelease: true`,
   `make_latest: false` and skip Homebrew.
   A final release is rebuilt with the stable version; do not merely clear a
   beta release's prerelease flag or replace already-published package bytes.

The channel setting shipped in stable 1.3.0. For a subsequent feature, use
increasing `X.Y.Z-beta.N` tags, then `X.Y.Z`. Beta users receive
the final release too, and their preference stays enabled for later betas.
Desktop and daemon versions remain paired. Protocol and data changes must
remain readable by supported stable clients before beta writers use them.

The desktop defaults to Stable. Settings › About dMsg › Receive Beta updates
opts only this installation in. Turning it off cancels and invalidates a
pending beta update; an installed beta waits for a newer stable version and
is never automatically downgraded. Public prerelease assets are still publicly
downloadable. `DMSG_VERSION=1.4.0-beta.1` explicitly selects that version in
the shell or PowerShell installer; the default `dmsg update` stays stable.

Homebrew owns its binaries: use `brew upgrade dmsg dmsg-desktop` and restart
the app. Beta opt-in is disabled while the desktop uses a Homebrew runtime.
To switch, quit dMsg, stop its daemon, uninstall the Homebrew cask/formula
without `--zap`, then install the official desktop package. Keep `~/.dmsg`;
the app installs its shared runtime on the next start. No second daemon or
separate beta account/workspace is created.

The `dmsg-release` environment holds the signing secrets. macOS release
packaging requires Developer ID signing and notarization for both channels;
local ad hoc builds can still offer manual downloads but cannot auto-install:

| Name | Use |
| --- | --- |
| `MAC_CSC_LINK`, `MAC_CSC_KEY_PASSWORD` | Yiwen Developer ID Application certificate with private key (.p12, base64) and export password |
| `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`, `APPLE_TEAM_ID` | Notarization account, its app-specific password and `63G8VNQPXB` |
| `MAC_FSKIT_PROFILE_BASE64` | Base64 of the Yiwen Developer ID distribution `.provisionprofile` for `net.dmsg.desktop.SecretFiles` with the FSKit Module capability |
| `CERTUM_USERNAME`, `CERTUM_OTP_URI` | SimplySign login and complete TOTP provisioning URI, both Environment secrets |
| `HOMEBREW_TAP_TOKEN` | Write access to the tap |

Starting with 1.6.0-beta.1, both stable and beta use the same Yiwen environment.
The signing job builds `SecretsFS.appex` on an arm64 runner with the macOS 27 SDK.
The desktop job embeds it and verifies the app, daemon, extension, shared App
Group and notarization ticket in the final ZIP. Run the workflow on a branch to
obtain signed artifacts for testing. A matching bare Git tag triggers publication.
See the [macOS transfer runbook](../../src/dmsg_desktop/docs/macos-team-transfer.md).

### Certum signing on the hosted Windows runner

1. Once Certum issues the cloud code signing certificate, export its **public leaf
   certificate** in Base64 X.509 PEM format and commit it as
   `scripts/dmsg/certum-signer.pem` before tagging. For a DER export, convert it
   with `openssl x509 -inform DER -in publisher.cer -out scripts/dmsg/certum-signer.pem`.
   Check the subject and fingerprint against the issued certificate. The workflow
   derives both the store thumbprint and the desktop updater publisher from this file.
2. Under **Settings → Environments → dmsg-release**, set `CERTUM_USERNAME` and
   `CERTUM_OTP_URI`. The latter is the full `otpauth://totp/…` provisioning URI
   for the enrolled account. It grants ongoing access to generate login codes.
   Keep it in the Environment secret. The runner logs into SimplySign Desktop via
   the pinned [setup action](https://github.com/dismine/windows-app-signing-setup-action/tree/89ae3b032d4bc7a5b98d1a42a34e61ecb6faad64).
   The signing key stays with Certum. The account must support unattended signing
   in the authenticated session.
3. Run `workflow_dispatch` on a commit already merged into main with successful
   push CI. Download the Windows artifacts. On Windows, check the CLI and
   installer with `signtool verify /pa /all /v /tw FILE.exe`. On the next version
   tag, both Windows jobs are required. The standalone Windows CLI comes directly
   from the signed desktop package. A failed signature or timestamp check stops publication.
4. For renewal, commit the replacement public PEM before tagging the first release
   with the new certificate. Preserve the desktop publisher subject across renewal
   or plan a publisher transition for clients already installed.

For Windows releases after 1.6.0, the installer and CLI updater check the
Authenticode signature and timestamp against the public certificate from the
official source tag, including when downloading from a mirror. On Linux, the
installer and updater verify each binary against its published `.sha256` file.

The bridge page has no release workflow now
([`src/dmsg_connect`](../../src/dmsg_connect/README.md#release)).
