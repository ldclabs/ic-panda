#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 scripts/sync-agent-protocols.py --check

packages=(-p dmsg_protocol -p dmsg_runtime -p dmsg_types -p dmsg_user -p dmsg_handle -p dmsg_cose -p dmsg_directory -p dmsg_payment -p membership -p dmsg_commerce -p dmsg_wallet -p dmsg_wallet_hub -p ic_cose_chain_key -p dmsg_account_product)
cargo test --locked "${packages[@]}"
cargo clippy --locked "${packages[@]}" -p dmsg_integration -p dmsg_test_ledger -p dmsg_test_sns -p dmsg_test_cmc --all-targets --features dmsg_integration/pocketic-tests -- -D warnings
cargo build --locked --release --target wasm32-unknown-unknown -p dmsg_user -p dmsg_handle -p dmsg_cose -p dmsg_directory -p dmsg_payment -p membership -p dmsg_commerce -p dmsg_wallet -p dmsg_wallet_hub -p dmsg_test_ledger -p dmsg_test_sns -p dmsg_test_cmc -p dmsg_account_product
# Built alone so its dependency features don't change the dMsg Wasm above.
cargo build --locked --release --target wasm32-unknown-unknown -p ic_signin_with

task_tmp=$(mktemp -d)
trap 'rm -rf "$task_tmp"' EXIT
wasm_dir=${DMSG_WASM_DIR:-${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release}
wasm_dir=$(cd "$wasm_dir" && pwd)
for canister in dmsg_user dmsg_handle dmsg_cose dmsg_directory dmsg_payment membership dmsg_commerce dmsg_wallet dmsg_wallet_hub ic_signin_with; do
  candid-extractor "$wasm_dir/$canister.wasm" > "$task_tmp/$canister.did"
  diff -u "src/$canister/$canister.did" "$task_tmp/$canister.did"
done
cargo run --locked --quiet -p dmsg_types --example protocol_vectors > "$task_tmp/vectors.json"
diff -u src/dmsg_types/tests/protocol_vectors.json "$task_tmp/vectors.json"
node scripts/verify-dmsg-vectors.mjs "$task_tmp/vectors.json"
cargo run --locked --quiet -p dmsg_types --example commerce_vectors > "$task_tmp/commerce.json"
diff -u src/dmsg_types/tests/commerce_vectors.json "$task_tmp/commerce.json"
node scripts/verify-commerce-vectors.mjs "$task_tmp/commerce.json"

# PocketIC 16.0.0, matching the pinned host crate. Set POCKET_IC_BIN to use an
# installed server; otherwise the host crate downloads that fixed release.
export DMSG_WASM_DIR="$wasm_dir"
export DMSG_EXTERNAL_FIXTURE="$task_tmp/authentication-pocketic.cbor"
# Each test builds its own PocketIC instance, so the tests run in parallel.
pocketic=(cargo test --locked -p dmsg_integration --features pocketic-tests --test control_plane --test directory --test signin_with --)
if [ -n "${DMSG_POCKETIC_SHARD:-}" ]; then
  # CI splits the suite across runners: shard k of n runs every n-th test from
  # the k-th, plus the test that exports the proof the SDK checks below.
  tests=$("${pocketic[@]}" --list --format terse | sed -n 's/: test$//p' |
    awk -v k="${DMSG_POCKETIC_SHARD%/*}" -v n="${DMSG_POCKETIC_SHARD#*/}" \
      '(NR - 1) % n == k - 1 || $0 == "commerce::external_integration::external_authentication_certificate_retry_pause_and_upgrade"')
  # shellcheck disable=SC2086 # one test name per word
  "${pocketic[@]}" --exact $tests
else
  "${pocketic[@]}"
fi


# Third-party contracts and independent browser verifier. Use the real Wasm
# authentication proof exported by the normal PocketIC run above.
python3 scripts/generate-integration-sdk.py --check
cargo run --locked --quiet -p dmsg_types --example integration_vectors > "$task_tmp/integration.json"
diff -u src/dmsg_types/tests/integration_vectors.json "$task_tmp/integration.json"
cargo run --locked --quiet -p dmsg_protocol --example authentication_vector > "$task_tmp/authentication.json"
diff -u src/dmsg_types/tests/authentication_vector.json "$task_tmp/authentication.json"
cargo run --locked --quiet -p dmsg_protocol --example attestation_vector > "$task_tmp/attestation.json"
diff -u src/dmsg_types/tests/attestation_vector.json "$task_tmp/attestation.json"
node --test packages/dmsg-sdk/tests/*.test.ts
