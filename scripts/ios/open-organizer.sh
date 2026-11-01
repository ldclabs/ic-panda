#!/bin/bash
# Give Organizer the same rsync environment as testflight.py's IPA export.
set -euo pipefail

if [[ $# != 1 || ! -f "$1/Info.plist" || ! -d "$1/Products/Applications" ]]; then
  echo "Usage: bash scripts/ios/open-organizer.sh path/to/dMsg.xcarchive" >&2
  exit 1
fi

archive_path="$(cd "$1" && pwd -P)"
# xcrun follows DEVELOPER_DIR / xcode-select, including alternate Xcode installs.
xcodebuild_path="$(/usr/bin/xcrun --find xcodebuild)"
xcode_app="${xcodebuild_path%/Contents/Developer/usr/bin/xcodebuild}"
if [[ ! -x "$xcode_app/Contents/MacOS/Xcode" ]]; then
  echo "Select a full Xcode installation with xcode-select or DEVELOPER_DIR." >&2
  exit 1
fi

echo "Opening $archive_path in a new Xcode Organizer process."
echo "Use this new window for Validate App / Distribute App. Existing Xcode windows keep their environment."
# Reusing an existing Xcode process would keep its potentially Homebrew-first PATH.
# Do not change the user's shell, global launch environment, or installed rsync.
exec /usr/bin/open -n -a "$xcode_app" \
  --env 'PATH=/usr/bin:/bin:/usr/sbin:/sbin' "$archive_path"
