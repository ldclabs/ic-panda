#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# The last committed source before PrincipalDocument display metadata:
# user schema 14 and directory schema 6, built without source modifications.
baseline=70d67c869960c1351977ceb29d9ab1d4a34fed67
build_dir=${CARGO_TARGET_DIR:-$PWD/target}
mkdir -p "$build_dir"
build_dir=$(cd "$build_dir" && pwd)

if [ -z "${DMSG_PRE_PRINCIPAL_WASM_DIR:-}" ]; then
  old_source=$(mktemp -d)
  trap 'rm -rf "$old_source"' EXIT
  git archive "$baseline" | tar -x -C "$old_source"
  # A separate target directory prevents Cargo from reusing current path crates
  # when archive timestamps are older than their existing build artifacts.
  (
    cd "$old_source"
    CARGO_TARGET_DIR="$build_dir/principal-upgrade-old" cargo build --locked --release \
      --target wasm32-unknown-unknown -p dmsg_user -p dmsg_directory
  )
  export DMSG_PRE_PRINCIPAL_WASM_DIR="$build_dir/principal-upgrade-old/wasm32-unknown-unknown/release"
  printf '%s\n' "$baseline" > "$DMSG_PRE_PRINCIPAL_WASM_DIR/source-commit"
fi
test "$(cat "$DMSG_PRE_PRINCIPAL_WASM_DIR/source-commit")" = "$baseline"

CARGO_TARGET_DIR="$build_dir" cargo build --locked --release --target wasm32-unknown-unknown \
  -p dmsg_user -p dmsg_directory -p dmsg_handle -p dmsg_cose -p dmsg_payment \
  -p dmsg_commerce -p membership -p dmsg_test_ledger -p dmsg_test_sns
export DMSG_WASM_DIR="$build_dir/wasm32-unknown-unknown/release"

for canister in dmsg_user dmsg_directory; do
  candid-extractor "$DMSG_PRE_PRINCIPAL_WASM_DIR/$canister.wasm" > "$build_dir/$canister.pre-principal.did"
  candid-extractor "$DMSG_WASM_DIR/$canister.wasm" > "$build_dir/$canister.principal-upgrade.did"
  didc check "$build_dir/$canister.principal-upgrade.did" "$build_dir/$canister.pre-principal.did"
  diff -u "src/$canister/$canister.did" "$build_dir/$canister.principal-upgrade.did"
done

CARGO_TARGET_DIR="$build_dir" cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane principal_upgrade:: -- --ignored --nocapture
