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

device() (
  : "${ANDROID_SERIAL:?Select the dedicated test device with ANDROID_SERIAL}"
  adb -s "$ANDROID_SERIAL" get-state
  local marker
  marker="$(adb -s "$ANDROID_SERIAL" shell mktemp /data/local/tmp/dmsg-captures.XXXXXX | tr -d '\r')"
  trap 'adb -s "$ANDROID_SERIAL" shell rm -f "$marker" >/dev/null || true' EXIT
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
  screenshots "$marker"
)

# The QA suite copies its captures (kit gallery, root pages, languages, windows) to
# /sdcard/Download/dmsg-*. Keep this run's captures with the reports for comparison.
screenshots() {
  local output="src/dmsg_android/app/build/reports/dmsg/screenshots"
  local paths png target
  paths="$(adb -s "$ANDROID_SERIAL" shell "for d in /sdcard/Download/dmsg-*; do
    if [ -d \"\$d\" ]; then find \"\$d\" -type f -name '*.png' -newer '$1' || exit; fi
  done" | tr -d '\r')"
  rm -rf "$output"
  mkdir -p "$output"
  while IFS= read -r png; do
    [[ -n "$png" ]] || continue
    target="$output/${png#/sdcard/Download/}"
    mkdir -p "${target%/*}"
    adb -s "$ANDROID_SERIAL" pull -q "$png" "$target"
  done <<< "$paths"
  printf 'Screenshots: %s (%s files)\n' "$output" "$(find "$output" -name '*.png' | wc -l | tr -d ' ')"
}

case "$mode" in
  resources) resources ;;
  build) build ;;
  device) device ;;
  all) build; device ;;
  *) printf 'Usage: %s [resources|build|device|all]\n' "$0" >&2; exit 2 ;;
esac
