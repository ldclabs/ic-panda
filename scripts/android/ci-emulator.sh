#!/usr/bin/env bash
# Owns only its temporary AVD. Never erases or reuses a developer's emulator.
set -euo pipefail
: "${ANDROID_HOME:?Android SDK required}"
: "${DMSG_ANDROID_IMAGE:?Select an installed x86_64 system image}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/22.0/bin:$PATH"
export ANDROID_SERIAL=emulator-5580
if adb -s "$ANDROID_SERIAL" get-state >/dev/null 2>&1; then
  printf 'Refusing to reuse %s\n' "$ANDROID_SERIAL" >&2
  exit 1
fi
avd_root="$(mktemp -d)"
export ANDROID_AVD_HOME="$avd_root"
emulator_pid=""
cleanup() {
  if [[ -n "$emulator_pid" ]]; then
    kill "$emulator_pid" 2>/dev/null || true
    wait "$emulator_pid" 2>/dev/null || true
  fi
  rm -rf "$avd_root"
}
trap cleanup EXIT
printf 'no\n' | avdmanager create avd --name dmsg_ci --package "$DMSG_ANDROID_IMAGE" --device pixel_4
mkdir -p "$repo_root/src/dmsg_android/app/build/reports/dmsg"
emulator -avd dmsg_ci -port 5580 -no-window -no-audio -no-boot-anim \
  -no-snapshot -gpu swiftshader_indirect -memory 2048 -cores 2 \
  > "$repo_root/src/dmsg_android/app/build/reports/dmsg/emulator.log" 2>&1 &
emulator_pid=$!
booted=false
for ((attempt=0; attempt<150; attempt++)); do
  kill -0 "$emulator_pid" 2>/dev/null || { printf 'Emulator exited before boot\n' >&2; exit 1; }
  if [[ "$(adb -s "$ANDROID_SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == 1 ]]; then
    booted=true
    break
  fi
  sleep 2
done
[[ "$booted" == true ]] || { printf 'Emulator boot timed out\n' >&2; exit 1; }
# The adaptive suite exercises the >=840 dp two-pane layout in landscape.
# Pixel 4 defaults to only 829 dp there; use the validated test viewport.
adb -s "$ANDROID_SERIAL" shell wm size 1080x2400
adb -s "$ANDROID_SERIAL" shell wm density 420
adb -s "$ANDROID_SERIAL" shell input keyevent 82
adb -s "$ANDROID_SERIAL" shell settings put global window_animation_scale 0
adb -s "$ANDROID_SERIAL" shell settings put global transition_animation_scale 0
adb -s "$ANDROID_SERIAL" shell settings put global animator_duration_scale 0
adb -s "$ANDROID_SERIAL" shell getconf PAGE_SIZE > "$repo_root/src/dmsg_android/app/build/reports/dmsg/page-size.txt"
export DMSG_ANDROID_ABIS=x86_64
bash "$repo_root/scripts/android/verify.sh" device
