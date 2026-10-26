#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
archive_path="src/dmsg_ios/.build/dMsg.xcarchive"
if [[ "${1:-}" == --archive-path ]]; then
  archive_path="${2:?Pass an archive output path}"
  shift 2
fi
python3 scripts/ios/localizations.py
# Never archive Swift Release against a development Rust XCFramework.
DMSG_IOS_PROFILE=release bash scripts/ios/build-core.sh
python3 scripts/ios/artifacts.py verify --profile release
xcodebuild -project src/dmsg_ios/dMsg.xcodeproj -scheme dMsg -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$archive_path" archive "$@"
