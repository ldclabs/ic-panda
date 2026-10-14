# dMsg client releases

One release carries the `dmsg` CLI and daemon and the desktop app, tagged
`dmsg-v<version>`; the version is `src/dmsg_cli/Cargo.toml`'s and
`src/dmsg_desktop/package.json`'s, which must agree. It is the repository's
latest release (canister releases, `v*`, are published with
`make_latest: false`), so `dmsg update`, these install scripts and the desktop
update feed all read `https://github.com/ldclabs/ic-panda/releases/latest`.

| Asset | Content |
| --- | --- |
| `dmsg-macos-arm64`, `dmsg-linux-x86_64`, `dmsg-linux-arm64`, `dmsg-windows-x64.exe` | The binary, each with `.sha256` |
| `dMsg-mac-arm64.dmg`, `dMsg-mac-arm64.zip` | The macOS app (the zip is what updates download) |
| `dMsg-win-x64.exe` | The Windows installer |
| `latest-mac.yml`, `latest.yml`, `*.blockmap` | The desktop update feed |

## Scripts

| Script | Role |
| --- | --- |
| `install.sh` | macOS Apple Silicon and Linux: download, check the SHA-256, `dmsg install`, add the directory to PATH; `--uninstall` |
| `install.ps1` | Windows x64, the same; `-Uninstall` |
| `sign-macos.sh` | Sign the macOS binary with the Developer ID certificate (identifier `net.dmsg.dmsg`, hardened runtime) and notarize it |
| `publish-homebrew.sh` | Write `Formula/dmsg.rb` and `Casks/dmsg-desktop.rb` (which depends on the formula) to `ldclabs/homebrew-tap`; `DRY_RUN=1` prints them |

## Releasing

1. Set the same version in `src/dmsg_cli/Cargo.toml` and
   `src/dmsg_desktop/package.json`, commit, and push the tag `dmsg-v<version>`.
2. `.github/workflows/dmsg-release.yml` builds the four binaries, signs and
   notarizes the macOS one, packages the desktop app around the release's own
   binary, publishes the GitHub release and updates the tap.

The `dmsg-release` environment holds the signing secrets; without them the
release still builds, unsigned, and the macOS app has no update feed:

| Name | Use |
| --- | --- |
| `MAC_CSC_LINK`, `MAC_CSC_KEY_PASSWORD` | Developer ID Application certificate (.p12, base64) |
| `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`, `APPLE_TEAM_ID` | Notarization |
| `WIN_CSC_LINK`, `WIN_CSC_KEY_PASSWORD`, variable `DMSG_WINDOWS_PUBLISHER` | Windows code signing |
| `HOMEBREW_TAP_TOKEN` | Write access to the tap |

Windows builds are not verified yet: a failing Windows job does not hold the
release. The bridge page has no release workflow now
([`src/dmsg_connect`](../../src/dmsg_connect/README.md#release)).
