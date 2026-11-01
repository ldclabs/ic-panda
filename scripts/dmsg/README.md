# dMsg client releases

One release carries the `dmsg` CLI and daemon and the desktop app. Starting
with 1.3.0, its tag is the bare version: `1.3.0` or `1.4.0-beta.1`.
`src/dmsg_cli/Cargo.toml` and `src/dmsg_desktop/package.json` must agree.
Only stable client releases become the repository's Latest; beta releases
are prereleases and canister tags remain `canisters-v*`. The default CLI,
install scripts, website downloads and Homebrew stay on stable releases.
Historical `dmsg-v*` tags remain downloadable, but new releases do not create
compatibility aliases. Older CLI installations can rerun the current installer.

| Asset | Content |
| --- | --- |
| `dmsg-macos-arm64`, `dmsg-linux-x86_64`, `dmsg-linux-arm64`, `dmsg-windows-x64.exe` | The binary, each with `.sha256` |
| `dMsg-mac-arm64.dmg`, `dMsg-mac-arm64.zip` | The macOS app (the zip is what updates download) |
| `dMsg-win-x64.exe` | The Windows installer |
| `latest-mac.yml`, `latest.yml`, `*.blockmap` | Stable desktop update metadata |
| `beta-mac.yml`, `beta.yml`, `*.blockmap` | Beta desktop update metadata |

## Scripts

| Script | Role |
| --- | --- |
| `install.sh` | macOS Apple Silicon and Linux: download, check the SHA-256, `dmsg install`, add the directory to PATH; `--uninstall` |
| `install.ps1` | Windows x64, the same; `-Uninstall` |
| `sign-macos.sh` | Sign the macOS binary with the Developer ID certificate (identifier `net.dmsg.dmsg`, hardened runtime) and notarize it |
| `publish-homebrew.sh` | Write `Formula/dmsg.rb` and `Casks/dmsg-desktop.rb` (which depends on the formula) to `ldclabs/homebrew-tap`; `DRY_RUN=1` prints them |

## Releasing

1. Set the same version in `src/dmsg_cli/Cargo.toml` and
   `src/dmsg_desktop/package.json`, update `Cargo.lock` and the client changelog,
   commit, and push the bare version tag. `scripts/dmsg/release-config.cjs`
   validates `X.Y.Z` or `X.Y.Z-beta.N`, the two versions, and the tag.
2. `.github/workflows/dmsg-release.yml` builds the four binaries, signs and
   notarizes the macOS one, packages the desktop app around the release's own
   binary, verifies its update manifest and checksums, uploads to a draft and
   only then publishes it. Stable releases become Latest and update the tap;
   beta releases have `prerelease: true`, `make_latest: false` and skip Homebrew.
   A final release is rebuilt with the stable version; do not merely clear a
   beta release's prerelease flag or replace already-published package bytes.

Publish stable 1.3.0 first to deliver the channel setting. For a subsequent
feature, use `1.4.0-beta.1`, `1.4.0-beta.2`, then `1.4.0`. Beta users receive
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
| `MAC_CSC_LINK`, `MAC_CSC_KEY_PASSWORD` | Developer ID Application certificate (.p12, base64) |
| `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`, `APPLE_TEAM_ID` | Notarization |
| `WIN_CSC_LINK`, `WIN_CSC_KEY_PASSWORD`, variable `DMSG_WINDOWS_PUBLISHER` | Windows code signing |
| `HOMEBREW_TAP_TOKEN` | Write access to the tap |

Windows jobs are still optional: a failing Windows job does not hold the
macOS/Linux release, and incomplete Windows releases are not offered by the
desktop selector. Real Windows installation remains a separate validation.
The bridge page has no release workflow now
([`src/dmsg_connect`](../../src/dmsg_connect/README.md#release)).
