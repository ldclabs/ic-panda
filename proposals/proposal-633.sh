#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.9"
shasum -a 256 -c <<< "06765ec22d48d34c34c753f00fc14cbcf51c7a705780059ae88f5dd4d1f18b29  $CANISTERS_PATH/dmsg_commerce.wasm.gz"

SUMMARY="Upgrade dmsg_commerce to canisters-v3.1.9 to allow a new App Store subscription purchase on another dMsg account after the previous account's paid term has fully expired.

Allow a new, live, non-revoked annual-plan transaction to continue an original Apple subscription chain on another account only when its purchase time is at or after the recorded previous expiry. Cancelling renewal, refunding a purchase or losing a resource lease does not release the remaining paid interval, and storage packs cannot reassign the subscription chain.

Keep each recorded transaction bound to its original beneficiary. Restores, refunds and retries for an older account cannot rebind the current subscription chain or extend the old transaction into the successor account's term. Preserve production/sandbox isolation and historical expiry bounds retained by pruning, without changing stable record layouts.

This proposal upgrades the commerce canister only. Client account-detection fixes and Apple verifier deployment are released separately.

Module: dmsg_commerce.wasm.gz of canisters-v3.1.9, SHA-256 06765ec22d48d34c34c753f00fc14cbcf51c7a705780059ae88f5dd4d1f18b29."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" --target-canister-id "uwist-sqaaa-aaaaj-qsjyq-cai" --wasm-path "$CANISTERS_PATH/dmsg_commerce.wasm.gz" --mode upgrade --title "Upgrade dmsg_commerce canister to canisters-v3.1.9" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.9" "$PROPOSAL_NEURON_ID" > proposal-message.json

# quill send proposal-message.json
