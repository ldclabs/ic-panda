#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal $PROPOSAL_NEURON_ID --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "5szpn-tiaaa-aaaaj-qncoq-cai" --wasm-path "$CANISTERS_PATH/ic_oss_cluster.wasm.gz" --mode upgrade --title "Upgrade ic_oss_cluster canister to v1.2.0" --summary "chore: update with latest ICP crates" --url "https://github.com/ldclabs/ic-oss/releases/tag/v1.2.0" > proposal-message.json

# quill send proposal-message.json