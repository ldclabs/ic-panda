#!/bin/bash
# Records the raw takes for the App Store preview video: one simulator recording
# per scene and language, driven by dMsgUITests/AppPreviewTests.
#
#   bash scripts/ios/record-app-preview.sh SIMULATOR_UUID [OUTPUT_DIR]
#
# PREVIEW_LANGS ("zh en") and PREVIEW_SCENES pick a subset. Each take gets a
# .mov, the xcodebuild log, and a .marks file with each PREVIEW-MARK as seconds
# from the start of that recording. Use an iPhone 18 Pro Max simulator, then cut
# the video with scripts/ios/app-preview (see its README).
set -euo pipefail
cd "$(dirname "$0")/../.."
simulator="${1:?Pass an installed iOS simulator UUID}"
if [[ ! "$simulator" =~ ^[A-Fa-f0-9-]{36}$ ]]; then
  echo "Expected a simulator UUID" >&2
  exit 2
fi
out="${2:-misc/dmsg-ios-app-preview/raw}"
langs="${PREVIEW_LANGS:-zh en}"
scenes="${PREVIEW_SCENES:-Scene1Keep Scene2Devices Scene3Send Scene4aRequest Scene4bArrived}"
derived="src/dmsg_ios/.build/PreviewDerivedData"
mkdir -p "$out"

now() { python3 -c 'import time; print(f"{time.time():.3f}")'; }

xcrun simctl boot "$simulator" 2>/dev/null || true
xcrun simctl bootstatus "$simulator" -b >/dev/null
xcrun simctl ui "$simulator" appearance light
xcrun simctl status_bar "$simulator" override --time 9:41 --dataNetwork wifi --wifiMode active --wifiBars 3 \
  --cellularMode active --cellularBars 4 --operatorName "" --batteryState discharging --batteryLevel 100

(cd src/dmsg_ios && xcodegen generate --quiet)
xcodebuild -project src/dmsg_ios/dMsg.xcodeproj -scheme dMsg \
  -destination "platform=iOS Simulator,id=$simulator" -derivedDataPath "$derived" \
  CODE_SIGNING_ALLOWED=NO build-for-testing -quiet

for lang in $langs; do
  for scene in $scenes; do
    take="$out/$lang-$scene"
    echo "Recording $take"
    rm -f "$take.mov" "$take.log" "$take.marks" "$take.rec.log"
    xcrun simctl terminate "$simulator" net.dmsg.ios >/dev/null 2>&1 || true
    xcrun simctl io "$simulator" recordVideo --codec=h264 --force "$take.mov" >"$take.rec.log" 2>&1 &
    recorder=$!
    for _ in $(seq 100); do grep -q "Recording started" "$take.rec.log" 2>/dev/null && break; sleep 0.1; done
    started="$(now)"
    status=0
    TEST_RUNNER_PREVIEW_LANG="$lang" xcodebuild -project src/dmsg_ios/dMsg.xcodeproj -scheme dMsg \
      -destination "platform=iOS Simulator,id=$simulator" -derivedDataPath "$derived" \
      -only-testing:"dMsgUITests/AppPreviewTests/test$scene" CODE_SIGNING_ALLOWED=NO \
      test-without-building >"$take.log" 2>&1 || status=$?
    kill -INT "$recorder" 2>/dev/null || true
    wait "$recorder" || true
    python3 - "$take.log" "$started" >"$take.marks" <<'EOF'
import re, sys
start = float(sys.argv[2])
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    match = re.search(r"PREVIEW-MARK (\S+) ([0-9.]+)", line)
    if match:
        print(f"{float(match.group(2)) - start:7.3f} {match.group(1)}")
EOF
    if [[ $status -ne 0 ]]; then
      echo "Take $take failed; see $take.log" >&2
      exit "$status"
    fi
    cat "$take.marks"
  done
done
xcrun simctl status_bar "$simulator" clear
