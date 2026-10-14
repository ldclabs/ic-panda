#!/bin/sh
# Write the Homebrew formula `dmsg` (the CLI and daemon) and, when the release
# has the macOS desktop package, the cask `dmsg-desktop`, and publish them to
# the tap. The cask depends on the formula, so the desktop app keeps using
# Homebrew's dmsg (`dmsg install` leaves it alone).
#
# Usage: scripts/dmsg/publish-homebrew.sh dmsg-v0.2.0
#
# Environment:
#   HOMEBREW_TAP_TOKEN   token with write access to the tap (CI)
#   HOMEBREW_TAP_REPO    default ldclabs/homebrew-tap
#   HOMEBREW_TAP_DIR     an existing tap checkout instead of cloning
#   CHECKSUM_DIR         read <asset>.sha256 files here instead of downloading
#   DRY_RUN=1            print the formula and cask only

set -eu

REPO=ldclabs/ic-panda
TAP_REPO="${HOMEBREW_TAP_REPO:-ldclabs/homebrew-tap}"
TAG="${1:-${GITHUB_REF_NAME:-}}"

error() { printf 'Error: %s\n' "$1" >&2; exit 1; }

case "$TAG" in
    dmsg-v*) VERSION="${TAG#dmsg-v}" ;;
    *) error "usage: $0 dmsg-v<version>" ;;
esac
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

checksum() {
    if [ -n "${CHECKSUM_DIR:-}" ]; then
        [ -f "${CHECKSUM_DIR}/$1.sha256" ] || return 1
        awk '{print $1}' "${CHECKSUM_DIR}/$1.sha256"
    else
        curl -fsSL "${BASE_URL}/$1.sha256" | awk '{print $1}'
    fi
}

MACOS_ARM64=$(checksum dmsg-macos-arm64) || error "no checksum for dmsg-macos-arm64"
LINUX_X86_64=$(checksum dmsg-linux-x86_64) || error "no checksum for dmsg-linux-x86_64"
LINUX_ARM64=$(checksum dmsg-linux-arm64) || error "no checksum for dmsg-linux-arm64"
DESKTOP=$( (checksum dMsg-mac-arm64.dmg) 2>/dev/null || true)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# Homebrew style keeps url/sha256 out of on_macos: the macOS binary is the
# top-level url and on_linux overrides it per architecture.
cat >"${WORK}/dmsg.rb" <<EOF
class Dmsg < Formula
  desc "CLI and daemon for the private dMsg workspace and its agents"
  homepage "https://dmsg.net/"
  url "${BASE_URL}/dmsg-macos-arm64", using: :nounzip
  version "${VERSION}"
  sha256 "${MACOS_ARM64}"
  license "Apache-2.0"

  on_macos do
    depends_on arch: :arm64
  end

  on_linux do
    on_arm do
      url "${BASE_URL}/dmsg-linux-arm64", using: :nounzip
      sha256 "${LINUX_ARM64}"
    end
    on_intel do
      url "${BASE_URL}/dmsg-linux-x86_64", using: :nounzip
      sha256 "${LINUX_X86_64}"
    end
  end

  def install
    binary = Dir["dmsg-*"].first
    chmod 0755, binary
    bin.install binary => "dmsg"
  end

  def caveats
    <<~TEXT
      Write ~/.dmsg/config.toml, then start the daemon and log in:
        dmsg start && dmsg login
      After an upgrade, restart a running daemon to use the new binary:
        dmsg restart
    TEXT
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/dmsg --version")
  end
end
EOF

if [ -n "$DESKTOP" ]; then
    cat >"${WORK}/dmsg-desktop.rb" <<EOF
cask "dmsg-desktop" do
  version "${VERSION}"
  sha256 "${DESKTOP}"

  url "https://github.com/${REPO}/releases/download/dmsg-v#{version}/dMsg-mac-arm64.dmg"
  name "dMsg"
  desc "Desktop workspace and approvals for dMsg"
  homepage "https://dmsg.net/"

  auto_updates true
  depends_on arch: :arm64
  depends_on formula: "dmsg"
  depends_on :macos

  app "dMsg.app"

  # The formula's dmsg owns the daemon and its login registration; ~/.dmsg
  # holds the vault and stays.
  zap trash: [
    "~/Library/Application Support/dMsg",
    "~/Library/Logs/dMsg",
    "~/Library/Preferences/net.dmsg.desktop.plist",
    "~/Library/Saved Application State/net.dmsg.desktop.savedState",
  ]
end
EOF
fi

if [ "${DRY_RUN:-}" = "1" ]; then
    cat "${WORK}"/*.rb
    exit 0
fi

if [ -n "${HOMEBREW_TAP_DIR:-}" ]; then
    TAP="$HOMEBREW_TAP_DIR"
else
    [ -n "${HOMEBREW_TAP_TOKEN:-}" ] || error "set HOMEBREW_TAP_TOKEN or HOMEBREW_TAP_DIR"
    TAP="${WORK}/tap"
    cat >"${WORK}/askpass.sh" <<'EOF'
#!/bin/sh
case "$1" in
    *Username*) printf '%s\n' x-access-token ;;
    *) printf '%s\n' "${HOMEBREW_TAP_TOKEN:?}" ;;
esac
EOF
    chmod 700 "${WORK}/askpass.sh"
    GIT_ASKPASS="${WORK}/askpass.sh" GIT_TERMINAL_PROMPT=0 git clone --depth 1 "https://github.com/${TAP_REPO}.git" "$TAP"
fi

mkdir -p "${TAP}/Formula" "${TAP}/Casks"
cp "${WORK}/dmsg.rb" "${TAP}/Formula/dmsg.rb"
PATHS="Formula/dmsg.rb"
if [ -f "${WORK}/dmsg-desktop.rb" ]; then
    cp "${WORK}/dmsg-desktop.rb" "${TAP}/Casks/dmsg-desktop.rb"
    PATHS="${PATHS} Casks/dmsg-desktop.rb"
fi
# shellcheck disable=SC2086 # PATHS is a list of tap paths
if [ -z "$(git -C "$TAP" status --porcelain -- $PATHS)" ]; then
    printf 'The tap already has dmsg %s.\n' "$VERSION"
    exit 0
fi
git -C "$TAP" config user.name "${GIT_COMMITTER_NAME:-github-actions[bot]}"
git -C "$TAP" config user.email "${GIT_COMMITTER_EMAIL:-41898282+github-actions[bot]@users.noreply.github.com}"
# shellcheck disable=SC2086
git -C "$TAP" add $PATHS
git -C "$TAP" commit -m "dmsg ${VERSION}"
if [ -n "${HOMEBREW_TAP_DIR:-}" ]; then
    printf 'Committed %s in %s; push it yourself.\n' "$PATHS" "$TAP"
else
    GIT_ASKPASS="${WORK}/askpass.sh" GIT_TERMINAL_PROMPT=0 git -C "$TAP" push origin HEAD
    printf 'Published %s to %s.\n' "$PATHS" "$TAP_REPO"
fi
