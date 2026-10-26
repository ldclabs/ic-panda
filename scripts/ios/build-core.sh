#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export IPHONEOS_DEPLOYMENT_TARGET=18.0
profile="${DMSG_IOS_PROFILE:-debug}"
case "$profile" in debug|release) ;; *) echo "DMSG_IOS_PROFILE must be debug or release" >&2; exit 2;; esac
flags=()
if [[ "$profile" == release ]]; then flags+=(--release); fi
bridge="$PWD/src/dmsg_ios/CoreBridge"
mkdir -p "$bridge/Generated" "$bridge/Headers" "$bridge/Artifacts"
build_source_sha256="$(python3 scripts/ios/artifacts.py fingerprint)"
cargo build --locked -p dmsg_mobile "${flags[@]}"
cargo run --locked -p dmsg_mobile --features bindgen --bin dmsg-mobile-bindgen -- generate \
  --library "target/$profile/libdmsg_mobile.dylib" --language swift \
  --out-dir "$bridge/Generated"
cp "$bridge/Generated/DMsgCoreFFI.h" "$bridge/Headers/"
cp "$bridge/Generated/DMsgCoreFFI.modulemap" "$bridge/Headers/module.modulemap"
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
  cargo build --locked -p dmsg_mobile --target "$target" "${flags[@]}"
done
# xcodebuild refuses an existing output. Remove only this generated artifact.
rm -rf "$bridge/Artifacts/DMsgCoreFFI.xcframework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/$profile/libdmsg_mobile.a" -headers "$bridge/Headers" \
  -library "target/aarch64-apple-ios-sim/$profile/libdmsg_mobile.a" -headers "$bridge/Headers" \
  -output "$bridge/Artifacts/DMsgCoreFFI.xcframework"
shasum -a 256 Cargo.lock src/dmsg_ios/CoreBridge/Generated/DMsgCore.swift > "$bridge/Artifacts/build.sha256"
python3 scripts/ios/artifacts.py record --profile "$profile" --source-sha256 "$build_source_sha256"
xcodegen generate --spec src/dmsg_ios/project.yml
