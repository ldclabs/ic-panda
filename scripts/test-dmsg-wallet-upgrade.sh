#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# canisters-v3.1.0, before claim_code existed. Build the historical wallet
# unchanged; testing only two copies of the new module misses this regression.
baseline=b1abacdaa8974dc85a11b2ceaf6983f8b421c6a1
build_dir=${CARGO_TARGET_DIR:-$PWD/target}
mkdir -p "$build_dir"
build_dir=$(cd "$build_dir" && pwd)

if [ -z "${DMSG_PRE_CLAIM_WASM_DIR:-}" ]; then
  old_source=$(mktemp -d)
  trap 'rm -rf "$old_source"' EXIT
  git archive "$baseline" | tar -x -C "$old_source"
  (
    cd "$old_source"
    CARGO_TARGET_DIR="$build_dir/wallet-upgrade-old" cargo build --locked --release \
      --target wasm32-unknown-unknown -p dmsg_wallet
  )
  export DMSG_PRE_CLAIM_WASM_DIR="$build_dir/wallet-upgrade-old/wasm32-unknown-unknown/release"
  printf '%s\n' "$baseline" > "$DMSG_PRE_CLAIM_WASM_DIR/source-commit"
fi
test "$(cat "$DMSG_PRE_CLAIM_WASM_DIR/source-commit")" = "$baseline"

CARGO_TARGET_DIR="$build_dir" cargo build --locked --release --target wasm32-unknown-unknown \
  -p dmsg_user -p dmsg_handle -p dmsg_cose -p dmsg_payment -p dmsg_commerce -p membership \
  -p dmsg_wallet -p dmsg_wallet_hub -p dmsg_test_ledger -p dmsg_test_sns -p dmsg_test_cmc
export DMSG_WASM_DIR="$build_dir/wasm32-unknown-unknown/release"

CARGO_TARGET_DIR="$build_dir" cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane wallet_upgrade_ -- --include-ignored --nocapture
