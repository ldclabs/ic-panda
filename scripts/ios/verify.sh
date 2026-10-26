#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
simulator="${1:?Pass an installed iOS simulator UUID}"
if [[ ! "$simulator" =~ ^[A-Fa-f0-9-]{36}$ ]]; then
  echo "Expected a simulator UUID" >&2
  exit 2
fi
cargo fmt --check -p dmsg_client -p dmsg_protocol -p dmsg_mobile -p dmsg_cli -p dmsg_commerce -p dmsg_types
cargo test --locked -p dmsg_client -p dmsg_protocol -p dmsg_mobile -p dmsg_cli -p dmsg_commerce -p dmsg_types
cargo clippy --locked -p dmsg_client -p dmsg_mobile -p dmsg_cli -p dmsg_commerce -p dmsg_types --all-targets -- -D warnings
python3 scripts/ios/localizations.py
bash scripts/ios/build-core.sh
result="src/dmsg_ios/.build/Tests-$(date -u +%Y%m%dT%H%M%SZ).xcresult"
mkdir -p src/dmsg_ios/.build
xcodebuild -project src/dmsg_ios/dMsg.xcodeproj -scheme dMsg \
  -destination "platform=iOS Simulator,id=$simulator" \
  -derivedDataPath src/dmsg_ios/.build/DerivedData -resultBundlePath "$result" \
  CODE_SIGNING_ALLOWED=NO test

python3 scripts/ios/localizations.py --derived-data src/dmsg_ios/.build/DerivedData
