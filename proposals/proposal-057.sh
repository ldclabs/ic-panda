#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --network ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=41:nat; evidence=blob "\41\44\8b\6d\c2\51\be\65\15\6e\19\79\f9\33\bf\7c\72\65\4e\09\5a\c2\67\56\12\01\52\1f\84\c5\bf\6a"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.5.8\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.5.8.\n\n1. Chore: improve prize claiming validation.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json