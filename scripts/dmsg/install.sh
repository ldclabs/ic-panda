#!/bin/sh
# Install the dmsg CLI and daemon (macOS Apple Silicon, Linux x86_64/arm64).
#
#   curl -fsSL https://raw.githubusercontent.com/ldclabs/ic-panda/main/scripts/dmsg/install.sh | sh
#   sh install.sh --uninstall
#
# The binary comes from the latest dmsg-v* release and is checked against its
# published SHA-256, then `dmsg install` puts it in DMSG_INSTALL_DIR
# (default ~/.local/bin), never over a newer one. Upgrade an installed dmsg
# with `dmsg update`, which also restarts the daemon safely. Uninstalling
# stops the daemon, removes the login registration and the binary, and keeps
# the data in ~/.dmsg.
#
# Environment: DMSG_VERSION (e.g. 0.2.0; default the latest release),
# DMSG_INSTALL_DIR, DMSG_RELEASE_URL (a mirror laid out like GitHub releases).

set -eu

RELEASES="${DMSG_RELEASE_URL:-https://github.com/ldclabs/ic-panda/releases}"
INSTALL_DIR="${DMSG_INSTALL_DIR:-${HOME}/.local/bin}"

info() { printf '%s\n' "$1"; }
error() { printf 'Error: %s\n' "$1" >&2; exit 1; }

uninstall() {
    DMSG="${INSTALL_DIR}/dmsg"
    [ -x "$DMSG" ] || error "no dmsg at ${DMSG}"
    "$DMSG" stop >/dev/null 2>&1 || true
    "$DMSG" autostart uninstall >/dev/null
    rm -f "$DMSG"
    info "Removed ${DMSG} and its login registration."
    info "Your data stays in ${DMSG_HOME:-${HOME}/.dmsg}; delete it yourself only after it has synced."
    exit 0
}

[ "${1:-}" = "--uninstall" ] && uninstall
[ "$#" -eq 0 ] || error "usage: install.sh [--uninstall]"

command -v curl >/dev/null 2>&1 || error "curl is required"
case "$(uname -s)/$(uname -m)" in
    Darwin/arm64) ASSET=dmsg-macos-arm64 ;;
    Linux/x86_64 | Linux/amd64) ASSET=dmsg-linux-x86_64 ;;
    Linux/aarch64 | Linux/arm64) ASSET=dmsg-linux-arm64 ;;
    *) error "no dmsg release for $(uname -s) $(uname -m); on Windows use install.ps1" ;;
esac

if [ -n "${DMSG_VERSION:-}" ]; then
    TAG="dmsg-v${DMSG_VERSION#v}"
else
    TAG=$(curl -fsSL -H 'Accept: application/json' "${RELEASES}/latest" |
        sed -n 's/.*"tag_name" *: *"\([^"]*\)".*/\1/p')
    case "$TAG" in
        dmsg-v*) ;;
        *) error "the latest release (${TAG:-none}) is not a dMsg client release" ;;
    esac
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
info "Downloading ${ASSET} ${TAG}..."
curl -fsSL -o "${WORK}/dmsg" "${RELEASES}/download/${TAG}/${ASSET}"
curl -fsSL -o "${WORK}/dmsg.sha256" "${RELEASES}/download/${TAG}/${ASSET}.sha256"
EXPECTED=$(awk '{print $1}' "${WORK}/dmsg.sha256")
if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL=$(sha256sum "${WORK}/dmsg" | awk '{print $1}')
else
    ACTUAL=$(shasum -a 256 "${WORK}/dmsg" | awk '{print $1}')
fi
[ "$EXPECTED" = "$ACTUAL" ] || error "${ASSET} does not match its published SHA-256"
chmod 755 "${WORK}/dmsg"
"${WORK}/dmsg" install --dir "$INSTALL_DIR"

case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        case "$(basename "${SHELL:-sh}")" in
            zsh) PROFILE="${HOME}/.zshrc"; LINE="export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
            bash) PROFILE="${HOME}/.bashrc"; LINE="export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
            fish) PROFILE="${HOME}/.config/fish/config.fish"; LINE="fish_add_path \"${INSTALL_DIR}\"" ;;
            *) PROFILE="${HOME}/.profile"; LINE="export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
        esac
        if ! grep -F "$INSTALL_DIR" "$PROFILE" >/dev/null 2>&1; then
            mkdir -p "$(dirname "$PROFILE")"
            printf '\n# dmsg\n%s\n' "$LINE" >>"$PROFILE"
            info "Added ${INSTALL_DIR} to PATH in ${PROFILE}; open a new terminal to use it."
        fi
        ;;
esac

info "Next: write ~/.dmsg/config.toml, then \`dmsg start\` and \`dmsg login\`."
info "\`dmsg autostart install\` starts the daemon when you log in."
