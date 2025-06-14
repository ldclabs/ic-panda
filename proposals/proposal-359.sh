#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal $PROPOSAL_NEURON_ID --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "e7tgb-6aaaa-aaaap-akqfa-cai" --wasm-path "$CANISTERS_PATH/ic_tee_identity_canister.wasm.gz" --mode upgrade --title "Upgrade ic_tee_identity_canister to v0.6.0" --summary "chore: update with ic-cdk v0.18.3" --url "https://github.com/ldclabs/ic-tee/releases/tag/v0.6.0" > proposal-message.json

# quill send proposal-message.json