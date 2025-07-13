#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "ijyxz-wyaaa-aaaaj-qa4qa-cai" --wasm-path "$CANISTERS_PATH/ic_message_profile.wasm.gz" --mode upgrade --title "Upgrade ic_message_profile canister to v2.12.0" --summary "chore: update dependencies" --url "https://github.com/ldclabs/ic-panda/releases/tag/v2.12.0" $PROPOSAL_NEURON_ID > proposal-message.json

# quill send proposal-message.json