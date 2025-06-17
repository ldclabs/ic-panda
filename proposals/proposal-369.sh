#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "zof5a-5yaaa-aaaai-acr2q-cai" --wasm-path "$CANISTERS_PATH/ic_message_channel.wasm.gz" --mode upgrade --title "Upgrade ic_message_channel_02 canister to v2.10.0" --summary "chore: upgrade ic-cdk to v0.18" --url "https://github.com/ldclabs/ic-panda/releases/tag/v2.10.0" $PROPOSAL_NEURON_ID > proposal-message.json

# quill send proposal-message.json