#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# gh release download canisters-v3.0.0 -R ldclabs/ic-panda -D misc/release/canisters-v3.0.0
export CANISTERS_PATH="$(pwd)/misc/release/canisters-v3.0.0"
shasum -a 256 -c <<< "2e93ff4442ef44f99d13010d4abfea0f8c47eed04b7c2b7420c14ef1e1e212c5  $CANISTERS_PATH/ic_message_channel.wasm.gz"

SUMMARY="feat: prepare the legacy ic_message_channel_03 service for the dMsg migration.

1. feat: add the legacy freeze steps admin_legacy_acknowledge_baseline, admin_legacy_drain and admin_legacy_seal, each with a validate_admin_legacy_* query so SNS generic function proposals can run them: drain stops new business writes, seal makes the canister read-only once the writes in flight finish, and business writes never reopen;
2. feat: track the business writes in flight, so a seal waits for them; admin_legacy_resolve settles an interrupted one after review;
3. feat: add legacy_snapshot, served once the canister is read-only: bounded, digest-chained pages of the frozen channels, members and messages that each caller could read, each reply certified through its ingress request status;
4. chore: upgrade Rust dependencies and the ICP crates, and replace ciborium with cbor2.

The upgrade itself freezes nothing: ic_message_channel_03 keeps serving as before until its drain proposal executes. After the seal, message, member and file reads, including file download tokens, keep working. No upgrade argument is passed, and the new Candid interface is compatible with the deployed one.

Module: ic_message_channel.wasm.gz of canisters-v3.0.0, SHA-256 2e93ff4442ef44f99d13010d4abfea0f8c47eed04b7c2b7420c14ef1e1e212c5."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "4jxyd-pqaaa-aaaah-qdqtq-cai" --wasm-path "$CANISTERS_PATH/ic_message_channel.wasm.gz" --mode upgrade --title "Upgrade ic_message_channel_03 canister to canisters-v3.0.0" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.0.0" $PROPOSAL_NEURON_ID > proposal-message.json

# quill send proposal-message.json
