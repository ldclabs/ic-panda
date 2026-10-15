#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

# https://forum.dfinity.org/t/icrc-ledger-suite-upgrade/40655

quill sns make-upgrade-canister-proposal $PROPOSAL_NEURON_ID --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "qc6wh-6yaaa-aaaap-anuza-cai" --wasm-path "$CANISTERS_PATH/ic-icrc1-index-ng.wasm.gz" --mode upgrade --title "Upgrade dmsg_index_canister to ledger-suite-icrc-2024-10-17" --summary "upgrade: ledger-suite-icrc-2024-10-17" --url "https://github.com/dfinity/ic/releases/tag/ledger-suite-icrc-2024-10-17" > proposal-message.json

# quill send proposal-message.json