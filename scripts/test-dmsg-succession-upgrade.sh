#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# The last committed source before device successions: user schema 16,
# built without source modifications. The directory and handle layouts did
# not change, so their current modules serve both sides.
baseline=11064564f885535254070e5cace66793487a7aad
build_dir=${CARGO_TARGET_DIR:-$PWD/target}
mkdir -p "$build_dir"
build_dir=$(cd "$build_dir" && pwd)

if [ -z "${DMSG_PRE_SUCCESSION_WASM_DIR:-}" ]; then
  old_source=$(mktemp -d)
  trap 'rm -rf "$old_source"' EXIT
  git archive "$baseline" | tar -x -C "$old_source"
  # A separate target directory prevents Cargo from reusing current path crates
  # when archive timestamps are older than their existing build artifacts.
  (
    cd "$old_source"
    CARGO_TARGET_DIR="$build_dir/succession-upgrade-old" cargo build --locked --release \
      --target wasm32-unknown-unknown -p dmsg_user
  )
  export DMSG_PRE_SUCCESSION_WASM_DIR="$build_dir/succession-upgrade-old/wasm32-unknown-unknown/release"
  printf '%s\n' "$baseline" > "$DMSG_PRE_SUCCESSION_WASM_DIR/source-commit"
fi
test "$(cat "$DMSG_PRE_SUCCESSION_WASM_DIR/source-commit")" = "$baseline"

CARGO_TARGET_DIR="$build_dir" cargo build --locked --release --target wasm32-unknown-unknown \
  -p dmsg_user -p dmsg_directory -p dmsg_handle -p dmsg_cose -p dmsg_payment \
  -p dmsg_commerce -p membership -p dmsg_test_ledger -p dmsg_test_sns
export DMSG_WASM_DIR="$build_dir/wasm32-unknown-unknown/release"

# Old clients keep working: the new interface is a Candid subtype of the old.
candid-extractor "$DMSG_PRE_SUCCESSION_WASM_DIR/dmsg_user.wasm" > "$build_dir/dmsg_user.pre-succession.did"
candid-extractor "$DMSG_WASM_DIR/dmsg_user.wasm" > "$build_dir/dmsg_user.succession-upgrade.did"
didc check "$build_dir/dmsg_user.succession-upgrade.did" "$build_dir/dmsg_user.pre-succession.did"
diff -u src/dmsg_user/dmsg_user.did "$build_dir/dmsg_user.succession-upgrade.did"

CARGO_TARGET_DIR="$build_dir" cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane -- --include-ignored --nocapture hardware::schema_16_upgrade
