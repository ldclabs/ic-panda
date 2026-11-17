#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Last public commit before Android certificate policy and Google Play receipts.
baseline=9488d02e7006b6f2e088c7aa4d8906618cb32255
build_dir=${CARGO_TARGET_DIR:-$PWD/target}
mkdir -p "$build_dir"
build_dir=$(cd "$build_dir" && pwd)

if [ -z "${DMSG_PRE_ANDROID_WASM_DIR:-}" ]; then
  old_source=$(mktemp -d)
  trap 'rm -rf "$old_source"' EXIT
  git archive "$baseline" | tar -x -C "$old_source"
  (
    cd "$old_source"
    CARGO_TARGET_DIR="$build_dir/android-upgrade-old" cargo build --locked --release \
      --target wasm32-unknown-unknown -p dmsg_auth -p dmsg_commerce
  )
  export DMSG_PRE_ANDROID_WASM_DIR="$build_dir/android-upgrade-old/wasm32-unknown-unknown/release"
  printf '%s\n' "$baseline" > "$DMSG_PRE_ANDROID_WASM_DIR/source-commit"
fi
test "$(cat "$DMSG_PRE_ANDROID_WASM_DIR/source-commit")" = "$baseline"

CARGO_TARGET_DIR="$build_dir" cargo build --locked --release --target wasm32-unknown-unknown \
  -p dmsg_auth -p dmsg_commerce -p dmsg_user -p dmsg_directory -p dmsg_handle \
  -p dmsg_cose -p dmsg_payment -p membership -p dmsg_test_ledger -p dmsg_test_sns
export DMSG_WASM_DIR="$build_dir/wasm32-unknown-unknown/release"

# Forward state decoding alone does not prove compatibility with released clients.
for canister in dmsg_auth dmsg_commerce; do
  candid-extractor "$DMSG_PRE_ANDROID_WASM_DIR/$canister.wasm" > "$build_dir/$canister.pre-android.did"
  candid-extractor "$DMSG_WASM_DIR/$canister.wasm" > "$build_dir/$canister.android-upgrade.did"
  didc check "$build_dir/$canister.android-upgrade.did" "$build_dir/$canister.pre-android.did"
  diff -u "src/$canister/$canister.did" "$build_dir/$canister.android-upgrade.did"
done

export DMSG_PRE_GOOGLE_PLAY_WASM="$DMSG_PRE_ANDROID_WASM_DIR/dmsg_commerce.wasm"
export DMSG_PRE_ANDROID_AUTH_WASM="$DMSG_PRE_ANDROID_WASM_DIR/dmsg_auth.wasm"
CARGO_TARGET_DIR="$build_dir" cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane --test passkey -- --include-ignored \
  commerce::google_play:: pre_android_upgrade_preserves_identity_sessions_and_exact_certificate_policy
