#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

dfx build ic_panda_luckypool --ic
gzip "$CANISTERS_PATH/ic_panda_luckypool/ic_panda_luckypool.wasm"

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "a7cug-2qaaa-aaaap-ab3la-cai" --wasm-path "$CANISTERS_PATH/ic_panda_luckypool/ic_panda_luckypool.wasm.gz" $PROPOSAL_NEURON_ID > proposal-message.json

quill send proposal-message.json