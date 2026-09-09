#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 scripts/sync-cose-chain-key.py
packages=(-p dmsg_protocol -p dmsg_runtime -p dmsg_types -p dmsg_user -p dmsg_handle -p dmsg_cose -p dmsg_payment -p ic_cose_chain_key)
cargo test --locked "${packages[@]}"
cargo clippy --locked "${packages[@]}" -p dmsg_integration -p dmsg_test_ledger --all-targets --features dmsg_integration/pocketic-tests -- -D warnings
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_user -p dmsg_handle -p dmsg_cose -p dmsg_payment -p dmsg_test_ledger

task_tmp=$(mktemp -d)
trap 'rm -rf "$task_tmp"' EXIT
wasm_dir=${DMSG_WASM_DIR:-${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release}
wasm_dir=$(cd "$wasm_dir" && pwd)
for canister in dmsg_user dmsg_handle dmsg_cose dmsg_payment; do
  candid-extractor "$wasm_dir/$canister.wasm" > "$task_tmp/$canister.did"
  diff -u "src/$canister/$canister.did" "$task_tmp/$canister.did"
done
cargo run --locked --quiet -p dmsg_types --example protocol_vectors > "$task_tmp/vectors.json"
diff -u src/dmsg_types/tests/protocol_vectors.json "$task_tmp/vectors.json"
node scripts/verify-dmsg-vectors.mjs "$task_tmp/vectors.json"

# PocketIC 16.0.0, matching the pinned host crate. Set POCKET_IC_BIN to use an
# installed server; otherwise the host crate downloads that fixed release.
export DMSG_WASM_DIR="$wasm_dir"
cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane -- --test-threads=1
