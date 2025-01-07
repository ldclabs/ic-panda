#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal $PROPOSAL_NEURON_ID --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "2rgax-kyaaa-aaaap-anvba-cai" --wasm-path "$CANISTERS_PATH/ic_name_identity.wasm.gz" --mode upgrade --title "Upgrade ic_name_identity canister to v2.9.4" --summary "chore: remove permission control for the 'get_delegators' API" --url "https://github.com/ldclabs/ic-panda/releases/tag/v2.9.4" > proposal-message.json

# quill send proposal-message.json