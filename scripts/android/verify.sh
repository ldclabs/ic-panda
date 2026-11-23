#!/usr/bin/env bash
# Secret-free checks. Device mode requires an explicitly selected disposable target.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
mode="${1:-build}"
abis="${DMSG_ANDROID_ABIS:-arm64-v8a,x86_64}"

resources() {
  python3 src/dmsg_android/tools/sync-localizations.py --check --complete
  python3 src/dmsg_android/tools/sync-notifications.py --check
}

build() {
  resources
  cargo test --locked -p dmsg_protocol -p dmsg_mobile
  (
    cd src/dmsg_android
    ./gradlew --console=plain --dependency-verification=strict \
      -Pdmsg.abis="$abis" \
      :app:testProductionNoneDebugUnitTest \
      :app:compileProductionGmsDebugKotlin \
      :app:compileStagingNoneDebugKotlin \
      :app:compileStagingGmsDebugKotlin \
      :app:lintProductionNoneDebug :app:lintProductionGmsDebug \
      :app:assembleProductionNoneRelease :app:bundleProductionNoneRelease \
      :app:assembleProductionGmsRelease :app:bundleProductionGmsRelease
  )
  bash scripts/android/build-core.sh --verify --abis "$abis"
  mkdir -p src/dmsg_android/app/build/reports/dmsg
  for channel in none gms; do
    case "$channel" in none) flavor=productionNone ;; gms) flavor=productionGms ;; esac
    output="src/dmsg_android/app/build/outputs/apk/$flavor/release"
    # Signing injection changes the filename, not the checks applied to it.
    shopt -s nullglob
    apks=("$output"/*.apk)
    [[ ${#apks[@]} -eq 1 ]] || { printf 'Expected one release APK in %s\n' "$output" >&2; exit 1; }
    python3 scripts/android/verify-package.py "${apks[0]}" --abis "$abis" \
      --output "src/dmsg_android/app/build/reports/dmsg/$flavor.json"
    bundles=("src/dmsg_android/app/build/outputs/bundle/${flavor}Release/"*.aab)
    [[ ${#bundles[@]} -eq 1 ]] || { printf 'Expected one release bundle for %s\n' "$flavor" >&2; exit 1; }
    python3 scripts/android/verify-package.py "${bundles[0]}" --abis "$abis" \
      --output "src/dmsg_android/app/build/reports/dmsg/$flavor-bundle.json"
  done
}

device() {
  : "${ANDROID_SERIAL:?Select the dedicated test device with ANDROID_SERIAL}"
  adb -s "$ANDROID_SERIAL" get-state
  (
    cd src/dmsg_android
    ./gradlew --console=plain --dependency-verification=strict \
      -Pdmsg.abis="$abis" -Pdmsg.testBuildType=qa \
      :app:testProductionNoneQaUnitTest \
      :app:connectedProductionNoneQaAndroidTest
    # The two channels intentionally share a package ID. Install them sequentially.
    ./gradlew --console=plain --dependency-verification=strict \
      -Pdmsg.abis="$abis" -Pdmsg.testBuildType=qa :app:connectedProductionGmsQaAndroidTest
    # This is the ordinary native library: QA fixture code must not enter it.
    ./gradlew --console=plain --dependency-verification=strict \
      -Pdmsg.abis="$abis" -Pdmsg.testBuildType=debug \
      -Pandroid.testInstrumentationRunnerArguments.networkProbe=true \
      :app:connectedProductionNoneDebugAndroidTest
  )
  screenshots
}

# The QA suite copies its captures (kit gallery, root pages, languages, windows) to
# /sdcard/Download/dmsg-*. Keep them with the reports so a change's before/after runs
# can be compared directory by directory.
screenshots() {
  local output="src/dmsg_android/app/build/reports/dmsg/screenshots"
  rm -rf "$output"
  mkdir -p "$output"
  local dir
  while IFS= read -r dir; do
    [[ -n "$dir" ]] || continue
    adb -s "$ANDROID_SERIAL" pull -q "$dir" "$output/"
  done < <(adb -s "$ANDROID_SERIAL" shell 'for d in /sdcard/Download/dmsg-*; do [ -d "$d" ] && printf "%s\n" "$d"; done' | tr -d '\r')
  find "$output" -type f ! -name '*.png' -delete
  printf 'Screenshots: %s (%s files)\n' "$output" "$(find "$output" -name '*.png' | wc -l | tr -d ' ')"
}

case "$mode" in
  resources) resources ;;
  build) build ;;
  device) device ;;
  all) build; device ;;
  *) printf 'Usage: %s [resources|build|device|all]\n' "$0" >&2; exit 2 ;;
esac
