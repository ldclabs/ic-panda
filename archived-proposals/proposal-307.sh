#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal $PROPOSAL_NEURON_ID --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "zof5a-5yaaa-aaaai-acr2q-cai" --wasm-path "$CANISTERS_PATH/ic_message_channel.wasm.gz" --mode upgrade --title "Upgrade ic_message_channel_02 canister to v2.9.4" --summary "chore: update MAX_CHANNEL_MEMBERS to 995" --url "https://github.com/ldclabs/ic-panda/releases/tag/v2.9.4" > proposal-message.json

# quill send proposal-message.json