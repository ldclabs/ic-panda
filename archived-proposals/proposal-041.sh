#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh
export CANISTERS_PATH="$HOME/git/github.com/ldclabs/iclighthouse/ICDex-Trader/.dfx/ic/canisters"

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "6sul7-xqaaa-aaaap-ahdsq-cai" --wasm-path "$CANISTERS_PATH/Trader/Trader.wasm" $PROPOSAL_NEURON_ID > proposal-message.json

# quill send proposal-message.json