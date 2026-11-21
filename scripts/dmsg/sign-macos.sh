#!/bin/sh
# Sign the macOS dmsg release binary with the Developer ID certificate and
# notarize it. A bare executable cannot be stapled; Gatekeeper looks the
# ticket up online. The identifier stays net.dmsg.dmsg so the keychain items
# the daemon creates stay readable across `dmsg update`, which installs only
# a binary signed by the same team.
#
# Usage: scripts/dmsg/sign-macos.sh release/dmsg-macos-arm64 [SecretsFS.zip]
#
# Environment (the dmsg-release GitHub environment):
#   MAC_CSC_LINK, MAC_CSC_KEY_PASSWORD  base64 Developer ID Application .p12 and its password
#   APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD, APPLE_TEAM_ID  the notary account
#   MAC_FSKIT_PROFILE_BASE64  Developer ID provisioning profile for SecretsFS

set -eu

# Issuer of current Developer ID certificates; codesign embeds the chain.
DEVELOPER_ID_CA_URL="https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer"

error() { printf 'Error: %s\n' "$1" >&2; exit 1; }

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || error "usage: $0 <dmsg binary> [SecretsFS.zip]"
FILE="$1"
FSKIT_ARCHIVE=${2:-}
SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname "$0")" && pwd)
DESKTOP="$SCRIPT_DIR/../../src/dmsg_desktop"
: "${MAC_CSC_LINK:?}" "${MAC_CSC_KEY_PASSWORD:?}"
: "${APPLE_ID:?}" "${APPLE_APP_SPECIFIC_PASSWORD:?}" "${APPLE_TEAM_ID:?}"
node "$SCRIPT_DIR/macos-signing.cjs" check-team "$APPLE_TEAM_ID"
if [ -n "$FSKIT_ARCHIVE" ]; then : "${MAC_FSKIT_PROFILE_BASE64:?}"; fi

WORK=$(mktemp -d)
KEYCHAIN="${WORK}/signing.keychain-db"
KEYCHAIN_PASSWORD=$(openssl rand -hex 24)
USER_KEYCHAINS=$(security list-keychains -d user | tr -d '"')
INSTALLED_PROFILE=

cleanup() {
    if [ -n "$INSTALLED_PROFILE" ]; then rm -f "$INSTALLED_PROFILE"; fi
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

IDENTITY=$(security find-identity -v -p codesigning "$KEYCHAIN" | awk -v team="$APPLE_TEAM_ID" '/"Developer ID Application:/ && index($0, "(" team ")\"") { print $2; exit }')
[ -n "$IDENTITY" ] || error "MAC_CSC_LINK has no Developer ID Application identity for $APPLE_TEAM_ID"

# The team's App Group is where the sandboxed FSKit secret-files module
# reaches the daemon's socket (`native_directory` in secret_mounts.rs).
cat >"${WORK}/entitlements.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.security.application-groups</key>
	<array>
		<string>${APPLE_TEAM_ID}.net.dmsg.secrets</string>
	</array>
</dict>
</plist>
PLIST
codesign --force --sign "$IDENTITY" --keychain "$KEYCHAIN" --options runtime --timestamp \
    --identifier net.dmsg.dmsg --entitlements "${WORK}/entitlements.plist" "$FILE"
codesign --verify --strict --verbose=2 "$FILE"
# The notary team also names the FSKit IPC group. Refuse a certificate/team
# mismatch before notarization; never ship an entitlement for the wrong team.
codesign --verify --strict -R "=identifier \"net.dmsg.dmsg\" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"${APPLE_TEAM_ID}\"" "$FILE"

if [ -n "$FSKIT_ARCHIVE" ]; then
    profile="$WORK/SecretsFS.provisionprofile"
    printf '%s' "$MAC_FSKIT_PROFILE_BASE64" | base64 --decode >"$profile"
    uuid=$(node "$DESKTOP/scripts/fskit-package.cjs" profile-uuid "$profile")
    profiles="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
    mkdir -p "$profiles"
    if [ -e "$profiles/$uuid.provisionprofile" ]; then
        cmp -s "$profile" "$profiles/$uuid.provisionprofile" || error "Conflicting installed FSKit profile"
    else
        INSTALLED_PROFILE="$profiles/$uuid.provisionprofile"
        install -m 600 "$profile" "$INSTALLED_PROFILE"
    fi
    python3 "$DESKTOP/native/fskit/build-module.py" --output "$WORK/fskit" \
        --configuration Release --team "$APPLE_TEAM_ID" --identity "$IDENTITY" --profile "$uuid"
    extension="$WORK/fskit/derived/Build/Products/Release/SecretsFS.appex"
    embedded=$(node "$DESKTOP/scripts/fskit-package.cjs" verify-module "$extension" "$FILE")
    [ "$embedded" = "$uuid" ] || error "Unexpected embedded FSKit profile"
    # Preserve Mach-O permissions and the nested signature across Actions jobs.
    ditto -c -k --keepParent "$extension" "$FSKIT_ARCHIVE"
fi

mkdir "${WORK}/notarize"
cp "$FILE" "${WORK}/notarize/"
ditto -c -k "${WORK}/notarize" "${WORK}/notarize.zip"
notary() {
    xcrun notarytool "$@" --apple-id "$APPLE_ID" --password "$APPLE_APP_SPECIFIC_PASSWORD" --team-id "$APPLE_TEAM_ID"
}

response="$WORK/notarization.json"
notary submit "$WORK/notarize.zip" --no-wait --output-format json >"$response"
submission=$(plutil -extract id raw -o - "$response")
[ -n "$submission" ] || error "Apple returned no notarization submission ID"
printf 'Submitted %s to Apple for notarization: %s\n' "$FILE" "$submission"

deadline=$(( $(date +%s) + 4 * 3600 ))
failures=0
while :; do
    if notary info "$submission" --output-format json >"$response"; then
        status=$(plutil -extract status raw -o - "$response")
        failures=0
        case "$status" in
            Accepted) break ;;
            "In Progress") ;;
            Invalid|Rejected)
                notary log "$submission" >&2 || true
                error "Apple notarization $submission: $status" ;;
            *) error "Unexpected status for notarization $submission: $status" ;;
        esac
    else
        failures=$((failures + 1))
        [ "$failures" -lt 5 ] || error "Could not query notarization $submission after five attempts; check it with notarytool info"
        printf 'Retrying notarization status query for %s\n' "$submission" >&2
    fi
    [ "$(date +%s)" -lt "$deadline" ] || error "Stopped waiting for notarization $submission after 4h; check it with notarytool info"
    sleep 60
done
printf 'Signed and notarized %s\n' "$FILE"
