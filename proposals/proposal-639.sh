#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.10"
shasum -a 256 -c <<< "2f6e6663703063a1e646d32388cf612a2bd0a0d9044b31a418952481f56c1321  $CANISTERS_PATH/dmsg_commerce.wasm.gz"

SUMMARY="Upgrade dmsg_commerce to canisters-v3.1.10 to prevent refunded or superseded App Store subscriptions from regaining benefits and to restore eligible service correctly after a refund reversal.

Record a zero-service termination when a refund arrives before initial delivery, and end the preceding Apple plan even when upgrade delivery is out of order. Retain Apple replacement and refund boundaries across monthly pruning while the original receipt remains valid. Add the backward-compatible optional is_upgraded field; a transaction marked as upgraded cannot grant new rights or clear that flag in a later observation.

Only a newer, verified current Apple observation may reverse a subscription refund. Restore only service not replaced by another contract, exclude refunded intervals from monthly execution allowance and retain consumed usage. Rebuild missing termination records for legacy refunds, and recheck admission so a production Apple refund reversal cannot bypass an existing non-Apple membership.

Preserve account binding and production/sandbox isolation. Verified revocations may be submitted without an additional Apple status query; granting or restoring rights still requires a current status check. Previously issued Apple resource leases remain bounded to one hour, so revocation is not instantaneous across services.

This proposal upgrades only the commerce canister. Deploy it before the Apple verifier update that submits is_upgraded. Local validation covered refund ordering, replay, plan admission and an in-place upgrade from 00d99bf8; real Apple transactions and production configuration were not validated by those tests.

Module: dmsg_commerce.wasm.gz of canisters-v3.1.10, SHA-256 2f6e6663703063a1e646d32388cf612a2bd0a0d9044b31a418952481f56c1321."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" --target-canister-id "uwist-sqaaa-aaaaj-qsjyq-cai" --wasm-path "$CANISTERS_PATH/dmsg_commerce.wasm.gz" --mode upgrade --title "Upgrade dmsg_commerce canister to canisters-v3.1.10" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.10" "$PROPOSAL_NEURON_ID" > proposal-message.json

# quill send proposal-message.json
