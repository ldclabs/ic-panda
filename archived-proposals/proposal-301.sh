#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/.dfx/ic/github"

quill sns make-upgrade-canister-proposal $PROPOSAL_NEURON_ID --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "n3bau-gaaaa-aaaaj-qa4oq-cai" --wasm-path "$CANISTERS_PATH/ic_cose_canister.wasm.gz" --mode upgrade --title "Upgrade ic_cose_canister canister to v0.5.4" --summary "chore: add governance_canister" --url "https://github.com/ldclabs/ic-cose/releases/tag/v0.5.4" --canister-upgrade-arg '(opt variant {Upgrade = record { governance_canister = opt principal "dwv6s-6aaaa-aaaaq-aacta-cai"; }})' > proposal-message.json

# quill send proposal-message.json