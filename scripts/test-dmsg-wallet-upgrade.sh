#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Test the actual published schema-1 bytes, not a current module with an
# entry point renamed. Never install this fixture on a live wallet.
build_dir=${CARGO_TARGET_DIR:-$PWD/target}
mkdir -p "$build_dir"
build_dir=$(cd "$build_dir" && pwd)
old_dir="$build_dir/wallet-upgrade-v3.1.2"
mkdir -p "$old_dir"
old_module=${DMSG_SCHEMA1_WALLET_WASM:-$old_dir/dmsg_wallet.wasm.gz}
if [ ! -f "$old_module" ]; then
  curl --fail --location --retry 3 \
    https://github.com/ldclabs/ic-panda/releases/download/canisters-v3.1.2/dmsg_wallet.wasm.gz \
    --output "$old_module"
fi
test "$(shasum -a 256 "$old_module" | cut -d ' ' -f 1)" = \
  e292e1cba5e54cde276e1d0bf10c5a916de0a80a91d650c1cfe3b46018a7bf5f
export DMSG_SCHEMA1_WALLET_WASM="$old_module"

CARGO_TARGET_DIR="$build_dir" cargo build --locked --release --target wasm32-unknown-unknown \
  -p dmsg_user -p dmsg_handle -p dmsg_cose -p dmsg_directory -p dmsg_payment \
  -p membership -p dmsg_commerce -p dmsg_wallet -p dmsg_wallet_hub \
  -p dmsg_test_ledger -p dmsg_test_sns -p dmsg_test_cmc
export DMSG_WASM_DIR="$build_dir/wasm32-unknown-unknown/release"
CARGO_TARGET_DIR="$build_dir" cargo test --locked -p dmsg_wallet
CARGO_TARGET_DIR="$build_dir" cargo test --locked -p dmsg_integration --features pocketic-tests \
  --test control_plane wallet_upgrade_ -- --include-ignored --nocapture
