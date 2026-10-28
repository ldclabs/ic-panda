#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "asxpf-ciaaa-aaaap-an33a-cai" --wasm-path "$CANISTERS_PATH/ic_delegation_store.wasm.gz" --mode upgrade --title "Upgrade ic_delegation_store canister to v2.15.1" --summary "perf: reduce canister cycle costs.\n\n1. refactor: rework the delegation relay and its origin config;\n2. chore: upgrade Rust dependencies and replace ciborium with cbor2." --url "https://github.com/ldclabs/ic-panda/releases/tag/v2.15.1" $PROPOSAL_NEURON_ID > proposal-message.json

# quill send proposal-message.json