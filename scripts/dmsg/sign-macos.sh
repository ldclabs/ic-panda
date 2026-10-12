#!/bin/sh
# Sign the macOS dmsg release binary with the Developer ID certificate and
# notarize it. A bare executable cannot be stapled; Gatekeeper looks the
# ticket up online. The identifier stays net.dmsg.dmsg so the keychain items
# the daemon creates stay readable across `dmsg update`, which installs only
# a binary signed by the same team.
#
# Usage: scripts/dmsg/sign-macos.sh release/dmsg-macos-arm64
#
# Environment (the dmsg-release GitHub environment):
#   MAC_CSC_LINK, MAC_CSC_KEY_PASSWORD  base64 Developer ID Application .p12 and its password
#   APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD, APPLE_TEAM_ID  the notary account

set -eu

# Issuer of current Developer ID certificates; codesign embeds the chain.
DEVELOPER_ID_CA_URL="https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer"

error() { printf 'Error: %s\n' "$1" >&2; exit 1; }

[ "$#" -eq 1 ] || error "usage: $0 <dmsg binary>"
FILE="$1"
: "${MAC_CSC_LINK:?}" "${MAC_CSC_KEY_PASSWORD:?}"
: "${APPLE_ID:?}" "${APPLE_APP_SPECIFIC_PASSWORD:?}" "${APPLE_TEAM_ID:?}"

WORK=$(mktemp -d)
KEYCHAIN="${WORK}/signing.keychain-db"
KEYCHAIN_PASSWORD=$(openssl rand -hex 24)
USER_KEYCHAINS=$(security list-keychains -d user | tr -d '"')

cleanup() {
    # shellcheck disable=SC2086 # one keychain path per word
    security list-keychains -d user -s $USER_KEYCHAINS
    security delete-keychain "$KEYCHAIN" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
printf '%s' "$MAC_CSC_LINK" | base64 --decode >"${WORK}/certificate.p12"
security import "${WORK}/certificate.p12" -k "$KEYCHAIN" -P "$MAC_CSC_KEY_PASSWORD" -T /usr/bin/codesign
curl -fsSL "$DEVELOPER_ID_CA_URL" -o "${WORK}/developer-id-ca.cer"
security import "${WORK}/developer-id-ca.cer" -k "$KEYCHAIN" 2>/dev/null || true
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
# shellcheck disable=SC2086
security list-keychains -d user -s "$KEYCHAIN" $USER_KEYCHAINS

IDENTITY=$(security find-identity -v -p codesigning "$KEYCHAIN" | awk '/"Developer ID Application:/ { print $2; exit }')
[ -n "$IDENTITY" ] || error "MAC_CSC_LINK has no Developer ID Application identity"

codesign --force --sign "$IDENTITY" --keychain "$KEYCHAIN" --options runtime --timestamp \
    --identifier net.dmsg.dmsg "$FILE"
codesign --verify --strict --verbose=2 "$FILE"

mkdir "${WORK}/notarize"
cp "$FILE" "${WORK}/notarize/"
ditto -c -k "${WORK}/notarize" "${WORK}/notarize.zip"
xcrun notarytool submit "${WORK}/notarize.zip" \
    --apple-id "$APPLE_ID" --password "$APPLE_APP_SPECIFIC_PASSWORD" --team-id "$APPLE_TEAM_ID" \
    --wait --timeout 1h --output-format json >"${WORK}/notarization.json" || true
STATUS=$(plutil -extract status raw -o - "${WORK}/notarization.json" 2>/dev/null || true)
if [ "$STATUS" != "Accepted" ]; then
    cat "${WORK}/notarization.json" >&2
    SUBMISSION=$(plutil -extract id raw -o - "${WORK}/notarization.json" 2>/dev/null || true)
    [ -z "$SUBMISSION" ] || xcrun notarytool log "$SUBMISSION" \
        --apple-id "$APPLE_ID" --password "$APPLE_APP_SPECIFIC_PASSWORD" --team-id "$APPLE_TEAM_ID" >&2 || true
    error "notarization did not succeed (status: ${STATUS:-unknown})"
fi
printf 'Signed and notarized %s\n' "$FILE"
