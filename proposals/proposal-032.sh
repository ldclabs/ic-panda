#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --network ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=36:nat; evidence=blob "\a1\74\f0\7f\ae\b5\60\b1\d7\56\0e\2e\72\ea\23\55\67\1e\73\2f\47\0c\27\60\3c\c9\4b\ce\59\e9\55\19"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v1.5.3\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v1.5.3.\n\n1. Chore: improve frontend.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

quill send proposal-message.json